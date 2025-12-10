mod cli;
mod error;
mod peer_id;
mod rtc;
mod signaling;
mod transfer;

use crate::cli::{Cli, Commands};
use crate::error::{AppError, Result};
use crate::peer_id::generate_peer_id;
use crate::rtc::{setup_data_channel_handlers, WebRtcPeer};
use crate::signaling::{PeerJsClient, ServerMessage};
use crate::transfer::{FileReceiver, FileSender};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};
use uuid::Uuid;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let result = match cli.command {
        Commands::Send { file, peer_id } => run_sender(file, peer_id, &cli.server).await,
        Commands::Receive { peer_id, key, output } => {
            run_receiver(peer_id, key, output, &cli.server).await
        }
    };

    if let Err(e) = result {
        error!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

async fn run_sender(file: PathBuf, peer_id: Option<String>, server: &str) -> Result<()> {
    // Validate file exists
    if !file.exists() {
        return Err(AppError::FileNotFound(file.display().to_string()));
    }

    // Generate or use provided peer ID
    let peer_id = peer_id.unwrap_or_else(generate_peer_id);

    info!("Starting sender...");

    // Connect to PeerJS server
    let mut signaling = PeerJsClient::connect(&peer_id, Some(server)).await?;
    signaling.wait_for_open().await?;

    // Generate encryption key early so we can display it
    let key_preview = {
        use crate::transfer::crypto::{generate_key, key_to_base64};
        let key = generate_key();
        (key, key_to_base64(&key))
    };

    println!("\nYour peer ID: {}", peer_id);
    println!("Encryption key: {}", key_preview.1);
    println!("\nShare BOTH with the receiver. Waiting for connection...\n");

    // Create WebRTC peer
    let mut webrtc_peer = WebRtcPeer::new().await?;

    // Create data channel before receiving offer
    let data_channel = webrtc_peer.create_data_channel("file-transfer").await?;

    // Set up data channel message handler
    let (message_tx, message_rx) = mpsc::channel(100);
    let (open_tx, open_rx) = oneshot::channel();
    setup_data_channel_handlers(&data_channel, message_tx, Some(open_tx));

    // Wait for offer from receiver
    let (remote_peer_id, remote_sdp, remote_connection_id) = loop {
        match signaling.recv_message().await? {
            ServerMessage::Offer { src, payload, .. } => {
                info!("Received offer from: {}", src);
                debug!("SDP type: {}, SDP content length: {}", payload.sdp.sdp_type, payload.sdp.sdp.len());
                debug!("SDP: {}", payload.sdp.sdp);
                break (src, payload.sdp, payload.connection_id);
            }
            ServerMessage::Heartbeat => {
                signaling.send_heartbeat().await?;
            }
            msg => {
                debug!("Ignoring message: {:?}", msg);
            }
        }
    };

    println!("Receiver connected!");

    // Set remote description
    let remote_desc = RTCSessionDescription::offer(remote_sdp.sdp)?;
    webrtc_peer.set_remote_description(remote_desc).await?;

    // Create and send answer
    let answer = webrtc_peer.create_answer().await?;
    webrtc_peer
        .set_local_description(answer.clone())
        .await?;
    signaling
        .send_answer(&remote_peer_id, &answer.sdp, &remote_connection_id)
        .await?;

    // Handle ICE candidate exchange with timeout for data channel open
    let mut open_rx = Some(open_rx);
    let timeout_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);

    loop {
        let timeout = tokio::time::sleep_until(timeout_deadline);
        tokio::pin!(timeout);

        tokio::select! {
            Some(candidate) = webrtc_peer.ice_candidate_rx.recv() => {
                let candidate_json = candidate.to_json()?;
                signaling.send_candidate(
                    &remote_peer_id,
                    &candidate_json.candidate,
                    candidate_json.sdp_mid.as_deref(),
                    candidate_json.sdp_mline_index,
                    &remote_connection_id,
                ).await?;
            }
            msg = signaling.recv_message() => {
                match msg? {
                    ServerMessage::Candidate { payload, .. } => {
                        let candidate = RTCIceCandidateInit {
                            candidate: payload.candidate.candidate,
                            sdp_mid: payload.candidate.sdp_mid,
                            sdp_mline_index: payload.candidate.sdp_m_line_index,
                            username_fragment: None,
                        };
                        webrtc_peer.add_ice_candidate(candidate).await?;
                    }
                    ServerMessage::Heartbeat => {
                        signaling.send_heartbeat().await?;
                    }
                    _ => {}
                }
            }
            _ = &mut timeout => {
                return Err(AppError::Timeout);
            }
        }

        // Check if data channel is open
        if let Some(mut rx) = open_rx.take() {
            match rx.try_recv() {
                Ok(()) => {
                    info!("Data channel opened!");
                    break;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    open_rx = Some(rx);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    // Channel was closed, data channel might be open
                    break;
                }
            }
        }
    }

    // Wait a bit for the connection to stabilize
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Send the file (using the pre-generated key)
    let mut sender = FileSender::new(file, data_channel, message_rx, key_preview.0);
    sender.send().await?;

    // Clean up
    webrtc_peer.close().await?;

    Ok(())
}

