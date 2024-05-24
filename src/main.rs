use crate::server::serve;
use std::env;
use tracing_subscriber::EnvFilter;

mod dto;
mod server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
        .init();

    let addr = env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8090".to_string());

    serve(&addr).await.expect("Cannot bind the addr")
}
