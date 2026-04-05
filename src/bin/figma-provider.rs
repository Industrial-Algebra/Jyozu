use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[path = "../bridge/mod.rs"]
mod bridge;

use bridge::server::{DEFAULT_HTTP_PORT, DEFAULT_WS_PORT, run_bridge};

/// Figma Provider for Lonis harness
#[derive(Parser)]
#[command(name = "figma-provider")]
#[command(about = "Lonis-compatible Figma provider", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show provider manifest
    Manifest,
    /// List available tools
    ToolsList,
    /// Describe a specific tool
    ToolsDescribe {
        /// Tool name
        #[arg(value_name = "TOOL")]
        tool: String,
    },
    /// Call a tool in Lonis machine mode. Canonical request should be passed via stdin.
    Call {
        /// Optional tool name for compatibility mode
        #[arg(value_name = "TOOL")]
        tool: Option<String>,
        /// Optional JSON arguments for compatibility mode
        #[arg(value_name = "ARGS")]
        args: Option<Value>,
    },
    /// Show provider status
    Status,
    /// Run provider diagnostics
    Doctor,
    /// Run the persistent local bridge used by the Figma plugin
    #[command(hide = true, name = "bridge-serve")]
    BridgeServe {
        #[arg(long, default_value_t = DEFAULT_WS_PORT)]
        ws_port: u16,
        #[arg(long, default_value_t = DEFAULT_HTTP_PORT)]
        http_port: u16,
    },
}

#[derive(Serialize)]
struct Manifest {
    name: String,
    version: String,
    display_name: String,
    description: String,
    provider_type: String,
    protocol_version: String,
    runtime: Value,
    tools: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Serialize)]
struct ToolSummary {
    name: String,
    description: String,
    verification_status: String,
}

#[derive(Serialize)]
struct ToolsListResponse {
    provider: String,
    tools: Vec<ToolSummary>,
}

