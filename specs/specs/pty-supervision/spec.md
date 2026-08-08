# Capability: pty-supervision

## Overview

PTY-based process supervision for unmodified CLI agents, with a VT100 grid
view that makes the child's terminal state machine-readable. Foundation for
the A2AWrapper sidecar: the child's stdin/stdout stays a real terminal
(aider renders its full TUI), while the wrapper keeps an authoritative view
of the rendered screen.

## Goals

- Spawn an arbitrary CLI agent (PoC: aider) inside a PTY (portable-pty, echo
  mode) and supervise the process lifecycle.
- Feed the child's escape-sequence output into a `vt100` parser and expose
  the rendered grid programmatically.
- Forward host stdin → child stdin so the harness is interactive.
- Live-render the child's output to the host terminal (the harness IS the
  user's terminal view during calibration).
- Provide a debug grid dump (Ctrl+Q in `--debug-grid` mode) so grid contents
  can be inspected and sentinel states calibrated.

## Non-Goals (this change)

- No A2A transport / framing (capability `a2a-transport`, later).
- No completion state machine / silent-timeout logic (capability
  `completion-detection`, later).
- No raw-mode host terminal handling (echo mode; input interception is a
  single control character).
- No tokio/async — sync threads are sufficient until the transport step
  requires it.
- No aider LLM wiring — calibration uses UI-only commands (`/help`, `/add`,
  `/clear`) that render UI without a model endpoint.

## Design

### CLI (minimal, std::env for now)

```
a2a-bridge [--debug-grid] [-- <agent args...>]
```

- Default agent: `aider`. Args after `--` pass through to the child.
- `--debug-grid`: intercept Ctrl+Q in the input loop and dump the current
  grid rows to stderr.

### Module layout

```
crates/a2a-bridge/src/
├── main.rs        # arg parse, thread wiring, host I/O
└── pty/
    ├── mod.rs     # spawn(): PtyPair (echo mode), reader/input threads
    └── grid.rs    # Grid: vt100::Parser wrapper (text(), rows(), row(n))
```

### Threads (sync)

1. **Reader**: master read → write raw bytes to host stdout (live render) +
   feed `vt100::Parser`.
2. **Input**: host stdin → master writer; in `--debug-grid` mode, Ctrl+Q is
   consumed and triggers a grid dump to stderr instead of forwarding.

### Grid wrapper

- `Grid::new()` — owns `vt100::Parser` behind a mutex (reader thread
  writes, dump reads).
- `Grid::text()` — trimmed grid text (rows joined, trailing whitespace and
  blank rows stripped).
- `Grid::rows()` — vector of trimmed row strings.
- Rendering approach (implementation detail): either re-render whole grid on
  each flush, or a diffed approach if flicker is an issue — decide by
  observation during implementation.

## Acceptance Criteria

1. `cargo build` + `cargo fmt --check` + `cargo clippy -- -D warnings` +
   `cargo test` all pass.
2. Running the binary shows aider's TUI rendered live through the
   PTY → vt100 → host path (proof that the escape stream round-trips).
3. `--debug-grid` + Ctrl+Q dumps readable grid rows containing aider UI
   text (e.g. "Added ... to the chat", the input prompt line).
4. `Grid` row extraction carries one runnable check (canned VT100 escape
   sequence → asserted rows).
5. Blast radius: new files only; no pre-existing code modified.
