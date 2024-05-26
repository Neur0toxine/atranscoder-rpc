use crate::dto::ConvertResponse;
use crate::transcoder::{Transcoder, TranscoderParams};
use ffmpeg_next::channel_layout::ChannelLayout;
use ffmpeg_next::{format, Dictionary};
use mime_guess::from_path;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{debug, error};
use ureq::Response;

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
        if let Err(err) = self.clone().transcode() {
            std::fs::remove_file(Path::new(&self.params.input_path)).ok();
            std::fs::remove_file(Path::new(&self.params.output_path)).ok();
            send_error(
                self.id,
                format!("Couldn't transcode: {}", err).as_str(),
                &self.params.upload_url,
            )
            .ok();
            return Err(err);
        }

        std::fs::remove_file(Path::new(&self.params.input_path)).ok();

        if let Err(err) = upload_file(
            &self.id.to_string(),
            &self.params.output_path,
            &self.params.upload_url,
        ) {
            error!(
                "couldn't upload result for job id={}, file path {}: {}",
                &self.id.to_string(),
                &self.params.output_path,
                err
            );
        } else {
            debug!(
                "job id={} result was uploaded to {}",
                &self.id.to_string(),
                &self.params.upload_url
            );
        }

        std::fs::remove_file(Path::new(&self.params.output_path)).ok();
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
                bit_rate: self.params.bit_rate,
                max_bit_rate: self.params.max_bit_rate,
                sample_rate: self.params.sample_rate,
                channel_layout: match self.params.channel_layout.as_str() {
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
    pub bit_rate: usize,
    pub max_bit_rate: usize,
    pub sample_rate: i32,
    pub channel_layout: String,
    pub input_path: String,
    pub output_path: String,
    pub upload_url: String,
}

fn send_error(
    id: uuid::Uuid,
    error: &str,
    url: &str,
) -> Result<Response, Box<dyn std::error::Error>> {
    let response = ureq::post(url)
        .set("Content-Type", "application/json")
        .send_json(ConvertResponse {
            id: Some(id.to_string()),
            error: Some(error.to_string()),
        })?;

    if response.status() == 200 {
        Ok(response)
    } else {
        Err(format!("Failed to send an error. Status: {}", response.status()).into())
    }
}

fn upload_file<P: AsRef<Path>>(
    id: &str,
    file_path: P,
    url: &str,
) -> Result<Response, Box<dyn std::error::Error>> {
    let path = file_path.as_ref();
    let file_name = path
        .file_name()
        .ok_or("Invalid file path")?
        .to_str()
        .ok_or("Invalid file name")?;

    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let mime_type = from_path(path).first_or_octet_stream();

    let response = ureq::post(url)
        .set("Content-Type", mime_type.as_ref())
        .set(
            "Content-Disposition",
            &format!("attachment; filename=\"{}\"", file_name),
        )
        .set("X-Task-Id", id)
        .send_bytes(&buffer)?;

    if response.status() == 200 {
        Ok(response)
    } else {
        Err(format!("Failed to upload file. Status: {}", response.status()).into())
    }
}

fn params_to_avdictionary(input: &str) -> Dictionary {
    let mut dict: Dictionary = Dictionary::new();
    for pair in input.split(';') {
        let mut parts = pair.split(':');

        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            dict.set(key, value);
        }
    }
    dict
}
