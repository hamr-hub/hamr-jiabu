use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub hamr_app_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: std::env::var("PORT").unwrap_or_else(|_| "8082".to_string()).parse()?,
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://hamr:hamr@localhost:5432/hamr_jiabu".to_string()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret".to_string()),
            hamr_app_url: std::env::var("HAMR_APP_URL")
                .unwrap_or_else(|_| "http://hamr-app-api:8081".to_string()),
        })
    }
}
