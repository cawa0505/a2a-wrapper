# Tasks — acp-engine

OpenSpec flow: proposal → approved spec → tasks → implementation → verify → close.
One task in progress at a time; each micro-task independently verifiable.

- [ ] T1: per-agent ACP evidence — `docs/acp-agent-profiles.md`; one section
      per candidate agent: subcommand, handshake, update-event shapes,
      permission semantics, session persistence, exit behavior; each section
      marked verified (with repro command) or unverified. First candidate:
      `opencode acp`. Blocks T3+.
- [ ] T2: `acp/` module scaffold — `protocol.rs` (ACP dialect types:
      initialize / session methods / `session/update` variants, written
      against verified profiles only), `client.rs` (spawn + NDJSON framing +
      request/response correlation), `mod.rs` re-exports. No dispatch wiring.
- [ ] T3: engine dispatch — `Engine` enum (`Exec`, `Acp`), per-adapter
      profile defaulting to `Exec`; serve-mode routing. Whitelist mirror
      lock untouched (labels unchanged). Depends on T1 declaring at least
      one verified profile.
- [ ] T4: ACP → TaskOutcome mapping — session completion →
      `Completed{artifacts}` (git-diff artifact + optional event summary);
      structured error / denied permission / EOF mid-session →
      `Failed{reason}`; unit tests with the fake agent, no LLM.
- [ ] T5: fake ACP agent test double — script emitting canned NDJSON lines
      (handshake, updates, completion, error paths) covering the proposal
      acceptance matrix.
- [ ] T6: verification — `cargo fmt --check`, `cargo clippy -- -D warnings`,
      `cargo test`; wire-regression suite passes unchanged; live smoke
      against the verified profile from T1 (LLM-gated, manual).
- [ ] T7: close — draft `specs/specs/acp-engine/spec.md`, mark proposal
      applied, update `specs/openproj/project.md` capabilities.

## Evidence references

- docs/a2a-landscape-research.md (implications for the wrapper design)
- docs/acp-agent-profiles.md (created by T1; per-agent verified facts)
- specs/specs/a2a-transport/spec.md (the wire contract this change must not
  touch; dispatch point only)
