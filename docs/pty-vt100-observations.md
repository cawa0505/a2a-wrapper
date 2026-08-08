# PTY/VT100 Harness Observations (initial calibration)

> Status: 初始校準 — aider 真實終端 TUI 校準待使用者互動執行補完。
> 對應能力：`pty-supervision`（specs/specs/pty-supervision/spec.md）。

## Harness 驗證結果（2026-08-08）

| 測試 | 情境 | 結果 |
|------|------|------|
| Unit: escape stream → rows | 純文字 / 尾端空白 trim | pass ×2 |
| Empirical 1: plain lines | `printf "hello\nworld\n"` | grid `[00] hello, [01] world` ✓ |
| Empirical 2: CRLF | `printf "hello\r\nworld\r\n"` | 同 1（CRLF 正確處理）✓ |
| Empirical 3: clearscreen + cursor pos | `ESC[2J ESC[H hello ESC[2;1H world` | `[00] hello, [01] world` ✓ |
| Empirical 4: aider (line mode) | `aider` 於非 tty 環境 | grid 正確擷取 `/help` 與輸出 ✓ |

## 關鍵觀察

1. **aider 於非互動環境自動退化為 line-oriented 模式**（render.log 分析）：
   - 無 alt-screen（`?1049` 計數 = 0）、無 TUI 繪製
   - 輸出為彩色 rules（`─`×80）+ 警告行，CRLF 換行
   - 原因：harness 環境的 TERM/非 tty 繼承導致；**真實終端下 aider 才會啟用
     full TUI**（含輸入欄、repo 狀態列）— 該校準需使用者在真實終端執行
     `cargo run -- --debug-grid` 手動觀察
2. **Grid 在兩種模式都正確**：line 模式與 TUI 模式共用同一 vt100 解析路徑
3. **tracing 輸出污染**：初始版本 tracing 寫 stdout 混入 child 串流 — 已修正
   （`with_writer(io::stderr)`，session/input 模組重構時一併處理）
4. **輸入限制**：harness 使用 cooked-mode stdin（行緩衝），方向鍵/即時按鍵
   不會即時送達 child；校準場景可接受（type + Enter 即可），正式 sidecar 為
   非互動式，無此問題
5. **PTY 尺寸固定 24×80**：vt100::Parser 與 PtySize 一致；aider TUI 若要求
   更大尺寸會自行 resize 請求（未驗證）

## 對 completion-detection（下一能力）的影響

- **line 模式**（非 tty 部署）：完成偵測可直接用輸出結尾 + exit code，不需
  VT100 狀態機 — 這是 `zero-oneline-exec-note.md` Engine A 論點的另一佐證
- **TUI 模式**（真實終端）：grid 狀態機目標列為 prompt 行（`>` 輸入欄）、
  repo 狀態列、完成標記 — 待真實終端校準後定義
- Silent Timeout 兜底：TUI 動畫（thinking spinner 等）期間無狀態變更時靠它
  判定 — 閾值待校準
