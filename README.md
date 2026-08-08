# A2AWrapper

A Rust sidecar that turns **unmodified CLI coding agents** into
[A2A (Agent2Agent)](https://a2a-protocol.org) protocol nodes over a custom
**stdio/UDS binding** — no agent code changes required.

> Status: **PoC phase**. PTY supervision and VT100 grid parsing are implemented
> and verified; the A2A transport layer is the next milestone. See
> [Status](#status).

## Why

The A2A v1 specification (Linux Foundation, Apache-2.0) standardizes the agent
data model and operations (Task, Message, AgentCard) but ships HTTP-based
bindings only. CLI coding agents — aider, OpenCode, zero — speak TTY and
files, not JSON-RPC over HTTP. A2AWrapper is a thin sidecar that sits next to
an unmodified agent binary, drives it over a PTY (or its headless one-shot
mode), and exposes the result as A2A tasks over a stdio/UDS bus — the
[Custom Bindings transport the spec explicitly allows](https://a2a-protocol.org/latest/specification/) (spec §12).

## Architecture

```
a2a-bridge
├── pty/          # portable-pty supervision, VT100 grid view
├── transport/    # A2A stdio/UDS binding (a2a-lf core types)  [in progress]
└── adapter/      # per-agent prompt/artifact translation        [planned]
```

Two execution engines for task completion:

| Engine | Mechanism | Use case |
|--------|-----------|----------|
| **A (Direct Exec)** | spawn headless one-shot (`aider -m`, `zero exec`), exit code + output | deterministic, non-interactive tasks |
| **B (PTY + VT100 grid)** | drive interactive TUI in a PTY, detect completion from rendered grid + silent timeout | interactive / multi-turn agents |

## Status

Implemented (PoC):

- PTY supervision: spawn aider in a PTY, forward I/O, live render
- VT100 grid parsing (`pty/grid.rs`, unit-tested): grid text extraction, debug
  dump (`--debug-grid` + Ctrl+Q)
- Engine A smoke-verified: `aider -m` headless → exit code + `git diff` artifact
- OpenSpec workflow (`specs/`) with an approved `pty-supervision` capability

Roadmap:

- A2A transport: stdio/UDS custom binding over `a2a-lf` core types, JSON-RPC framing
- Completion-detection state machine over the VT100 grid
- Agent Card + bus integration (A2A `Task`/`Message` lifecycle)
- `adapter/aider` (artifact translation), `adapter/zero` (stream-JSON)
- MCP bridge for MCP-capable agents (OpenCode)

## Quickstart

```sh
cargo build --release
# PTY harness: spawn aider and observe the VT100 grid
cargo run -- --debug-grid -- aider
# Ctrl+Q in --debug-grid mode dumps the current grid to stderr
```

Requires Rust 1.85+.

## Docs

- [`docs/a2a-landscape-research.md`](docs/a2a-landscape-research.md) — A2A protocol & SDK landscape research
- [`docs/zero-oneline-exec-note.md`](docs/zero-oneline-exec-note.md) — zero oneline-exec verification (Engine A)

## License

Apache-2.0 — see [LICENSE](LICENSE).
