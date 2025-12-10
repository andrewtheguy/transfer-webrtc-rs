use serde::{Deserialize, Serialize};

/// Chunk size for file transfer (16KB)
pub const CHUNK_SIZE: usize = 16 * 1024;

/// Message types for the file transfer protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TransferMessage {
    /// Sender -> Receiver: File metadata
    #[serde(rename = "file_info")]
    FileInfo {
        filename: String,
        size: u64,
        chunk_size: u32,
        total_chunks: u64,
    },

    /// Receiver -> Sender: Ready to receive
    #[serde(rename = "ready")]
    Ready,

    /// Sender -> Receiver: File chunk (binary data sent separately)
    #[serde(rename = "chunk")]
    Chunk { index: u64 },

    /// Receiver -> Sender: Acknowledge chunk receipt
    #[serde(rename = "ack")]
    Ack { index: u64 },

    /// Sender -> Receiver: Transfer complete
    #[serde(rename = "done")]
    Done,

    /// Either direction: Error occurred
    #[serde(rename = "error")]
    Error { message: String },
}

impl TransferMessage {
    pub fn file_info(filename: &str, size: u64) -> Self {
        let total_chunks = (size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64;
        Self::FileInfo {
            filename: filename.to_string(),
            size,
            chunk_size: CHUNK_SIZE as u32,
            total_chunks,
        }
    }

    pub fn ready() -> Self {
        Self::Ready
    }

    pub fn chunk(index: u64) -> Self {
        Self::Chunk { index }
    }

    pub fn ack(index: u64) -> Self {
        Self::Ack { index }
    }

    pub fn done() -> Self {
        Self::Done
    }

    pub fn error(message: &str) -> Self {
        Self::Error {
            message: message.to_string(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let json = serde_json::to_string(self).unwrap();
        let mut bytes = vec![0u8]; // Message type marker (0 = JSON control message)
        bytes.extend(json.as_bytes());
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        // Check if this is a control message (starts with 0)
        if data[0] == 0 {
            let json_str = std::str::from_utf8(&data[1..]).ok()?;
            serde_json::from_str(json_str).ok()
        } else {
            None
        }
    }
}

/// Binary chunk data with index
#[derive(Debug)]
pub struct ChunkData {
    pub index: u64,
    pub data: Vec<u8>,
}

impl ChunkData {
    pub fn new(index: u64, data: Vec<u8>) -> Self {
        Self { index, data }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![1u8]; // Message type marker (1 = binary chunk)
        bytes.extend(&self.index.to_be_bytes());
        bytes.extend(&self.data);
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 9 || data[0] != 1 {
            return None;
        }

        let index = u64::from_be_bytes(data[1..9].try_into().ok()?);
        let chunk_data = data[9..].to_vec();

        Some(Self {
            index,
            data: chunk_data,
        })
    }
}

/// Parse incoming data as either a control message or chunk data
pub enum ParsedMessage {
    Control(TransferMessage),
    Chunk(ChunkData),
    EncryptedChunk(crate::transfer::crypto::EncryptedChunk),
}

impl ParsedMessage {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        match data[0] {
            0 => TransferMessage::from_bytes(data).map(ParsedMessage::Control),
            1 => ChunkData::from_bytes(data).map(ParsedMessage::Chunk),
            2 => crate::transfer::crypto::EncryptedChunk::from_bytes(data)
                .map(ParsedMessage::EncryptedChunk),
            _ => None,
        }
    }
}
