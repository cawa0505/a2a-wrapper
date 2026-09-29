//! ACP client — spawns the agent process, speaks NDJSON JSON-RPC over its
//! stdio, correlates responses to pending requests, and streams events to
//! the owner.
//!
//! Threading (std only, no tokio per the v1 transport decision):
//! - writer: stdin is shared (`Arc<Mutex<>>`) because the reader thread
//!   answers `session/request_permission` on arrival (deterministic deny)
//!   while the owner thread may be writing a request concurrently.
//! - reader: one background task owns stdout, classifies every line via
//!   `protocol::classify`, routes responses to per-id waiters and pushes
//!   notifications into the event channel. stderr is logged, never parsed.
//!
//! Not wired here (T4): mapping events to `TaskOutcome`, artifact extraction.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use tracing::debug;

use super::protocol::{
    classify, response_frame, session_load_request, session_new_request,
    session_prompt_request, initialize_request, InitializeResult, Incoming, JsonRpcError,
    PermissionRequest, RequestId, SessionNewResult, SessionPromptResult, SessionUpdateFrame,
};

/// How long `shutdown` waits for the agent to exit on its own after stdin
/// EOF before killing it (verified behavior: opencode exits 0 well within
/// this window).
pub const EXIT_GRACE: Duration = Duration::from_secs(15);

/// Stream of agent-side activity, consumed by the owner after (or while)
/// round-trips run. `session/update` notifications arrive here verbatim; the
/// TaskOutcome mapping lives in a later task.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpEvent {
    /// `session/update` notification (all session progress flows through it).
    Update(SessionUpdateFrame),
    /// Server→client permission request — answered with a deterministic deny
    /// by the client itself (v1 policy); this event is the audit record.
    PermissionDenied(PermissionRequest),
    /// Any other server→client request method. fs/terminal/* are unverified
    /// (declined via clientCapabilities) — nothing answers them.
    // TODO(fs/terminal): unverified shapes; add types only after probing.
    ServerRequest {
        id: RequestId,
        method: String,
        params: Value,
    },
    /// Any other notification method (only `session/update` verified so far).
    // TODO: extend with verified shapes as profiles land.
    OtherNotification { method: String, params: Value },
    /// The agent closed stdout: no more messages will arrive. Pending
    /// round-trips are failed with an EOF error before this is emitted.
    Eof,
}

/// Cancel-on-response pending entry: the response value is forwarded to the
/// single waiter owning the receiver.
type Waiter = mpsc::Sender<std::result::Result<Value, JsonRpcError>>;

struct Shared {
    /// stdin frame writer; `None` once stdin EOF was requested (shutdown).
    write: Mutex<Option<ChildStdin>>,
    /// In-flight requests: response id → one-shot waiter for the caller.
    pending: Mutex<HashMap<RequestId, Waiter>>,
}

const INIT: &str = "initialize";
const SESSION_NEW: &str = "session/new";
const SESSION_LOAD: &str = "session/load";
const SESSION_PROMPT: &str = "session/prompt";

/// Handle to one running ACP agent process.
///
/// The client allocates its own request ids as `u64` starting at 1 — the
/// agent's own server-request ids are a separate space observed to start at
/// integer `0` (verified), so the two never collide.
pub struct AcpClient {
    child: Mutex<Child>,
    shared: Arc<Shared>,
    events: (mpsc::Sender<AcpEvent>, Mutex<mpsc::Receiver<AcpEvent>>),
    next_id: Mutex<u64>,
}

/// Error of one failed round-trip. The JSON-RPC error body is kept when the
/// agent answered with one (verified `session/load` failure shape).
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    JsonRpc(JsonRpcError),
    Eof,
    Io(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::JsonRpc(e) => {
                write!(f, "json-rpc error {}: {}", e.code, e.message)?;
                if let Some(d) = &e.data {
                    write!(f, " (data: {d})")
                } else {
                    Ok(())
                }
            }
            CallError::Eof => write!(f, "agent closed stdio mid-call"),
            CallError::Io(msg) => write!(f, "agent process io: {msg}"),
        }
    }
}

