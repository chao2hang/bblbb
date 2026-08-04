use std::process::ExitCode;

use bblbb_backend::{build_router, AppConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load configuration: {error}");
            return ExitCode::FAILURE;
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log_filter))
        .init();

    let listener = match tokio::net::TcpListener::bind(config.bind_address).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(address = %config.bind_address, %error, "failed to bind server");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(address = %config.bind_address, "server listening");
    if let Err(error) = axum::serve(listener, build_router(config)).await {
        tracing::error!(%error, "server stopped unexpectedly");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