#[derive(Serialize, Clone)]
struct ToolDescription {
    name: String,
    description: String,
    input_schema: Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CallRequest {
    tool: String,
    #[serde(default = "default_schema_version")]
    schema_version: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    context: Value,
}

fn default_schema_version() -> String {
    "1".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BridgeSession {
    channel: String,
    joined_at: u64,
    client_name: Option<String>,
    client_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeSessionsResponse {
    sessions: Vec<BridgeSession>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BridgeHealthResponse {
    status: String,
    ws_port: u16,
    http_port: u16,
    session_count: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Manifest => {
            println!("{}", serde_json::to_string_pretty(&provider_manifest())?);
        }
        Commands::ToolsList => {
            let tools = canonical_tool_names()
                .iter()
                .filter_map(|name| {
                    get_canonical_tool_contract(name).map(|contract| ToolSummary {
                        name: name.to_string(),
                        description: contract
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        verification_status: contract
                            .get("verification")
                            .and_then(|v| v.get("status"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("implemented")
                            .to_string(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&ToolsListResponse {
                    provider: "figma".to_string(),
                    tools,
                })?
            );
        }
        Commands::ToolsDescribe { tool } => {
            if let Some(contract) = get_canonical_tool_contract(&tool) {
                println!("{}", serde_json::to_string_pretty(&contract)?);
            } else {
                eprintln!("Tool not found: {}", tool);
                std::process::exit(1);
            }
        }
        Commands::Call { tool, args } => {
            let envelope = handle_call(tool, args)?;
            println!("{}", serde_json::to_string_pretty(&envelope)?);
        }
        Commands::Status => {
            let sessions = fetch_sessions().unwrap_or_default();
            let health = fetch_health().ok();
            let status = json!({
                "provider": "figma",
                "ok": health.is_some(),
                "bridge_reachable": health.is_some(),
                "plugin_connected": !sessions.is_empty(),
                "active_sessions": sessions.len(),
                "lock_present": false,
                "audit_log_path": Value::Null,
                "bridge": health,
                "sessions": sessions,
            });
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        Commands::Doctor => {
            let health = fetch_health().ok();
            let sessions = fetch_sessions().unwrap_or_default();
            let checks = vec![
                json!({
                    "name": "bridge",
                    "ok": health.is_some(),
                    "message": if health.is_some() {
                        format!("Bridge reachable at {}", bridge_http_base())
                    } else {
                        format!("Bridge not responding on {}. Start it with: figma-provider bridge-serve", bridge_http_base())
                    }
                }),
                json!({
                    "name": "plugin_connection",
                    "ok": !sessions.is_empty(),
                    "message": if sessions.is_empty() {
                        "No active plugin connection found".to_string()
                    } else {
                        format!("{} active plugin session(s)", sessions.len())
                    }
                }),
                json!({
                    "name": "session_inference",
                    "ok": sessions.len() <= 1,
                    "message": if sessions.len() == 1 {
                        "Single plugin session detected; session_id may be omitted".to_string()
                    } else if sessions.is_empty() {
                        "No plugin session available to infer".to_string()
                    } else {
                        "Multiple sessions active; explicit session_id required".to_string()
                    }
                }),
            ];
            let doctor = json!({
                "provider": "figma",
                "ok": health.is_some() && !sessions.is_empty(),
                "checks": checks,
            });
            println!("{}", serde_json::to_string_pretty(&doctor)?);
        }
        Commands::BridgeServe { ws_port, http_port } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_bridge(ws_port, http_port))?;
        }
    }

    Ok(())
}

fn handle_call(tool: Option<String>, args: Option<Value>) -> Result<Value> {
    let started = std::time::Instant::now();
    let request = load_call_request(tool, args)?;
    let tool_name = request.tool.clone();

    let response = match execute_call_request(request) {
        Ok(result) => json!({
            "ok": true,
            "tool": tool_name,
            "provider": "figma",
            "schema_version": "1",
            "result": result,
            "meta": {
                "duration_ms": started.elapsed().as_millis(),
                "warnings": [],
                "artifacts": []
            }
        }),
        Err((code, message, details)) => json!({
            "ok": false,
            "tool": tool_name,
            "provider": "figma",
            "schema_version": "1",
            "error": {
                "code": code,
                "message": message,
                "details": details
            },
            "meta": {
                "duration_ms": started.elapsed().as_millis(),
                "warnings": [],
                "artifacts": []
            }
        }),
    };

    Ok(response)
}

fn load_call_request(tool: Option<String>, args: Option<Value>) -> Result<CallRequest> {
    if let Some(args) = args {
        let tool = tool.ok_or_else(|| anyhow!("Tool name required when using positional args"))?;
        return Ok(CallRequest {
            tool,
            schema_version: default_schema_version(),
            input: args,
            context: Value::Object(Default::default()),
        });
    }

    let mut stdin = String::new();
    use std::io::Read;
    std::io::stdin().read_to_string(&mut stdin)?;
    let stdin = stdin.trim();
    if stdin.is_empty() {
        return Err(anyhow!(
            "No request provided. Pass a canonical JSON request via stdin or use compatibility mode: call <tool> <args-json>"
        ));
    }

    let parsed: Value = serde_json::from_str(stdin)?;
    if parsed.get("tool").is_some() && parsed.get("input").is_some() {
        return Ok(serde_json::from_value(parsed)?);
    }

    let tool = tool.ok_or_else(|| {
        anyhow!("Tool name required when stdin payload is not a canonical request envelope")
    })?;
    Ok(CallRequest {
        tool,
        schema_version: default_schema_version(),
        input: parsed,
        context: Value::Object(Default::default()),
    })
}

fn execute_call_request(
    request: CallRequest,
) -> std::result::Result<Value, (String, String, Value)> {
    let canonical_tool = resolve_external_tool_name(&request.tool).ok_or_else(|| {
        error_tuple(
            "unknown_tool",
            format!("Unknown tool: {}", request.tool),
            json!({}),
        )
    })?;

    let contract = get_canonical_tool_contract(canonical_tool).ok_or_else(|| {
        error_tuple(
            "unknown_tool",
            format!("Unknown tool: {canonical_tool}"),
            json!({}),
        )
    })?;

    let input = request.input.as_object().ok_or_else(|| {
        error_tuple(
            "invalid_input",
            "input must be an object".to_string(),
            json!({}),
        )
    })?;

    validate_contract_input(&contract, input)?;

    let session_id = resolve_session_id(input)
        .map_err(|message| error_tuple("provider_unavailable", message.to_string(), json!({})))?;

    if canonical_tool == "figma.delete_node" {
        let confirm = input
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let dry_run = input
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let node_id = input
            .get("node_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                error_tuple(
                    "invalid_input",
                    "Missing required argument: node_id".to_string(),
                    json!({"field":"node_id"}),
                )
            })?;
        if !confirm {
            return Err(error_tuple(
                "confirmation_required",
                "Destructive action requires confirm=true".to_string(),
                json!({"field":"confirm"}),
            ));
        }
        if dry_run {
            return Ok(json!({
                "node_id": node_id,
                "deleted": false,
                "dry_run": true
            }));
        }
    }

    let bridge_params = canonical_input_to_bridge_input(canonical_tool, &session_id, input);
    let legacy_tool = canonical_to_legacy_tool(canonical_tool);
    let plugin_command = plugin_command_name(legacy_tool);
    let raw = send_bridge_command(&session_id, &plugin_command, bridge_params)
        .map_err(map_bridge_error)?;
    normalize_result(canonical_tool, input, raw)
}

fn error_tuple(code: &str, message: String, details: Value) -> (String, String, Value) {
    (code.to_string(), message, details)
}

fn validate_contract_input(
    contract: &Value,
    args: &serde_json::Map<String, Value>,
) -> std::result::Result<(), (String, String, Value)> {
    let input_schema = contract.get("input_schema").cloned().unwrap_or(Value::Null);
    let required = input_schema
        .get("required")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for field in required.iter().filter_map(|v| v.as_str()) {
        if !args.contains_key(field) {
            return Err(error_tuple(
                "invalid_input",
                format!("Missing required argument: {field}"),
                json!({"field": field}),
            ));
        }
    }

    let properties = input_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let additional_allowed = input_schema
        .get("additionalProperties")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    for (name, value) in args {
        let Some(prop) = properties.get(name) else {
            if !additional_allowed {
                return Err(error_tuple(
                    "invalid_input",
                    format!("Unexpected argument: {name}"),
                    json!({"field": name}),
                ));
            }
            continue;
        };
        let Some(expected) = prop.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        let ok = match expected {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => true,
        };
        if !ok {
            return Err(error_tuple(
                "invalid_input",
                format!("Invalid argument type for {name}: expected {expected}"),
                json!({"field": name, "expected": expected}),
            ));
        }
    }

    Ok(())
}

fn resolve_session_id(args: &serde_json::Map<String, Value>) -> Result<String> {
    let sessions = fetch_sessions()
        .map_err(|_| anyhow!("Bridge unavailable. Start it with: figma-provider bridge-serve"))?;
    args.get("session_id")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| args.get("channel").and_then(|value| value.as_str()).map(str::to_string))
        .or_else(|| infer_channel(&sessions).ok())
        .ok_or_else(|| anyhow!(
            "No session_id provided and unable to infer one. If exactly one plugin is connected, the session_id can be omitted; otherwise pass session_id explicitly."
        ))
}

fn canonical_input_to_bridge_input(
    canonical_tool: &str,
    _session_id: &str,
    input: &serde_json::Map<String, Value>,
) -> Value {
    let mut out = serde_json::Map::new();

    for (k, v) in input {
        match k.as_str() {
            "session_id" | "channel" => {}
            "page_id" => {
                out.insert("pageId".to_string(), v.clone());
            }
            "node_id" => {
                out.insert("nodeId".to_string(), v.clone());
            }
            "parent_id" => {
                out.insert("parentId".to_string(), v.clone());
            }
            "dry_run" => {
                out.insert("dryRun".to_string(), v.clone());
            }
            "text" if canonical_tool == "figma.create_text" => {
                out.insert("characters".to_string(), v.clone());
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }

    Value::Object(out)
}

fn resolve_external_tool_name(name: &str) -> Option<&'static str> {
    match name {
        "figma.get_document" | "get_document_info" => Some("figma.get_document"),
        "figma.get_page" | "get_page_info" => Some("figma.get_page"),
        "figma.get_selection" | "get_selection" => Some("figma.get_selection"),
        "figma.get_node" | "get_node_info" => Some("figma.get_node"),
        "figma.create_frame" | "create_frame" => Some("figma.create_frame"),
        "figma.create_text" | "create_text" => Some("figma.create_text"),
        "figma.set_fill" | "set_fill" => Some("figma.set_fill"),
        "figma.move_node" | "move_node" => Some("figma.move_node"),
        "figma.delete_node" | "delete_node" => Some("figma.delete_node"),
        _ => None,
    }
}

fn canonical_to_legacy_tool(name: &str) -> &str {
    match name {
        "figma.get_document" => "get_document_info",
        "figma.get_page" => "get_page_info",
        "figma.get_selection" => "get_selection",
        "figma.get_node" => "get_node_info",
        "figma.create_frame" => "create_frame",
        "figma.create_text" => "create_text",
        "figma.set_fill" => "set_fill",
        "figma.move_node" => "move_node",
        "figma.delete_node" => "delete_node",
        _ => name,
    }
}

fn map_bridge_error(err: anyhow::Error) -> (String, String, Value) {
    let message = err.to_string();
    if message.contains("Bridge unavailable") || message.contains("No plugin connected") {
        error_tuple("provider_unavailable", message, json!({}))
    } else if message.contains("timeout") {
        error_tuple("timeout", message, json!({}))
    } else if message.contains("Node not found") || message.contains("Page not found") {
        error_tuple("not_found", message, json!({}))
    } else {
        error_tuple("internal_error", message, json!({}))
    }
}

fn normalize_result(
    canonical_tool: &str,
    input: &serde_json::Map<String, Value>,
    raw: Value,
) -> std::result::Result<Value, (String, String, Value)> {
    match canonical_tool {
        "figma.get_document" => Ok(json!({
            "document_id": raw.get("id").cloned().unwrap_or(Value::Null),
            "name": raw.get("name").cloned().unwrap_or(Value::Null),
            "pages": raw.get("children").and_then(|v| v.as_array()).map(|pages| pages.iter().map(|p| json!({
                "id": p.get("id").cloned().unwrap_or(Value::Null),
                "name": p.get("name").cloned().unwrap_or(Value::Null)
            })).collect::<Vec<_>>()).unwrap_or_default()
        })),
        "figma.get_page" => Ok(json!({
            "id": raw.get("id").cloned().unwrap_or(Value::Null),
            "name": raw.get("name").cloned().unwrap_or(Value::Null),
            "node_count": raw.get("childCount").cloned().unwrap_or(Value::Null)
        })),
        "figma.get_selection" => {
            let nodes = raw
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|node| {
                    json!({
                        "id": node.get("id").cloned().unwrap_or(Value::Null),
                        "name": node.get("name").cloned().unwrap_or(Value::Null),
                        "type": node.get("type").cloned().unwrap_or(Value::Null)
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({"nodes": nodes}))
        }
        "figma.get_node" => Ok(json!({
            "id": raw.get("id").cloned().unwrap_or(Value::Null),
            "name": raw.get("name").cloned().unwrap_or(Value::Null),
            "type": raw.get("type").cloned().unwrap_or(Value::Null),
            "parent_id": raw.get("parentId").cloned().unwrap_or(Value::Null),
            "x": raw.get("x").cloned().unwrap_or(Value::Null),
            "y": raw.get("y").cloned().unwrap_or(Value::Null),
            "width": raw.get("width").cloned().unwrap_or(Value::Null),
            "height": raw.get("height").cloned().unwrap_or(Value::Null)
        })),
        "figma.create_frame" => Ok(json!({
            "node_id": raw.get("id").cloned().unwrap_or(Value::Null),
            "name": raw.get("name").cloned().unwrap_or(Value::Null),
            "type": "FRAME"
        })),
        "figma.create_text" => Ok(json!({
            "node_id": raw.get("id").cloned().unwrap_or(Value::Null),
            "type": "TEXT",
            "text": input.get("text").cloned().unwrap_or(Value::Null)
        })),
        "figma.set_fill" => Ok(json!({
            "node_id": input.get("node_id").cloned().unwrap_or(Value::Null),
            "fill_applied": raw.get("success").and_then(|v| v.as_bool()).unwrap_or(true)
        })),
        "figma.move_node" => Ok(json!({
            "node_id": input.get("node_id").cloned().unwrap_or(Value::Null),
            "x": input.get("x").cloned().unwrap_or(Value::Null),
            "y": input.get("y").cloned().unwrap_or(Value::Null)
        })),
        "figma.delete_node" => Ok(json!({
            "node_id": input.get("node_id").cloned().unwrap_or(Value::Null),
            "deleted": raw.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
            "dry_run": input.get("dry_run").cloned().unwrap_or(Value::Bool(false))
        })),
        _ => Err(error_tuple(
            "unknown_tool",
            format!("Unsupported canonical tool: {canonical_tool}"),
            json!({}),
        )),
    }
}

#[allow(dead_code)]
fn validate_args_against_schema(
    desc: &ToolDescription,
    args: &serde_json::Map<String, Value>,
) -> Result<()> {
    let required = desc
        .input_schema
        .get("required")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for field in required.iter().filter_map(|v| v.as_str()) {
        if field == "channel" {
            continue;
        }
        if !args.contains_key(field) {
            return Err(anyhow!("Missing required argument: {field}"));
        }
    }

    let properties = desc
        .input_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    for (name, value) in args {
        let Some(prop) = properties.get(name) else {
            continue;
        };
        let Some(expected) = prop.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        let ok = match expected {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => true,
        };
        if !ok {
            return Err(anyhow!(
                "Invalid argument type for {name}: expected {expected}"
            ));
        }
    }

    Ok(())
}

fn plugin_command_name(tool: &str) -> String {
    match tool {
        "get_node_info" => "getNode".to_string(),
        "set_font" => "setFontName".to_string(),
        _ => snake_to_camel(tool),
    }
}

fn snake_to_camel(value: &str) -> String {
    let mut out = String::new();
    let mut uppercase_next = false;
    for ch in value.chars() {
        if ch == '_' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            out.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn provider_manifest() -> Manifest {
    Manifest {
        name: "figma".to_string(),
        version: "0.1.0".to_string(),
        display_name: "Figma Tool Surface".to_string(),
        description: "External Lonis provider for interacting with a live Figma session"
            .to_string(),
        provider_type: "external-executable".to_string(),
        protocol_version: "0".to_string(),
        runtime: json!({
            "language": "rust",
            "entrypoint": "figma-provider"
        }),
        tools: canonical_tool_names()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        capabilities: vec![
            "diagnostics".to_string(),
            "external_application".to_string(),
            "mutable_operations".to_string(),
        ],
    }
}

fn canonical_tool_names() -> &'static [&'static str] {
    &[
        "figma.get_document",
        "figma.get_page",
        "figma.get_selection",
        "figma.get_node",
        "figma.create_frame",
        "figma.create_text",
        "figma.set_fill",
        "figma.move_node",
        "figma.delete_node",
    ]
}

fn get_canonical_tool_contract(tool: &str) -> Option<Value> {
    let tool = resolve_external_tool_name(tool)?;
    Some(match tool {
        "figma.get_document" => json!({
            "name": "figma.get_document",
            "provider": "figma",
            "schema_version": "1",
            "description": "Get metadata and pages for the active Figma document",
            "input_schema": {
                "type": "object",
                "properties": { "session_id": { "type": "string" } },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["document_id", "name", "pages"],
                "properties": {
                    "document_id": { "type": "string" },
                    "name": { "type": "string" },
                    "pages": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "name"],
                            "properties": {
                                "id": { "type": "string" },
                                "name": { "type": "string" }
                            }
                        }
                    }
                }
            },
            "capabilities": ["read_external_application"],
            "side_effects": [],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.get_page" => json!({
            "name": "figma.get_page",
            "provider": "figma",
            "schema_version": "1",
            "description": "Get metadata for a page in the active Figma document",
            "input_schema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "page_id": { "type": "string" }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["id", "name"],
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "node_count": { "type": "number" }
                }
            },
            "capabilities": ["read_external_application"],
            "side_effects": [],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.get_selection" => json!({
            "name": "figma.get_selection",
            "provider": "figma",
            "schema_version": "1",
            "description": "Get currently selected nodes in the active Figma session",
            "input_schema": {
                "type": "object",
                "properties": { "session_id": { "type": "string" } },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["nodes"],
                "properties": {
                    "nodes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "name", "type"],
                            "properties": {
                                "id": { "type": "string" },
                                "name": { "type": "string" },
                                "type": { "type": "string" }
                            }
                        }
                    }
                }
            },
            "capabilities": ["read_external_application"],
            "side_effects": [],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.get_node" => json!({
            "name": "figma.get_node",
            "provider": "figma",
            "schema_version": "1",
            "description": "Get detailed information for a Figma node by ID",
            "input_schema": {
                "type": "object",
                "required": ["node_id"],
                "properties": {
                    "session_id": { "type": "string" },
                    "node_id": { "type": "string" }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["id", "name", "type"],
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "type": { "type": "string" },
                    "parent_id": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "width": { "type": "number" },
                    "height": { "type": "number" }
                }
            },
            "capabilities": ["read_external_application"],
            "side_effects": [],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.create_frame" => json!({
            "name": "figma.create_frame",
            "provider": "figma",
            "schema_version": "1",
            "description": "Create a Figma frame",
            "input_schema": {
                "type": "object",
                "required": ["name", "x", "y", "width", "height"],
                "properties": {
                    "session_id": { "type": "string" },
                    "parent_id": { "type": "string" },
                    "name": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "width": { "type": "number" },
                    "height": { "type": "number" }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["node_id", "name", "type"],
                "properties": {
                    "node_id": { "type": "string" },
                    "name": { "type": "string" },
                    "type": { "type": "string" }
                }
            },
            "capabilities": ["mutates_external_application"],
            "side_effects": ["writes_figma_document"],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.create_text" => json!({
            "name": "figma.create_text",
            "provider": "figma",
            "schema_version": "1",
            "description": "Create a Figma text node",
            "input_schema": {
                "type": "object",
                "required": ["text", "x", "y"],
                "properties": {
                    "session_id": { "type": "string" },
                    "parent_id": { "type": "string" },
                    "text": { "type": "string" },
                    "name": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["node_id", "type", "text"],
                "properties": {
                    "node_id": { "type": "string" },
                    "type": { "type": "string" },
                    "text": { "type": "string" }
                }
            },
            "capabilities": ["mutates_external_application"],
            "side_effects": ["writes_figma_document"],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.set_fill" => json!({
            "name": "figma.set_fill",
            "provider": "figma",
            "schema_version": "1",
            "description": "Set the fill color of a Figma node",
            "input_schema": {
                "type": "object",
                "required": ["node_id", "color"],
                "properties": {
                    "session_id": { "type": "string" },
                    "node_id": { "type": "string" },
                    "color": {
                        "type": "object",
                        "required": ["r", "g", "b"],
                        "properties": {
                            "r": { "type": "number" },
                            "g": { "type": "number" },
                            "b": { "type": "number" },
                            "a": { "type": "number" }
                        }
                    }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["node_id"],
                "properties": {
                    "node_id": { "type": "string" },
                    "fill_applied": { "type": "boolean" }
                }
            },
            "capabilities": ["mutates_external_application"],
            "side_effects": ["writes_figma_document"],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.move_node" => json!({
            "name": "figma.move_node",
            "provider": "figma",
            "schema_version": "1",
            "description": "Move a Figma node to a new position",
            "input_schema": {
                "type": "object",
                "required": ["node_id", "x", "y"],
                "properties": {
                    "session_id": { "type": "string" },
                    "node_id": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["node_id", "x", "y"],
                "properties": {
                    "node_id": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                }
            },
            "capabilities": ["mutates_external_application"],
            "side_effects": ["writes_figma_document"],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": false,
                "requires_confirmation": false,
                "supports_dry_run": false
            }
        }),
        "figma.delete_node" => json!({
            "name": "figma.delete_node",
            "provider": "figma",
            "schema_version": "1",
            "description": "Delete a node from the active Figma document",
            "input_schema": {
                "type": "object",
                "required": ["node_id", "confirm"],
                "properties": {
                    "session_id": { "type": "string" },
                    "node_id": { "type": "string" },
                    "confirm": { "type": "boolean" },
                    "dry_run": { "type": "boolean" }
                },
                "additionalProperties": false
            },
            "output_schema": {
                "type": "object",
                "required": ["node_id"],
                "properties": {
                    "node_id": { "type": "string" },
                    "deleted": { "type": "boolean" },
                    "dry_run": { "type": "boolean" }
                }
            },
            "capabilities": ["mutates_external_application"],
            "side_effects": ["writes_figma_document", "deletes_user_content"],
            "determinism": "deterministic",
            "cost": "low",
            "verification": { "status": "verified" },
            "safety": {
                "destructive": true,
                "requires_confirmation": true,
                "supports_dry_run": true,
                "policy_tags": ["shared_resource_risk"]
            }
        }),
        _ => return None,
    })
}

fn bridge_http_base() -> String {
    std::env::var("FIGMA_BRIDGE_HTTP_URL")
        .unwrap_or_else(|_| format!("http://127.0.0.1:{}", DEFAULT_HTTP_PORT))
}

fn fetch_health() -> Result<BridgeHealthResponse> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}/health", bridge_http_base()))
        .send()?;
    Ok(response.error_for_status()?.json()?)
}

fn fetch_sessions() -> Result<Vec<BridgeSession>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}/sessions", bridge_http_base()))
        .send()?;
    let body: BridgeSessionsResponse = response.error_for_status()?.json()?;
    Ok(body.sessions)
}

