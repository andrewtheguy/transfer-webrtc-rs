use crate::error::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_credential_type::RTCIceCredentialType;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

const STUN_SERVER: &str = "stun:stun.l.google.com:19302";

// TURN server configuration
struct TurnServer {
    url: &'static str,
    username: &'static str,
    credential: &'static str,
}

const TURN_SERVERS: &[TurnServer] = &[
    TurnServer {
        url: "turn:eu-0.turn.peerjs.com:3478",
        username: "peerjs",
        credential: "peerjsp",
    },
    TurnServer {
        url: "turn:us-0.turn.peerjs.com:3478",
        username: "peerjs",
        credential: "peerjsp",
    },
];

pub struct WebRtcPeer {
    peer_connection: Arc<RTCPeerConnection>,
    pub ice_candidate_rx: mpsc::Receiver<RTCIceCandidate>,
    pub data_channel_rx: mpsc::Receiver<Arc<RTCDataChannel>>,
}

impl WebRtcPeer {
    pub async fn new() -> Result<Self> {
        let mut ice_servers = vec![
            // STUN server for NAT traversal discovery
            RTCIceServer {
                urls: vec![STUN_SERVER.to_owned()],
                ..Default::default()
            },
        ];

        // Add TURN servers with individual credentials
        for turn_server in TURN_SERVERS {
            ice_servers.push(RTCIceServer {
                urls: vec![turn_server.url.to_owned()],
                username: turn_server.username.to_owned(),
                credential: turn_server.credential.to_owned(),
                credential_type: RTCIceCredentialType::Password,
            });
        }

        let config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        let mut media_engine = MediaEngine::default();
        media_engine.register_default_codecs()?;

        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        let (ice_candidate_tx, ice_candidate_rx) = mpsc::channel(50);
        let (data_channel_tx, data_channel_rx) = mpsc::channel(1);

        // Set up ICE candidate handler
        let ice_tx = ice_candidate_tx.clone();
        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            let ice_tx = ice_tx.clone();
            Box::pin(async move {
                if let Some(candidate) = candidate {
                    debug!("New ICE candidate: {}", candidate.to_string());
                    let _ = ice_tx.send(candidate).await;
                }
            })
        }));

        // Set up connection state handler
        peer_connection.on_peer_connection_state_change(Box::new(move |state| {
            info!("Peer connection state changed: {}", state);
            Box::pin(async move {
                match state {
                    RTCPeerConnectionState::Connected => {
                        info!("WebRTC connection established!");
                    }
                    RTCPeerConnectionState::Disconnected => {
                        info!("WebRTC connection disconnected");
                    }
                    RTCPeerConnectionState::Failed => {
                        info!("WebRTC connection failed");
                    }
                    RTCPeerConnectionState::Closed => {
                        info!("WebRTC connection closed");
                    }
                    _ => {}
                }
            })
        }));

        // Set up data channel handler (for incoming data channels)
        let dc_tx = data_channel_tx.clone();
        peer_connection.on_data_channel(Box::new(move |dc| {
            let dc_tx = dc_tx.clone();
            info!("New data channel: {}", dc.label());
            Box::pin(async move {
                let _ = dc_tx.send(dc).await;
            })
        }));

        Ok(Self {
            peer_connection,
            ice_candidate_rx,
            data_channel_rx,
        })
    }

    pub async fn create_data_channel(&self, label: &str) -> Result<Arc<RTCDataChannel>> {
        let dc = self.peer_connection.create_data_channel(label, None).await?;
        info!("Created data channel: {}", label);
        Ok(dc)
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription> {
        let offer = self.peer_connection.create_offer(None).await?;
        debug!("Created offer");
        Ok(offer)
    }

    pub async fn create_answer(&self) -> Result<RTCSessionDescription> {
        let answer = self.peer_connection.create_answer(None).await?;
        debug!("Created answer");
        Ok(answer)
    }

    pub async fn set_local_description(&self, sdp: RTCSessionDescription) -> Result<()> {
        self.peer_connection.set_local_description(sdp).await?;
        debug!("Set local description");
        Ok(())
    }

    pub async fn set_remote_description(&self, sdp: RTCSessionDescription) -> Result<()> {
        self.peer_connection.set_remote_description(sdp).await?;
        debug!("Set remote description");
        Ok(())
    }

    pub async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        self.peer_connection.add_ice_candidate(candidate).await?;
        debug!("Added ICE candidate");
        Ok(())
    }

    pub fn connection_state(&self) -> RTCPeerConnectionState {
        self.peer_connection.connection_state()
    }

    pub async fn close(&self) -> Result<()> {
        self.peer_connection.close().await?;
        Ok(())
    }
}

/// Set up handlers for a data channel to send/receive messages
pub fn setup_data_channel_handlers(
    dc: &Arc<RTCDataChannel>,
    message_tx: mpsc::Sender<Vec<u8>>,
    open_tx: Option<tokio::sync::oneshot::Sender<()>>,
) {
    let dc_label = dc.label().to_string();

    // On open
    if let Some(open_tx) = open_tx {
        let label = dc_label.clone();
        dc.on_open(Box::new(move || {
            info!("Data channel '{}' opened", label);
            let _ = open_tx.send(());
            Box::pin(async {})
        }));
    }

    // On message
    let label = dc_label.clone();
    dc.on_message(Box::new(move |msg: DataChannelMessage| {
        let message_tx = message_tx.clone();
        let label = label.clone();
        Box::pin(async move {
            debug!("Received {} bytes on channel '{}'", msg.data.len(), label);
            let _ = message_tx.send(msg.data.to_vec()).await;
        })
    }));

    // On error
    let label = dc_label.clone();
    dc.on_error(Box::new(move |err| {
        tracing::error!("Data channel '{}' error: {}", label, err);
        Box::pin(async {})
    }));

    // On close
    dc.on_close(Box::new(move || {
        info!("Data channel '{}' closed", dc_label);
        Box::pin(async {})
    }));
}
