---
id: a2a-transport
title: A2A stdio binding — server side
status: approved (pending proposal gate)
---

# Capability spec — a2a-transport

## Goal

`a2a-bridge --serve` exposes the wrapped CLI agent as an A2A v1 node over
stdio: JSON-RPC 2.0, newline-delimited. A bus/host can spawn the sidecar, send
a task, and receive the completed Task with the git-diff artifact.

Types are hand-rolled `serde_json` matching the sole client (Argus
`argus-core/src/agent.rs`) wire 1:1 — no `a2a-lf` dependency (decision B,
2026-09-27; see proposal). The message shape is the boundary.

## Wire contract

- Transport: the bridge's stdin/stdout, line-oriented.
- Framing: **NDJSON** — each line is exactly one JSON-RPC 2.0 message
  (request, response, or notification). No id → notification (no response).
- Encoding: UTF-8. `serde_json` never emits raw newlines inside values, so
  line splitting is unambiguous.
- Logging in `--serve` mode: **stdout carries protocol only**; all diagnostics
  (tracing) go to stderr.
- Method names: PascalCase strings — `SendMessage`, `GetTask`,
  `GetExtendedAgentCard` — matched as plain `&str` (a2a-lf-compatible names,
  no SDK dependency). The A2A HTTP spec's `tasks/send` strings are NOT used.
- Task state on the wire: the string constants Argus parses —
  `TASK_STATE_COMPLETED`, `TASK_STATE_FAILED`, `TASK_STATE_WORKING`.
- Agent card: served via the `GetExtendedAgentCard` JSON-RPC method as a
  hand-rolled JSON object (name/description/version/protocols/capabilities).

## Request surface (v1)

| Method | Request params | Response result | Errors |
|--------|---------------|-----------------|--------|
| `GetExtendedAgentCard` | — | `AgentCard` | — |
| `SendMessage` | `SendMessageRequest` | `Task` (final state) | task-in-progress, invalid params |
| `GetTask` | `GetTaskRequest` | `Task` (cached) | task-not-found |

- Any method outside `a2a::jsonrpc::methods::is_valid` → JSON-RPC `-32601`.
- A valid-but-unsupported method (`ListTasks`, `CancelTask`, `SubscribeToTask`,
  push-config family, `SendStreamingMessage`) → JSON-RPC `-32601` in v1.
- Malformed JSON line → JSON-RPC `-32700` (parse error) with the request id if
  determinable, else `null`.

## Task lifecycle (Engine A)

1. `SendMessage` arrives with a message (user prompt from
   `params.message.parts[].text`; role=`ROLE_USER`; no `task_id` → new task).
2. If the single slot is occupied (a task exists in non-terminal state):
   JSON-RPC error `-32000` "task in progress".
3. Create Task: `id` = 128-bit hex (stdlib, no `uuid` crate); `status.state` =
   `TASK_STATE_WORKING`; `context_id` = the task id.
4. Run the adapter headless (dispatch on `--adapter`: aider / zero / codex /
   rho) with optional `--cwd`.
5. Map `TaskOutcome`:
   - `Completed{artifacts}` → `status.state = TASK_STATE_COMPLETED`,
     `artifacts = [{ parts: [{ text }] }, ...]` (one entry per artifact text;
     empty vec → no artifacts).
   - `Failed{reason}` → `status.state = TASK_STATE_FAILED`,
     `status.message.parts[0].text` = reason.
 6. Store the final Task in the slot (kept for `GetTask`), reply with it.
   Blocking: the response **is** the final task.

## Agent Card

- `name`: "a2a-bridge"
- `description`: "Sidecar wrapping the `<adapter>` CLI coding agent as an A2A
  node (stdio binding)".
- `version`: crate version.
- `protocols`: ["a2a"]; `capabilities.streaming`: false;
  `capabilities.push_notifications`: false; `skills`: [].
- Hand-rolled JSON object; `name`/`description` adapter-aware (`--adapter`).

## Non-goals (v1)

- No `a2a-client-lf` usage (client binding = follow-up change).
- No UDS listener, no HTTP, no tokio.
- No `CancelTask` (killing a running agent lands with that method).
- No Engine B (PTY interactive) task execution.
- No concurrent tasks, no persistence.

## Verification

1. Unit: framing round-trip; slot semantics; card shape.
2. In-process integration: `serve(run)` with an injected fake runner
   (no LLM): send → Completed+artifact; send-while-busy → error; unknown
   method → -32601; malformed line → -32700.
3. Live E2E (manual, LLM-gated): pipe NDJSON `tasks/send` into
   `a2a-iris --serve --adapter aider` (AIDER_OPENAI_API_KEY set) in a git
   repo; assert `Completed` + `diff.patch` artifact + `agent/card` shape.
