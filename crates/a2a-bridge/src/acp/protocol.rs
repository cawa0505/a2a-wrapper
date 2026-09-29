//! ACP wire dialect — JSON-RPC 2.0 envelope + Agent Client Protocol types
//! (client side of the dialect; the agent process is the server).
//!
//! Hand-rolled with `serde_json` per house style (no `serde` derive
//! dependency, mirroring `transport/`). Every typed shape is limited to what
//! `docs/acp-agent-profiles.md` verified against `opencode acp` 1.18.32;
//! shapes not yet observed stay `Value` pass-through with a TODO naming the
//! missing fact — never invented.

use serde_json::{json, Value};

/// ACP protocol version negotiated in `initialize` — verified integer `1`.
pub const PROTOCOL_VERSION: u64 = 1;

/// JSON-RPC request id, mirroring serde's `#[serde(untagged)]` semantics by
/// hand (the `serde` derive crate is not a dependency of this workspace).
///
/// Rationale: each ACP side mints its own id space and the JSON-RPC spec
/// allows `number | string`. opencode's server-initiated requests use
/// integers starting at `0` (verified: `session/request_permission` arrived
/// with `"id": 0`), while clients typically use increasing integers. Trying
/// `u64` then `i64` then `String` reproduces untagged resolution exactly:
/// non-negative numbers land in `U64`, negatives fall through to `I64`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestId {
    U64(u64),
    I64(i64),
    String(String),
}

impl RequestId {
    pub fn from_value(v: &Value) -> Option<RequestId> {
        if let Some(n) = v.as_u64() {
            return Some(RequestId::U64(n));
        }
        if let Some(n) = v.as_i64() {
            return Some(RequestId::I64(n));
        }
        v.as_str().map(|s| RequestId::String(s.to_owned()))
    }

    pub fn to_value(&self) -> Value {
        match self {
            RequestId::U64(n) => json!(n),
            RequestId::I64(n) => json!(n),
            RequestId::String(s) => json!(s),
        }
    }
}

/// JSON-RPC error object. Verified in the wild: opencode answers a
/// `session/load` with an unknown sessionId
/// `{"code":-32603,"message":"Internal error: OpenCode service failure",
/// "data":{"service":"session"}}` — hence `data` is carried as-is.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn from_value(v: &Value) -> Option<JsonRpcError> {
        Some(JsonRpcError {
            code: v.get("code")?.as_i64()?,
            message: v.get("message")?.as_str()?.to_owned(),
            data: v.get("data").cloned(),
        })
    }
}

/// One inbound frame, classified by JSON-RPC shape: a response has `id` plus
/// `result`/`error`; a server→client request has `id` plus `method`; a
/// notification has `method` but no `id`.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Response {
        id: RequestId,
        result: Option<Value>,
        error: Option<JsonRpcError>,
    },
    ServerRequest {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// Neither a request, notification, nor response — kept out of routing.
    Ignored,
}

/// Parse and classify one NDJSON line. A line that is not valid JSON yields
/// `None` (the caller logs and skips; opencode never emits such lines in
/// practice, but stdin noise must not kill the reader).
pub fn classify(line: &str) -> Option<Incoming> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return None,
    };
    let method = v.get("method").and_then(Value::as_str).map(String::from);
    let id = v.get("id");
    let has_payload = v.get("result").is_some() || v.get("error").is_some();
    match (method, id) {
        (Some(method), Some(id)) => {
            let id = RequestId::from_value(id)?;
            if has_payload {
                Some(Incoming::Response {
                    id,
                    result: v.get("result").cloned(),
                    error: v.get("error").and_then(JsonRpcError::from_value),
                })
            } else {
                Some(Incoming::ServerRequest {
                    id,
                    method,
                    params: v.get("params").cloned().unwrap_or(Value::Null),
                })
            }
        }
        (Some(method), None) => Some(Incoming::Notification {
            method,
            params: v.get("params").cloned().unwrap_or(Value::Null),
        }),
        _ => Some(Incoming::Ignored),
    }
}

/// Build a client→agent request frame. `params` must already be wire-shaped
/// (the dialect builders below produce verified shapes).
pub fn request_frame(id: &RequestId, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.to_value(), "method": method, "params": params })
}

/// Build the reply to a server→client request (e.g. answering
/// `session/request_permission`). Shape verified via the permission probe.
pub fn response_frame(id: &RequestId, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.to_value(), "result": result })
}

/// `initialize` request. `clientCapabilities` pins fs/terminal to `false`
/// (verified against opencode: with fs capabilities declined, the agent
/// performs its own file I/O and no `fs/*` server requests were observed).
pub fn initialize_request(id: &RequestId) -> Value {
    request_frame(
        id,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "clientCapabilities": {
                "fs": { "readTextFile": false, "writeTextFile": false },
                "terminal": false
            }
        }),
    )
}

/// `session/new` request. `mcpServers` is always empty in v1 (verified);
/// server-side MCP item shapes are unverified and stay unmodeled.
pub fn session_new_request(id: &RequestId, cwd: &str) -> Value {
    request_frame(id, "session/new", json!({ "cwd": cwd, "mcpServers": [] }))
}

