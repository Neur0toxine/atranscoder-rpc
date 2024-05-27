FROM alpine:3.20 AS builder

# Build FFmpeg.
RUN apk add --no-cache \
    rust \
    cargo \
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
    lame-dev \
    opus-dev \
    libvorbis-dev \
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
      --enable-libass \
      --enable-libfdk-aac \
      --enable-libmp3lame \
      --enable-libopus \
      --enable-libvorbis \
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

FROM alpine:3.20 AS fdk-builder
COPY ./.docker/APKBUILD /fdk-aac/APKBUILD
RUN apk add --no-cache sudo abuild build-base cmake samurai && \
    cd /fdk-aac && \
    adduser -G abuild -g "Alpine Package Builder" -s /bin/ash -D builder && \
    echo "builder ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers && \
    echo "root ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers && \
    chown -R builder:abuild /fdk-aac && \
    chmod 777 /tmp && \
    sudo -u builder sh -c 'abuild-keygen -ani && abuild -r' && \
    find /home/builder -name 'fdk-aac*' -exec mv {} /fdk-aac.apk \;

FROM alpine:3.20
WORKDIR /
RUN apk add --no-cache  \
    ffmpeg-libavutil  \
    ffmpeg-libavformat  \
    ffmpeg-libavfilter  \
    ffmpeg-libavdevice  \
    dumb-init  \
    mailcap  \
    tzdata  \
    gnutls
COPY --from=fdk-builder /fdk-aac.apk /tmp/fdk-aac.apk
RUN apk add --allow-untrusted /tmp/fdk-aac.apk && rm /tmp/fdk-aac.apk
COPY --from=builder /build/target/release/atranscoder-rpc /usr/local/bin
EXPOSE 8090
ENTRYPOINT ["/usr/bin/dumb-init", "--", "/usr/local/bin/atranscoder-rpc"]
