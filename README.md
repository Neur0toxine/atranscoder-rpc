# atranscoder-rpc

[![Docker Automated build](https://img.shields.io/docker/automated/neur0toxine/atranscoder-rpc.svg)](https://hub.docker.com/r/neur0toxine/atranscoder-rpc/)
[![docker Status](https://github.com/Neur0toxine/atranscoder-rpc/workflows/docker/badge.svg)](https://github.com/Neur0toxine/atranscoder-rpc/actions?query=workflow%3Adocker)

Audio transcoder with simple HTTP API. Work in progress.

# How to Use

Transcoding can be done like this:
1. Use [`neur0toxine/atranscoder-rpc`](https://hub.docker.com/r/neur0toxine/atranscoder-rpc/) Docker image.
2. Upload file for transcoding:
```bash
curl --location 'http://localhost:8090/enqueue' \
--form 'file=@"/home/user/Music/test.mp3"' \
--form 'format="mp4"' \
--form 'codec="libfdk_aac"' \
--form 'codecOpts="profile=aac_he"' \
--form 'bitRate="160000"' \
--form 'maxBitRate="160000"' \
--form 'sampleRate="44100"' \
--form 'channelLayout="stereo"' \
--form 'callbackUrl="http://127.0.0.1:8909/callback"'
```
3. Your `callbackUrl` will receive JSON response with job ID and error in case of failure. Error will be null if transcoding was successful.
4. You can download transcoded file like this (replace `job_id` with the ID you've received):
```bash
curl -L http://localhost:8090/get/job_id -o file.mp4
```

You can change configuration using this environment variables:
- `LISTEN` - change this environment variable to change TCP listen address. Default is `0.0.0.0:8090`.
- `NUM_WORKERS` - can be used to change how many threads will be used to transcode incoming files. Default is equal to logical CPUs.
- `TEMP_DIR` - this can be used to change which directory should be used to store incoming downloads and transcoding results. Useful if you want to use a Docker volume for this. Default is system temp directory (`/tmp` for Linux).
- `LOG_LEVEL` - changes log verbosity, default is `info`.
- `MAX_BODY_SIZE` - changes max body size for `/enqueue`. Default is 100MB.
- `FFMPEG_VERBOSE` - if set to `1` changes FFmpeg log level from quiet to trace.

# Roadmap
- [x] Implement somewhat acceptable error handling.
- [x] Remove old conversion results and input files that are older than 1 hour.
- [x] Remove input file after transcoding it.
- [x] Do not upload files directly, add download route with streaming instead.
- [ ] If FFmpeg fails, `send_error` won't be called - fix that.
- [ ] Default errors are returned in plain text. Change it to the JSON.
- [ ] Conversion from OGG Opus mono to HE-AAC v1 Stereo outputs high-pitched crackling audio.
- [ ] Conversion from OGG Opus mono to AAC sometimes crashes the app with SIGSEGV (this can be seen more often with very short audio).
- [ ] Docker image for `amd64` and `arm64` (currently only `amd64` is supported because `arm64` cross-compilation with QEMU is sloooooooooooowwwww...).
- [ ] Tests!