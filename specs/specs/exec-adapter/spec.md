# exec-adapter capability

Status: proposed (change: exec-adapter)

## Goal

Run a coding-agent task as a **headless one-shot subprocess** (Engine A) and
classify its outcome deterministically: completed / failed / aborted, plus
extract the `git diff` artifact for the A2A `Task` result.

## Design

```
src/adapter/
├── mod.rs        # re-exports only
├── outcome.rs    # TaskOutcome enum + classification (non-trivial logic)
├── runner.rs     # spawn → capture → wait → classify; git-diff artifact
├── aider.rs      # aider -m command construction + error patterns
└── zero.rs       # zero exec command construction + error patterns
```

### TaskOutcome

```rust
pub enum TaskOutcome {
    Completed { artifact: Option<String> }, // git diff (HEAD) in the workdir
    Failed { reason: String },              // nonzero exit OR matched error pattern
    Aborted,                                // reserved; kill path arrives with CancelTask
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
- Workdir: inherited cwd; zero gets `-C <cwd>` explicitly.
- Artifact: after a Completed outcome, run `git diff HEAD` in the workdir.
  `ponytail:` plain `git diff HEAD` — staged/untracked handling refined when
  the A2A artifact schema lands.

### Adapters

| Agent | Command | Approval-gating flags (headless) |
|-------|---------|----------------------------------|
| aider | `aider -m "<msg>" --yes --no-auto-commits` | `--yes` |
| zero  | `zero exec "<prompt>" --auto high --skip-permissions-unsafe` | `--auto high` / `--skip-permissions-unsafe` |

`--no-auto-commits` keeps the working tree dirty so the git-diff artifact is
meaningful. `--auto high` prevents zero from blocking on approval prompts.

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
