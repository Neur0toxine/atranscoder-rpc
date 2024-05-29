use axum_typed_multipart::{FieldData, TryFromMultipart};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Serialize, Deserialize)]
pub struct ConvertResponse {
    pub id: Option<String>,
    pub error: Option<String>,
}

#[derive(TryFromMultipart)]
pub struct ConvertRequest {
    pub format: String,
    pub codec: String,
    pub codec_opts: Option<String>,
    pub bit_rate: Option<usize>,
    pub max_bit_rate: Option<usize>,
    pub sample_rate: i32,
    pub channel_layout: Option<String>,
    pub callback_url: Option<String>,

    #[form_data(limit = "1GiB")]
    pub file: FieldData<NamedTempFile>,
}

#[derive(Serialize, Deserialize)]
pub struct ConvertURLRequest {
    pub format: String,
    pub codec: String,
    pub codec_opts: Option<String>,
    pub bit_rate: Option<usize>,
    pub max_bit_rate: Option<usize>,
    pub sample_rate: i32,
    pub channel_layout: Option<String>,
    pub url: String,
    pub callback_url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ErrorResponse {
    pub(crate) error: String
}