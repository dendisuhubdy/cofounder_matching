use tokio::sync::broadcast;
use uuid::Uuid;

/// What a client is told has happened. Serialized with a `type` tag so the
/// frontend can switch on it without inspecting which fields are present.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    NewMessage {
        conversation_id: Uuid,
        sender_id: Uuid,
        /// Enough to render a notification without a second request. Not the
        /// whole body: the thread is fetched when it is opened.
        preview: String,
    },
    UnreadCount {
        count: i64,
    },
}

/// An event plus who it is for. Every subscriber receives every envelope and
/// discards the ones not addressed to it, so the filter in the SSE handler
/// is the only thing keeping one user's messages away from another's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub recipient_id: Uuid,
    pub event: Event,
}

/// In-process fan-out. This works only while the backend is a single
/// process: two instances behind a load balancer would each see only the
/// events their own requests produced. That is a deliberate limit for a
/// single-droplet deployment, not an oversight — moving to Postgres
/// LISTEN/NOTIFY would replace this type and nothing else.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Envelope>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(256);
        Self { sender }
    }

    /// Deliberately infallible. `send` errors when nobody is subscribed,
    /// which is the ordinary case for a user with no browser tab open — and
    /// the message it describes is already committed either way.
    pub fn publish(&self, recipient_id: Uuid, event: Event) {
        let _ = self.sender.send(Envelope {
            recipient_id,
            event,
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Envelope> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
