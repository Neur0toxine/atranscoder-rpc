use crate::dto::ConvertResponse;
use crate::transcoder::{Transcoder, TranscoderParams};
use ffmpeg_next::channel_layout::ChannelLayout;
use ffmpeg_next::{format, Dictionary};
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::{debug, error};
use ureq::Error as UreqError;

#[derive(Clone)]
pub struct Task {
    id: uuid::Uuid,
    params: TaskParams,
}

impl Task {
    pub fn new(id: uuid::Uuid, params: TaskParams) -> Self {
        Task { id, params }
    }

    pub fn execute(self) -> Result<(), Box<dyn Error>> {
        if let Some(download_url) = &self.params.url {
            if let Err(err) = download_file(
                download_url,
                &self.params.input_path,
                self.params.max_body_size,
            ) {
                std::fs::remove_file(Path::new(&self.params.input_path)).ok();
                if let Err(send_err) = send_error(
                    self.id,
                    &format!("Couldn't download the file: {}", err),
                    self.params.callback_url,
                ) {
                    eprintln!("Failed to send error callback: {}", send_err);
                }
                return Err(err);
            }
        }

        if let Err(err) = self.clone().transcode() {
            std::fs::remove_file(Path::new(&self.params.input_path)).ok();
            std::fs::remove_file(Path::new(&self.params.output_path)).ok();
            send_error(
                self.id,
                format!("Couldn't transcode: {}", err).as_str(),
                self.params.callback_url,
            )
            .ok();
            return Err(err);
        }

        std::fs::remove_file(Path::new(&self.params.input_path)).ok();

        if let Err(err) = send_ok(self.id, self.params.clone().callback_url) {
            error!(
                "couldn't send result callback for job id={}, url {}: {}",
                &self.id.to_string(),
                &self.params.callback_url.unwrap_or_default(),
                err
            );
        } else {
            debug!(
                "job id={} result was sent to callback {}",
                &self.id.to_string(),
                &self.params.callback_url.unwrap_or_default()
            );
        }

        Ok(())
    }

    pub fn transcode(self) -> Result<(), Box<dyn Error>> {
        debug!(
            "performing transcoding for task with id: {}",
            self.id.to_string()
        );
        let mut ictx = match format::input(&self.params.input_path) {
            Ok(val) => val,
            Err(err) => {
                error!("couldn't initialize input context: {:?}", err);
                return Err(err.into());
            }
        };

        let octx = if let Some(codec_opts) = &self.params.codec_opts {
            format::output_as_with(
                &self.params.output_path,
                &self.params.format,
                params_to_avdictionary(&codec_opts),
            )
        } else {
            format::output_as(&self.params.output_path, &self.params.format)
        };

        let mut octx = match octx {
            Ok(val) => val,
            Err(err) => {
                error!("couldn't initialize output context: {:?}", err);
                return Err(err.into());
            }
        };

        let transcoder = Transcoder::new(
            &mut ictx,
            &mut octx,
            TranscoderParams {
                codec: self.params.codec,
                codec_opts: self.params.codec_opts,
                bit_rate: self.params.bit_rate,
                max_bit_rate: self.params.max_bit_rate,
                sample_rate: self.params.sample_rate,
                channel_layout: match self.params.channel_layout.unwrap_or_default().as_str() {
                    "stereo" => ChannelLayout::STEREO,
                    "mono" => ChannelLayout::MONO,
                    "stereo_downmix" => ChannelLayout::STEREO_DOWNMIX,
                    _ => ChannelLayout::STEREO,
                },
            },
        );

        let mut transcoder = match transcoder {
            Ok(val) => val,
            Err(err) => {
                error!("couldn't initialize FFmpeg transcoder: {:?}", err);
                return Err(err.into());
            }
        };

        octx.set_metadata(ictx.metadata().to_owned());

        if let Err(err) = octx.write_header() {
            error!("couldn't start transcoding: {:?}", err);
            return Err(err.into());
        }

        for (stream, mut packet) in ictx.packets() {
            if stream.index() == transcoder.stream {
                packet.rescale_ts(stream.time_base(), transcoder.in_time_base);

                if let Err(err) = transcoder.send_packet_to_decoder(&packet) {
                    error!("error sending packet to decoder: {:?}", err);
                    return Err(err.into());
                }

                transcoder
                    .receive_and_process_decoded_frames(&mut octx)
                    .unwrap_or_else(|err| {
                        error!("failure during processing decoded frames: {:?}", err)
                    });
            }
        }

        if let Err(err) = transcoder.send_eof_to_decoder() {
            error!("error sending EOF to decoder: {:?}", err);
            return Err(err.into());
        }

        if let Err(err) = transcoder.receive_and_process_decoded_frames(&mut octx) {
            error!("error receiving and processing decoded frames: {:?}", err);
            return Err(err.into());
        }

        if let Err(err) = transcoder.flush_filter() {
            error!("couldn't flush filter: {:?}", err);
            return Err(err.into());
        }

        transcoder
            .get_and_process_filtered_frames(&mut octx)
            .unwrap_or_else(|err| error!("failure during processing filtered frames: {:?}", err));

        if let Err(err) = transcoder.send_eof_to_encoder() {
            error!("couldn't send EOF to encoder: {:?}", err);
            return Err(err.into());
        }

        transcoder
            .receive_and_process_encoded_packets(&mut octx)
            .unwrap_or_else(|err| error!("failure during transcoding: {:?}", err));

        if let Err(err) = octx.write_trailer() {
            error!("couldn't finish transcoding: {:?}", err);
            return Err(err.into());
        }

        debug!(
            "finished transcoding for task with id: {}",
            self.id.to_string()
        );

        Ok(())
    }
}

