mod auth;
mod config;
mod sanitizer;
mod server;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

use crate::config::Config;
use crate::server::HostCmdServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("allowed_commands.yaml"));

    tracing::info!("Loading config from: {}", config_path.display());
    let config = Arc::new(Config::load(&config_path)?);

    tracing::info!(
        "Loaded {} commands: {:?}",
        config.commands.len(),
        config.commands.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    let bind_addr = config.bind.clone();
    let auth_token = config.auth_token.clone();
    let ct = tokio_util::sync::CancellationToken::new();

    let config_for_factory = config.clone();
    let service = StreamableHttpService::new(
        move || Ok(HostCmdServer::new(config_for_factory.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );

    let router = auth::with_bearer_auth(
        Router::new().nest_service("/mcp", service),
        auth_token,
    );

    let tcp_listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("gatecmd listening on http://{bind_addr}/mcp");

    axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("Shutting down...");
            ct.cancel();
        })
        .await?;

    Ok(())
}
