use axum_typed_multipart::{FieldData, TryFromMultipart};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Serialize, Deserialize)]
pub struct ConvertResponse {
    pub id: Option<String>,
    pub error: Option<String>,
}

#[derive(TryFromMultipart)]
#[try_from_multipart(rename_all = "camelCase")]
pub struct ConvertRequest {
    pub format: String,
    pub codec: String,
    pub codec_opts: Option<String>,
    pub bit_rate: usize,
    pub max_bit_rate: usize,
    pub sample_rate: i32,
    pub channel_layout: String,
    pub callback_url: String,

    #[form_data(limit = "1GiB")]
    pub file: FieldData<NamedTempFile>,
}
