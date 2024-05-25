extern crate ffmpeg_next as ffmpeg;

use std::error::Error;
use std::sync::Arc;

use ffmpeg::{codec, filter, format, frame, media};
use ffmpeg_next::codec::Audio;
use ffmpeg_next::Codec;

pub struct Transcoder {
    params: Arc<TranscoderParams>,
    pub(crate) stream: usize,
    filter: filter::Graph,
    decoder: codec::decoder::Audio,
    encoder: codec::encoder::Audio,
    pub(crate) in_time_base: ffmpeg::Rational,
    out_time_base: ffmpeg::Rational,
}

pub struct TranscoderParams {
    pub codec: String,
    pub bit_rate: usize,
    pub max_bit_rate: usize,
    pub sample_rate: i32,
    pub channel_layout: ffmpeg::channel_layout::ChannelLayout,
}

impl Transcoder {
    pub fn new(
        ictx: &mut format::context::Input,
        octx: &mut format::context::Output,
        params: TranscoderParams,
    ) -> Result<Transcoder, Box<dyn Error>> {
        let input = ictx
            .streams()
            .best(media::Type::Audio)
            .expect("could not find best audio stream");
        let context = codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = match context.decoder().audio() {
            Ok(val) => val,
            Err(err) => {
                return Err(
                    format!("couldn't find decoder for input file: {}", err.to_string()).into(),
                )
            }
        };
        let codec = match ffmpeg::encoder::find_by_name(&*params.codec) {
            None => return Err(format!("couldn't find codec with name: {}", params.codec).into()),
            Some(val) => match val.audio() {
                Ok(val) => val,
                Err(err) => return Err(err.into()),
            },
        };
        let global = octx
            .format()
            .flags()
            .contains(format::flag::Flags::GLOBAL_HEADER);

        decoder.set_parameters(input.parameters())?;

        let mut output = octx.add_stream(codec)?;
        let context = codec::context::Context::from_parameters(output.parameters())?;
        let mut encoder = context.encoder().audio()?;

        if global {
            encoder.set_flags(codec::flag::Flags::GLOBAL_HEADER);
        }

        let sample_rate = if params.sample_rate > 0 {
            params.sample_rate
        } else {
            decoder.rate() as i32
        };

        encoder.set_rate(sample_rate);
        encoder.set_channel_layout(params.channel_layout);
        #[cfg(not(feature = "ffmpeg_7_0"))]
        {
            encoder.set_channels(params.channel_layout.channels());
        }
        encoder.set_format(
            codec
                .formats()
                .expect(
                    format!(
                        "failed to get supported formats for codec: {}",
                        codec.name()
                    )
                    .as_str(),
                )
                .next()
                .unwrap(),
        );

        if params.bit_rate > 0 {
            encoder.set_bit_rate(params.bit_rate);
        } else {
            encoder.set_bit_rate(decoder.bit_rate());
        }

        if params.max_bit_rate > 0 {
            encoder.set_max_bit_rate(params.bit_rate);
        } else {
            encoder.set_max_bit_rate(decoder.max_bit_rate());
        }

        encoder.set_time_base((1, sample_rate));
        output.set_time_base((1, sample_rate));

        let encoder = encoder.open_as(codec)?;
        output.set_parameters(&encoder);

        let filter = filter("anull", &decoder, &encoder)?;

        let in_time_base = decoder.time_base();
        let out_time_base = output.time_base();

        Ok(Transcoder {
            stream: input.index(),
            params: Arc::new(params),
            filter,
            decoder,
            encoder,
            in_time_base,
            out_time_base,
        })
    }

    fn send_frame_to_encoder(&mut self, frame: &ffmpeg::Frame) -> Result<(), ffmpeg::Error> {
        self.encoder.send_frame(frame)
    }

    pub(crate) fn send_eof_to_encoder(&mut self) {
        self.encoder.send_eof().unwrap();
    }

    pub(crate) fn receive_and_process_encoded_packets(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut encoded = ffmpeg::Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(0);
            encoded.rescale_ts(self.in_time_base, self.out_time_base);

            match encoded.write_interleaved(octx) {
                Err(err) => return Err(err.into()),
                Ok(_) => (),
            }
        }
        Ok(())
    }

    fn add_frame_to_filter(&mut self, frame: &ffmpeg::Frame) {
        self.filter.get("in").unwrap().source().add(frame).unwrap();
    }

    pub(crate) fn flush_filter(&mut self) {
        self.filter.get("in").unwrap().source().flush().unwrap();
    }

    pub(crate) fn get_and_process_filtered_frames(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut filtered = frame::Audio::empty();
        loop {
            let mut ctx: ffmpeg::filter::Context = match self.filter.get("out") {
                None => return Err(Box::from("cannot get context from filter")),
                Some(val) => val,
            };

            if !ctx.sink().frame(&mut filtered).is_ok() {
                return Err(Box::from("frame is suddenly invalid, stopping..."));
            }

            match self.send_frame_to_encoder(&filtered) {
                Err(err) => return Err(err.into()),
                Ok(_) => (),
            };
            match self.receive_and_process_encoded_packets(octx) {
                Err(err) => return Err(err.into()),
                Ok(_) => (),
            }
        }
    }

    pub(crate) fn send_packet_to_decoder(
        &mut self,
        packet: &ffmpeg::Packet,
    ) -> Result<(), Box<dyn Error>> {
        match self.decoder.send_packet(packet) {
            Err(err) => return Err(err.into()),
            Ok(_) => Ok(()),
        }
    }

    pub(crate) fn send_eof_to_decoder(&mut self) -> Result<(), Box<dyn Error>> {
        match self.decoder.send_eof() {
            Err(err) => return Err(err.into()),
            Ok(_) => Ok(()),
        }
    }

    pub(crate) fn receive_and_process_decoded_frames(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut decoded = frame::Audio::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let timestamp = decoded.timestamp();
            decoded.set_pts(timestamp);
            self.add_frame_to_filter(&decoded);

            match self.get_and_process_filtered_frames(octx) {
                Err(err) => return Err(err.into()),
                Ok(_) => (),
            }
        }
        Ok(())
    }
}

fn filter(
    spec: &str,
    decoder: &codec::decoder::Audio,
    encoder: &codec::encoder::Audio,
) -> Result<filter::Graph, ffmpeg::Error> {
    let mut filter = filter::Graph::new();

    let args = format!(
        "time_base={}:sample_rate={}:sample_fmt={}:channel_layout=0x{:x}",
        decoder.time_base(),
        decoder.rate(),
        decoder.format().name(),
        decoder.channel_layout().bits()
    );

    filter.add(&filter::find("abuffer").unwrap(), "in", &args)?;
    filter.add(&filter::find("abuffersink").unwrap(), "out", "")?;

    {
        let mut out = filter.get("out").unwrap();

        out.set_sample_format(encoder.format());
        out.set_channel_layout(encoder.channel_layout());
        out.set_sample_rate(encoder.rate());
    }

    filter.output("in", 0)?.input("out", 0)?.parse(spec)?;
    filter.validate()?;

    println!("{}", filter.dump());

    if let Some(codec) = encoder.codec() {
        if !codec
            .capabilities()
            .contains(codec::capabilities::Capabilities::VARIABLE_FRAME_SIZE)
        {
            filter
                .get("out")
                .unwrap()
                .sink()
                .set_frame_size(encoder.frame_size());
        }
    }

    Ok(filter)
}
