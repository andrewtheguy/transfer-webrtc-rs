use crate::error::{AppError, Result};
use crate::transfer::protocol::{ChunkData, ParsedMessage, TransferMessage};
use bytes::Bytes;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use webrtc::data_channel::RTCDataChannel;

pub struct FileReceiver {
    output_dir: PathBuf,
    data_channel: Arc<RTCDataChannel>,
    message_rx: mpsc::Receiver<Vec<u8>>,
}

impl FileReceiver {
    pub fn new(
        output_dir: impl AsRef<Path>,
        data_channel: Arc<RTCDataChannel>,
        message_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            data_channel,
            message_rx,
        }
    }

    pub async fn receive(&mut self) -> Result<PathBuf> {
        // Wait for file info
        info!("Waiting for file info...");
        let (filename, file_size, total_chunks) = loop {
            let data = self
                .message_rx
                .recv()
                .await
                .ok_or(AppError::ChannelClosed)?;

            if let Some(ParsedMessage::Control(TransferMessage::FileInfo {
                filename,
                size,
                total_chunks,
                ..
            })) = ParsedMessage::from_bytes(&data)
            {
                break (filename, size, total_chunks);
            }
        };

        info!(
            "Receiving file: {} ({} bytes, {} chunks)",
            filename, file_size, total_chunks
        );

        // Create output file
        let output_path = self.output_dir.join(&filename);
        let mut file = File::create(&output_path).await?;

        // Send ready message
        let ready_msg = TransferMessage::ready();
        self.send_message(&ready_msg).await?;
        info!("Ready to receive");

        // Set up progress bar
        let progress = ProgressBar::new(file_size);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );

        // Receive chunks
        let mut bytes_received = 0u64;
        let mut expected_chunk = 0u64;
        let mut pending_chunk_header: Option<u64> = None;

        loop {
            let data = self
                .message_rx
                .recv()
                .await
                .ok_or(AppError::ChannelClosed)?;

            match ParsedMessage::from_bytes(&data) {
                Some(ParsedMessage::Control(TransferMessage::Chunk { index })) => {
                    // Received chunk header, expect chunk data next
                    pending_chunk_header = Some(index);
                }
                Some(ParsedMessage::Chunk(chunk_data)) => {
                    // Received chunk data
                    let expected_index = pending_chunk_header.unwrap_or(expected_chunk);

                    if chunk_data.index != expected_index {
                        warn!(
                            "Received out-of-order chunk: expected {}, got {}",
                            expected_index, chunk_data.index
                        );
                    }

                    // Write chunk to file
                    file.write_all(&chunk_data.data).await?;
                    bytes_received += chunk_data.data.len() as u64;
                    progress.set_position(bytes_received);

                    debug!(
                        "Received chunk {} ({} bytes)",
                        chunk_data.index,
                        chunk_data.data.len()
                    );

                    // Send acknowledgment
                    let ack_msg = TransferMessage::ack(chunk_data.index);
                    self.send_message(&ack_msg).await?;

                    expected_chunk = chunk_data.index + 1;
                    pending_chunk_header = None;
                }
                Some(ParsedMessage::Control(TransferMessage::Done)) => {
                    info!("Transfer complete signal received");
                    break;
                }
                Some(ParsedMessage::Control(TransferMessage::Error { message })) => {
                    return Err(AppError::Transfer(format!("Sender error: {}", message)));
                }
                _ => {
                    // Unknown message, try parsing as raw chunk data
                    if let Some(chunk_data) = ChunkData::from_bytes(&data) {
                        let expected_index = pending_chunk_header.unwrap_or(expected_chunk);

                        file.write_all(&chunk_data.data).await?;
                        bytes_received += chunk_data.data.len() as u64;
                        progress.set_position(bytes_received);

                        let ack_msg = TransferMessage::ack(chunk_data.index);
                        self.send_message(&ack_msg).await?;

                        expected_chunk = expected_index + 1;
                        pending_chunk_header = None;
                    }
                }
            }
        }

        // Ensure file is flushed
        file.flush().await?;

        progress.finish_with_message("Transfer complete!");
        info!(
            "File received: {} ({} bytes)",
            output_path.display(),
            bytes_received
        );

        Ok(output_path)
    }

    async fn send_message(&self, msg: &TransferMessage) -> Result<()> {
        let bytes = msg.to_bytes();
        self.data_channel
            .send(&Bytes::copy_from_slice(&bytes))
            .await
            .map_err(|e| AppError::Transfer(format!("Failed to send message: {}", e)))?;
        Ok(())
    }
}