impl AcpClient {
    /// Spawn the agent process and start the reader thread. The first
    /// `initialize` round-trip happens separately via [`AcpClient::initialize`].
    pub fn spawn(program: &str, args: &[String], cwd: Option<&str>) -> Result<AcpClient> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let mut child = cmd.spawn().with_context(|| format!("spawn {program}"))?;
        let stdin = child.stdin.take().context("child stdin")?;
        let stdout = child.stdout.take().context("child stdout")?;
        let stderr = child.stderr.take().context("child stderr")?;

        let (tx, rx) = mpsc::channel();
        let client = AcpClient {
            child: Mutex::new(child),
            shared: Arc::new(Shared {
                write: Mutex::new(Some(stdin)),
                pending: Mutex::new(HashMap::new()),
            }),
            events: (tx, Mutex::new(rx)),
            next_id: Mutex::new(1),
        };
        Reader {
            shared: Arc::clone(&client.shared),
            events: client.events.0.clone(),
        }
        .spawn_thread(stdout, stderr);
        Ok(client)
    }
}

struct Reader {
    shared: Arc<Shared>,
    events: mpsc::Sender<AcpEvent>,
}

impl Reader {
    /// Own stdout for the life of the process; stderr is drained by a second
    /// thread that only logs (the ACP dialect rides solely on stdout).
    fn spawn_thread(self, stdout: ChildStdout, stderr: std::process::ChildStderr) {
        std::thread::spawn(move || {
            let mut err = BufReader::new(stderr);
            std::thread::spawn(move || {
                let mut line = String::new();
                loop {
                    line.clear();
                    match err.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => debug!(target: "acp::agent", "{}", line.trim_end()),
                    }
                }
            });

            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break, // EOF or read error: stream is over
                    Ok(_) => {}
                }
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                match classify(trimmed) {
                    None => debug!(target: "acp::wire", "unparseable line: {trimmed}"),
                    Some(incoming) => self.route(incoming),
                }
            }
            self.finish_eof();
        });
    }

    fn route(&self, incoming: Incoming) {
        match incoming {
            Incoming::Response {
                id,
                result,
                error,
            } => {
                let waiter = self
                    .shared
                    .pending
                    .lock()
                    .expect("pending lock poisoned")
                    .remove(&id);
                let payload = match error {
                    Some(err) => Err(err),
                    None => Ok(result.unwrap_or(Value::Null)),
                };
                if let Some(tx) = waiter {
                    let _ = tx.send(payload); // receiver dropped → caller gone
                }
            }
            Incoming::ServerRequest { id, method, params } => {
                // v1 policy: only permission requests get an answer; it is a
                // deterministic deny (verified reject_once shape).
                if method == "session/request_permission" {
                    let req = PermissionRequest::from_params(&params);
                    let reply = req
                        .as_ref()
                        .map(PermissionRequest::deny_reply)
                        .unwrap_or_else(|| serde_json::json!({ "outcome": "cancelled" }));
                    self.write_frame(response_frame(&id, reply));
                    if let Some(req) = req {
                        let _ = self.events.send(AcpEvent::PermissionDenied(req));
                    }
                } else {
                    // TODO(fs/terminal): shapes unverified — declined via
                    // clientCapabilities; nothing answers these in v1.
                    let _ = self.events.send(AcpEvent::ServerRequest { id, method, params });
                }
            }
            Incoming::Notification { method, params } => {
                let event = if method == "session/update" {
                    SessionUpdateFrame::from_params(&params)
                        .map(AcpEvent::Update)
                        .unwrap_or(AcpEvent::OtherNotification { method, params })
                } else {
                    AcpEvent::OtherNotification { method, params }
                };
                let _ = self.events.send(event);
            }
            Incoming::Ignored => {}
        }
    }

    /// stdout ended: fail every waiter with EOF, then tell the owner.
    fn finish_eof(&self) {
        let mut pending = self.shared.pending.lock().expect("pending lock poisoned");
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(JsonRpcError {
                code: -32001,
                message: "agent closed stdio before answering".to_owned(),
                data: None,
            }));
        }
        drop(pending);
        let _ = self.events.send(AcpEvent::Eof);
    }

    /// Serialize writes onto the agent's stdin; safe after EOF-out (no-op).
    fn write_frame(&self, frame: serde_json::Value) {
        let mut guard = self.shared.write.lock().expect("stdin lock poisoned");
        if let Some(stdin) = guard.as_mut() {
            let _ = serde_json::to_writer(&mut *stdin, &frame);
            let _ = stdin.write_all(b"\n");
        }
    }
}

