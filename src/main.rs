use std::env;

use tracing_subscriber::EnvFilter;

use crate::server::Server;
use crate::thread_pool::ThreadPool;

mod dto;
mod server;
mod task;
mod thread_pool;
mod transcoder;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
        .init();

    let addr = env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8090".to_string());
    let pool = ThreadPool::new(match env::var("NUM_WORKERS") {
        Ok(val) => match val.parse::<usize>() {
            Ok(val) => {
                if val > 0 {
                    Some(val);
                }
                None
            }
            Err(_) => None,
        },
        Err(_) => None,
    });
    let temp_dir = env::var("TEMP_DIR").unwrap_or_else(|_| {
        env::temp_dir()
            .to_str()
            .expect("Cannot get system temp directory")
            .parse()
            .unwrap()
    });
    Server::new(pool, temp_dir)
        .serve(&addr)
        .await
        .expect("Cannot bind the addr")
}