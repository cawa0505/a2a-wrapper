---
id: a2a-transport
title: A2A stdio binding — server side (transport)
status: proposed
depends_on: [exec-adapter]
---

# Proposal — A2A stdio binding (transport)

## Problem

`a2a-bridge` can already execute tasks headlessly (Engine A) and supervise
TUIs (Engine B harness), but it is not yet an **A2A node**: nothing can send it
an A2A task over the wire. The whole product thesis is a sidecar that turns an
unmodified CLI agent into an A2A node — this change is where that happens.

## Decision: hand-rolled serde_json matching the Argus client wire (no a2a-lf)

Superseded the earlier "official a2a-lf types" plan (2026-09-27). The sole
real consumer of this transport is Argus (`argus-core/src/agent.rs`), and it
does **not** use `a2a-lf`: it hand-rolls the NDJSON wire with `serde_json` and
its module doc states plainly *"the message shape is the boundary."* Adopting
`a2a-lf` on the server side would buy zero interoperability with the actual
client while pulling a heavyweight external crate (whose availability could not
be verified offline, and whose T1 dependency wiring targeted a hypothetical
`crates/a2a-xyz/` crate path that does not exist). Per the project's YAGNI
stance we match the client's fixed wire shape directly.

The wire shape is the contract (mirrored 1:1 from `agent.rs`):

- Request: `{ "jsonrpc": "2.0", "id": <n>, "method": "SendMessage",
  "params": { "message": { "messageId", "role": "ROLE_USER",
  "parts": [ { "text": <prompt> } ] } } }`.
- Completed response: `result.task.status.state = "TASK_STATE_COMPLETED"`,
  `result.task.artifacts[].parts[].text` = artifact text(s).
- Failed response: `result.task.status.state = "TASK_STATE_FAILED"`,
  `result.task.status.message.parts[0].text` = reason.
- JSON-RPC `error.code` for protocol failures (Argus maps any error → Failed).

Method names stay the a2a-lf-style PascalCase constants (`SendMessage`,
`GetTask`, `GetExtendedAgentCard`) so a future a2a-lf client can still speak to
us, but they are matched as plain strings — no SDK dependency.

## Key choices (v1)

| Choice | Decision | Why |
|--------|----------|-----|
| Transport | **stdio** (UDS deferred) | Matches MCP ecosystem deployment (host spawns sidecar, speaks NDJSON); trivially testable; PTY owns the agent's stdio so bridge stdio is protocol-only |
| Framing | **NDJSON** (JSON-RPC 2.0, one message per line) | serde_json escapes newlines in strings, so lines are unambiguous; same framing as tapedeck MCP — ecosystem precedent |
| Types | **hand-rolled `serde_json::Value` + `json!`** (no a2a-lf) | The only client (Argus) hand-rolls the same way; the wire shape is the boundary; zero external protocol dep |
| Task id | 128-bit hex (stdlib, no `uuid` crate) | Mirrors `agent.rs::uuid_like`; enough uniqueness for one-task-per-process |
| Async | **none** (sync loop) | Single task at a time; executor comes only when needed |
| methods v1 | `SendMessage` (blocking), `GetTask`, `GetExtendedAgentCard` | The minimal node surface; no cancel/push/subscribe yet (YAGNI). Wire names = PascalCase strings; unknown/unsupported → `-32601` |
| Task store | single in-memory slot | One task in progress (repo rule); concurrent tasks deferred |
| Engine | Engine A (exec) only | Engine B (PTY interactive) is a later change; v1 tasks run headless |
| Adapters | whitelist `[aider, codex, rho, zero]` | Mirror-locked with Argus `spec.rs`; dispatch is exhaustive over this set |

## Alternatives rejected

- **Custom envelope**: obsolete — official types carry full lifecycle semantics.
- **HTTP JSON server**: adds a web stack (hyper/tokio) for no benefit in the
  PoC; the closed layer spawns us via stdio.
- **UDS first**: socket setup + path management with no consumer yet; same
  framing, mechanical to add later.

## Blast radius

~8 files: `src/transport/*` (4 new), `args.rs`, `main.rs` edits, `Cargo.toml`
(+3 deps), `specs/`. No changes to existing `adapter/` or `pty/` behavior.