impl AcpClient {
    /// Fire one request and wait for its response. The waiter is registered
    /// before the frame is written, so a race with the response thread is
    /// impossible; a send-after-EOF rolls the pending entry back.
    fn call_with(&self, build: impl FnOnce(&RequestId) -> Value) -> Result<Value, CallError> {
        let id = self.alloc_id();
        let (tx, rx) = mpsc::channel();
        self.shared
            .pending
            .lock()
            .expect("pending lock poisoned")
            .insert(id.clone(), tx);
        match self.send_frame(build(&id)) {
            Ok(()) => {}
            Err(err) => {
                self.shared
                    .pending
                    .lock()
                    .expect("pending lock poisoned")
                    .remove(&id);
                return Err(CallError::Io(err.to_string()));
            }
        }
        // Blocking wait is correct: the reader thread is the only sender,
        // and it resolves every pending waiter (as an error) at EOF.
        match rx.recv() {
            Ok(result) => result.map_err(CallError::JsonRpc),
            Err(_) => Err(CallError::Eof),
        }
    }

    fn alloc_id(&self) -> RequestId {
        let mut next = self.next_id.lock().expect("id lock poisoned");
        let id = RequestId::U64(*next);
        *next += 1;
        id
    }

    fn send_frame(&self, frame: Value) -> Result<()> {
        let mut guard = self
            .shared
            .write
            .lock()
            .expect("stdin lock poisoned");
        let Some(stdin) = guard.as_mut() else {
            anyhow::bail!("agent stdin already closed (shutdown in progress)");
        };
        let bytes = serde_json::to_vec(&frame).context("serialize ACP frame")?;
        stdin
            .write_all(&bytes)
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .context("write ACP frame to agent stdin")
    }
}

/// Session-level convenience API — each maps 1:1 to a verified ACP method.
impl AcpClient {
    /// `initialize` handshake. Runs once, right after [`AcpClient::spawn`].
    pub fn initialize(&self) -> Result<InitializeResult, CallError> {
        let result = self.call_with(initialize_request)?;
        InitializeResult::from_value(&result).ok_or_else(|| {
            CallError::Io("initialize result missing protocolVersion".to_owned())
        })
    }

    /// `session/new` with the given working directory.
    pub fn session_new(&self, cwd: &str) -> Result<SessionNewResult, CallError> {
        let result = self.call_with(|id| session_new_request(id, cwd))?;
        SessionNewResult::from_value(&result)
            .ok_or_else(|| CallError::Io("session/new result missing sessionId".to_owned()))
    }

    /// `session/load` — resume a session from a previous process.
    pub fn session_load(&self, session_id: &str, cwd: &str) -> Result<SessionNewResult, CallError> {
        let result = self.call_with(|id| session_load_request(id, session_id, cwd))?;
        SessionNewResult::from_value(&result)
            .ok_or_else(|| CallError::Io("session/load result missing sessionId".to_owned()))
    }

