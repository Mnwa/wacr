# WACR
WACR is a backend for WebRTC -> [VK ASR](https://vk.com/voice-tech) (speech recognition technology) interaction.

## How it works
The client just calls to WACR backend by WebRTC technology. 
After that WACR save an audio stream from WebRTC to file system.
Saved audio stream will send to VK ACR backend by [API](https://dev.vk.com/api/voice-tech). 
Then WACR save in-memory recognized text and send it to client.

## Usage
### Compile and run from Cargo
```bash
RUST_LOG=debug;VK_API_SERVICE_TOKEN=XXX;VK_API_SERVICE_KEY=YYY cargo run --package wacr --bin wacr
```
### Compile from Cargo and run the binary
```bash
cargo build --package wacr --bin wacr --release
RUST_LOG=debug;VK_API_SERVICE_TOKEN=XXX;VK_API_SERVICE_KEY=YYY ./target/release/wacr
```

## API
### Get JWT Token. 
Query must be extracted from [mini apps launch params](https://dev.vk.com/mini-apps/development/launch-params-sign)
#### Request
```http request
POST http://127.0.0.1:8080/token/generate
Content-Type: application/json

{
  "query": "XXX"
}
```

#### Response
```json
{
  "token": "xxx",
  "expiration": 1664718489
}
```
### Create session
Creating connection by WebRTC. access_token must be got from Get JWT Token API.
Offer is client [local WebRTC offer](https://developer.mozilla.org/en-US/docs/Web/API/RTCPeerConnection/createOffer).
#### Request
```http request
POST http://127.0.0.1:8080/session/create?access_token=XXX
Content-Type: application/json

{
  "offer": {}
}
```

#### Response
```json
{
  "session_id": "a3b26e68-7fda-4534-bbdd-92a98230a824",
  "offer": {}
}
```

### Recognise the speech
Start recognising of speech accepted from Create session. access_token must be got from Get JWT Token API.
#### Request
```http request
POST http://127.0.0.1:8080/session/asr?access_token=XXX
Content-Type: application/json

{
  "session_id": "a3b26e68-7fda-4534-bbdd-92a98230a824"
}
```

#### Response
```json
{
  "text": "Hello world!"
}
```

### Listen recorded audio
```http request
GET http://127.0.0.1:8080/session/listen/{session_id}?access_token=XXX
```

## Startup environments
### Required
```bash
VK_API_SERVICE_TOKEN=XXX # Service token for requesting VK API endpoints
VK_API_SERVICE_KEY=YYY # Service key for validating query on token generation
```

### Optional
```bash
LISTEN_ADDRESS=127.0.0.1:8080 # Listening address
JWT_EXPIRATION=3600 # Time how much access token will valid
AUDIO_PATH=/tmp # The directory where audio files saving
```