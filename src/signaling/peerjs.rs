use crate::error::{AppError, Result};
use crate::signaling::messages::{
    CandidatePayload, ClientMessage, IceCandidate, SdpPayload, ServerMessage, SessionDescription,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tokio_tungstenite::{
    connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const PEERJS_SERVER: &str = "0.peerjs.com";
const PEERJS_PATH: &str = "/peerjs";
const PEERJS_KEY: &str = "peerjs";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

pub struct PeerJsClient {
    peer_id: String,
    ws_write: futures_util::stream::SplitSink<
        WebSocketStream<MaybeTlsStream<TcpStream>>,
        Message,
    >,
    message_rx: mpsc::Receiver<ServerMessage>,
    _heartbeat_handle: tokio::task::JoinHandle<()>,
}

impl PeerJsClient {
    pub async fn connect(peer_id: &str, server: Option<&str>) -> Result<Self> {
        let server = server.unwrap_or(PEERJS_SERVER);
        let token = Uuid::new_v4().to_string();

        let url = format!(
            "wss://{}{}?key={}&id={}&token={}",
            server, PEERJS_PATH, PEERJS_KEY, peer_id, token
        );

        info!("Connecting to PeerJS server: {}", server);
        debug!("WebSocket URL: {}", url);

        let (ws_stream, _) = connect_async(&url).await?;
        let (ws_write, mut ws_read) = ws_stream.split();

        let (message_tx, message_rx) = mpsc::channel(100);
        let (heartbeat_tx, mut heartbeat_rx) = mpsc::channel::<Message>(10);

        // Spawn message reader task
        let message_tx_clone = message_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_result) = ws_read.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        debug!("Received: {}", text);
                        match serde_json::from_str::<ServerMessage>(&text) {
                            Ok(server_msg) => {
                                if message_tx_clone.send(server_msg).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse server message: {} - {}", e, text);
                            }
                        }
                    }
                    Ok(Message::Ping(data)) => {
                        debug!("Received ping");
                        let _ = heartbeat_tx.send(Message::Pong(data)).await;
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket closed by server");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Create heartbeat sender
        let heartbeat_handle = tokio::spawn(async move {
            let mut heartbeat_interval = interval(HEARTBEAT_INTERVAL);
            loop {
                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        // Heartbeat tick - handled in main write loop
                    }
                    Some(_msg) = heartbeat_rx.recv() => {
                        // Pong response - would need ws_write access
                    }
                }
            }
        });

        Ok(Self {
            peer_id: peer_id.to_string(),
            ws_write,
            message_rx,
            _heartbeat_handle: heartbeat_handle,
        })
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    pub async fn wait_for_open(&mut self) -> Result<()> {
        while let Some(msg) = self.message_rx.recv().await {
            match msg {
                ServerMessage::Open => {
                    info!("Connected to PeerJS server as: {}", self.peer_id);
                    return Ok(());
                }
                ServerMessage::IdTaken => {
                    return Err(AppError::PeerIdTaken);
                }
                ServerMessage::InvalidKey => {
                    return Err(AppError::Signaling("Invalid API key".to_string()));
                }
                ServerMessage::Error { payload } => {
                    let msg = payload
                        .map(|p| p.message)
                        .unwrap_or_else(|| "Unknown error".to_string());
                    return Err(AppError::Signaling(msg));
                }
                _ => {
                    debug!("Ignoring message while waiting for OPEN: {:?}", msg);
                }
            }
        }
        Err(AppError::ChannelClosed)
    }

    pub async fn recv_message(&mut self) -> Result<ServerMessage> {
        self.message_rx
            .recv()
            .await
            .ok_or(AppError::ChannelClosed)
    }

    pub async fn send_heartbeat(&mut self) -> Result<()> {
        let msg = ClientMessage::heartbeat();
        self.send_raw(&msg).await
    }

    pub async fn send_offer(
        &mut self,
        dst: &str,
        sdp: &str,
        connection_id: &str,
    ) -> Result<()> {
        let payload = SdpPayload {
            sdp: SessionDescription {
                sdp: sdp.to_string(),
                sdp_type: "offer".to_string(),
            },
            connection_type: "data".to_string(),
            connection_id: connection_id.to_string(),
            browser: Some("sendfile-webrtc-rs".to_string()),
            label: Some(connection_id.to_string()),
            reliable: Some(true),
            serialization: Some("binary".to_string()),
        };

        let msg = ClientMessage::offer(&self.peer_id, dst, payload);
        self.send_raw(&msg).await
    }

    pub async fn send_answer(
        &mut self,
        dst: &str,
        sdp: &str,
        connection_id: &str,
    ) -> Result<()> {
        let payload = SdpPayload {
            sdp: SessionDescription {
                sdp: sdp.to_string(),
                sdp_type: "answer".to_string(),
            },
            connection_type: "data".to_string(),
            connection_id: connection_id.to_string(),
            browser: Some("sendfile-webrtc-rs".to_string()),
            label: None,
            reliable: None,
            serialization: None,
        };

        let msg = ClientMessage::answer(&self.peer_id, dst, payload);
        self.send_raw(&msg).await
    }

    pub async fn send_candidate(
        &mut self,
        dst: &str,
        candidate: &str,
        sdp_mid: Option<&str>,
        sdp_m_line_index: Option<u16>,
        connection_id: &str,
    ) -> Result<()> {
        let payload = CandidatePayload {
            candidate: IceCandidate {
                candidate: candidate.to_string(),
                sdp_m_line_index,
                sdp_mid: sdp_mid.map(|s| s.to_string()),
                username_fragment: None,
            },
            connection_type: "data".to_string(),
            connection_id: connection_id.to_string(),
        };

        let msg = ClientMessage::candidate(&self.peer_id, dst, payload);
        self.send_raw(&msg).await
    }

    async fn send_raw(&mut self, msg: &ClientMessage) -> Result<()> {
        let json = serde_json::to_string(msg)?;
        debug!("Sending: {}", json);
        self.ws_write.send(Message::Text(json)).await?;
        Ok(())
    }
}