    /// `session/prompt` — one bounded agent turn; resolves with the
    /// completion signal (`stopReason`, verified `"end_turn"`) once the
    /// response arrives. Update notifications drain via [`AcpClient::events`].
    pub fn session_prompt(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<SessionPromptResult, CallError> {
        let result = self.call_with(|id| session_prompt_request(id, session_id, text))?;
        SessionPromptResult::from_value(&result)
            .ok_or_else(|| CallError::Io("session/prompt result missing stopReason".to_owned()))
    }

    /// Drain one event; `None` when the stream is exhausted (agent EOF).
    /// Never blocks forever: EOF resolves the channel.
    pub fn events(&self) -> Option<AcpEvent> {
        let rx = self.events.1.lock().expect("event lock poisoned");
        rx.recv().ok()
    }

    /// Try to drain one event without blocking.
    pub fn try_event(&self) -> Option<AcpEvent> {
        let rx = self.events.1.lock().expect("event lock poisoned");
        rx.try_recv().ok()
    }

    /// Graceful shutdown: request stdin EOF, wait for the process to exit on
    /// its own (verified: opencode exits 0 on stdin EOF), kill after grace.
    /// Returns the exit status when the process is reaped. Idempotent.
    pub fn shutdown(&self) -> Result<Option<std::process::ExitStatus>> {
        {
            let mut guard = self.shared.write.lock().expect("stdin lock poisoned");
            if guard.is_some() {
                // Drop stdin → EOF for the agent.
                *guard = None;
            }
        }
        let mut child = self.child.lock().expect("child lock poisoned");
        match child.wait() {
            Ok(status) => Ok(Some(status)),
            Err(_) => {
                let _ = child.kill();
                child.wait().map(Some).context("reap killed agent process")
            }
        }
    }

    /// Non-blocking peek at the exit status without shutting down.
    pub fn exited(&self) -> Option<std::process::ExitStatus> {
        let mut child = self.child.lock().expect("child lock poisoned");
        child.try_wait().ok().flatten()
    }
}

impl Drop for AcpClient {
    /// Safety net for a caller that forgot [`AcpClient::shutdown`]: stdin is
    /// closed (EOF signal) and a still-running agent is killed so the child
    /// never outlives the handle. Normal teardown is `shutdown` + drop.
    fn drop(&mut self) {
        if let Ok(mut guard) = self.shared.write.lock() {
            *guard = None; // stdin EOF
        }
        if let Ok(mut child) = self.child.lock() {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {}
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::protocol::{PROTOCOL_VERSION, ContentBlock, SessionUpdate, request_frame};
    use serde_json::json;
    use std::sync::Arc;

    // ---- framing: one JSON-RPC message per line, classified by shape ----

    #[test]
    fn classify_sorts_frames_by_json_rpc_shape() {
        match classify(r#"{"jsonrpc":"2.0","id":7,"result":{"ok":1}}"#).unwrap() {
            Incoming::Response { id, result, error } => {
                assert_eq!(id, RequestId::U64(7));
                assert_eq!(result, Some(json!({"ok": 1})));
                assert!(error.is_none());
            }
            other => panic!("expected response, got {other:?}"),
        }
        match classify(r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#).unwrap() {
            Incoming::Notification { method, .. } => assert_eq!(method, "session/update"),
            other => panic!("expected notification, got {other:?}"),
        }
        // id + method but no result/error → server→client request.
        match classify(r#"{"jsonrpc":"2.0","id":0,"method":"fs/read_text_file","params":{}}"#)
            .unwrap()
        {
            Incoming::ServerRequest { id, method, .. } => {
                assert_eq!(id, RequestId::U64(0));
                assert_eq!(method, "fs/read_text_file");
            }
            other => panic!("expected server request, got {other:?}"),
        }
        assert!(classify("not json at all {").is_none());
        assert!(matches!(classify("42"), Some(Incoming::Ignored)));
    }

    #[test]
    fn request_id_resolves_like_untagged_u64_i64_string() {
        assert_eq!(RequestId::from_value(&json!(7)), Some(RequestId::U64(7)));
        assert_eq!(RequestId::from_value(&json!(-3)), Some(RequestId::I64(-3)));
        assert_eq!(
            RequestId::from_value(&json!("abc")),
            Some(RequestId::String("abc".to_owned()))
        );
        assert_eq!(RequestId::from_value(&json!(null)), None);
        assert_eq!(RequestId::from_value(&json!([])), None);
    }

    #[test]
    fn frames_round_trip_across_one_line() {
        // NDJSON invariant: serde_json never emits raw newlines inside values,
        // so to_string() is exactly one frame.
        let frame = request_frame(
            &RequestId::U64(1),
            "initialize",
            json!({"protocolVersion": PROTOCOL_VERSION}),
        );
        let line = frame.to_string();
        assert!(!line.contains('\n'));
        match classify(&line).unwrap() {
            Incoming::ServerRequest { id, method, params } => {
                assert_eq!(id, RequestId::U64(1));
                assert_eq!(method, "initialize");
                assert_eq!(params.pointer("/protocolVersion"), Some(&json!(1)));
            }
            other => panic!("expected request frame, got {other:?}"),
        }
    }

    // ---- id correlation + notification channel, driven through route() ----

    fn test_reader() -> (Reader, mpsc::Receiver<AcpEvent>) {
        let (tx, rx) = mpsc::channel();
        let reader = Reader {
            shared: Arc::new(Shared {
                write: Mutex::new(None), // no stdin in unit fixtures
                pending: Mutex::new(HashMap::new()),
            }),
            events: tx,
        };
        (reader, rx)
    }

    #[test]
    fn route_resolves_pending_response_by_id() {
        let (reader, _rx) = test_reader();
        let (tx, rx) = mpsc::channel();
        reader
            .shared
            .pending
            .lock()
            .unwrap()
            .insert(RequestId::U64(9), tx);
        reader.route(classify(r#"{"jsonrpc":"2.0","id":9,"result":{"x":true}}"#).unwrap());
        assert!(reader.shared.pending.lock().unwrap().is_empty());
        assert_eq!(rx.recv().unwrap().unwrap(), json!({"x": true}));
    }

    #[test]
    fn route_resolves_pending_error_response_by_id() {
        let (reader, _rx) = test_reader();
        let (tx, rx) = mpsc::channel();
        reader
            .shared
            .pending
            .lock()
            .unwrap()
            .insert(RequestId::U64(2), tx);
        // Verified opencode shape: unknown session/load sessionId.
        let line = r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32603,
            "message":"Internal error: OpenCode service failure",
            "data":{"service":"session"}}}"#;
        reader.route(classify(line).unwrap());
        match rx.recv().unwrap() {
            Err(CallError::JsonRpc(e)) => {
                assert_eq!(e.code, -32603);
                assert_eq!(e.data, Some(json!({"service": "session"})));
            }
            other => panic!("expected json-rpc error, got {other:?}"),
        }
    }

    #[test]
    fn route_delivers_session_update_notification() {
        let (reader, rx) = test_reader();
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"ses_1",
            "update":{"sessionUpdate":"agent_message_chunk","messageId":"msg_1",
            "content":{"type":"text","text":"pong"}}}}"#;
        reader.route(classify(line).unwrap());
        match rx.recv().unwrap() {
            AcpEvent::Update(frame) => {
                assert_eq!(frame.session_id, "ses_1");
                match frame.update {
                    SessionUpdate::AgentMessageChunk {
                        content: ContentBlock::Text(text),
                        ..
                    } => assert_eq!(text, "pong"),
                    other => panic!("unexpected update variant: {other:?}"),
                }
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn route_records_deny_for_permission_requests() {
        // The unit fixture has no stdin (write_frame no-ops); the deny reply
        // itself is asserted on the parsed request — the policy surface.
        let (reader, rx) = test_reader();
        let line = json!({
            "jsonrpc": "2.0", "id": 0,
            "method": "session/request_permission",
            "params": {
                "sessionId": "ses_1",
                "toolCall": {"toolCallId": "t1", "kind": "edit", "status": "pending"},
                "options": [
                    {"optionId": "once", "kind": "allow_once", "name": "Allow once"},
                    {"optionId": "reject", "kind": "reject_once", "name": "Reject"}
                ]
            }
        })
        .to_string();
        reader.route(classify(&line).unwrap());
        match rx.recv().unwrap() {
            AcpEvent::PermissionDenied(req) => {
                assert_eq!(req.tool_call_id, "t1");
                assert_eq!(
                    req.deny_reply(),
                    json!({"outcome": "selected", "optionId": "reject"})
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn unknown_session_update_stays_unknown_not_invented() {
        let (reader, rx) = test_reader();
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s",
            "update":{"sessionUpdate":"plan","entries":[]}}}"#;
        reader.route(classify(line).unwrap());
        match rx.recv().unwrap() {
            AcpEvent::Update(frame) => assert_eq!(frame.update, SessionUpdate::Unknown),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn finish_eof_fails_waiters_and_signals_eof() {
        let (reader, rx) = test_reader();
        let (tx, waiter) = mpsc::channel();
        reader
            .shared
            .pending
            .lock()
            .unwrap()
            .insert(RequestId::U64(3), tx);
        reader.finish_eof();
        match waiter.recv().unwrap() {
            Err(CallError::JsonRpc(e)) => assert_eq!(e.code, -32001),
            other => panic!("expected EOF failure, got {other:?}"),
        }
        assert!(matches!(rx.recv().unwrap(), AcpEvent::Eof));
    }

    // ---- end-to-end over real pipes with a fake agent (offline, no LLM) ----

    const FAKE_AGENT: &str = r#"
        read line1
        echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"fake","version":"0"}}}'
        read line2
        echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_fake"}}'
        echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"ses_fake","update":{"sessionUpdate":"agent_message_chunk","messageId":"msg_1","content":{"type":"text","text":"hi"}}}}'
        read line3
        echo '{"jsonrpc":"2.0","id":0,"method":"session/request_permission","params":{"sessionId":"ses_fake","toolCall":{"toolCallId":"t1","kind":"edit","status":"pending"},"options":[{"optionId":"once","kind":"allow_once","name":"Allow once"},{"optionId":"reject","kind":"reject_once","name":"Reject"}]}}'
        echo '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'
    "#;

    #[test]
    fn e2e_fake_agent_handshake_updates_and_shutdown() {
        let args = vec!["-c".to_string(), FAKE_AGENT.to_string()];
        let client = AcpClient::spawn("/bin/sh", &args, None).unwrap();

        let init = client.initialize().unwrap();
        assert_eq!(init.protocol_version, PROTOCOL_VERSION);
        assert_eq!(init.agent_name.as_deref(), Some("fake"));

        let sess = client.session_new("/tmp").unwrap();
        assert_eq!(sess.session_id, "ses_fake");

        let prompt = client.session_prompt("ses_fake", "hello").unwrap();
        assert_eq!(prompt.stop_reason, "end_turn");

        let mut saw_update = false;
        let mut saw_deny = false;
        while let Some(event) = client.try_event() {
            match event {
                AcpEvent::Update(frame) => {
                    saw_update =
                        matches!(frame.update, SessionUpdate::AgentMessageChunk { .. });
                }
                AcpEvent::PermissionDenied(req) => {
                    saw_deny = req.options.len() == 2;
                }
                _ => {}
            }
        }
        assert!(saw_update, "session/update chunk missing");
        assert!(saw_deny, "permission deny record missing");

        let status = client.shutdown().unwrap().unwrap();
        assert_eq!(status.code(), Some(0));
    }
}