/// `session/prompt` request with a single text content block — the only
/// prompt shape verified (image/audio/resource blocks are unverified TODOs).
pub fn session_prompt_request(id: &RequestId, session_id: &str, text: &str) -> Value {
    request_frame(
        id,
        "session/prompt",
        json!({
            "sessionId": session_id,
            "prompt": [ { "type": "text", "text": text } ]
        }),
    )
}

/// `session/load` request — reconnect to a session from a previous agent
/// process (verified: opencode replays the full history as `session/update`
/// notifications before answering).
pub fn session_load_request(id: &RequestId, session_id: &str, cwd: &str) -> Value {
    request_frame(
        id,
        "session/load",
        json!({ "sessionId": session_id, "cwd": cwd, "mcpServers": [] }),
    )
}

/// `session/new` response — `sessionId` verified; `configOptions` carries the
/// model/mode pickers (verified keys: id/name/category/type/currentValue/
/// options) but has no consumer in v1, so it passes through untouched.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionNewResult {
    pub session_id: String,
    pub config_options: Value,
}

impl SessionNewResult {
    pub fn from_value(v: &Value) -> Option<SessionNewResult> {
        Some(SessionNewResult {
            session_id: v.get("sessionId")?.as_str()?.to_owned(),
            config_options: v.get("configOptions").cloned().unwrap_or(Value::Null),
        })
    }
}

/// `session/prompt` response. Verified `stopReason` value: `"end_turn"` (the
/// completion signal). Other stop reasons (max_tokens, refusal, …) are
/// unverified — the raw string is carried so later tasks can extend the map.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionPromptResult {
    pub stop_reason: String,
}

impl SessionPromptResult {
    pub fn from_value(v: &Value) -> Option<SessionPromptResult> {
        Some(SessionPromptResult {
            stop_reason: v.get("stopReason")?.as_str()?.to_owned(),
        })
    }
}

/// Content block inside message chunks. Only `text` is verified;
/// `Unknown` preserves unobserved block kinds (`image`, `audio`, …) without
/// inventing their fields.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    Text(String),
    Unknown,
}

impl ContentBlock {
    pub fn from_value(v: &Value) -> ContentBlock {
        if v.get("type").and_then(Value::as_str) == Some("text") {
            ContentBlock::Text(
                v.get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            )
        } else {
            ContentBlock::Unknown
        }
    }
}

/// `session/update` notification body. The discriminant is the nested
/// `params.update.sessionUpdate` string (verified), not the JSON-RPC method.
/// Only shapes observed on opencode 1.18.32 are modeled; anything else maps
/// to `SessionUpdate::Unknown` — TODO(plan/current_mode_update/
/// file_system_tree_update): unverified, pass through, never invent.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdateFrame {
    pub session_id: String,
    pub update: SessionUpdate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionUpdate {
    /// Verified: `messageId`, `content:{type:"text",text}`.
    UserMessageChunk {
        message_id: Option<String>,
        content: ContentBlock,
    },
    AgentMessageChunk {
        message_id: Option<String>,
        content: ContentBlock,
    },
    AgentThoughtChunk {
        message_id: Option<String>,
        content: ContentBlock,
    },
    /// Verified fields: toolCallId/title/kind/status/locations/rawInput
    /// (kind observed "edit"; status "pending"). Other kind/status values
    /// are unverified — carried as plain strings.
    ToolCall {
        tool_call_id: String,
        title: Option<String>,
        kind: Option<String>,
        status: Option<String>,
        rest: Value,
    },
    /// Terminal tool state lives here: status "completed" (with
    /// content/rawOutput) or "failed" (rawOutput.error carries the reason —
    /// verified: denied permission fails the tool call, no JSON-RPC error).
    ToolCallUpdate {
        tool_call_id: String,
        status: Option<String>,
        rest: Value,
    },
    /// Verified: `availableCommands` array of {name, description, …}.
    AvailableCommandsUpdate { commands: Value },
    /// Verified: used/size/cost{amount,currency}.
    UsageUpdate {
        used: Option<u64>,
        size: Option<u64>,
        cost: Value,
    },
    Unknown,
}

/// `initialize` response — protocol version, agent capability flags, auth
/// methods and agent info are all verified shapes (see
/// docs/acp-agent-profiles.md §1); unknown extra keys stay in `raw`.
#[derive(Debug, Clone, PartialEq)]
pub struct InitializeResult {
    pub protocol_version: u64,
    /// Verified capability: opencode advertises `loadSession: true`.
    pub load_session: bool,
    /// Verified: `agentInfo.name = "OpenCode"`, `agentInfo.version`.
    pub agent_name: Option<String>,
    pub agent_version: Option<String>,
    /// Full response body for pass-through consumers.
    pub raw: Value,
}

impl InitializeResult {
    pub fn from_value(v: &Value) -> Option<InitializeResult> {
        Some(InitializeResult {
            protocol_version: v.get("protocolVersion")?.as_u64()?,
            load_session: v
                .pointer("/agentCapabilities/loadSession")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            agent_name: v
                .pointer("/agentInfo/name")
                .and_then(Value::as_str)
                .map(String::from),
            agent_version: v
                .pointer("/agentInfo/version")
                .and_then(Value::as_str)
                .map(String::from),
            raw: v.clone(),
        })
    }
}



