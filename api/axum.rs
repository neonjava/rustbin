use rustbin::{app, config::Config, database::Firestore};
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;
use vercel_runtime::{Error, axum::VercelLayer};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
    let config = Config::from_env().map_err(|error| std::io::Error::other(error.to_string()))?;
    let store = Firestore::new(&config.project_id, config.firestore_emulator.as_deref());
    let service = ServiceBuilder::new()
        .layer(VercelLayer::new())
        .service(app::router(store, config));
    vercel_runtime::run(service).await
}
