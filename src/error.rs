use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("WebRTC error: {0}")]
    WebRtc(#[from] webrtc::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Signaling error: {0}")]
    Signaling(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Transfer error: {0}")]
    Transfer(String),

    #[error("Peer ID already taken")]
    PeerIdTaken,

    #[error("Invalid peer ID format")]
    InvalidPeerId,

    #[error("Connection timeout")]
    Timeout,

    #[error("Peer disconnected")]
    PeerDisconnected,

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Channel closed")]
    ChannelClosed,
}

pub type Result<T> = std::result::Result<T, AppError>;
