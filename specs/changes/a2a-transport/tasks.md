# Tasks — a2a-transport

> Impl note: T2–T5 landed consolidated in `transport/mod.rs` (one file: NDJSON
> framing + single-slot store + AgentCard + dispatch loop + tests) rather than
> four files — the surface is small and fixed; splitting adds no value.

## T1 — Dependencies (decision B: hand-rolled serde_json, no a2a-lf)
- [x] Add `serde_json = "1"` to `crates/a2a-bridge/Cargo.toml` (the only new
      dep; no `a2a-lf`, no `uuid` — task id is stdlib 128-bit hex mirroring
      `agent.rs::uuid_like`).
- [x] Wire shape frozen against the sole client `argus-core/src/agent.rs`:
      request `SendMessage` with `params.message.parts[].text`; response
      `result.task.status.state` ∈ {TASK_STATE_COMPLETED, TASK_STATE_FAILED,
      TASK_STATE_WORKING}; artifacts `result.task.artifacts[].parts[].text`;
      failure reason `result.task.status.message.parts[0].text`.
- [x] `cargo build` passes.

## T2 — transport/jsonrpc.rs — NDJSON framing
- [x] Helpers: read one line from stdin → `JsonRpcRequest` (tolerate trailing
      whitespace); `JsonRpcResponse` → one line to stdout; parse-error
      response for malformed lines.
- [x] Unit test: request/response round-trip; escaped-newline-in-string
      survives line framing; malformed line yields parse error.

## T3 — transport/tasks.rs — single-slot store + lifecycle
- [x] `TaskStore`: one slot; `submit(prompt) -> Task`, `get(id) -> Option<Task>`;
      reject submit while running.
- [x] Lifecycle mapping: run (injected `Fn(&str) -> TaskOutcome`) → final Task
      (Completed + diff.patch artifact / Failed + reason message).
- [x] Tests: submit→completed; submit-while-running rejected; get by id;
      get unknown id → None.

## T4 — transport/card.rs — AgentCard builder
- [x] `build(adapter: &str, version: &str) -> AgentCard` per spec.
- [x] Test: card fields (sendMessage true, pushNotifications false, name).

## T5 — transport/server.rs — dispatch loop
- [x] `serve(run)` — sync stdin/stdout loop; dispatch `GetExtendedAgentCard`,
      `SendMessage`, `GetTask`; valid-but-unsupported method → -32601;
      notifications ignored.
- [x] Integration tests (in-process, fake runner): full SendMessage→completed
      flow; busy rejection; unknown method; malformed line;
      GetExtendedAgentCard.
- [x] stdout carries protocol only (tracing to stderr in serve mode).

## T6 — CLI wiring
- [x] `args.rs`: `Mode::Serve { adapter, cwd }` from
      `--serve [--adapter aider|zero|codex|rho] [--cwd DIR]`.
- [x] `main.rs`: serve mode → build adapter dispatch from `--adapter`, wire
      `serve(run)`; adapter selection removes the dead-code allowance on
      `zero_exec`/`codex_exec`; tracing to stderr.

## T7 — Verification
- [x] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` green
      (33 tests: transport round-trip/busy/unknown-method/malformed/card +
      adapter whitelist `{aider,codex,rho,zero}` mirror lock).
- [ ] Live E2E (manual, litellm key): `printf` NDJSON `SendMessage` into
      `a2a-bridge --serve --adapter aider` in a git repo → assert `Completed`
      + `diff.patch` artifact; assert `GetExtendedAgentCard` JSON.
- [ ] Record results in `docs/` (transport binding verification note).
- [ ] Close: spec + proposal status, project.md capability, commit.
