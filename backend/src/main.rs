use std::sync::Arc;

use cofounder_api::email::console::{ConsoleMailer, LastLinkMailer};
use cofounder_api::email::Mailer;
use cofounder_api::{app, config::Config, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;

    let test_mailer = if config.app_env == "test" {
        Some(Arc::new(LastLinkMailer::default()))
    } else {
        None
    };

    let mailer: Arc<dyn Mailer> = match &test_mailer {
        Some(m) => m.clone(),
        None => Arc::new(ConsoleMailer),
    };

    let state = app::AppState {
        db: pool,
        mailer,
        base_url: config.base_url.clone(),
        secure_cookies: !config.is_development(),
        test_mailer,
    };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app::router(state)).await?;

    Ok(())
}
