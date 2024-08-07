use std::env;

use tracing_subscriber::EnvFilter;

use crate::server::Server;
use crate::thread_pool::ThreadPool;

mod dto;
mod filepath;
mod server;
mod task;
mod thread_pool;
mod transcoder;
mod api_key;

const WORK_DIR_IN_OUT_LIFETIME: u64 = 60 * 60;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
        .init();

    let addr = env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8090".to_string());
    let pool = ThreadPool::new(
        env::var("NUM_WORKERS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .filter(|&val| val > 0),
    );
    let temp_dir = env::var("TEMP_DIR").unwrap_or_else(|_| {
        env::temp_dir()
            .to_str()
            .expect("Cannot get system temp directory")
            .parse()
            .unwrap()
    });
    let api_keys = env::var("API_KEYS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    Server::new(pool, temp_dir, api_keys)
        .start_cleanup_task(
            env::var("RESULT_TTL_SEC")
                .ok()
                .and_then(|val| val.parse::<u64>().ok())
                .map_or(WORK_DIR_IN_OUT_LIFETIME, |val| val),
        )
        .serve(&addr)
        .await
        .expect("Cannot bind the addr")
}
