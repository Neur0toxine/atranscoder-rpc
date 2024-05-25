FROM alpine:latest
RUN apk add --no-cache ffmpeg-libavutil ffmpeg-libavformat ffmpeg-libavfilter ffmpeg-libavdevice clang16