---
id: acp-engine
status: draft
title: Engine C — ACP stdio engine with per-adapter engine dispatch
---

# ACP engine proposal

## Problem

Serve mode (`--serve --adapter <name>`) today drives every whitelisted agent
through Engine A (headless one-shot exec) and classifies the outcome from the
process boundary: exit code × error patterns. That ceiling is documented in
this repo already:

- aider exits 0 on auth failures (exec-adapter evidence) — error patterns are
  the safety net.
- `opencode run` exits 0 with empty stdout when tool calls are auto-rejected
  (upstream #36413) — `require_output` is the safety net.

Engine B (PTY + VT100 grid) observes the agent's terminal instead, but
completion detection there is grid-heuristic by nature and the
completion-detection capability does not exist yet.

Meanwhile, a growing set of CLI agents speak the Agent Client Protocol (ACP)
natively: a JSON-RPC session over stdio with an explicit lifecycle
(`initialize` → `session/new` → `session/prompt`), a structured event stream
(`session/update` notifications: message chunks, tool calls, plans),
permission requests, and session-level configuration switching. Driving such
agents over ACP replaces output heuristics with protocol signals: completion
becomes a protocol event, failures arrive as structured errors, and tool-call
activity is observable without scraping a terminal grid.

The two approaches are complementary, not competing: protocol-level control
for agents that speak ACP; process/grid observation for agents that never
will. This change adds the ACP path without touching either existing engine.

## What changes

- **New capability `acp-engine` (Engine C)**: a minimal ACP client that
  drives one ACP-capable CLI in `--serve` mode, mapping the ACP session onto
  the existing `Task` lifecycle via `TaskOutcome`.
- **Engine dispatch**: `adapter::Adapter` gains an engine profile
  (`exec` default; `acp` only after a verified per-agent profile exists,
  see Evidence gates). Serve dispatches accordingly. No silent engine
  switching at runtime: if the ACP engine fails to establish a session, the
  task fails with the structured reason.
- **A2A wire contract unchanged.** Not one byte of the NDJSON boundary in
  a2a-transport changes: same three methods, same task-state strings,
  `capabilities.streaming` still false. ACP session events are consumed
  internally as (a) the completion signal, (b) a structured failure reason,
  and (c) an optional event-summary artifact alongside the existing
  git-diff artifact.

## Evidence gates (do not implement ahead of verification)

The ACP surface of a given CLI must be verified before its engine profile
flips from `exec` to `acp`. Per-agent facts (subcommand, handshake details,
event shapes, permission semantics, session persistence, exit behavior) land
as a reference doc in `docs/` — same pattern as `docs/zero-oneline-exec-note.md`
— each section marked verified (with repro command) or unverified.

Candidate known from this repo: `opencode acp` (stdio sub-negotiation,
deferred from the opencode-adapter change as "another capability" — this is
that capability, generalized). Verification of the candidate agents'
implementations is in progress out-of-band; this proposal deliberately
asserts no per-agent ACP facts.

## Spec changes

- ADD capability `acp-engine` (capability spec drafted after this proposal
  is approved, per the OpenSpec flow).
- MODIFY capability `a2a-transport`: dispatch point only — its wire contract
  section is untouched.

## Out of scope (this change)

- A2A streaming / push notifications (unchanged v1 non-goals).
- `CancelTask` and cross-engine hung-task timeouts — one concern when that
  method lands; ACP liveness is revisited then (a live ACP session emits
  traffic, unlike a silent exec child).
- Engine B tasks over A2A — PTY remains the calibration harness until
  completion-detection exists.
- Interactive permission handling: v1 answers permission requests with a
  deterministic deny and classifies the task `Failed{reason: ...}`;
  human-in-the-loop is a later change.
- Engine fallback: no automatic `acp → exec` fallback at runtime (artifact
  semantics would silently change); an agent without a verified profile
  simply stays on `exec`.
- Crate/binary naming housekeeping (see Naming note).

## Naming note

In this repo the crate and the binary are consistently named `a2a-bridge`;
prose and docs — including the ACP discussion above — refer to the product
by that name.

## Acceptance

1. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` green.
2. ACP wiring unit-tested against a fake ACP agent script (no LLM):
   handshake → session → update events → completion mapped to
   `Completed` with artifacts; JSON-RPC error response → `Failed{reason}`;
   permission request → denied → `Failed{reason: permission-denied…}`;
   EOF mid-session → `Failed`.
3. Wire-contract regression: the existing serve framing/slot/card tests pass
   unchanged.
4. No whitelisted adapter changes labels or dispatch behavior until its
   profile evidence lands in `docs/`.
5. Dispatch test: an `acp`-profiled adapter routes to the ACP path;
   `exec`-profiled adapters keep their existing classify semantics
   byte-for-byte.
