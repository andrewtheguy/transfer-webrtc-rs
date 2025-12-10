use crate::error::{AppError, Result};
use crate::transfer::crypto::{
    decrypt_chunk, decrypt_metadata, EncryptedMetadata, KEY_SIZE, NONCE_SIZE,
};
use crate::transfer::protocol::{ParsedMessage, TransferMessage};
use bytes::Bytes;
use indicatif::{ProgressBar, ProgressStyle};
use std::convert::TryInto;
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
    key: [u8; KEY_SIZE],
}

impl FileReceiver {
    pub fn new(
        output_dir: impl AsRef<Path>,
        data_channel: Arc<RTCDataChannel>,
        message_rx: mpsc::Receiver<Vec<u8>>,
        key: [u8; KEY_SIZE],
    ) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            data_channel,
            message_rx,
            key,
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

            match ParsedMessage::from_bytes(&data) {
                Some(ParsedMessage::Control(
                    TransferMessage::EncryptedFileInfo { nonce, ciphertext },
                )) => {
                    let nonce: [u8; NONCE_SIZE] = nonce
                        .as_slice()
                        .try_into()
                        .map_err(|_| AppError::Transfer("Invalid metadata nonce".to_string()))?;

                    let metadata = EncryptedMetadata { nonce, ciphertext };
                    let file_info = decrypt_metadata(&self.key, &metadata)?;

                    break (file_info.filename, file_info.size, file_info.total_chunks);
                }
                Some(ParsedMessage::Control(TransferMessage::FileInfo { .. })) => {
                    return Err(AppError::Transfer(
                        "Received unencrypted file metadata; please update the sender"
                            .to_string(),
                    ));
                }
                _ => {}
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

        // Receive encrypted chunks
        let mut bytes_received = 0u64;
        let mut expected_chunk = 0u64;

        loop {
            let data = self
                .message_rx
                .recv()
                .await
                .ok_or(AppError::ChannelClosed)?;

            match ParsedMessage::from_bytes(&data) {
                Some(ParsedMessage::EncryptedChunk(encrypted_chunk)) => {
                    // Decrypt and verify chunk
                    if encrypted_chunk.index != expected_chunk {
                        warn!(
                            "Received out-of-order chunk: expected {}, got {}",
                            expected_chunk, encrypted_chunk.index
                        );
                    }

                    let plaintext = decrypt_chunk(&self.key, &encrypted_chunk)?;

                    // Write decrypted chunk to file
                    file.write_all(&plaintext).await?;
                    bytes_received += plaintext.len() as u64;
                    progress.set_position(bytes_received);

                    debug!(
                        "Received and decrypted chunk {} ({} bytes)",
                        encrypted_chunk.index,
                        plaintext.len()
                    );

                    // Send acknowledgment
                    let ack_msg = TransferMessage::ack(encrypted_chunk.index);
                    self.send_message(&ack_msg).await?;

                    expected_chunk = encrypted_chunk.index + 1;
                }
                Some(ParsedMessage::Control(TransferMessage::Done)) => {
                    info!("Transfer complete signal received");
                    break;
                }
                Some(ParsedMessage::Control(TransferMessage::Error { message })) => {
                    return Err(AppError::Transfer(format!("Sender error: {}", message)));
                }
                _ => {
                    debug!("Ignoring unknown message type");
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
