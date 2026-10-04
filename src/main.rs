use anyhow::Context;
use rustbin::{app, config, database::Firestore};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let config = config::Config::from_env()?;
    let address = SocketAddr::new(config.host, config.port);
    let store = Firestore::new(&config.project_id, config.firestore_emulator.as_deref());
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("bind {address}"))?;
    tracing::info!(%address, "rustbin listening");
    axum::serve(
        listener,
        app::router(store, config).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
