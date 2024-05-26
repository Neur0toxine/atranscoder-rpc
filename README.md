# atranscoder-rpc

[![Docker Automated build](https://img.shields.io/docker/automated/neur0toxine/atranscoder-rpc.svg)](https://hub.docker.com/r/neur0toxine/atranscoder-rpc/)
[![docker Status](https://github.com/Neur0toxine/atranscoder-rpc/workflows/docker/badge.svg)](https://github.com/Neur0toxine/atranscoder-rpc/actions?query=workflow%3Adocker)

Audio transcoder with simple HTTP API. Work in progress.

# How to Use

Transcoding can be done like this:
1. Use `cargo run` or [`neur0toxine/atranscoder-rpc`](https://hub.docker.com/r/neur0toxine/atranscoder-rpc/) Docker image.
2. Upload file for transcoding:
```bash
curl --location 'http://localhost:8090/enqueue' \
    --form 'file=@"/home/user/Music/test.mp3"' \
    --form 'format="adts"' \
    --form 'codec="aac"' \
    --form 'bitRate="64000"' \
    --form 'maxBitRate="64000"' \
    --form 'sampleRate="8000"' \
    --form 'channelLayout="mono"' \
    --form 'uploadUrl="http://127.0.0.1:8909/upload"'
```
3. Your `uploadUrl` will receive JSON response with job ID and error in case of failure and the entire transcoded file contents in case of success. Use `Content-Type` header to differentiate between the two data types.

You can change configuration using this environment variables:
- `LISTEN` - change this environment variable to change TCP listen address. Default is `0.0.0.0:8090`.
- `NUM_WORKERS` - can be used to change how many threads will be used to transcode incoming files. Default is equal to logical CPUs.
- `TEMP_DIR` - this can be used to change which directory should be used to store incoming downloads and transcoding results. Useful if you want to use a Docker volume for this. Default is system temp directory (`/tmp` for Linux).
- `LOG_LEVEL` - changes log verbosity, default is `info`.

# Roadmap
- [x] Implement somewhat acceptable error handling.
- [x] Remove old conversion results and input files that are older than 1 hour.
- [x] Remove input file after transcoding it.
- [x] Implement file upload to `uploadUrl` (if `Content-Type: application/json` then conversion was not successful and body contains an error info).
- [x] Remove transcoding result after uploading it to the `uploadUrl`.
- [x] Docker image for `amd64` and `aarch64`.
- [ ] ~~Restart threads in case of panic.~~ It's better to not panic. Current error handling seems ok for now.
- [ ] ~~Statically linked binary for Docker image & result docker image based on `scratch` (reduce image size).~~ Not yet, see [Dockerfile.scratch](Dockerfile.scratch).
- [ ] Tests!