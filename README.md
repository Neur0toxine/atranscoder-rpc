# atranscoder-rpc

Audio transcoder with simple HTTP API. Work in progress.

# What works

Transcoding:
1. `cargo run`
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
3. You will receive JSON response with job ID. The transcoding result will be saved into `/tmp/{job_id}.out.atranscoder`

# Roadmap
- [ ] ~~Restart threads in case of panic.~~ It's better to not panic. Current error handling seems ok for now.
- [x] Implement somewhat acceptable error handling.
- [x] Remove old conversion results and input files that are older than 1 hour.
- [x] Remove input file after transcoding it.
- [x] Implement file upload to `uploadUrl` (if `Content-Type: application/json` then conversion was not successful and body contains an error info).
- [x] Remove transcoding result after uploading it to the `uploadUrl`.
- [x] Docker image for `amd64` and `aarch64`.
- [ ] ~~Statically linked binary for Docker image & result docker image based on `scratch` (reduce image size).~~ Not yet, see [Dockerfile.scratch](Dockerfile.scratch).
- [ ] Tests!