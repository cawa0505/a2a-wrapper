---
id: pty-vt100-harness
status: applied
title: PTY + VT100 Observation Harness
---

# Change: pty-vt100-harness

## Problem Statement

The sidecar's hardest unknown is whether a CLI agent's TUI can be made
machine-readable through VT100 grid parsing. Before any A2A wiring, we must
prove that aider's rendered screen can be captured as an inspectable grid —
this is the calibration surface for the later completion state machine
(`completion-detection`).

## Change Summary

Introduce capability **`pty-supervision`**: a `pty/` module in
`crates/a2a-bridge` that spawns an unmodified CLI agent in a PTY, feeds its
output into a `vt100` parser while live-forwarding to the host terminal, and
exposes the grid via a `Grid` wrapper with a debug dump trigger.

## Spec Changes

- New capability spec: `specs/specs/pty-supervision/spec.md`
- Workspace: root `Cargo.toml` (workspace) + `crates/a2a-bridge` crate.
- Dependencies added: `portable-pty`, `vt100`, `anyhow`, `tracing`,
  `tracing-subscriber`. (Sync std threads; no tokio yet.)
- Out of scope: A2A transport, completion detection, MCP bridge, Zero/OpenCode
  adapters.

## Tasks

See `tasks.md` in this change directory. Implemented and verified per
`specs/specs/pty-supervision/spec.md` acceptance criteria.
