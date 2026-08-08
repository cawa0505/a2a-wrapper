# Zero Oneline Exec — MVP Direct-Exec 構想（待查證）

> Status: **已查證**（2026-08-08，本機 CLI 實證 + 官方文件；結果見文末「查證結果」）。
> 來源：使用者貼文（2026-08-08），提及 gitlawb/zero 的 oneline exec 特性。
> 原始內文逐字保存（修改僅限本標頭與文末查證章節）；其中程式碼為概念草稿，未編譯。

## 核心主張

Gitlawb/zero 支援 oneline exec（類似 `zero "help me add unit test for main.rs"` 或
`zero -m "."`），代表實作 a2a-bridge 時，不需要一開始就硬碰硬解複雜的 PTY/VT100
互動選單與 TUI 繪製。MVP 階段可直接採用更乾淨、更可靠、零誤判的 **Direct Exec Mode**。

### Oneline Exec 對 a2a-bridge 的降維打擊優勢

- **無需 VT100 畫面猜測**：原本 TUI 模式需用 vt100 解析 ANSI 逃逸字元、判斷游標
  位置與靜默窗口、擔心 TUI 動畫干擾。Oneline Exec 模式直接透過
  `tokio::process::Command` 啟動 `zero "<task_instruction>"` 子進程 —
  進程退出 code 0 = Task Completed，非零 = Task Failed，狀態邊界 100% 精準。
- **Stdio 直接管道化**：不需要分配 PTY，直接 capture stdout/stderr，日誌與錯誤
  追蹤極度簡單。
- **極致執行速度**：不需等待 TUI 渲染延遲，Oneline 模式直奔主題
  （讀檔 → 改檔 → 退出）。

## 雙軌架構（Dual-Engine Architecture）

```
a2a-bridge
(Linux Foundation A2A Stdio Transport)
        │
  ┌─────┴─────┐
  ▼           ▼
【 Engine A: Direct Exec Mode 】   【 Engine B: PTY TUI Mode 】
 • 用於: zero, aider -m, etc.      • 用於: 互動式 TUI / 需多輪交談
 • 機制: Process Exit Code + Stdout • 機制: portable-pty + VT100
 • 狀態: MVP 第一天搞定 (100% 穩定)  • 狀態: Phase 2 擴充
```

## MVP 實作藍圖（概念草稿）

```rust
// crates/a2a-bridge/src/adapter/zero.rs

Use tokio::process::Command;
Use a2a_lf::types::{TaskStatus, Artifact};

Pub async fn execute_zero_task(instruction: &str) -> Result<(TaskStatus, Option<Artifact>)> {
    // 1. 直接呼叫 zero oneline exec
    Let output = Command::new("zero")
        .arg(instruction)
        .output()
        .await?;

    If output.status.success() {
        // 2. 執行成功，抓取 git diff 作為 A2A Artifact
        Let git_diff = Command::new("git").args(["diff", "HEAD"]).output().await?;
        Let diff_str = String::from_utf8_lossy(&git_diff.stdout).to_string();

        Let artifact = Artifact {
            Name: "git_diff".into(),
            Content: diff_str,
        };

        Ok((TaskStatus::Completed, Some(artifact)))
    } else {
        Let err_log = String::from_utf8_lossy(&output.stderr).to_string();
        Ok((TaskStatus::Failed { reason: err_log }, None))
    }
}
```

## 結論（原作者）

gitlawb/zero 的 oneline exec 特性直接把最高風險的 PTY 複雜度「消滅」掉。

## 待查證事項（調研清單）

1. `zero "<task>"` oneline exec 的確切 CLI 語法與是否存在（對照已確認的
   `zero exec --input-format stream-json --output-format stream-json` 與
   `zero serve --mcp`，見 `a2a-landscape-research.md` §11）。
2. `-m "."` 旗標的實際意義（message 參數？）。
3. aider 是否真有對應的 `-m`（oneline message）模式 — 若有，Engine A 可擴及 aider。
4. Engine A 與 A2A Task 語意的對應：非零 exit code → TaskFailed；stream-json
   輸出與 TaskMessage/Artifact 的映射。
