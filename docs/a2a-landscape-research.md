# A2A Protocol & Ecosystem — Research Reference

> Research snapshot. Author: orchestrator (self-investigation, 2026-08-08).
> All primary sources verified directly from official spec / repos at time of writing.

## 1. A2A Protocol status

- **Current released version: `1.0.0`** (previous: `0.3.0`, `0.2.6`, `0.1.0`).
- Governance: **Linux Foundation** open-source project, contributed by Google, licensed **Apache-2.0**.
- Spec: <https://a2a-protocol.org/latest/specification/> · Repo: <https://github.com/a2aproject/A2A>
- **Normative source of truth: `spec/a2a.proto`** (protobuf). Generated JSON (`a2a.json`) is a non-normative build artifact. SDKs/schemas MUST be regenerated from proto, not hand-edited.
- Change control: deprecated names kept until next major release; migration docs required for breaking changes.

## 2. Three-layer architecture

| Layer | Content |
|-------|---------|
| L1 Data Model | `Task`, `Message`, `AgentCard`, `Part`, `Artifact`, `Extension` — protocol-agnostic, defined as protobuf messages |
| L2 Abstract Operations | Binding-independent capabilities (send / get / list / cancel / subscribe / push / extended card) |
| L3 Protocol Bindings | JSON-RPC · gRPC · HTTP/REST · **Custom Bindings (explicitly allowed)** |

Layered design means new bindings can be added without touching the data model — core semantics stay consistent across bindings.

## 3. Core data model (summary)

- **Task**: stateful unit of work, unique server-generated `taskId`, defined lifecycle.
  - Terminal states: `COMPLETED`, `FAILED`, `CANCELED`, `REJECTED`.
  - Interrupted states: `INPUT_REQUIRED`, `AUTH_REQUIRED`.
- **Message**: one turn, `role` = `user` | `agent`, contains one or more `Parts`.
- **Part**: smallest content unit — text, file reference, or structured data.
- **Artifact**: agent output (doc, image, structured data) composed of `Parts`.
- **AgentCard**: JSON discovery document — identity, skills, `supportedInterfaces` (url + protocolBinding + version), `capabilities` (streaming / pushNotifications / extendedAgentCard), securitySchemes.
- **contextId**: logical group of related tasks (multi-turn continuity); `referenceTaskIds` for explicit cross-task references.
- **Opaque execution**: agents exchange declared capabilities + information, never internal state/memory/tools.

## 4. Operations

- `SendMessage` — blocking by default (`return_immediately: false` waits for terminal/interrupted state); non-blocking option returns task immediately.
- `SendStreamingMessage` — SSE-style stream; Message-only stream or Task-lifecycle stream (status/artifact update events).
- `GetTask` · `ListTasks` (cursor pagination, sorted by status timestamp desc) · `CancelTask` · `SubscribeToTask`.
- Push notification configs: `Create/Get/List/Delete` (webhook HTTP POST, requires capability flag).
- `GetExtendedAgentCard` — richer card after auth, requires `capabilities.extendedAgentCard`.
- Capability validation: unsupported optional ops MUST return errors (`UnsupportedOperationError`, `PushNotificationNotSupportedError`, ...).
- Update delivery: polling / streaming / push webhooks.
- A2A-specific errors: `TaskNotFoundError`, `TaskNotCancelableError`, `ContentTypeNotSupportedError`, `VersionNotSupportedError`, `InvalidAgentResponseError`, `ExtensionSupportRequiredError`, etc.

## 5. Standard bindings

| Binding | Transport | Rust SDK support |
|---------|-----------|------------------|
| JSON-RPC 2.0 | HTTP(S); SSE for streaming | `a2a-server` (axum), `a2a-client` |
| gRPC | HTTP/2 via tonic | `a2a-grpc` |
| HTTP+JSON / REST | HTTP(S) | `a2a-server`, `a2a-client` |
| SLIMRPC | SLIMRPC | `a2a-slimrpc` |

## 6. Custom bindings — the key mechanism for a sidecar

- Spec **§12 "Custom Binding Guidelines"**: implementers **MAY** create custom bindings for additional transports; **MUST** comply with §5 requirements (error-code mapping, stream mechanism definition, auth integration documentation).
- Binding identified in Agent Card: `supportedInterfaces[].protocolBinding` = URI string, e.g. `https://example.com/bindings/websocket/v1`. Version negotiation via `A2A-Version` service parameter (standard params prefixed `a2a-`).
- **No stdio transport exists in core spec.** Proposal filed: GitHub **issue #1074 "Stdio transport"** (2025-09-16) — communicating A2A over UNIX stdin/stdout; status: open proposal, not merged. Greenfield for us.

