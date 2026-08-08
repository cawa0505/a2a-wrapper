---
id: exec-adapter
status: applied
title: Engine A — headless one-shot execution (exec-adapter capability)
---

# Exec-adapter proposal

## Problem

The PTY/VT100 path (Engine B) drives interactive TUIs but cannot detect
completion reliably without grid heuristics. Meanwhile, both PoC agents ship
**headless one-shot modes** that turn a task into a plain subprocess:

- aider: `aider -m "<message>"` — disables chat mode, processes the reply, exits
- zero (gitlawb/zero): `zero exec "<prompt>"` / `zero -p "<prompt>"` — one-shot exec

For these, task completion is a **process outcome**: exit code + output. That
is deterministic and needs no VT100 guessing. This change adds the
`exec-adapter` capability: spawn, capture, classify, and extract the
`git diff` artifact.

## Evidence (verified 2026-08-08, see docs/zero-oneline-exec-note.md)

- `zero exec "<prompt>"` positional prompt; `-o json|stream-json` structured output;
  `-C/--cwd`, `-w/--worktree`; `--auto <low|medium|high>`, `--skip-permissions-unsafe`
  for approval gating. Note: `-m` on zero is `--model`, **not** message.
- `aider -m` / `--message` one-shot confirmed via local CLI (0.86.2).
- Live smoke test (litellm gateway, gemma4-12b): `aider -m` applied the edit,
  exit 0, `git diff` captured. **Trap confirmed: aider exits 0 even on
  auth failure** → exit code alone is insufficient; error-pattern detection on
  output is required.

## Spec changes

Adds new capability `exec-adapter` (see specs/specs/exec-adapter/spec.md).

## Out of scope (this change)

- A2A transport / stdio framing / Agent Card (next capability)
- Engine B integration (PTY grid state machine)
- zero `stream-json` output parsing (adapter refinement, later)
- tokio migration (runner stays sync; mechanical migration when transport lands)
- runner timeout/cancellation (killing the child arrives with A2A CancelTask)

## Acceptance

1. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` green
2. Outcome classification unit-tested without any LLM call (fake subprocesses)
3. Live smoke: `aider -m` run succeeds, returns `Completed` + git-diff artifact;
   forced failure path returns `Failed` with reason
4. Modular layout: one concern per file under `src/adapter/`