async fn run_receiver(
    peer_id: String,
    key_base64: String,
    output: Option<PathBuf>,
    server: &str,
) -> Result<()> {
    // Parse the encryption key
    let key = crate::transfer::key_from_base64(&key_base64)?;

    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
    let our_peer_id = generate_peer_id();
    let connection_id = Uuid::new_v4().to_string();

    info!("Starting receiver...");
    println!("Connecting to peer {}...", peer_id);

    // Connect to PeerJS server
    let mut signaling = PeerJsClient::connect(&our_peer_id, Some(server)).await?;
    signaling.wait_for_open().await?;

    // Create WebRTC peer
    let mut webrtc_peer = WebRtcPeer::new().await?;

    // Create a data channel first - this is required for the SDP to include data channel info
    // The sender also creates one, and they'll be negotiated
    let _local_dc = webrtc_peer.create_data_channel("file-transfer").await?;

    // Create and send offer
    let offer = webrtc_peer.create_offer().await?;
    webrtc_peer.set_local_description(offer.clone()).await?;

    debug!("Sending offer SDP length: {}", offer.sdp.len());
    debug!("Offer SDP: {}", offer.sdp);

    signaling
        .send_offer(&peer_id, &offer.sdp, &connection_id)
        .await?;

    info!("Sent offer to {}", peer_id);

    // Wait for answer
    let remote_sdp = loop {
        match signaling.recv_message().await? {
            ServerMessage::Answer { src, payload, .. } => {
                if src == peer_id {
                    info!("Received answer from: {}", src);
                    break payload.sdp;
                }
            }
            ServerMessage::Heartbeat => {
                signaling.send_heartbeat().await?;
            }
            ServerMessage::Expire => {
                return Err(AppError::Connection("Connection expired - peer not found".to_string()));
            }
            msg => {
                debug!("Ignoring message: {:?}", msg);
            }
        }
    };

    println!("Connected!");

    // Set remote description
    let remote_desc = RTCSessionDescription::answer(remote_sdp.sdp)?;
    webrtc_peer.set_remote_description(remote_desc).await?;

    // Wait for data channel and exchange ICE candidates
    let data_channel: Arc<webrtc::data_channel::RTCDataChannel>;
    let (message_tx, message_rx) = mpsc::channel(100);
    let timeout_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);

    loop {
        let timeout = tokio::time::sleep_until(timeout_deadline);
        tokio::pin!(timeout);

        tokio::select! {
            Some(candidate) = webrtc_peer.ice_candidate_rx.recv() => {
                let candidate_json = candidate.to_json()?;
                signaling.send_candidate(
                    &peer_id,
                    &candidate_json.candidate,
                    candidate_json.sdp_mid.as_deref(),
                    candidate_json.sdp_mline_index,
                    &connection_id,
                ).await?;
            }
            msg = signaling.recv_message() => {
                match msg? {
                    ServerMessage::Candidate { payload, .. } => {
                        let candidate = RTCIceCandidateInit {
                            candidate: payload.candidate.candidate,
                            sdp_mid: payload.candidate.sdp_mid,
                            sdp_mline_index: payload.candidate.sdp_m_line_index,
                            username_fragment: None,
                        };
                        webrtc_peer.add_ice_candidate(candidate).await?;
                    }
                    ServerMessage::Heartbeat => {
                        signaling.send_heartbeat().await?;
                    }
                    _ => {}
                }
            }
            Some(dc) = webrtc_peer.data_channel_rx.recv() => {
                info!("Received data channel: {}", dc.label());
                setup_data_channel_handlers(&dc, message_tx.clone(), None);
                data_channel = dc;
                break;
            }
            _ = &mut timeout => {
                return Err(AppError::Timeout);
            }
        }
    }

    // Wait a bit for the connection to stabilize
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Receive the file
    let mut receiver = FileReceiver::new(output_dir, data_channel, message_rx, key);
    let output_path = receiver.receive().await?;

    println!("\nFile saved to: {}", output_path.display());

    // Clean up
    webrtc_peer.close().await?;

    Ok(())
}
