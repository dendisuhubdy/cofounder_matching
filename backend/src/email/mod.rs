pub mod console;

#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    async fn send_login_link(&self, to: &str, link: &str) -> anyhow::Result<()>;
}
