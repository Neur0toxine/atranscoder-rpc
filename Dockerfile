FROM alpine:3.20 AS builder

# Build FFmpeg.
RUN apk add --no-cache \
    rust \
    cargo \
    fdk-aac \
    fdk-aac-dev \
    clang16 \
    clang16-static \
    clang16-libclang \
    llvm16-dev \
    libc-dev \
    pkgconf \
    git \
    build-base \
    nasm \
    yasm \
    aom-dev \
    dav1d-dev \
    lame-dev \
    opus-dev \
    svt-av1-dev \
    libvorbis-dev \
    libvpx-dev \
    x264-dev \
    x265-dev \
    numactl-dev \
    libass-dev \
    libunistring-dev \
    gnutls-dev && \
    mkdir -p /ffmpeg/{ffmpeg_build,bin} && \
    cd /ffmpeg && \
    wget -O ffmpeg-7.0.1.tar.bz2 https://ffmpeg.org/releases/ffmpeg-7.0.1.tar.bz2 && \
    tar xjvf ffmpeg-7.0.1.tar.bz2 && \
    cd ffmpeg-7.0.1 && \
    PKG_CONFIG_PATH="/ffmpeg/ffmpeg_build/lib/pkgconfig" ./configure \
      --prefix="/ffmpeg/ffmpeg_build" \
      --pkg-config-flags="--static" \
      --extra-cflags="-I/ffmpeg/ffmpeg_build/include" \
      --extra-ldflags="-L/ffmpeg/ffmpeg_build/lib" \
      --extra-libs="-lpthread -lm" \
      --ld="g++" \
      --bindir="/ffmpeg/bin" \
      --enable-gpl \
      --enable-gnutls \
      --enable-libaom \
      --enable-libass \
      --enable-libfdk-aac \
      --enable-libfreetype \
      --enable-libmp3lame \
      --enable-libopus \
      --enable-libsvtav1 \
      --enable-libdav1d \
      --enable-libvorbis \
      --enable-libvpx \
      --enable-libx264 \
      --enable-libx265 \
      --enable-nonfree && \
    PATH="/ffmpeg/bin:$PATH" make -j$(nproc) && \
    make install && \
    hash -r

# Build the app.
WORKDIR /build
COPY ./ ./
RUN cd /build && \
    export PKG_CONFIG_PATH="/ffmpeg/ffmpeg_build/lib/pkgconfig" && \
    cargo build --verbose --release

FROM alpine:3.20
WORKDIR /
RUN apk add --no-cache  \
    ffmpeg-libavutil  \
    ffmpeg-libavformat  \
    ffmpeg-libavfilter  \
    ffmpeg-libavdevice  \
    fdk-aac  \
    dumb-init  \
    mailcap  \
    tzdata  \
    gnutls
COPY --from=builder /build/target/release/atranscoder-rpc /usr/local/bin
EXPOSE 8090
ENTRYPOINT ["/usr/bin/dumb-init", "--", "/usr/local/bin/atranscoder-rpc"]
