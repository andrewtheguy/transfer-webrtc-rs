use crate::error::{AppError, Result};
use crate::transfer::protocol::{ChunkData, ParsedMessage, TransferMessage, CHUNK_SIZE};
use bytes::Bytes;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tracing::{debug, info};
use webrtc::data_channel::RTCDataChannel;

pub struct FileSender {
    file_path: std::path::PathBuf,
    data_channel: Arc<RTCDataChannel>,
    message_rx: mpsc::Receiver<Vec<u8>>,
}

impl FileSender {
    pub fn new(
        file_path: impl AsRef<Path>,
        data_channel: Arc<RTCDataChannel>,
        message_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            file_path: file_path.as_ref().to_path_buf(),
            data_channel,
            message_rx,
        }
    }

    pub async fn send(&mut self) -> Result<()> {
        // Open file and get metadata
        let mut file = File::open(&self.file_path).await.map_err(|e| {
            AppError::FileNotFound(format!("{}: {}", self.file_path.display(), e))
        })?;

        let metadata = file.metadata().await?;
        let file_size = metadata.len();
        let filename = self
            .file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let total_chunks = (file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64;

        info!(
            "Sending file: {} ({} bytes, {} chunks)",
            filename, file_size, total_chunks
        );

        // Send file info
        let file_info = TransferMessage::file_info(&filename, file_size);
        self.send_message(&file_info).await?;

        // Wait for ready message
        info!("Waiting for receiver to be ready...");
        loop {
            let data = self
                .message_rx
                .recv()
                .await
                .ok_or(AppError::ChannelClosed)?;

            if let Some(ParsedMessage::Control(TransferMessage::Ready)) =
                ParsedMessage::from_bytes(&data)
            {
                info!("Receiver is ready");
                break;
            }
        }

        // Set up progress bar
        let progress = ProgressBar::new(file_size);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );

        // Send file chunks
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut chunk_index = 0u64;
        let mut bytes_sent = 0u64;

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            // Send chunk header
            let chunk_msg = TransferMessage::chunk(chunk_index);
            self.send_message(&chunk_msg).await?;

            // Send chunk data
            let chunk_data = ChunkData::new(chunk_index, buffer[..bytes_read].to_vec());
            self.send_bytes(&chunk_data.to_bytes()).await?;

            bytes_sent += bytes_read as u64;
            progress.set_position(bytes_sent);

            debug!(
                "Sent chunk {} ({} bytes)",
                chunk_index, bytes_read
            );

            // Wait for acknowledgment
            loop {
                let data = self
                    .message_rx
                    .recv()
                    .await
                    .ok_or(AppError::ChannelClosed)?;

                if let Some(ParsedMessage::Control(TransferMessage::Ack { index })) =
                    ParsedMessage::from_bytes(&data)
                {
                    if index == chunk_index {
                        break;
                    }
                }
            }

            chunk_index += 1;
        }

        // Send done message
        let done_msg = TransferMessage::done();
        self.send_message(&done_msg).await?;

        progress.finish_with_message("Transfer complete!");
        info!("File transfer complete: {} bytes sent", bytes_sent);

        Ok(())
    }

    async fn send_message(&self, msg: &TransferMessage) -> Result<()> {
        let bytes = msg.to_bytes();
        self.send_bytes(&bytes).await
    }

    async fn send_bytes(&self, data: &[u8]) -> Result<()> {
        self.data_channel
            .send(&Bytes::copy_from_slice(data))
            .await
            .map_err(|e| AppError::Transfer(format!("Failed to send data: {}", e)))?;
        Ok(())
    }
}
