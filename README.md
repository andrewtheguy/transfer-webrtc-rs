# transfer-webrtc-rs

A peer-to-peer file transfer CLI tool using WebRTC data channels. Transfer files directly between computers without uploading to a server.

## Features

- **Peer-to-peer**: Files transfer directly between sender and receiver
- **End-to-end encrypted**: AES-256-GCM encryption with offline key sharing
- **No server hosting required**: Uses public PeerJS signaling servers
- **Human-friendly peer IDs**: Easy-to-share IDs like `brave-mountain-river`
- **Progress display**: Real-time transfer progress with speed indication
- **Cross-platform**: Works on Linux, macOS, and Windows

## Installation

```bash
# Clone the repository
git clone https://github.com/peerlink-xyz/transfer-webrtc-rs.git
cd transfer-webrtc-rs

# Build release binary
cargo build --release

# Binary will be at ./target/release/transfer-webrtc-rs
```

## Usage

### Sending a file

```bash
# Start sender and get a peer ID + encryption key
transfer-webrtc-rs send myfile.zip

# Output:
# Your peer ID: brave-mountain-river
# Encryption key: Abc123...XYZ=
#
# Share BOTH with the receiver. Waiting for connection...
```

### Receiving a file

```bash
# Connect using the peer ID and encryption key from the sender
transfer-webrtc-rs receive brave-mountain-river --key "Abc123...XYZ="

# Output:
# Connecting to peer brave-mountain-river...
# Connected!
# Receiving: myfile.zip (152.3 MB)
# [████████████████████████████████] 100% | 12.5 MB/s
# File saved to: ./myfile.zip
```

### Options

```
transfer-webrtc-rs send <FILE> [OPTIONS]
transfer-webrtc-rs receive <PEER_ID> --key <KEY> [OPTIONS]

Options:
  -s, --server <SERVER>  PeerJS server URL [default: 0.peerjs.com]
  -v, --verbose          Enable verbose logging
  -h, --help             Print help

Send options:
  -p, --peer-id <ID>     Use a custom peer ID instead of generating one

Receive options:
  -k, --key <KEY>        Encryption key (base64, required)
  -o, --output <DIR>     Output directory for received files [default: current directory]
```

## How it works

1. **Signaling**: Both peers connect to a PeerJS signaling server via WebSocket
2. **Key Exchange**: Sender generates a random AES-256 key, shares it offline with receiver
3. **Connection**: The receiver initiates a WebRTC connection by sending an offer
4. **ICE Exchange**: Both peers exchange ICE candidates for NAT traversal
5. **Data Channel**: Once connected, a WebRTC data channel is established
6. **Transfer**: File chunks are encrypted with AES-256-GCM before sending (16KB chunks)

```
┌─────────────┐     WebSocket      ┌─────────────────┐
│   Sender    │◄──────────────────►│  PeerJS Server  │
│             │    Signaling       │ (0.peerjs.com)  │
└─────────────┘                    └─────────────────┘
       │                                   ▲
       │  WebRTC Data Channel              │
       │  (P2P file transfer)              │
       ▼                                   │
┌─────────────┐     WebSocket      ────────┘
│  Receiver   │◄──────────────────►
└─────────────┘
```

## Requirements

- Rust 1.70 or later
- Internet connection (for signaling and STUN)
- Both peers must be able to establish a P2P connection (works through most NATs)

## Security

- **Scope**: The shared AES-256-GCM key encrypts everything sent over the data channel: filenames, sizes, and every file chunk. Signaling via PeerJS (peer IDs, ICE) is not end-to-end encrypted but carries no file contents.
- **Key sharing**: Sender generates a 32-byte key and shows it as base64; you must share it out-of-band. It is never transmitted by the app.
- **Integrity + nonces**: Every encrypted payload is authenticated. Chunks use nonces derived from the chunk index plus a random salt to avoid reuse; metadata uses a random nonce.
- **What is not protected**: Signaling traffic and traffic analysis (timing/total bytes) are not hidden. There is no forward secrecy—use a fresh key per transfer.

## Protocol payloads

- **Control messages** (`0` prefix byte, JSON):
  - `file_info_enc`: `nonce` (12 bytes) + `ciphertext` (AES-256-GCM of `{"filename","size","chunk_size","total_chunks"}`).
  - `ready`, `ack { index }`, `done`, `error { message }`.
- **Encrypted file chunks** (`2` prefix byte, binary):
  - Layout: `[2][8-byte index][12-byte nonce][ciphertext+tag]` where nonce = `chunk_index || 4-byte salt`.
  - Payload plaintext is up to 16KB (see `CHUNK_SIZE`), encrypted with the shared key.
- Filenames and sizes never travel in plaintext; receivers reject unencrypted metadata.

## Dependencies

- [webrtc-rs](https://github.com/webrtc-rs/webrtc) - WebRTC implementation
- [aes-gcm](https://github.com/RustCrypto/AEADs) - AES-256-GCM encryption
- [tokio](https://tokio.rs/) - Async runtime
- [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite) - WebSocket client
- [clap](https://clap.rs/) - CLI argument parsing
- [indicatif](https://github.com/console-rs/indicatif) - Progress bars

## License

MIT
