use std::sync::Arc;

use cofounder_api::{app, config::Config, db, email::console::ConsoleMailer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;

    let state = app::AppState {
        db: pool,
        mailer: Arc::new(ConsoleMailer),
        base_url: config.base_url.clone(),
        secure_cookies: !config.is_development(),
    };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app::router(state)).await?;

    Ok(())
}
