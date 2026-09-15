mod config;
mod db;
mod errors;
mod handlers;
mod metrics;
mod middleware;
mod models;
mod routes;

use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub use config::Config;
pub use db::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let config = Config::from_env()?;
    let state = AppState::new(&config.database_url).await?;
    state.run_migrations().await?;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // iter-loop-20260901 r5: HTTP request metrics wrap the WHOLE router
    // (including /metrics itself) so we capture scrape traffic too. Order
    // matters: this layer must be OUTSIDE TraceLayer so a panic in metrics
    // can't bypass observability.
    let app = routes::build_router(state)
        .layer(axum::middleware::from_fn(metrics::http_metrics_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("JiaBu Server listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
