use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MineChatMessage {
    #[serde(rename = "AUTH")]
    Auth { payload: AuthPayload },
    #[serde(rename = "AUTH_ACK")]
    AuthAck { payload: AuthAckPayload },
    #[serde(rename = "CHAT")]
    Chat { payload: ChatPayload },
    #[serde(rename = "BROADCAST")]
    Broadcast { payload: BroadcastPayload },
    #[serde(rename = "DISCONNECT")]
    Disconnect { payload: DisconnectPayload },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthPayload {
    pub client_uuid: String,
    pub link_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthAckPayload {
    pub status: String,
    pub message: String,
    pub minecraft_uuid: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatPayload {
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BroadcastPayload {
    pub from: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DisconnectPayload {
    pub reason: String,
}