impl SessionUpdateFrame {
    /// Parse from an incoming notification's `params`. The decode is T1-
    /// shaped: unknown `sessionUpdate` discriminants stay `Unknown` with the
    /// raw body dropped by design (v1 has no consumer for them; dump
    /// debugging belongs to a logging task, not the type layer).
    pub fn from_params(params: &Value) -> Option<SessionUpdateFrame> {
        let session_id = params.get("sessionId")?.as_str()?.to_owned();
        let update = params.get("update")?;
        let kind = update.get("sessionUpdate").and_then(Value::as_str);
        let body = |k: &str| update.get(k).cloned();
        let su = match kind {
            Some("user_message_chunk") => SessionUpdate::UserMessageChunk {
                message_id: body("messageId").and_then(|v| v.as_str().map(String::from)),
                content: body("content")
                    .map(|c| ContentBlock::from_value(&c))
                    .unwrap_or(ContentBlock::Unknown),
            },
            Some("agent_message_chunk") => SessionUpdate::AgentMessageChunk {
                message_id: body("messageId").and_then(|v| v.as_str().map(String::from)),
                content: body("content")
                    .map(|c| ContentBlock::from_value(&c))
                    .unwrap_or(ContentBlock::Unknown),
            },
            Some("agent_thought_chunk") => SessionUpdate::AgentThoughtChunk {
                message_id: body("messageId").and_then(|v| v.as_str().map(String::from)),
                content: body("content")
                    .map(|c| ContentBlock::from_value(&c))
                    .unwrap_or(ContentBlock::Unknown),
            },
            Some("tool_call") => SessionUpdate::ToolCall {
                tool_call_id: body("toolCallId").and_then(|v| v.as_str().map(String::from))?,
                title: body("title").and_then(|v| v.as_str().map(String::from)),
                kind: body("kind").and_then(|v| v.as_str().map(String::from)),
                status: body("status").and_then(|v| v.as_str().map(String::from)),
                rest: update.clone(),
            },
            Some("tool_call_update") => SessionUpdate::ToolCallUpdate {
                tool_call_id: body("toolCallId").and_then(|v| v.as_str().map(String::from))?,
                status: body("status").and_then(|v| v.as_str().map(String::from)),
                rest: update.clone(),
            },
            Some("available_commands_update") => {
                SessionUpdate::AvailableCommandsUpdate {
                    commands: body("availableCommands").unwrap_or(Value::Null),
                }
            }
            Some("usage_update") => SessionUpdate::UsageUpdate {
                used: body("used").and_then(|v| v.as_u64()),
                size: body("size").and_then(|v| v.as_u64()),
                cost: body("cost").unwrap_or(Value::Null),
            },
            _ => SessionUpdate::Unknown,
        };
        Some(SessionUpdateFrame {
            session_id,
            update: su,
        })
    }
}

/// Server→client `session/request_permission` (verified shape, opencode
/// 1.18.32). `toolCall` mirrors the `tool_call` update body plus diff/undo
/// content; `options` carries allow/reject choices.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub session_id: String,
    pub tool_call_id: String,
    /// Verified option kinds: `allow_once`, `allow_always`, `reject_once`.
    /// Other kinds are unverified and match nothing (→ deny below).
    pub options: Vec<PermissionOption>,
    rest: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionOption {
    pub option_id: String,
    pub kind: String,
    pub name: Option<String>,
}

impl PermissionRequest {
    pub fn from_params(params: &Value) -> Option<PermissionRequest> {
        let session_id = params.get("sessionId")?.as_str()?.to_owned();
        let tool_call = params.get("toolCall")?;
        let options = params
            .get("options")?
            .as_array()?
            .iter()
            .filter_map(|o| {
                Some(PermissionOption {
                    option_id: o.get("optionId")?.as_str()?.to_owned(),
                    kind: o.get("kind")?.as_str()?.to_owned(),
                    name: o.get("name").and_then(Value::as_str).map(String::from),
                })
            })
            .collect();
        Some(PermissionRequest {
            session_id,
            tool_call_id: tool_call
                .get("toolCallId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            options,
            rest: params.clone(),
        })
    }

    /// v1 policy: deterministic deny — pick the first `reject_once` option;
    /// if the agent offers none (unverified behavior), cancel the request.
    pub fn deny_reply(&self) -> Value {
        let reject = self
            .options
            .iter()
            .find(|o| o.kind == "reject_once")
            .or_else(|| self.options.iter().find(|o| o.kind.starts_with("reject")));
        match reject {
            // Shape verified: {"outcome":"selected","optionId":"…"}.
            Some(o) => json!({ "outcome": "selected", "optionId": o.option_id }),
            // Outcome "cancelled" is the spec's other outcome kind; opencode
            // behavior for it is unverified, but there is no allow to grant.
            None => json!({ "outcome": "cancelled" }),
        }
    }
}
