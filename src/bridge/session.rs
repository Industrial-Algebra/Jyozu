use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use super::protocol::{MessageType, envelope};

#[derive(Clone)]
pub struct BridgeState {
    inner: Arc<Inner>,
}

struct Inner {
    sessions: Mutex<HashMap<String, Session>>,
    pending: Mutex<HashMap<String, PendingRequest>>,
}

struct Session {
    channel: String,
    outbound: mpsc::UnboundedSender<String>,
    joined_at: u64,
    client_name: Option<String>,
    client_version: Option<String>,
}

struct PendingRequest {
    channel: String,
    tx: oneshot::Sender<Result<Value, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub channel: String,
    pub joined_at: u64,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
}

impl BridgeState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                sessions: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn register_session(
        &self,
        channel: String,
        outbound: mpsc::UnboundedSender<String>,
        joined_at: u64,
        client_name: Option<String>,
        client_version: Option<String>,
    ) {
        self.inner.sessions.lock().await.insert(
            channel.clone(),
            Session {
                channel,
                outbound,
                joined_at,
                client_name,
                client_version,
            },
        );
    }

    pub async fn unregister_session(&self, channel: &str) {
        self.inner.sessions.lock().await.remove(channel);

        let mut pending = self.inner.pending.lock().await;
        let request_ids: Vec<String> = pending
            .iter()
            .filter_map(|(request_id, req)| (req.channel == channel).then_some(request_id.clone()))
            .collect();

        for request_id in request_ids {
            if let Some(req) = pending.remove(&request_id) {
                let _ = req.tx.send(Err("Plugin disconnected".to_string()));
            }
        }
    }

    pub async fn handle_response(
        &self,
        request_id: &str,
        result: Option<Value>,
        error: Option<String>,
    ) {
        let pending = self.inner.pending.lock().await.remove(request_id);
        if let Some(req) = pending {
            let _ = req.tx.send(match error {
                Some(message) => Err(message),
                None => Ok(result.unwrap_or(Value::Null)),
            });
        }
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.inner
            .sessions
            .lock()
            .await
            .values()
            .map(|session| SessionInfo {
                channel: session.channel.clone(),
                joined_at: session.joined_at,
                client_name: session.client_name.clone(),
                client_version: session.client_version.clone(),
            })
            .collect()
    }

    pub async fn session_count(&self) -> usize {
        self.inner.sessions.lock().await.len()
    }

    pub async fn send_command(
        &self,
        channel: &str,
        command: &str,
        params: Value,
        timeout_ms: u64,
    ) -> Result<Value> {
        let outbound = {
            let sessions = self.inner.sessions.lock().await;
            let session = sessions
                .get(channel)
                .ok_or_else(|| anyhow!("No plugin connected for channel: {channel}"))?;
            session.outbound.clone()
        };

        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(
            request_id.clone(),
            PendingRequest {
                channel: channel.to_string(),
                tx,
            },
        );

        let payload = json!({
            "command": command,
            "params": params,
            "requestId": request_id.clone(),
        });
        let env = envelope(
            Uuid::new_v4().to_string(),
            MessageType::Message,
            Some(channel.to_string()),
            Some(payload),
        );
        let text = serde_json::to_string(&env)?;
        outbound
            .send(text)
            .map_err(|_| anyhow!("Failed to send command to plugin"))?;

        match tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => Err(anyhow!(message)),
            Ok(Err(_)) => Err(anyhow!("Plugin response channel closed")),
            Err(_) => {
                self.inner.pending.lock().await.remove(&request_id);
                Err(anyhow!("Command timeout after {timeout_ms}ms"))
            }
        }
    }
}
