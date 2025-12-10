use serde::{Deserialize, Serialize};

/// Messages received from the PeerJS server
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "OPEN")]
    Open,

    #[serde(rename = "ID-TAKEN")]
    IdTaken,

    #[serde(rename = "INVALID-KEY")]
    InvalidKey,

    #[serde(rename = "ERROR")]
    Error { payload: Option<ErrorPayload> },

    #[serde(rename = "OFFER")]
    Offer {
        src: String,
        dst: String,
        payload: SdpPayload,
    },

    #[serde(rename = "ANSWER")]
    Answer {
        src: String,
        dst: String,
        payload: SdpPayload,
    },

    #[serde(rename = "CANDIDATE")]
    Candidate {
        src: String,
        dst: String,
        payload: CandidatePayload,
    },

    #[serde(rename = "LEAVE")]
    Leave { src: String },

    #[serde(rename = "EXPIRE")]
    Expire,

    #[serde(rename = "HEARTBEAT")]
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    #[serde(rename = "msg")]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdpPayload {
    pub sdp: SessionDescription,
    #[serde(rename = "type")]
    pub connection_type: String,
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reliable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    pub sdp: String,
    #[serde(rename = "type")]
    pub sdp_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatePayload {
    pub candidate: IceCandidate,
    #[serde(rename = "type")]
    pub connection_type: String,
    #[serde(rename = "connectionId")]
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(rename = "sdpMLineIndex")]
    pub sdp_m_line_index: Option<u16>,
    #[serde(rename = "sdpMid")]
    pub sdp_mid: Option<String>,
    #[serde(rename = "usernameFragment")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_fragment: Option<String>,
}

/// Messages sent to the PeerJS server
#[derive(Debug, Clone, Serialize)]
pub struct ClientMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl ClientMessage {
    pub fn heartbeat() -> Self {
        Self {
            msg_type: "HEARTBEAT".to_string(),
            src: None,
            dst: None,
            payload: None,
        }
    }

    pub fn offer(src: &str, dst: &str, payload: SdpPayload) -> Self {
        Self {
            msg_type: "OFFER".to_string(),
            src: Some(src.to_string()),
            dst: Some(dst.to_string()),
            payload: Some(serde_json::to_value(payload).unwrap()),
        }
    }

    pub fn answer(src: &str, dst: &str, payload: SdpPayload) -> Self {
        Self {
            msg_type: "ANSWER".to_string(),
            src: Some(src.to_string()),
            dst: Some(dst.to_string()),
            payload: Some(serde_json::to_value(payload).unwrap()),
        }
    }

    pub fn candidate(src: &str, dst: &str, payload: CandidatePayload) -> Self {
        Self {
            msg_type: "CANDIDATE".to_string(),
            src: Some(src.to_string()),
            dst: Some(dst.to_string()),
            payload: Some(serde_json::to_value(payload).unwrap()),
        }
    }
}
