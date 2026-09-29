//! ACP engine (Engine C) — client side of the Agent Client Protocol.
//!
//! Drives one ACP-speaking CLI agent process over stdio NDJSON JSON-RPC.
//! Types are verified against `docs/acp-agent-profiles.md` (`opencode acp`
//! 1.18.32). This module is scaffolded ahead of dispatch wiring (T3/T4);
//! the `dead_code` allowance is scoped here and must be dropped when the
//! engine is routed from serve mode.

#![allow(dead_code)]

mod client;
mod protocol;

#[allow(unused_imports)]
pub use protocol::{
    classify, initialize_request, request_frame, response_frame, session_load_request,
    session_new_request, session_prompt_request, ContentBlock, Incoming, InitializeResult,
    JsonRpcError, PermissionRequest, RequestId, SessionNewResult, SessionPromptResult,
    SessionUpdate, SessionUpdateFrame, PROTOCOL_VERSION,
};
