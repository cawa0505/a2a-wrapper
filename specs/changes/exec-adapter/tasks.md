# Tasks — exec-adapter

OpenSpec flow: proposal → approved spec → tasks → implementation → verify → close.
One task in progress at a time; each micro-task independently verifiable.

- [x] T1: `src/adapter/` scaffold — `mod.rs` re-exports, `outcome.rs` with
      `TaskOutcome` enum; wire `mod adapter` into `main.rs`
- [x] T2: `runner.rs` — spawn (sync `std::process::Command`), capture
      stdout/stderr, wait, classify per the spec invariant; stderr tail as
      failure reason
- [x] T3: classification logic + unit tests — exit-code × error-pattern matrix
      (0+clean → Completed; 0+pattern → Failed; nonzero → Failed)
- [x] T4: `aider.rs` — command builder (`-m <msg> --yes --no-auto-commits`) +
      error-pattern list + construction test
- [x] T5: `zero.rs` — command builder (`exec <prompt> --auto high
      --skip-permissions-unsafe`, `-C <cwd>`) + error-pattern list +
      construction test
- [x] T6: artifact extraction — `git diff HEAD` in workdir on Completed +
      temp-git-repo test (no LLM)
- [x] T7: fake-agent subprocess tests — script exiting 0 / 1 / printing
      "Authentication Error" → asserts `TaskOutcome` end-to-end (no LLM)
- [x] T8: verification — `cargo fmt --check`, `cargo clippy -- -D warnings`,
      `cargo test`; live smoke `aider -m` (litellm gateway) → Completed +
      git-diff artifact; forced-failure run → Failed with reason
- [x] T9: close change — mark proposal applied, update `specs/openproj/project.md`

## Evidence references

- docs/zero-oneline-exec-note.md (CLI + smoke-test verification)
- specs/specs/exec-adapter/spec.md (design contract)
