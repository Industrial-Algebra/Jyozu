use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};
use uuid::Uuid;

use super::{
    protocol::{Envelope, MessageType, envelope, now_ms},
    session::{BridgeState, SessionInfo},
};

pub const DEFAULT_WS_PORT: u16 = 3055;
pub const DEFAULT_HTTP_PORT: u16 = 3056;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone)]
struct HttpState {
    bridge: BridgeState,
    ws_port: u16,
    http_port: u16,
}

#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
    #[serde(default)]
    params: Value,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    ws_port: u16,
    http_port: u16,
    session_count: usize,
}

#[derive(Debug, Serialize)]
struct SessionsResponse {
    sessions: Vec<SessionInfo>,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

pub async fn run_bridge(ws_port: u16, http_port: u16) -> Result<()> {
    let bridge = BridgeState::new();
    let ws_state = bridge.clone();
    let http_state = HttpState {
        bridge,
        ws_port,
        http_port,
    };

    let ws_task = tokio::spawn(async move { run_ws_server(ws_port, ws_state).await });
    let http_task = tokio::spawn(async move { run_http_server(http_port, http_state).await });

    tokio::select! {
        res = ws_task => res??,
        res = http_task => res??,
        _ = tokio::signal::ctrl_c() => {}
    }

    Ok(())
}

async fn run_ws_server(port: u16, bridge: BridgeState) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    eprintln!("[bridge] websocket listening on ws://{addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        let bridge = bridge.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_ws_connection(stream, bridge).await {
                eprintln!("[bridge] websocket connection error: {err}");
            }
        });
    }
}

async fn handle_ws_connection(stream: tokio::net::TcpStream, bridge: BridgeState) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();

    let writer = tokio::spawn(async move {
        while let Some(text) = outbound_rx.recv().await {
            if write.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let welcome = envelope(
        Uuid::new_v4().to_string(),
        MessageType::Ack,
        None,
        Some(json!({ "message": "Connected to Jyozu bridge" })),
    );
    let _ = outbound_tx.send(serde_json::to_string(&welcome)?);

    let mut current_channel: Option<String> = None;

    while let Some(message) = read.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        let incoming: Envelope = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("[bridge] invalid envelope: {err}");
                continue;
            }
        };

        match incoming.r#type {
            MessageType::Join => {
                let payload = incoming.payload.unwrap_or(Value::Null);
                let session_id = payload
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let client_name = payload
                    .get("client")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let client_version = payload
                    .get("client")
                    .and_then(|v| v.get("version"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let channel = incoming.channel.or(session_id.clone());
                if let Some(channel) = channel {
                    current_channel = Some(channel.clone());
                    bridge
                        .register_session(
                            channel.clone(),
                            outbound_tx.clone(),
                            now_ms(),
                            client_name,
                            client_version,
                        )
                        .await;
                    let ack = envelope(
                        Uuid::new_v4().to_string(),
                        MessageType::Ack,
                        Some(channel.clone()),
                        Some(json!({
                            "kind": "join",
                            "ok": true,
                            "success": true,
                            "sessionId": session_id.unwrap_or(channel.clone()),
                            "message": "Joined channel"
                        })),
                    );
                    let _ = outbound_tx.send(serde_json::to_string(&ack)?);
                    eprintln!("[bridge] joined channel {channel}");
                }
            }
            MessageType::Message => {
                let payload = incoming.payload.unwrap_or(Value::Null);
                if let Some(request_id) = payload.get("requestId").and_then(|v| v.as_str()) {
                    let result = payload.get("result").cloned();
                    let error = payload
                        .get("error")
                        .and_then(|v| v.get("message"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    bridge.handle_response(request_id, result, error).await;
                }
            }
            MessageType::Pong => {}
            _ => {}
        }
    }

    if let Some(channel) = current_channel {
        bridge.unregister_session(&channel).await;
        eprintln!("[bridge] channel disconnected: {channel}");
    }
    writer.abort();
    Ok(())
}

async fn run_http_server(port: u16, state: HttpState) -> Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/sessions", get(sessions))
        .route("/command/{channel}", post(command))
        .with_state(state.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    eprintln!("[bridge] http listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<HttpState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        ws_port: state.ws_port,
        http_port: state.http_port,
        session_count: state.bridge.session_count().await,
    })
}

async fn sessions(State(state): State<HttpState>) -> Json<SessionsResponse> {
    Json(SessionsResponse {
        sessions: state.bridge.list_sessions().await,
    })
}

async fn command(
    State(state): State<HttpState>,
    Path(channel): Path<String>,
    Json(req): Json<CommandRequest>,
) -> impl IntoResponse {
    match state
        .bridge
        .send_command(&channel, &req.command, req.params, req.timeout_ms)
        .await
    {
        Ok(result) => Json(json!({ "success": true, "result": result })),
        Err(err) => Json(json!({ "success": false, "error": err.to_string() })),
    }
}
