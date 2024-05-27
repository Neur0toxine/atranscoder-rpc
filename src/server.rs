use std::env;
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};

use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
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

use axum::body::Body;
use axum::body::Bytes;
use futures_util::StreamExt;
use std::path::Path as StdPath;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

const CONTENT_LENGTH_LIMIT: usize = 100 * 1024 * 1024;

pub struct Server {
    thread_pool: Arc<ThreadPool>,
    max_body_size: usize,
    work_dir: String,
}

impl Server {
    pub(crate) fn new(thread_pool: ThreadPool, work_dir: String) -> Server {
        Server {
            thread_pool: Arc::new(thread_pool),
            max_body_size: env::var("MAX_BODY_SIZE").map_or(CONTENT_LENGTH_LIMIT, |val| {
                val.parse().map_or(CONTENT_LENGTH_LIMIT, |val| val)
            }),
            work_dir,
        }
    }

    pub fn start_cleanup_task(self, ttl: u64) -> Self {
        let dir_path = self.work_dir.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;

                if let Err(err) = cleanup_directory(dir_path.as_str(), ttl).await {
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
                post(enqueue_file).layer(DefaultBodyLimit::max(this.max_body_size)),
            )
            .route("/get/:identifier", get(download_file))
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
    let input = StdPath::new(&server.work_dir).join(format!("{}.in.atranscoder", task_id));
    let output = StdPath::new(&server.work_dir).join(format!("{}.out.atranscoder", task_id));

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
                callback_url: req.callback_url,
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

async fn download_file(
    State(server): State<Arc<Server>>,
    Path(identifier): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let file_name = format!("{}.out.atranscoder", identifier);
    let file_path = StdPath::new(&server.work_dir).join(file_name);

    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut file = match File::open(&file_path).await {
        Ok(file) => file,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut buffer = [0; 512];
    let n = match file.read(&mut buffer).await {
        Ok(n) if n > 0 => n,
        _ => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mime_type = infer::get(&buffer[..n]).map_or("application/octet-stream".to_string(), |t| {
        t.mime_type().to_string()
    });

    let file = match File::open(&file_path).await {
        Ok(file) => file,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream.map(|result| match result {
        Ok(bytes) => Ok(Bytes::from(bytes)),
        Err(err) => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            err.to_string(),
        )),
    }));

    Ok(([(http::header::CONTENT_TYPE, mime_type)], body))
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

async fn cleanup_directory(dir_path: &str, ttl: u64) -> Result<(), Box<dyn std::error::Error>> {
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
                    if duration_since_modified > Duration::from_secs(ttl) {
                        fs::remove_file(file_path.clone()).await?;
                        debug!("removed file: {:?}", file_path);
                    }
                }
            }
        }
    }

    Ok(())
}
