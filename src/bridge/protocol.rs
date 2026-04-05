use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Join,
    Ack,
    Message,
    Progress,
    Error,
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: String,
    pub r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis() as u64
}

pub fn envelope(
    id: String,
    r#type: MessageType,
    channel: Option<String>,
    payload: Option<Value>,
) -> Envelope {
    Envelope {
        id,
        r#type,
        channel,
        timestamp: now_ms(),
        payload,
    }
}
