extern crate ffmpeg_next as ffmpeg;

use std::error::Error;

use ffmpeg::{codec, filter, format, frame, media};
use ffmpeg_next::error::EAGAIN;

use crate::task::params_to_avdictionary;

pub struct Transcoder {
    pub(crate) stream: usize,
    filter: filter::Graph,
    decoder: codec::decoder::Audio,
    encoder: codec::encoder::Audio,
    pub(crate) in_time_base: ffmpeg::Rational,
    out_time_base: ffmpeg::Rational,
}

pub struct TranscoderParams {
    pub codec: String,
    pub codec_opts: Option<String>,
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
            .ok_or("could not find best audio stream")?;

        let context = codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = context
            .decoder()
            .audio()
            .map_err(|err| format!("couldn't find decoder for input file: {}", err))?;

        let codec = ffmpeg::encoder::find_by_name(&params.codec)
            .ok_or_else(|| format!("couldn't find codec with name: {}", params.codec))?
            .audio()?;

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

        encoder.set_format(
            codec
                .formats()
                .ok_or_else(|| {
                    format!(
                        "failed to get supported formats for codec: {}",
                        codec.name()
                    )
                })?
                .next()
                .ok_or("no supported formats found for codec")?,
        );

        encoder.set_bit_rate(if params.bit_rate > 0 {
            params.bit_rate
        } else {
            decoder.bit_rate()
        });
        encoder.set_max_bit_rate(if params.max_bit_rate > 0 {
            params.max_bit_rate
        } else {
            decoder.max_bit_rate()
        });
        encoder.set_time_base((1, sample_rate));
        output.set_time_base((1, sample_rate));

        let in_time_base = decoder.time_base();
        let encoder = if let Some(codec_opts) = params.codec_opts {
            encoder.open_as_with(codec, params_to_avdictionary(codec_opts.as_str()))?
        } else {
            encoder.open_as(codec)?
        };
        output.set_parameters(&encoder);

        let filter = filter_graph("anull", &decoder, &encoder)?;

        Ok(Transcoder {
            stream: input.index(),
            filter,
            decoder,
            encoder,
            in_time_base,
            out_time_base: output.time_base(),
        })
    }

    fn send_frame_to_encoder(&mut self, frame: &ffmpeg::Frame) -> Result<(), ffmpeg::Error> {
        self.encoder.send_frame(frame)
    }

    pub(crate) fn send_eof_to_encoder(&mut self) -> Result<(), ffmpeg::Error> {
        self.encoder.send_eof()
    }

    pub(crate) fn receive_and_process_encoded_packets(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut encoded = ffmpeg::Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(0);
            encoded.rescale_ts(self.in_time_base, self.out_time_base);

            if let Err(err) = encoded.write_interleaved(octx) {
                return Err(err.into());
            }
        }
        Ok(())
    }

    fn add_frame_to_filter(&mut self, frame: &ffmpeg::Frame) -> Result<(), ffmpeg::Error> {
        if let Some(mut ctx) = self.filter.get("in") {
            let mut source = ctx.source();
            source.add(frame)
        } else {
            Err(ffmpeg::Error::Other { errno: 0 })
        }
    }

    pub(crate) fn flush_filter(&mut self) -> Result<(), ffmpeg::Error> {
        if let Some(mut ctx) = self.filter.get("in") {
            let mut source = ctx.source();
            source.flush()
        } else {
            Err(ffmpeg::Error::Other { errno: 0 })
        }
    }

    pub(crate) fn get_and_process_filtered_frames(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut filtered = frame::Audio::empty();
        loop {
            let mut ctx = self
                .filter
                .get("out")
                .ok_or("cannot get context from filter")?;

            if let Err(err) = ctx.sink().frame(&mut filtered) {
                if err != ffmpeg::Error::Eof {
                    return Err(err.into());
                }
                return Ok(());
            }

            self.send_frame_to_encoder(&filtered)?;
            self.receive_and_process_encoded_packets(octx)?;
        }
    }

    pub(crate) fn send_packet_to_decoder(
        &mut self,
        packet: &ffmpeg::Packet,
    ) -> Result<(), Box<dyn Error>> {
        self.decoder.send_packet(packet).map_err(|err| err.into())
    }

    pub(crate) fn send_eof_to_decoder(&mut self) -> Result<(), Box<dyn Error>> {
        self.decoder.send_eof().map_err(|err| err.into())
    }

    pub(crate) fn receive_and_process_decoded_frames(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), Box<dyn Error>> {
        let mut decoded = frame::Audio::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let timestamp = decoded.timestamp();
            decoded.set_pts(timestamp);
            self.add_frame_to_filter(&decoded)?;

            if let Err(mut err) = self.get_and_process_filtered_frames(octx) {
                let expected = ffmpeg::Error::Other { errno: EAGAIN };
                if err
                    .downcast_mut::<ffmpeg::error::Error>()
                    .ok_or(ffmpeg::Error::Bug)
                    == Err(expected)
                {
                    continue;
                }
            }
        }
        Ok(())
    }
}

fn filter_graph(
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

    let abuffer_filter = match filter::find("abuffer") {
        Some(filter) => filter,
        None => return Err(ffmpeg::Error::Unknown),
    };
    filter.add(&abuffer_filter, "in", &args)?;

    let abuffersink_filter = match filter::find("abuffersink") {
        Some(filter) => filter,
        None => return Err(ffmpeg::Error::Unknown),
    };
    filter.add(&abuffersink_filter, "out", "")?;

    let mut out = match filter.get("out") {
        Some(filter) => filter,
        None => return Err(ffmpeg::Error::Unknown),
    };
    out.set_sample_format(encoder.format());
    out.set_channel_layout(encoder.channel_layout());
    out.set_sample_rate(encoder.rate());

    filter.output("in", 0)?.input("out", 0)?.parse(spec)?;
    filter.validate()?;

    if let Some(codec) = encoder.codec() {
        if !codec
            .capabilities()
            .contains(codec::capabilities::Capabilities::VARIABLE_FRAME_SIZE)
        {
            if let Some(mut out_filter) = filter.get("out") {
                out_filter.sink().set_frame_size(encoder.frame_size());
            }
        }
    }

    Ok(filter)
}
