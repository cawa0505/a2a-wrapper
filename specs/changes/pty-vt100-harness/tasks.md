# Tasks — pty-vt100-harness

> Status: **complete**（2026-08-08）— 驗證結果見
> `docs/pty-vt100-observations.md`。

- [x] T1: workspace scaffold（root Cargo.toml + crates/a2a-bridge）
- [x] T2: PTY spawn + live render（pty/session.rs）
- [x] T3: VT100 grid integration（pty/grid.rs）
- [x] T4: input forwarding + Ctrl+Q debug dump（pty/input.rs）
- [x] T5: runnable check（grid.rs unit tests ×2）
- [x] T6: verification — cargo fmt/clippy `-D warnings`/test 全綠；
  3 項 empirical grid 測試通過；aider 於非 tty 環境 line-mode 實測通過

## 補充（使用者要求）

- [x] 模組化重構：session/input/args 拆分，AGENTS.md 加入模組化規範
- [ ] （待辦）aider 真實終端 TUI 校準 — 使用者手動 `cargo run -- --debug-grid`
      執行，結果餵入下一能力 `completion-detection`
