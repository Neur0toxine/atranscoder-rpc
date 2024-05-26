use std::error::Error;
use crate::transcoder::{Transcoder, TranscoderParams};
use ffmpeg_next::channel_layout::ChannelLayout;
use ffmpeg_next::{format, Dictionary};
use tracing::{debug, error};

pub struct Task {
    id: uuid::Uuid,
    params: TaskParams,
}

impl Task {
    pub fn new(id: uuid::Uuid, params: TaskParams) -> Self {
        Task { id, params }
    }

    pub fn execute(self) -> Result<(), Box<dyn Error>> {
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

        let octx = if let Some(codec_opts) = self.params.codec_opts {
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
