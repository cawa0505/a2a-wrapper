# AGENTS.md — A2AWrapper

Development norms for this repository. Read this before touching any code.
Companion research: `docs/a2a-landscape-research.md`.

## 1. Project Overview

Rust sidecar wrapper that turns unmodified CLI coding agents (PoC target: **aider**)
into A2A v1 protocol nodes. Uses official SDKs `a2a-lf` (core types) and
`a2a-client-lf` (client) as core dependencies; differentiator is a **stdio/UDS
custom binding** (spec §12) + PTY process supervision + VT100-grid completion
detection.

- Language: Rust (MSRV 1.85+, matching `a2a-lf`)
- License intent: Apache-2.0, OSS-bound core
- Status: PoC phase, greenfield

## 2. Development Norms (non-negotiable)

### 2.1 Documentation-First — OpenSpec

- All feature work goes through the **OpenSpec flow**: `specs/` directory,
  proposal → approved `spec.md` → `tasks.md` → implementation → verification → close.
- **No code before its spec change is approved.**
- Every technical research / investigation result MUST be persisted as a
  reference doc in `docs/` — never left only in conversation.
- Doc conventions: English-primary, clear section hierarchy; synchronized
  Traditional Chinese versions on request. Docs live in repo-root directories
  (`docs/`, `specs/`) — not scattered in source trees. No date stamps in
  filenames, no future dates. XDG-style directory hygiene.

### 2.2 Clean Code — No Placeholders

- **Strict modularity:** code is organized as small focused modules — one
  concern per file, no long single-file monoliths. `src/` uses subsystem
  subdirectories (`pty/`, `transport/`, `adapter/`); `mod.rs` only re-exports.
- **No placeholder code.** No `TODO`/`FIXME` stubs, no empty scaffolding "for
  later", no dead flexibility. If something is deferred, say so in prose or a
  `ponytail:` comment that names the ceiling — never ship a stub.
- YAGNI: no unrequested abstractions (no interface with one implementation, no
  factory for one product, no config for a constant).
- Non-trivial logic (branch/loop/parser/money-or-security path) keeps **one
  runnable check** (assert-based self-check or one small test) — no test
  frameworks, no fixtures unless asked.
- Prefer stdlib / already-installed deps over new dependencies.

### 2.3 Task Scheduling Before Development

- No implementation work starts without a clear, current task plan (todo list
  or `tasks.md`). Exactly one task in progress at a time.
- Break large work into micro-tasks (<40k tokens each). Each micro-task is
  independently verifiable.
- When a new request arrives mid-task, append it to the plan; do not silently
  reorder or drop existing items.
- Record milestone completion; update `specs/*/tasks.md` checkboxes as work lands.

## 3. Repository Structure

```
A2AWrapper/
├── AGENTS.md
├── Cargo.toml            # workspace root
├── crates/
│   └── a2a-bridge/       # binary: stdio framing + PTY supervision + A2A server
│       └── src/
│           ├── main.rs
│           ├── pty/          # portable-pty supervision, VT100 grid sentinel
│           ├── transport/    # stdio/UDS transport for a2a-lf / a2a-client-lf
│           └── adapter/      # aider-specific prompt/artifact translation
├── docs/                 # research references (mandatory persistence)
└── specs/                # OpenSpec proposals, specs, task breakdowns
```

## 4. Quality Gates (before "Done")

1. `cargo fmt --check` passes
2. `cargo clippy -- -D warnings` passes
3. `cargo test` passes (including the one runnable check per non-trivial logic)
4. Blast radius respected: <5 files normal · 5–15 summarize before proceeding ·
   >15 pause and present the change for approval
5. No secrets: never commit key **values**; they live only in untracked,
   gitignored `secrets.md` at repo root or runtime env. Diffs reviewed for
   leaks before every commit.

## 5. Safety Guardrails

Never run without explicit user approval: `rm -rf`, `git push --force`,
`git reset --hard`, `git clean -fd`, destructive production commands, sudo on
remote systems, overwriting files outside this repo.

## 6. Verification

- Non-trivial behavior changes: plan verification before implementing
  (narrowest evidence that confirms the behavior; broaden only if integration
  risk justifies it).
- PoC-stage checks: `cargo build` + `cargo test` + manual PTY harness run
  (observe aider TUI grid states).
- Architecture / protocol decisions: consult `docs/a2a-landscape-research.md`
  and record the decision in the relevant OpenSpec spec.
