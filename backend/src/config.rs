use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub base_url: String,
    pub app_env: String,
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            base_url: std::env::var("BASE_URL").context("BASE_URL is required")?,
            app_env: std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        })
    }

    pub fn is_development(&self) -> bool {
        self.app_env == "development"
    }
}