fn infer_channel(sessions: &[BridgeSession]) -> Result<String> {
    match sessions {
        [session] => Ok(session.channel.clone()),
        [] => Err(anyhow!("No connected plugin sessions")),
        _ => Err(anyhow!(
            "Multiple plugin sessions active; channel must be specified"
        )),
    }
}

fn send_bridge_command(channel: &str, command: &str, params: Value) -> Result<Value> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("{}/command/{}", bridge_http_base(), channel))
        .json(&json!({
            "command": command,
            "params": params,
            "timeout_ms": 30_000
        }))
        .send()?;
    let value: Value = response.error_for_status()?.json()?;
    if value.get("success") == Some(&Value::Bool(true)) {
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(anyhow!(
            value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Bridge command failed")
                .to_string()
        ))
    }
}
#[allow(dead_code, clippy::needless_return)]
fn get_tool_description(tool: &str) -> Option<ToolDescription> {
    match tool {
        "join_channel" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description:
                    "Connect to a Figma plugin channel. Required before using other tools."
                        .to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "url": { "type": "string" } } }),
            });
        }
        "get_document_info" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get information about the current Figma document including pages"
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        "get_page_info" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get information about a specific page or the current page"
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" } } }),
            });
        }
        "get_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get the currently selected nodes in Figma".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "includeGeometry": { "type": "boolean" } } }),
            });
        }
        "set_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set the current selection to specific nodes".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } } } }),
            });
        }
        "get_node_info" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get detailed information about a specific node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "includeChildren": { "type": "string" } } }),
            });
        }
        "find_nodes_by_name" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Find nodes by name pattern".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "type": { "type": "string" } } }),
            });
        }
        "create_frame" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new frame with optional auto-layout".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" }, "parentId": { "type": "string" }, "layoutMode": { "type": "string" }, "padding": { "type": "number" }, "gap": { "type": "number" } } }),
            });
        }
        "create_rectangle" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new rectangle".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_ellipse" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new ellipse or circle".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_text" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new text node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "text": { "type": "string" }, "fontSize": { "type": "number" }, "fontColor": { "type": "object" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_line" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new line".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x1": { "type": "string" }, "y1": { "type": "string" }, "x2": { "type": "string" }, "y2": { "type": "string" }, "parentId": { "type": "string" } } }),
            });
        }
        "delete_node" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Delete a node. Requires MCP server startup with --allow-destructive-tools and confirm=true".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "confirm": { "type": "boolean" }, "dryRun": { "type": "boolean" }, "reason": { "type": "string" } } }),
            });
        }
        "set_node_name" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Rename a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "name": { "type": "string" } } }),
            });
        }
        "move_node" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Move a node to a new position".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" } } }),
            });
        }
        "resize_node" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Resize a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "width": { "type": "number" }, "height": { "type": "number" } } }),
            });
        }
        "set_fill" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set the fill color of a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "color": { "type": "object" }, "opacity": { "type": "string" } } }),
            });
        }
        "set_stroke" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set the stroke/border of a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "color": { "type": "object" }, "weight": { "type": "string" } } }),
            });
        }
        "set_corner_radius" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set corner radius for a rectangle or frame".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "radius": { "type": "number" }, "topLeft": { "type": "string" }, "topRight": { "type": "string" }, "bottomLeft": { "type": "string" }, "bottomRight": { "type": "string" } } }),
            });
        }
        "set_effects" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Add effects like drop shadows to a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "dropShadow": { "type": "string" }, "innerShadow": { "type": "string" } } }),
            });
        }
        "set_auto_layout" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Enable or modify auto-layout on a frame".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "layoutMode": { "type": "string" }, "padding": { "type": "number" }, "gap": { "type": "number" }, "alignment": { "type": "string" } } }),
            });
        }
        "reorder_children" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Reorder children within a parent node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "parentId": { "type": "string" }, "childIds": { "type": "array", "items": { "type": "string" } } } }),
            });
        }
        "create_component" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Convert selected nodes into a reusable component".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "name": { "type": "string" }, "description": { "type": "string" } } }),
            });
        }
        "create_component_set" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Combine multiple components into a component set with variants"
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "componentIds": { "type": "array", "items": { "type": "string" } }, "name": { "type": "string" }, "propertyName": { "type": "string" } } }),
            });
        }
        "create_instance" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create an instance from a component".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "componentId": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "parentId": { "type": "string" } } }),
            });
        }
        "detach_instance" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Detach an instance to make it editable".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "instanceId": { "type": "string" } } }),
            });
        }
        "swap_component" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Swap an instance to use a different component".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "instanceId": { "type": "string" }, "newComponentId": { "type": "string" } } }),
            });
        }
        "reset_instance" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Reset all overrides on an instance to match the component"
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "instanceId": { "type": "string" } } }),
            });
        }
        "set_component_property" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set a variant property on a component instance".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "instanceId": { "type": "string" }, "propertyName": { "type": "string" }, "value": { "type": "string" } } }),
            });
        }
        "set_constraints" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set resize constraints for responsive design".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "horizontal": { "type": "string" }, "vertical": { "type": "string" } } }),
            });
        }
        "set_layout_grow" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set grow and shrink behavior in auto-layout".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "grow": { "type": "string" }, "shrink": { "type": "string" } } }),
            });
        }
        "export_node" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Export a node as an image or vector file".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "format": { "type": "string" }, "scale": { "type": "string" }, "suffix": { "type": "string" } } }),
            });
        }
        "export_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Export the current selection".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "format": { "type": "string" }, "scale": { "type": "string" } } }),
            });
        }
        "boolean_union" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Merge multiple shapes into one".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "parentId": { "type": "string" } } }),
            });
        }
        "boolean_subtract" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Subtract shapes from the first shape".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "parentId": { "type": "string" } } }),
            });
        }
        "boolean_intersect" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Keep only the overlapping area of shapes".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "parentId": { "type": "string" } } }),
            });
        }
        "boolean_exclude" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Remove the overlapping area of shapes".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "parentId": { "type": "string" } } }),
            });
        }
        "flatten_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Flatten vector nodes into a single layer".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } } } }),
            });
        }
        "outline_stroke" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Convert a stroke to a vector outline".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "group_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Group multiple nodes together".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "name": { "type": "string" } } }),
            });
        }
        "ungroup" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Ungroup a group".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "groupId": { "type": "string" } } }),
            });
        }
        "wrap_in_frame" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Wrap nodes in a new frame".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "name": { "type": "string" }, "padding": { "type": "number" } } }),
            });
        }
        "bring_to_front" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Bring node to front of layer stack".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "send_to_back" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Send node to back of layer stack".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "bring_forward" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Bring node forward one layer".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "send_backward" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Send node backward one layer".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "set_text_content" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Update the text content of a text node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "text": { "type": "string" } } }),
            });
        }
        "set_font" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Change the font family and style".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "family": { "type": "string" }, "style": { "type": "string" } } }),
            });
        }
        "set_font_size" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Change the font size".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "fontSize": { "type": "number" } } }),
            });
        }
        "set_font_weight" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Change the font weight".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "fontWeight": { "type": "number" } } }),
            });
        }
        "set_line_height" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set the line height/leading".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "lineHeight": { "type": "number" } } }),
            });
        }
        "set_letter_spacing" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set the letter spacing (tracking)".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "letterSpacing": { "type": "number" } } }),
            });
        }
        "set_paragraph_spacing" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set the spacing between paragraphs".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "paragraphSpacing": { "type": "number" } } }),
            });
        }
        "set_text_alignment" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set text alignment (horizontal and/or vertical)".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "horizontal": { "type": "string" }, "vertical": { "type": "string" } } }),
            });
        }
        "set_text_decoration" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set text decoration (underline, strikethrough)".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "decoration": { "type": "string" } } }),
            });
        }
        "set_text_case" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set text case transformation".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "textCase": { "type": "string" } } }),
            });
        }
        "set_text_auto_resize" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set text auto resize behavior".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "autoResize": { "type": "string" } } }),
            });
        }
        "set_text_hyperlink" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Add a hyperlink to text or a range of text".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "url": { "type": "string" }, "start": { "type": "string" }, "end": { "type": "string" } } }),
            });
        }
        "get_font_list" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get list of available fonts in the document. Returns a summarized preview instead of dumping the full font catalog into the MCP response.".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "familyContains": { "type": "string" }, "styleContains": { "type": "string" }, "limit": { "type": "string" } } }),
            });
        }
        "create_vector" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a custom vector shape from SVG path data".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "path": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_polygon" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a polygon with N sides".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "radius": { "type": "number" }, "sides": { "type": "string" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_star" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a star shape".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "outerRadius": { "type": "string" }, "innerRadius": { "type": "string" }, "points": { "type": "string" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_arrow" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create an arrow line with arrowhead".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x1": { "type": "string" }, "y1": { "type": "string" }, "x2": { "type": "string" }, "y2": { "type": "string" }, "arrowHead": { "type": "string" }, "parentId": { "type": "string" } } }),
            });
        }
        "create_section" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a section container (for organizing designs)".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" }, "color": { "type": "object" } } }),
            });
        }
        "set_gradient_fill" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Apply a gradient fill to a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "type": { "type": "string" }, "stops": { "type": "string" }, "angle": { "type": "string" } } }),
            });
        }
        "remove_fill" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Remove all fills from a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "remove_stroke" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Remove all strokes from a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "remove_effects" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Remove all effects from a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "copy_paste_style" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Copy styles from one node to another".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "sourceNodeId": { "type": "string" }, "targetNodeIds": { "type": "array", "items": { "type": "string" } } } }),
            });
        }
        "create_prototype_link" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create an interaction link between frames".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "sourceFrameId": { "type": "string" }, "targetFrameId": { "type": "string" }, "trigger": { "type": "string" }, "transition": { "type": "string" }, "duration": { "type": "string" } } }),
            });
        }
        "remove_prototype_link" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Remove prototype interaction from a node. Requires MCP server startup with --allow-destructive-tools and confirm=true".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "confirm": { "type": "boolean" }, "dryRun": { "type": "boolean" }, "reason": { "type": "string" } } }),
            });
        }
        "set_prototype_start" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set a frame as the prototype starting point".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "frameId": { "type": "string" }, "flowName": { "type": "string" } } }),
            });
        }
        "set_scroll_behavior" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set scroll behavior for a frame".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "frameId": { "type": "string" }, "direction": { "type": "string" } } }),
            });
        }
        "set_clip_content" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set whether a frame clips content outside its bounds".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "clip": { "type": "string" } } }),
            });
        }
        "create_variable" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new design variable".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "type": { "type": "string" }, "value": { "type": "string" }, "collectionId": { "type": "string" } } }),
            });
        }
        "set_variable_value" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Update a variable's value".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "variableId": { "type": "string" }, "value": { "type": "string" } } }),
            });
        }
        "apply_variable" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Apply a variable to a node property".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "variableId": { "type": "string" }, "property": { "type": "string" } } }),
            });
        }
        "enable_library" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Enable a library for the current file".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "libraryId": { "type": "string" } } }),
            });
        }
        "disable_library" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Disable a library for the current file".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "libraryId": { "type": "string" } } }),
            });
        }
        "import_component_from_library" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Import a component from a team library".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "libraryId": { "type": "string" }, "componentKey": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" } } }),
            });
        }
        "create_page" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Create a new page".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "name": { "type": "string" }, "index": { "type": "string" } } }),
            });
        }
        "delete_page" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Delete a page. Requires MCP server startup with --allow-destructive-tools and confirm=true".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" }, "confirm": { "type": "boolean" }, "dryRun": { "type": "boolean" }, "reason": { "type": "string" } } }),
            });
        }
        "rename_page" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Rename a page".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" }, "name": { "type": "string" } } }),
            });
        }
        "duplicate_page" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Duplicate a page".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" } } }),
            });
        }
        "set_current_page" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Switch to a specific page".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" } } }),
            });
        }
        "set_page_background" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Set page background color".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" }, "color": { "type": "object" } } }),
            });
        }
        "create_comment" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Add a comment to the canvas".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "x": { "type": "number" }, "y": { "type": "number" }, "message": { "type": "string" }, "parentId": { "type": "string" } } }),
            });
        }
        "get_comments" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get all comments on a page".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "pageId": { "type": "string" } } }),
            });
        }
        "resolve_comment" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Mark a comment as resolved".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "commentId": { "type": "string" } } }),
            });
        }
        "delete_comment" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Delete a comment. Requires MCP server startup with --allow-destructive-tools and confirm=true".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "commentId": { "type": "string" }, "confirm": { "type": "boolean" }, "dryRun": { "type": "boolean" }, "reason": { "type": "string" } } }),
            });
        }
        "select_all" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Select all nodes on the current page".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "type": { "type": "string" } } }),
            });
        }
        "select_by_type" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Select all nodes of a specific type".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "type": { "type": "string" } } }),
            });
        }
        "get_parent_chain" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get the parent hierarchy of a node".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "get_siblings" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get sibling nodes".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "is_node_visible" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Check if a node is visible in the viewport".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" } } }),
            });
        }
        "duplicate_nodes" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Duplicate multiple nodes".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeIds": { "type": "array", "items": { "type": "string" } }, "count": { "type": "string" }, "offsetX": { "type": "string" }, "offsetY": { "type": "string" } } }),
            });
        }
        "delete_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Delete all selected nodes. Requires MCP server startup with --allow-destructive-tools and confirm=true".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "confirm": { "type": "boolean" }, "dryRun": { "type": "boolean" }, "reason": { "type": "string" } } }),
            });
        }
        "align_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Align selected nodes".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "alignment": { "type": "string" } } }),
            });
        }
        "distribute_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Distribute selected nodes evenly".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "direction": { "type": "string" }, "spacing": { "type": "string" } } }),
            });
        }
        "resize_to_fit" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Resize a frame to fit its content".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "nodeId": { "type": "string" }, "padding": { "type": "number" } } }),
            });
        }
        "scale_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Scale selected nodes by a factor".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" }, "scale": { "type": "string" } } }),
            });
        }
        "get_local_components" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get all local components in the document".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        "get_prototype_flows" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get all prototype flows in the document".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        "get_variables" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get all design variables in the document".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        "get_variable_collections" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get all variable collections".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        "get_team_libraries" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Get available team libraries".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        "invert_selection" => {
            return Some(ToolDescription {
                name: tool.to_string(),
                description: "Invert the current selection".to_string(),
                input_schema: json!({ "type": "object", "properties": { "channel": { "type": "string" } } }),
            });
        }
        _ => None,
    }
}
#[allow(dead_code)]
fn get_tool_names() -> Vec<String> {
    vec![
        "join_channel".to_string(),
        "get_document_info".to_string(),
        "get_page_info".to_string(),
        "get_selection".to_string(),
        "set_selection".to_string(),
        "get_node_info".to_string(),
        "find_nodes_by_name".to_string(),
        "create_frame".to_string(),
        "create_rectangle".to_string(),
        "create_ellipse".to_string(),
        "create_text".to_string(),
        "create_line".to_string(),
        "delete_node".to_string(),
        "set_node_name".to_string(),
        "move_node".to_string(),
        "resize_node".to_string(),
        "set_fill".to_string(),
        "set_stroke".to_string(),
        "set_corner_radius".to_string(),
        "set_effects".to_string(),
        "set_auto_layout".to_string(),
        "reorder_children".to_string(),
        "create_component".to_string(),
        "create_component_set".to_string(),
        "create_instance".to_string(),
        "detach_instance".to_string(),
        "swap_component".to_string(),
        "get_local_components".to_string(),
        "reset_instance".to_string(),
        "set_component_property".to_string(),
        "set_constraints".to_string(),
        "set_layout_grow".to_string(),
        "export_node".to_string(),
        "export_selection".to_string(),
        "boolean_union".to_string(),
        "boolean_subtract".to_string(),
        "boolean_intersect".to_string(),
        "boolean_exclude".to_string(),
        "flatten_selection".to_string(),
        "outline_stroke".to_string(),
        "group_selection".to_string(),
        "ungroup".to_string(),
        "wrap_in_frame".to_string(),
        "bring_to_front".to_string(),
        "send_to_back".to_string(),
        "bring_forward".to_string(),
        "send_backward".to_string(),
        "set_text_content".to_string(),
        "set_font".to_string(),
        "set_font_size".to_string(),
        "set_font_weight".to_string(),
        "set_line_height".to_string(),
        "set_letter_spacing".to_string(),
        "set_paragraph_spacing".to_string(),
        "set_text_alignment".to_string(),
        "set_text_decoration".to_string(),
        "set_text_case".to_string(),
        "set_text_auto_resize".to_string(),
        "set_text_hyperlink".to_string(),
        "get_font_list".to_string(),
        "create_vector".to_string(),
        "create_polygon".to_string(),
        "create_star".to_string(),
        "create_arrow".to_string(),
        "create_section".to_string(),
        "set_gradient_fill".to_string(),
        "remove_fill".to_string(),
        "remove_stroke".to_string(),
        "remove_effects".to_string(),
        "copy_paste_style".to_string(),
        "create_prototype_link".to_string(),
        "remove_prototype_link".to_string(),
        "set_prototype_start".to_string(),
        "get_prototype_flows".to_string(),
        "set_scroll_behavior".to_string(),
        "set_clip_content".to_string(),
        "get_variables".to_string(),
        "get_variable_collections".to_string(),
        "create_variable".to_string(),
        "set_variable_value".to_string(),
        "apply_variable".to_string(),
        "get_team_libraries".to_string(),
        "enable_library".to_string(),
        "disable_library".to_string(),
        "import_component_from_library".to_string(),
        "create_page".to_string(),
        "delete_page".to_string(),
        "rename_page".to_string(),
        "duplicate_page".to_string(),
        "set_current_page".to_string(),
        "set_page_background".to_string(),
        "create_comment".to_string(),
        "get_comments".to_string(),
        "resolve_comment".to_string(),
        "delete_comment".to_string(),
        "select_all".to_string(),
        "select_by_type".to_string(),
        "invert_selection".to_string(),
        "get_parent_chain".to_string(),
        "get_siblings".to_string(),
        "is_node_visible".to_string(),
        "duplicate_nodes".to_string(),
        "delete_selection".to_string(),
        "align_selection".to_string(),
        "distribute_selection".to_string(),
        "resize_to_fit".to_string(),
        "scale_selection".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

    #[test]
    fn every_tool_has_a_description() {
        for tool in get_tool_names() {
            assert!(
                get_tool_description(&tool).is_some(),
                "missing description for {tool}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bridge_round_trip_dispatches_plugin_commands() {
        let ws_port = 4055;
        let http_port = 4056;

        let bridge_task =
            tokio::spawn(async move { run_bridge(ws_port, http_port).await.unwrap() });
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let plugin_task = tokio::spawn(async move {
            let (mut ws, _) = connect_async(format!("ws://127.0.0.1:{ws_port}"))
                .await
                .unwrap();

            let _ = ws.next().await; // welcome ack

            let join = json!({
                "id": "join-1",
                "type": "join",
                "channel": "TESTCHAN",
                "timestamp": 1,
                "payload": {
                    "sessionId": "TESTCHAN",
                    "client": { "name": "test-plugin", "version": "1.0" }
                }
            });
            ws.send(Message::Text(join.to_string().into()))
                .await
                .unwrap();
            let _ = ws.next().await; // join ack

            let msg = ws.next().await.unwrap().unwrap();
            let Message::Text(text) = msg else {
                panic!("expected text frame")
            };
            let env: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(env.get("type").and_then(|v| v.as_str()), Some("message"));
            let payload = env.get("payload").unwrap();
            assert_eq!(
                payload.get("command").and_then(|v| v.as_str()),
                Some("getDocumentInfo")
            );
            let request_id = payload
                .get("requestId")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();

            let response = json!({
                "id": "resp-1",
                "type": "message",
                "channel": "TESTCHAN",
                "timestamp": 2,
                "payload": {
                    "requestId": request_id,
                    "result": { "document": "ok" }
                }
            });
            ws.send(Message::Text(response.to_string().into()))
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();
        let response: Value = client
            .post(format!("http://127.0.0.1:{http_port}/command/TESTCHAN"))
            .json(&json!({
                "command": "getDocumentInfo",
                "params": {},
                "timeout_ms": 30_000
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response.get("success"), Some(&Value::Bool(true)));
        assert_eq!(
            response
                .get("result")
                .and_then(|v| v.get("document"))
                .and_then(|v| v.as_str()),
            Some("ok")
        );

        plugin_task.await.unwrap();
        bridge_task.abort();
    }
}
