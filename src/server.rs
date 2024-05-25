use std::path::Path;
use std::sync::Arc;

use axum::{Json, Router};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum_typed_multipart::TypedMultipart;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::dto::{ConvertRequest, ConvertResponse};
use crate::task::Task;
use crate::thread_pool::ThreadPool;

const CONTENT_LENGTH_LIMIT: usize = 30 * 1024 * 1024;

pub struct Server {
    thread_pool: Arc<ThreadPool>,
    work_dir: String,
}

impl Server {
    pub(crate) fn new(thread_pool: ThreadPool, work_dir: String) -> Server {
        Server {
            thread_pool: Arc::new(thread_pool),
            work_dir,
        }
    }

    pub async fn serve(self, addr: &str) -> std::io::Result<()> {
        let this = Arc::new(self);
        let app = Router::new()
            .route(
                "/enqueue",
                post(enqueue_file)
                    .layer(DefaultBodyLimit::max(CONTENT_LENGTH_LIMIT)),
            )
            .with_state(this)
            .layer(TraceLayer::new_for_http());

        tracing::info!("listening on {addr}");
        let listener = match TcpListener::bind(addr).await {
            Ok(listen) => listen,
            Err(err) => return Err(err),
        };
        axum::serve(listener, app).await
    }
}

async fn enqueue_file(
    State(server): State<Arc<Server>>,
    TypedMultipart(req): TypedMultipart<ConvertRequest>,
) -> (StatusCode, Json<ConvertResponse>) {
    let task_id = Uuid::new_v4();
    let input =
        Path::new(&server.work_dir).join(format!("{}.in.atranscoder", task_id.to_string()));
    let output =
        Path::new(&server.work_dir).join(format!("{}.out.atranscoder", task_id.to_string()));

    let file = req.file;

    match file.contents.persist(input.clone()) {
        Ok(_) => {
            let input_path = input.to_str();
            let output_path = output.to_str();

            if input_path.is_none() || output_path.is_none() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(ConvertResponse {
                        id: None,
                        error: Some(String::from("Input or output paths are not correct")),
                    }),
                );
            }

            let task = Task::new(
                task_id,
                req.codec,
                req.bit_rate,
                req.max_bit_rate,
                req.sample_rate,
                req.channel_layout,
                req.upload_url,
                input_path.unwrap().to_string(),
                output_path.unwrap().to_string(),
            );

            // Enqueue the task to the thread pool
            server.thread_pool.enqueue(task);

            (
                StatusCode::CREATED,
                Json::from(ConvertResponse {
                    id: Some(task_id.to_string()),
                    error: None,
                }),
            )
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json::from(ConvertResponse {
                id: Some(task_id.to_string()),
                error: Some(String::from("Cannot save the file")),
            }),
        ),
    }
}