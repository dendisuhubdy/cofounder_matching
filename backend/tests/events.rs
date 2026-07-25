use cofounder_api::messaging::events::{Event, EventBus};
use uuid::Uuid;

#[tokio::test]
async fn an_event_reaches_a_subscriber() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();
    let recipient = Uuid::new_v4();

    bus.publish(recipient, Event::UnreadCount { count: 3 });

    let envelope = receiver.recv().await.expect("an envelope");
    assert_eq!(envelope.recipient_id, recipient);
    assert_eq!(envelope.event, Event::UnreadCount { count: 3 });
}

#[tokio::test]
async fn publishing_with_nobody_listening_is_not_an_error() {
    // The message is already committed by the time it is published. A send
    // that fails because no stream is open must not fail the request.
    let bus = EventBus::new();

    bus.publish(Uuid::new_v4(), Event::UnreadCount { count: 1 });
}

#[tokio::test]
async fn every_subscriber_sees_every_envelope_and_filters_its_own() {
    // Addressing is by recipient_id on the envelope; the stream does the
    // filtering. If that filter is ever dropped, one user's messages are
    // delivered to everyone.
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();

    let mine = Uuid::new_v4();
    let theirs = Uuid::new_v4();

    bus.publish(theirs, Event::UnreadCount { count: 9 });
    bus.publish(mine, Event::UnreadCount { count: 1 });

    let first = receiver.recv().await.unwrap();
    let second = receiver.recv().await.unwrap();

    assert_eq!(first.recipient_id, theirs);
    assert_eq!(second.recipient_id, mine);
}

#[tokio::test]
async fn a_new_message_event_carries_what_a_client_needs() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();

    let conversation_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();

    bus.publish(
        Uuid::new_v4(),
        Event::NewMessage {
            conversation_id,
            sender_id,
            preview: "hello".into(),
        },
    );

    match receiver.recv().await.unwrap().event {
        Event::NewMessage {
            conversation_id: got,
            preview,
            ..
        } => {
            assert_eq!(got, conversation_id);
            assert_eq!(preview, "hello");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