## 7. Rust SDK — `a2aproject/a2a-rs` workspace

Crates published to crates.io (package → crate):

| Crate | Purpose |
|-------|---------|
| `a2a-lf` | Core A2A types, errors, events, JSON-RPC types, wire-compatible serde |
| `a2a-client-lf` | Async client, **transport abstraction** + protocol negotiation from agent card |
| `a2a-server-lf` | Async server framework (REST + JSON-RPC) on `axum` |
| `a2a-pb` | Protobuf schema + generated types + ProtoJSON conversion |
| `a2a-grpc` | gRPC client/server on `tonic` |
| `a2a-slimrpc` | SLIMRPC client/server |
| `a2a-cli` | Standalone client CLI (`card`, `send`, `stream`, `list-tasks`, `push-config`) |

- MSRV **Rust 1.85+**; Apache-2.0. `cargo add a2a-lf`.
- Key implication: client has a transport abstraction → custom transports are pluggable; core types crate is transport-agnostic → directly usable for a stdio binding.

## 8. Other SDKs

- Python `a2a-sdk`, Go `a2a-go` (v2, ships its own CLI), JS `@a2a-js/sdk`, Java, .NET (`A2A` on NuGet).
- Samples: `a2aproject/a2a-samples`. DeepLearning.AI course exists (A2A + MCP complementary).

## 9. Prior art for "agent wrappers"

- **`shashikanth-gs/a2a-wrapper`** (JS/TS): wraps AI agents as A2A **HTTP servers** — single `A2AExecutor` interface + Express wiring, agent card building, session TTL. Not a sidecar, not stdio.
- **No established Rust sidecar / stdio-wrapper project found.** GitHub code search for `a2a-lf` usage returned nothing public.
- Positioning gap: **sidecar process supervision + stdio/UDS custom binding** for unmodified CLI agents appears unoccupied in the Rust ecosystem.

## 10. Implications for A2AWrapper design

1. Do **not** roll a custom envelope protocol — use `a2a-lf` core types (`Task`/`Message`/`AgentCard`, task lifecycle, errors) as the data model.
2. Differentiator = **stdio/UDS custom binding** (LSP-style JSON-RPC framing over child-process stdio) + sidecar supervision + completion detection → our sidecar is a full A2A agent with an Agent Card declaring `protocolBinding`.
3. `a2acli` can serve as the PoC bus client / demo tool.
4. A2A semantics map directly onto wrapper concerns: completion detection → `Task` → `COMPLETED` + `Artifact` (e.g. git diff).

## 11. Target agent profiles (PoC context)

Three candidate agents from the vision doc; each maps to a different wrapper mode.

| Agent | Type | Interface for wrapper | Integration mode |
|-------|------|------------------------|------------------|
| aider | Python CLI, line-oriented / TUI, git-centric | PTY (renders TUI when tty) | PTY supervision + VT100 grid — PoC, hardest case |
| OpenCode | CLI TUI, MCP-capable | TUI raw mode + MCP | MCP bridge or PTY — second phase |
| Zero (`gitlawb/zero`) | Go terminal coding agent, MIT | **Headless `zero exec` stream-JSON protocol** + `zero serve --mcp` | Direct protocol adapter — no PTY needed, easiest case |

Zero specifics (verified from repo README, 2026-08-08):

- **Not** a full-OS/online agent — a local terminal coding agent (browser/terminal control helpers exist as gated tools).
- `zero exec --input-format stream-json --output-format stream-json` — scriptable structured I/O, isolated worktrees, spec-first runs, meaningful exit codes (CI-friendly).
- `zero serve --mcp` exposes Zero tools over MCP stdio.
- Stream-JSON contract: `docs/STREAM_JSON_PROTOCOL.md` in the repo.
- Go 1.26.5+, MIT license; distribution via npm wrapper + install scripts.

Implication: the three targets validate the wrapper concept — PTY (aider), MCP bridge (OpenCode), stream-JSON adapter (Zero). PoC proceeds with aider; Zero is the simplest future adapter.

## Sources

- <https://a2a-protocol.org/latest/specification/> (v1.0.0 spec)
- <https://github.com/a2aproject/A2A> (protocol repo, issue #1074)
- <https://github.com/a2aproject/a2a-rs> (Rust SDK workspace)
- <https://github.com/shashikanth-gs/a2a-wrapper> (JS/TS prior art)
