# transfer-webrtc-rs

A peer-to-peer file transfer CLI tool using WebRTC data channels. Transfer files directly between computers without uploading to a server.

## Features

- **Peer-to-peer**: Files transfer directly between sender and receiver
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
# Start sender and get a peer ID
transfer-webrtc-rs send myfile.zip

# Output:
# Your peer ID: brave-mountain-river
# Share this ID with the receiver. Waiting for connection...
```

### Receiving a file

```bash
# Connect using the peer ID from the sender
transfer-webrtc-rs receive brave-mountain-river

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
transfer-webrtc-rs receive <PEER_ID> [OPTIONS]

Options:
  -s, --server <SERVER>  PeerJS server URL [default: wss://0.peerjs.com/peerjs]
  -v, --verbose          Enable verbose logging
  -h, --help             Print help

Send options:
  -p, --peer-id <ID>     Use a custom peer ID instead of generating one

Receive options:
  -o, --output <DIR>     Output directory for received files [default: current directory]
```

## How it works

1. **Signaling**: Both peers connect to a PeerJS signaling server via WebSocket
2. **Connection**: The receiver initiates a WebRTC connection by sending an offer
3. **ICE Exchange**: Both peers exchange ICE candidates for NAT traversal
4. **Data Channel**: Once connected, a WebRTC data channel is established
5. **Transfer**: The file is sent in 16KB chunks with acknowledgments

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

## Dependencies

- [webrtc-rs](https://github.com/webrtc-rs/webrtc) - WebRTC implementation
- [tokio](https://tokio.rs/) - Async runtime
- [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite) - WebSocket client
- [clap](https://clap.rs/) - CLI argument parsing
- [indicatif](https://github.com/console-rs/indicatif) - Progress bars

## License

MIT
