use tracing::debug;

pub struct Task {
    id: uuid::Uuid,
    codec: String,
    bit_rate: usize,
    max_bit_rate: usize,
    channel_layout: String,
    input_path: String,
    output_path: String,
    upload_url: String,
}

impl Task {
    pub fn new(
        id: uuid::Uuid,
        codec: String,
        bit_rate: usize,
        max_bit_rate: usize,
        channel_layout: String,
        upload_url: String,
        input_path: String,
        output_path: String,
    ) -> Self {
        Task {
            id,
            codec,
            bit_rate,
            max_bit_rate,
            channel_layout,
            input_path,
            output_path,
            upload_url,
        }
    }

    pub fn execute(&self) {
        debug!("Executing task with id: {}", self.id.to_string());
    }
}
