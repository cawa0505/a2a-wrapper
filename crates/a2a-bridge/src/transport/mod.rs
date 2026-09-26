//! A2A v1 stdio binding — server side (`a2a-bridge --serve`).
//!
//! JSON-RPC 2.0 over NDJSON on stdin/stdout, hand-rolled with `serde_json` to
//! match the sole client (Argus `argus-core/src/agent.rs`) wire 1:1 — no
//! `a2a-lf` dependency (decision B, 2026-09-27). stdout carries protocol only;
//! diagnostics go to stderr. One task per process, server loop ends at EOF.

use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{Value, json};

use crate::adapter::{Adapter, TaskOutcome};

/// Run the serve loop over real stdin/stdout for the selected adapter.
pub fn serve(adapter: Adapter, cwd: Option<String>) -> Result<()> {
    let run = move |prompt: &str| {
        adapter
            .run(prompt, cwd.clone())
            .unwrap_or_else(|e| TaskOutcome::Failed {
                reason: e.to_string(),
            })
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve_with(adapter.label(), &run, stdin.lock(), stdout.lock())
}

/// Serve loop split from the process handles for testing.
fn serve_with(
    adapter_name: &str,
    run: &impl Fn(&str) -> TaskOutcome,
    reader: impl BufRead,
    mut writer: impl Write,
) -> Result<()> {
    let mut slot: Option<Value> = None;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle_line(&line, adapter_name, run, &mut slot) {
            writeln!(writer, "{resp}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Handle one NDJSON line. Returns the response line, or `None` for a
/// notification (no `id`).
fn handle_line(
    line: &str,
    adapter_name: &str,
    run: &impl Fn(&str) -> TaskOutcome,
    slot: &mut Option<Value>,
) -> Option<String> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        // Parse error: id undeterminable → null (JSON-RPC -32700).
        Err(_) => return Some(err(Value::Null, -32700, "parse error").to_string()),
    };

    let method = v.get("method").and_then(Value::as_str);
    let id = v.get("id").cloned();

    // No id → notification: no response (nothing to act on in v1).
    let id = id?;

    let Some(method) = method else {
        return Some(err(id, -32600, "invalid request: missing method").to_string());
    };

    let resp = match method {
        "GetExtendedAgentCard" => ok(id, agent_card(adapter_name)),
        "GetTask" => match slot {
            Some(task) => ok(id, json!({ "task": task })),
            None => err(id, -32001, "task not found"),
        },
        "SendMessage" => handle_send(id, &v, run, slot),
        // Valid-but-unsupported (ListTasks/CancelTask/…) and unknown alike.
        _ => err(id, -32601, "method not found"),
    };
    Some(resp.to_string())
}

fn handle_send(
    id: Value,
    v: &Value,
    run: &impl Fn(&str) -> TaskOutcome,
    slot: &mut Option<Value>,
) -> Value {
    // One task per process: a filled slot means this process already ran a task.
    if slot.is_some() {
        return err(id, -32000, "task in progress");
    }
    if v.pointer("/params/message").is_none() {
        return err(id, -32602, "invalid params: missing message");
    }
    let prompt = extract_prompt(v);
    let task_id = new_task_id();
    let task = match run(&prompt) {
        TaskOutcome::Completed { artifacts } => task_completed(&task_id, &artifacts),
        TaskOutcome::Failed { reason } => task_failed(&task_id, &reason),
    };
    *slot = Some(task.clone());
    ok(id, json!({ "task": task }))
}

fn extract_prompt(v: &Value) -> String {
    v.pointer("/params/message/parts")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text"))
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn task_completed(id: &str, artifacts: &[String]) -> Value {
    json!({
        "id": id,
        "contextId": id,
        "status": { "state": "TASK_STATE_COMPLETED" },
        "artifacts": artifacts
            .iter()
            .map(|t| json!({ "parts": [{ "text": t }] }))
            .collect::<Vec<_>>(),
    })
}

fn task_failed(id: &str, reason: &str) -> Value {
    json!({
        "id": id,
        "contextId": id,
        "status": {
            "state": "TASK_STATE_FAILED",
            "message": { "role": "ROLE_AGENT", "parts": [{ "text": reason }] }
        },
    })
}

fn agent_card(adapter: &str) -> Value {
    json!({
        "name": "a2a-bridge",
        "description": format!(
            "Sidecar wrapping the `{adapter}` CLI coding agent as an A2A node (stdio binding)"
        ),
        "version": env!("CARGO_PKG_VERSION"),
        "protocols": ["a2a"],
        "capabilities": { "streaming": false, "push_notifications": false },
        "skills": [],
    })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// 128-bit random-ish hex id (stdlib only, no `uuid` crate) — enough for a
/// one-task-per-process node.
fn new_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs().rotate_left(32))
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    format!(
        "{:016x}{:016x}",
        nanos,
        pid.rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed(text: &str) -> impl Fn(&str) -> TaskOutcome + '_ {
        move |_prompt| TaskOutcome::Completed {
            artifacts: vec![text.to_string()],
        }
    }

    fn send_line(prompt: &str) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "SendMessage",
            "params": { "message": { "role": "ROLE_USER", "parts": [{ "text": prompt }] } }
        })
        .to_string()
    }

    #[test]
    fn send_message_returns_completed_task_with_artifact() {
        let run = completed("+diff");
        let mut slot = None;
        let resp = handle_line(&send_line("do it"), "codex", &run, &mut slot).unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["id"], 1);
        assert_eq!(
            v["result"]["task"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );
        assert_eq!(
            v["result"]["task"]["artifacts"][0]["parts"][0]["text"],
            "+diff"
        );
        assert!(slot.is_some());
    }

    #[test]
    fn failed_outcome_maps_to_failed_task_with_reason() {
        let run = |_: &str| TaskOutcome::Failed {
            reason: "boom".into(),
        };
        let mut slot = None;
        let resp = handle_line(&send_line("x"), "rho", &run, &mut slot).unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["task"]["status"]["state"], "TASK_STATE_FAILED");
        assert_eq!(
            v["result"]["task"]["status"]["message"]["parts"][0]["text"],
            "boom"
        );
    }

    #[test]
    fn second_send_is_task_in_progress() {
        let run = completed("+diff");
        let mut slot = None;
        handle_line(&send_line("first"), "zero", &run, &mut slot).unwrap();
        let resp = handle_line(&send_line("second"), "zero", &run, &mut slot).unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32000);
    }

    #[test]
    fn get_task_returns_cached_then_missing() {
        let run = completed("+diff");
        let mut slot = None;
        // Missing before any task.
        let miss = handle_line(
            r#"{"jsonrpc":"2.0","id":9,"method":"GetTask","params":{}}"#,
            "aider",
            &run,
            &mut slot,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&miss).unwrap()["error"]["code"],
            -32001
        );
        // Run a task, then GetTask returns it.
        handle_line(&send_line("go"), "aider", &run, &mut slot).unwrap();
        let hit = handle_line(
            r#"{"jsonrpc":"2.0","id":9,"method":"GetTask","params":{}}"#,
            "aider",
            &run,
            &mut slot,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&hit).unwrap();
        assert_eq!(
            v["result"]["task"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );
    }

    #[test]
    fn agent_card_is_adapter_aware() {
        let run = completed("x");
        let mut slot = None;
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"GetExtendedAgentCard"}"#,
            "codex",
            &run,
            &mut slot,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["name"], "a2a-bridge");
        assert!(
            v["result"]["description"]
                .as_str()
                .unwrap()
                .contains("codex")
        );
        assert_eq!(v["result"]["capabilities"]["streaming"], false);
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let run = completed("x");
        let mut slot = None;
        for method in ["ListTasks", "CancelTask", "Frobnicate"] {
            let line = json!({"jsonrpc":"2.0","id":3,"method":method}).to_string();
            let resp = handle_line(&line, "zero", &run, &mut slot).unwrap();
            let v: Value = serde_json::from_str(&resp).unwrap();
            assert_eq!(v["error"]["code"], -32601, "method {method}");
        }
    }

    #[test]
    fn malformed_line_is_parse_error_with_null_id() {
        let run = completed("x");
        let mut slot = None;
        let resp = handle_line("{not json", "zero", &run, &mut slot).unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32700);
        assert!(v["id"].is_null());
    }

    #[test]
    fn notification_without_id_gets_no_response() {
        let run = completed("x");
        let mut slot = None;
        let line = r#"{"jsonrpc":"2.0","method":"SendMessage","params":{}}"#;
        assert!(handle_line(line, "zero", &run, &mut slot).is_none());
    }

    #[test]
    fn serve_loop_round_trips_over_readers() {
        let run = completed("+patch");
        let input = format!("{}\n", send_line("build it"));
        let mut out: Vec<u8> = Vec::new();
        serve_with("codex", &run, input.as_bytes(), &mut out).unwrap();
        let line = String::from_utf8(out).unwrap();
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(
            v["result"]["task"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );
    }
}
