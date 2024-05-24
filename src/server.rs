use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_typed_multipart::TypedMultipart;
use std::path::Path;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::dto;
use crate::dto::{Task, TaskResponse};

const CONTENT_LENGTH_LIMIT: usize = 30 * 1024 * 1024;

pub async fn serve(addr: &str) -> std::io::Result<()> {
    let app = Router::new()
        .route(
            "/enqueue",
            post(enqueue_file).layer(DefaultBodyLimit::max(CONTENT_LENGTH_LIMIT)),
        )
        .route("/download", get(download_file))
        .layer(TraceLayer::new_for_http());

    tracing::info!("listening on {addr}");
    let listener = match TcpListener::bind(addr).await {
        Ok(listen) => listen,
        Err(err) => return Err(err),
    };
    axum::serve(listener, app).await
}

async fn enqueue_file(
    TypedMultipart(Task { file, .. }): TypedMultipart<Task>,
) -> (StatusCode, Json<TaskResponse>) {
    let task_id = Uuid::new_v4().to_string();
    let path = Path::new(
        std::env::temp_dir()
            .to_str()
            .expect("Cannot get temporary directory"),
    )
    .join(format!("{}.bin", task_id));

    match file.contents.persist(path) {
        Ok(_) => (
            StatusCode::CREATED,
            Json::from(TaskResponse {
                id: Option::from(task_id),
                error: None,
            }),
        ),
        Err(_) => (
            StatusCode::CREATED,
            Json::from(TaskResponse {
                id: Option::from(task_id),
                error: Some(String::from("Cannot save the file")),
            }),
        ),
    }
}

async fn download_file() -> (StatusCode, Json<dto::Error>) {
    let resp = dto::Error {
        error: String::from("Not implemented yet."),
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(resp))
}
