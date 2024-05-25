use tracing::{debug, error};
use ffmpeg_next::{Error, format};
use ffmpeg_next::channel_layout::ChannelLayout;
use crate::transcoder::{Transcoder, TranscoderParams};

pub struct Task {
    id: uuid::Uuid,
    codec: String,
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
        debug!("performing transcoding for task with id: {}", self.id.to_string());
        let mut ictx = format::input(&self.input_path).unwrap();
        let mut octx = format::output_as(&self.output_path, &self.codec).unwrap();
        let transcoder = Transcoder::new(&mut ictx, &mut octx, TranscoderParams {
            codec: self.codec,
            bit_rate: self.bit_rate,
            max_bit_rate: self.max_bit_rate,
            sample_rate: self.sample_rate,
            channel_layout: match self.channel_layout.as_str() {
                "stereo" => ChannelLayout::STEREO,
                "mono" => ChannelLayout::MONO,
                "stereo_downmix" => ChannelLayout::STEREO_DOWNMIX,
                _ => ChannelLayout::STEREO,
            }
        });
        let mut transcoder = match transcoder {
            Ok(val) => val,
            Err(err) => {
                error!("couldn't initialize FFmpeg transcoder: {:?}", err);
                return
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