#[derive(Clone)]
pub struct TaskParams {
    pub format: String,
    pub codec: String,
    pub codec_opts: Option<String>,
    pub bit_rate: Option<usize>,
    pub max_bit_rate: Option<usize>,
    pub sample_rate: i32,
    pub channel_layout: Option<String>,
    pub url: Option<String>,
    pub input_path: String,
    pub output_path: String,
    pub callback_url: Option<String>,
    pub max_body_size: usize,
}

fn download_file(url: &str, output_path: &str, max_size: usize) -> Result<(), Box<dyn Error>> {
    let response = ureq::get(url).call();

    match response {
        Ok(response) => {
            if response.status() != 200 {
                return Err(format!("Failed to download file: HTTP {}", response.status()).into());
            }

            let mut reader = response.into_reader();
            let mut file = File::create(output_path)?;
            let mut buffer = vec![0; 8 * 1024]; // Read in 8KB chunks
            let mut total_size = 0;

            loop {
                let bytes_read = reader.read(&mut buffer)?;
                if bytes_read == 0 {
                    break;
                }

                total_size += bytes_read;
                if total_size > max_size {
                    return Err("Response body exceeds the limit".into());
                }

                file.write_all(&buffer[..bytes_read])?;
            }
        }
        Err(UreqError::Status(code, _response)) => {
            return Err(format!("Failed to download file: HTTP {}", code).into());
        }
        Err(e) => {
            return Err(format!("Failed to make request: {}", e).into());
        }
    }

    Ok(())
}

fn send_error(
    id: uuid::Uuid,
    error: &str,
    maybe_url: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = maybe_url.unwrap_or_default();
    if url.is_empty() {
        return Ok(());
    }

    let response = ureq::post(url.as_str())
        .set("Content-Type", "application/json")
        .send_json(ConvertResponse {
            id: Some(id.to_string()),
            error: Some(error.to_string()),
        })?;

    if response.status() == 200 {
        Ok(())
    } else {
        Err(format!(
            "failed to send callback to {}. Status: {}",
            url,
            response.status()
        )
        .into())
    }
}

fn send_ok(id: uuid::Uuid, maybe_url: Option<String>) -> Result<(), Box<dyn Error>> {
    let url = maybe_url.unwrap_or_default();
    if url.is_empty() {
        return Ok(());
    }

    let response = ureq::post(url.as_str())
        .set("Content-Type", "application/json")
        .send_json(ConvertResponse {
            id: Some(id.to_string()),
            error: None,
        })?;

    if response.status() == 200 {
        Ok(())
    } else {
        Err(format!(
            "failed to send callback to {}. Status: {}",
            url,
            response.status()
        )
        .into())
    }
}

pub fn params_to_avdictionary(input: &str) -> Dictionary {
    let mut dict: Dictionary = Dictionary::new();
    for pair in input.split(';') {
        let mut parts = pair.split('=');

        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            dict.set(key, value);
        }
    }
    dict
}
