use crate::transcoder::{Transcoder, TranscoderParams};
use ffmpeg_next::channel_layout::ChannelLayout;
use ffmpeg_next::format::context;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::{format, Dictionary, Error};
use tracing::{debug, error};

pub struct Task {
    id: uuid::Uuid,
    codec: String,
    codec_opts: Option<String>,
    bit_rate: usize,
    max_bit_rate: usize,
    sample_rate: i32,
    channel_layout: String,
    input_path: String,
    output_path: String,
    upload_url: String,
}

impl Task {
    pub fn new(
        id: uuid::Uuid,
        codec: String,
        codec_opts: Option<String>,
        bit_rate: usize,
        max_bit_rate: usize,
        sample_rate: i32,
        channel_layout: String,
        upload_url: String,
        input_path: String,
        output_path: String,
    ) -> Self {
        Task {
            id,
            codec,
            codec_opts,
            bit_rate,
            max_bit_rate,
            sample_rate,
            channel_layout,
            input_path,
            output_path,
            upload_url,
        }
    }

    pub fn execute(self) {
        debug!(
            "performing transcoding for task with id: {}",
            self.id.to_string()
        );
        let mut ictx = match format::input(&self.input_path) {
            Ok(val) => val,
            Err(err) => {
                error!("couldn't initialize input context: {:?}", err);
                return;
            }
        };

        let octx: Result<context::Output, ffmpeg_next::Error>;
        if self.codec_opts.is_some() {
            octx = format::output_as_with(
                &self.output_path,
                &self.codec,
                params_to_avdictionary(&self.codec_opts.unwrap_or_default()),
            );
        } else {
            octx = format::output_as(&self.output_path, &self.codec);
        }

        let mut octx = match octx {
            Ok(val) => val,
            Err(err) => {
                error!("couldn't initialize output context: {:?}", err);
                return;
            }
        };

        let transcoder = Transcoder::new(
            &mut ictx,
            &mut octx,
            TranscoderParams {
                codec: self.codec,
                bit_rate: self.bit_rate,
                max_bit_rate: self.max_bit_rate,
                sample_rate: self.sample_rate,
                channel_layout: match self.channel_layout.as_str() {
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
                return;
            }
        };
        octx.set_metadata(ictx.metadata().to_owned());
        octx.write_header()
            .unwrap_or_else(|err| error!("couldn't start transcoding: {:?}", err));

        for (stream, mut packet) in ictx.packets() {
            if stream.index() == transcoder.stream {
                packet.rescale_ts(stream.time_base(), transcoder.in_time_base);
                transcoder.send_packet_to_decoder(&packet);
                transcoder.receive_and_process_decoded_frames(&mut octx);
            }
        }

        transcoder.send_eof_to_decoder();
        transcoder.receive_and_process_decoded_frames(&mut octx);

        transcoder.flush_filter();
        transcoder.get_and_process_filtered_frames(&mut octx);

        transcoder.send_eof_to_encoder();
        transcoder.receive_and_process_encoded_packets(&mut octx);

        octx.write_trailer()
            .unwrap_or_else(|err| error!("couldn't finish transcoding: {:?}", err));
    }
}

fn params_to_avdictionary(input: &str) -> Dictionary {
    let mut dict: Dictionary = Dictionary::new();
    for pair in input.split(";") {
        let mut parts = pair.split(":");

        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            dict.set(key, value);
        }
    }
    dict
}