5. 是否影響既有決策（PoC target 仍為 aider、完成偵測 = VT100 Grid + Silent
   Timeout）— Engine A 是「新增第二引擎」而非取代，範圍需使用者確認。

## 查證結果（2026-08-08）

> 證據：本機 `zero 0.1.0` CLI（`本地安裝的 zero 0.1.0`）與 `aider 0.86.2`
> `--help` 實證；gitlawb/zero 官方文件。零成本（未呼叫任何 LLM）。

### 1. oneline exec — ✅ 存在，語法確認

```sh
zero exec "<prompt>"              # positional prompt（oneline exec）
zero exec --prompt "<prompt>"     # 旗標形式
zero -p "<prompt>"                # 頂層 one-shot 捷徑
```

- `zero exec [flags] [prompt]`：run a one-shot prompt through the Go agent runtime
- 結構化輸出：`-o text|json|stream-json`（`-i stream-json` 輸入亦然）
- 工作目錄：`-C/--cwd <path>`；隔離執行：`-w/--worktree [name]`
- 注意：zero 為 **v0.1.0（pre-1.0）**，CLI 可能變動；另有 `zero daemon`
  （背景 worker）與 `zero cron`（排程）

### 2. `-m` 旗標 — ❌ 修正：是 `--model`，不是 message

- `zero exec` 的 `-m/--model` 是 **選模型**；oneline prompt 是
  `-p/--prompt` 或 positional。原筆記 `zero -m "."` 猜測不成立。

### 3. aider `-m` — ✅ 存在，Engine A 可擴及 aider

```sh
aider -m "<task>"       # --message/--msg：disables chat mode, process reply, then exit
```

- aider 0.86.2 實證：`-m COMMAND`、`--message-file`、`--exit`、`--test`
  （run tests, fix problems, then exit）皆存在
- 即：**aider 也有 headless one-shot 路徑** — Engine A 對兩個 target 都適用

### 4. Exit code → A2A Task 映射 — ✅ 可行，但有三個確定性陷阱

| 陷阱 | 說明 | 緩解 |
|------|------|------|
| 審批阻斷 | prompt-gated 工具會等待互動確認 → 卡死 headless | `--auto high` / `--skip-permissions-unsafe` / 預先 sandbox grants |
| turn 上限 | `--max-turns` 用盡 = 非自願結束，不等於完成 | 檢查輸出/exit code 語意，超限標記 TaskFailed |
| provider 未就緒 | litellm 等 provider 未設 key → 啟動即失敗 | doctor 前檢（`zero doctor` / `aider --check-update` 類探針） |
| **auth 失敗仍 exit 0**（實測） | aider `-m` 在 API key 失效時印錯誤但 **仍回 exit 0** | **單靠 exit code 不可靠** → Engine A 須加輸出錯誤樣式偵測（Authentication Error / Error 等） |

- stream-json → TaskMessage/Artifact 映射屬 adapter 階段設計（`adapter/zero.rs`、
  `adapter/aider.rs`），本查證不定案
- 本機現況：zero 的 litellm provider **未設 key**（google/openrouter 有）；aider
  已配 litellm gateway（gemma4-12b）→ aider 可直接 smoke test，zero 需先設 key

**Live smoke test（2026-08-08，通過）**：`aider -m "add a short docstring to the
greet function"`（gemma4-12b @ litellm gateway，temp repo）→ exit=0、edit applied、
`git diff` artifact 完整擷取。Engine A 端到端成立：exit code + stdout + git diff
artifact。對照組：同指令在 key 失效時 exit 仍為 0（見上方陷阱列），確認錯誤樣式
偵測為必要。

### 5. MVP 範圍 — ✅ Engine A 成立，不取代既有決策

- **雙引擎架構確認可行**：Engine A（Direct Exec）for `zero exec` / `aider -m`；
  Engine B（PTY+VT100）for 互動式 TUI/多輪交談
- 既有決策不變：PoC target 仍為 aider；互動路徑完成偵測仍為 VT100 Grid +
  Silent Timeout。Engine A 為並行第二引擎，兩者共用同一 A2A transport
- 建議：MVP 先驗證 Engine A（成本低、確定性高），Engine B 校準持續進行；
  `completion-detection` 能力需涵蓋兩種模式的偵測契約
