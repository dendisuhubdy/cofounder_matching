use std::sync::Mutex;

use super::Mailer;

/// Development mailer: writes the login link to the log instead of sending it.
pub struct ConsoleMailer;

#[async_trait::async_trait]
impl Mailer for ConsoleMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        tracing::info!(recipient = %to, login_link = %link, "login link (not actually emailed)");
        Ok(())
    }
}

/// Test mailer: keeps every message in memory for assertions.
#[derive(Default)]
pub struct RecordingMailer {
    messages: Mutex<Vec<(String, String)>>,
}

impl RecordingMailer {
    pub fn sent(&self) -> Vec<(String, String)> {
        self.messages.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for RecordingMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        self.messages
            .lock()
            .unwrap()
            .push((to.to_string(), link.to_string()));
        Ok(())
    }
}

/// Development-and-test mailer that also retains the most recent link so the
/// e2e suite can follow it. Only constructed when APP_ENV=test.
#[derive(Default)]
pub struct LastLinkMailer {
    last: Mutex<Option<String>>,
}

impl LastLinkMailer {
    pub fn last(&self) -> Option<String> {
        self.last.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for LastLinkMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        tracing::info!(recipient = %to, login_link = %link, "login link (test mailer)");
        *self.last.lock().unwrap() = Some(link.to_string());
        Ok(())
    }
}
