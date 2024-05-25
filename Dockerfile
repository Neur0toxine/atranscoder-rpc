FROM alpine:latest AS builder
WORKDIR /build
COPY ./ ./
ARG PKG_CONFIG_PATH=/usr/lib/pkgconfig
RUN apk add --no-cache ffmpeg-libs ffmpeg-dev clang16 clang16-libclang pkgconf rust cargo
RUN cd /build && cargo build --release

FROM alpine:latest
WORKDIR /
RUN apk add --no-cache ffmpeg-libavutil ffmpeg-libavformat ffmpeg-libavfilter ffmpeg-libavdevice dumb-init mailcap tzdata
COPY --from=builder /build/target/release/atranscoder-rpc /usr/local/bin
EXPOSE 8090
ENTRYPOINT ["/usr/bin/dumb-init", "--", "/usr/local/bin/atranscoder-rpc"]
