# exec-adapter capability

Status: proposed (change: exec-adapter)

## Goal

Run a coding-agent task as a **headless one-shot subprocess** (Engine A) and
classify its outcome deterministically: completed / failed / aborted, plus
extract the `git diff` artifact for the A2A `Task` result.

## Design

```
src/adapter/
├── mod.rs        # `Adapter` enum — the whitelist itself (exhaustive dispatch)
├── outcome.rs    # TaskOutcome enum + classification (non-trivial logic)
├── runner.rs     # spawn → capture → wait → classify; git-diff artifact
├── aider.rs      # aider -m command construction + error patterns
├── codex.rs      # codex exec command construction + error patterns
├── opencode.rs   # opencode run command construction + #36413 empty-output guard
├── rho.rs        # rho run command construction + status-JSON artifact
└── zero.rs       # zero exec command construction + error patterns
```

### TaskOutcome

```rust
pub enum TaskOutcome {
    Completed { artifacts: Vec<String> }, // git diff (HEAD) in the workdir; rho adds its status JSON
    Failed { reason: String },           // nonzero exit OR matched error pattern
}
```

### Classification rule (the core invariant)

- `Completed` iff **exit code 0 AND no error pattern matched** in stdout/stderr.
- `Failed` if **nonzero exit OR any error pattern matched** (aider exits 0 on
  auth errors — empirically confirmed; error patterns are the safety net).
- Error patterns: per-adapter constant arrays, e.g. aider:
  `Authentication Error`, `Rate limit`, `Connection error`, `API key`.
  Extensible; documented per adapter.

### Runner

- `std::process::Command` (sync — tokio migration deferred to transport step;
  `tokio::process::Command` is a near drop-in).
- Captures stdout/stderr to memory; on failure keeps the stderr tail as reason.
- Workdir: inherited cwd; zero gets `-C <cwd>` explicitly. 當 `ExecSpec.workdir`
  有值時，runner SHALL 同時設 `current_dir` **並把 `PWD` 匯出成同一值**：
  實測 opencode 1.18.32 只從 `PWD` 解析 project root，單靠 `current_dir`
  會讓 agent 在 spawn 端的目錄動手（rc 仍 0、artifacts 空，只有看 evidence
  才發現）。
- Artifact: after a Completed outcome, run `git diff HEAD` in the workdir.
  `ponytail:` plain `git diff HEAD` — staged/untracked handling refined when
  the A2A artifact schema lands.

### Adapter whitelist

The `Adapter` enum in `mod.rs` **is** the whitelist: `Adapter::ALL` feeds
`parse()`, `label()` round-trips it, and `run()`'s match is compiler-exhaustive.
Its set SHALL be exactly `["aider", "codex", "opencode", "rho", "zero"]`, locked
by a test named `adapter_whitelist_is_exactly_aider_codex_opencode_rho_zero` —
consuming schedulers carry an identically named lock test so the two ends
cannot drift. `adapter_or_bail()` is the single enforcement point.

### Adapters

| Agent | Command | Approval-gating flags (headless) |
|-------|---------|----------------------------------|
| aider | `aider -m "<msg>" --yes --no-auto-commits` | `--yes` |
| codex | `codex exec "<prompt>" --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check [-C <cwd>]` | `--dangerously-bypass-approvals-and-sandbox` |
| opencode | `opencode run "<prompt>"` | none — permissions default to all-allowed |
| rho | `rho --config C --provider P --model M run "<prompt>" --timeout T --output-file <tmp>` | from pinned env; missing env → `Failed` before spawn |
| zero  | `zero exec "<prompt>" --auto high --skip-permissions-unsafe [-C <cwd>]` | `--auto high` / `--skip-permissions-unsafe` |

`--no-auto-commits` keeps the working tree dirty so the git-diff artifact is
meaningful. `--auto high` prevents zero from blocking on approval prompts.

No adapter hardcodes provider/model/keys: codex reads its own `~/.codex/config.toml`
(located via `CODEX_HOME`), opencode its own config (located via `OPENCODE_CONFIG`),
rho takes them from pinned `RHO_*` env. Deployment values stay out of this repo.

`opencode run` MAY exit 0 on task failure (upstream #36413), so its `ExecSpec`
sets `require_output: true` — exit 0 with blank stdout demotes to `Failed{reason:
no output}`. The check is opt-in per `ExecSpec`, so the other four keep their
existing classify semantics byte-for-byte.

## Non-goals

- No transport, no Agent Card, no stream-json parsing, no timeout/kill yet.
- No async runtime in this change.

## Verification

- Unit tests: outcome classification (exit code × error patterns matrix),
  aider/zero command construction, artifact extraction from a temp git repo.
- Fake-agent subprocess tests: a script that exits 0 / 1 / prints
  "Authentication Error" — asserts the resulting `TaskOutcome` without any
  LLM call.
- Live smoke: `aider -m` against the configured gateway (see acceptance in the
  change proposal).
