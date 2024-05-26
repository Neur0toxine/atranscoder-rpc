use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use axum_typed_multipart::TypedMultipart;
use tokio::fs;
use tokio::net::TcpListener;
use tokio::time::interval;
use tower_http::trace::TraceLayer;
use tracing::{debug, error};
use uuid::Uuid;

use crate::dto::{ConvertRequest, ConvertResponse};
use crate::task::{Task, TaskParams};
use crate::thread_pool::ThreadPool;

const CONTENT_LENGTH_LIMIT: usize = 30 * 1024 * 1024;
const WORK_DIR_IN_OUT_LIFETIME: u64 = 60 * 60;

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

    pub fn start_cleanup_task(self) -> Self {
        let dir_path = self.work_dir.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;

                if let Err(err) = cleanup_directory(dir_path.as_str()).await {
                    error!("could not perform working directory cleanup: {}", err);
                }
            }
        });
        self
    }

    pub async fn serve(self, addr: &str) -> std::io::Result<()> {
        let this = Arc::new(self);
        let app = Router::new()
            .route(
                "/enqueue",
                post(enqueue_file).layer(DefaultBodyLimit::max(CONTENT_LENGTH_LIMIT)),
            )
            .with_state(this)
            .layer(TraceLayer::new_for_http());

        tracing::info!("listening on {addr}");
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await
    }
}

async fn enqueue_file(
    State(server): State<Arc<Server>>,
    TypedMultipart(req): TypedMultipart<ConvertRequest>,
) -> (StatusCode, Json<ConvertResponse>) {
    let task_id = Uuid::new_v4();
    let input = Path::new(&server.work_dir).join(format!("{}.in.atranscoder", task_id));
    let output = Path::new(&server.work_dir).join(format!("{}.out.atranscoder", task_id));

    let file = req.file;

    match file.contents.persist(input.clone()) {
        Ok(_) => {
            let input_path = match input.to_str() {
                Some(path) => path,
                None => return error_response("Invalid input path"),
            };
            let output_path = match output.to_str() {
                Some(path) => path,
                None => return error_response("Invalid output path"),
            };

            let params = TaskParams {
                format: req.format,
                codec: req.codec,
                codec_opts: req.codec_opts,
                bit_rate: req.bit_rate,
                max_bit_rate: req.max_bit_rate,
                sample_rate: req.sample_rate,
                channel_layout: req.channel_layout,
                upload_url: req.upload_url,
                input_path: input_path.to_string(),
                output_path: output_path.to_string(),
            };
            let task = Task::new(task_id, params);

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
        Err(_) => error_response("Cannot save the file"),
    }
}

fn error_response(msg: &str) -> (StatusCode, Json<ConvertResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json::from(ConvertResponse {
            id: None,
            error: Some(msg.to_string()),
        }),
    )
}

async fn cleanup_directory(dir_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Get the current time
    let now = SystemTime::now();

    // Read the directory
    let mut entries = fs::read_dir(dir_path).await?;

    // Iterate over directory entries
    while let Some(entry) = entries.next_entry().await? {
        let file_path = entry.path();

        // Check if the entry is a file
        if file_path.is_file() {
            // Check if the file extension is ".atranscoder"
            if let Some(extension) = file_path
                .extension()
                .and_then(OsStr::to_str)
                .map(|ext| ext.to_lowercase())
            {
                if extension.eq("atranscoder") {
                    // Get the metadata of the file
                    let metadata = fs::metadata(&file_path).await?;

                    // Get the last modified time of the file
                    let modified_time = metadata.modified()?;

                    // Calculate the duration since the last modification
                    let duration_since_modified = now.duration_since(modified_time)?;

                    // If the file is older than one hour, remove it
                    if duration_since_modified > Duration::from_secs(WORK_DIR_IN_OUT_LIFETIME) {
                        fs::remove_file(file_path.clone()).await?;
                        debug!("removed file: {:?}", file_path);
                    }
                }
            }
        }
    }

    Ok(())
}