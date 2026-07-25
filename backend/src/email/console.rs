use std::collections::HashMap;
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

/// Development-and-test mailer that also retains the most recent link per
/// recipient so the e2e suite can follow it. Only constructed when APP_ENV=test.
///
/// Keyed by address rather than holding a single "most recent link": the e2e
/// suite runs spec files in parallel, so several sign-ins are routinely in
/// flight at once. With one shared slot, one browser consumes another's
/// token and the loser sees a 400 on a link that was never used.
#[derive(Default)]
pub struct LastLinkMailer {
    links: Mutex<HashMap<String, String>>,
}

/// Addresses are normalized before a link is issued, so the lookup has to
/// normalize identically or it reports no link for a user who has one.
fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

impl LastLinkMailer {
    pub fn last_for(&self, email: &str) -> Option<String> {
        self.links.lock().unwrap().get(&normalize(email)).cloned()
    }
}

#[async_trait::async_trait]
impl Mailer for LastLinkMailer {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()> {
        tracing::info!(recipient = %to, login_link = %link, "login link (test mailer)");
        self.links
            .lock()
            .unwrap()
            .insert(normalize(to), link.to_string());
        Ok(())
    }
}
