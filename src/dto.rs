use axum_typed_multipart::{FieldData, TryFromMultipart};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Serialize, Deserialize)]
pub struct Error {
    pub error: String,
}

#[derive(Serialize, Deserialize)]
pub struct TaskResponse {
    pub id: Option<String>,
    pub error: Option<String>,
}

#[derive(TryFromMultipart)]
#[try_from_multipart(rename_all = "camelCase")]
pub struct Task {
    pub codec: String,
    pub bit_rate: usize,
    pub max_bit_rate: usize,
    pub channel_layout: String,

    #[form_data(limit = "25MiB")]
    pub file: FieldData<NamedTempFile>,
}
