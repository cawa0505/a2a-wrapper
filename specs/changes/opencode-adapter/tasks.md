# Tasks — opencode-adapter

OpenSpec flow: proposal → approved spec → tasks → implementation → verify → close.
One task in progress at a time; each micro-task independently verifiable.

- [x] T1: `src/adapter/mod.rs` — `Adapter::Opencode` variant into `enum`/
      `ALL`/`label()`/exhaustive `run()`; whitelist lock test set →
      `["aider", "codex", "opencode", "rho", "zero"]` (same-named lock as
      the scheduler-side `spec.rs`)
- [x] T2: `src/adapter/opencode.rs` — `opencode_exec(prompt, workdir) ->
      ExecSpec`: program `opencode`, args `["run", "<prompt>"]`; workdir via
      runner `current_dir` (no `-C` flag exists on opencode); no
      provider/model/key flags; never pass `-` as the prompt placeholder
      (upstream #28407)
- [x] T3: empty-output classification — exit 0 + blank stdout →
      `Failed{reason: no output}` on the opencode path (upstream #36413);
      existing adapters' `classify()` semantics unchanged
- [x] T4: tests (no LLM) — argv construction (no `-C`, no `-` placeholder),
      whitelist five-value lock, empty-output case, fake-agent exit 0/1
      end-to-end
- [x] T5: verification — `cargo fmt --check`, `cargo clippy -- -D warnings`,
      `cargo test` (38 green); live smoke: real `opencode run` → Completed +
      git-diff artifact (`+two`), invalid model → Failed (exit 1, ~20s,
      pattern `Unexpected server error` matched → TASK_STATE_FAILED); error
      evidence recorded in `opencode.rs` comment
- [x] T5b: workdir 真實生效（排程端 E2E 才暴露的修正）— runner 在設
      `current_dir` 時同步 `cmd.env("PWD", dir)`：opencode 只從 `PWD` 決定
      project root，單靠 `current_dir` 會讓它在 spawn 端的 repo 動手
      （實測把 `"two"` 寫進排程端自己的 repo 根目錄，目標 repo 無 diff、
      artifacts 空）。測試 `workdir_is_exported_as_pwd` 鎖住
- [x] T6: close — status: applied; `specs/specs/exec-adapter/spec.md` 已同步
      （adapter 白名單五值條款 + codex/opencode/rho 三列 + `require_output`
      說明 + `Completed { artifacts: Vec<String> }` 現況 + PWD 匯出條款）；
      `specs/openproj/project.md` 描述去「PoC: aider」。本 repo 無 archive
      目錄（自製 `specs/` flow 以 close 收斂，change 記錄留在 `changes/`）

## Evidence references

- opencode 1.18.32 measurements: headless `run` exit 0 (setsid/非 TTY/piped
  stdin), invalid model → exit 1 (mimo catalog case ~3.2s local per research;
  T5 smoke `nosuchprovider/*` & `openrouter/<missing>` both ~20s, possibly
  network-touching; stderr masked as `UnknownError` / `Unexpected server
  error`), permissions
  default-allow, upstream issues #36413 / #28407
- specs/specs/exec-adapter/spec.md (capability contract)
- Authoritative adapter delta: promoted in the private spec-mirror repo;
  scheduler-side mirror: the consuming scheduler's worker-delegation change
