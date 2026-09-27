# opencode Engine-A adapter（add-opencode-adapter）

## Why

排程端 change（worker-delegation delta）已把 worker adapter 白名單擴為
五值並實作 `OPENCODE_CONFIG` 注入；本 repo 的 bridge enum 目前只有
`aider | codex | rho | zero`，兩端對不上（兩端同名鏡像鎖需同步落地）。

opencode 已就位可驗（本機實測 `opencode 1.18.32`，`anomalyco/opencode`）：

- `opencode run <prompt>` 是官方 one-shot 模式（文件明言 scripting /
  automation 用途）；headless 實測通過（`setsid` 脫離終端、非 TTY、piped
  stdin → exit 0，冷啟動 ~6s）。
- 不給 `--format` 時 stdout **只有最終答案文字**（乾淨，最省事）；TUI 風格
  header 走 stderr。
- 無效 model 一律 **exit 1** ⇒ exit code 即足以判 model 錯誤，不需前置
  查表。失敗耗時依 provider 而異：研究側 mimo 案例 ~3.2s 本地 catalog
  快敗；T5 smoke 實測 `nosuchprovider/*` 與 `openrouter/<不存在>` 兩案例
  皆 ~20s 才敗（可能含網路查詢），stderr 同為遮蔽 `UnknownError`。
- 權限**預設全開**（config docs：allows all operations）⇒ 不需審批旗標。
- 已知破口（upstream #36413）：tool call 被 auto-reject 時 **exit 0 + 空
  stdout** ⇒ adapter 必須把「exit 0 且無文字輸出」判為 Failed。

## What Changes

- `adapter/mod.rs`：enum 加 `Opencode`（`ALL`／`label()`／窮舉 `run()`
  分派）；白名單鏡像鎖測試改斷言恰為
  `["aider", "codex", "opencode", "rho", "zero"]`（與排程端 `spec.rs`
  同名鎖）。
- 新增 `adapter/opencode.rs`：組 `opencode run <prompt>` 的 `ExecSpec`——
  prompt 走 argv（variadic positional），workdir 走 runner `current_dir`
  且 runner 同步匯出 `PWD`（opencode 無 `-C` 旗標；實測 1.18.32 的
  project 探測只認 `PWD`，只給 `current_dir` 會讓它動手於 spawn 端的
  repo）；**MUST NOT** 帶
  provider／model／key 旗標（部署 config 經 `OPENCODE_CONFIG` 由排程端
  注入）；**MUST NOT** 以 `-` 為 prompt 佔位（upstream #28407 教訓）。
- 完成判定：exit 0 + stdout trim 空 → `Failed{reason: no output}`
  （#36413 安全網）；error patterns 依 smoke 實測收窄（不引入過寬 marker）。

## Spec changes

Extends capability `exec-adapter`：白名單五值（MODIFIED）＋ opencode 組裝
契約、#36413 空輸出分類、單一 artifact（ADDED）。權威 adapter 的 exec-adapter
delta 落於私有的 spec 鏡像 repo（升格處）；本 repo
`specs/specs/exec-adapter/spec.md` 於 close 時同步 adapter 表列。

## Out of scope (this change)

- `opencode acp`（stdio 子協商）與 `serve`／`--attach`（HTTP+SSE）——與
  `ExecSpec` one-shot 形狀不兼容，屬另一 capability（YAGNI）。
- `--format json` JSONL 解析——default format 的乾淨 stdout 已足夠。
- session 副作用隔離——opencode 每次 run 自動開 session row（SQLite，無法
  停用），無害；per-worker `XDG_DATA_HOME` 隔離留待需要。
- model 前置查表（`opencode models`）——無效 model 已 exit 1（快敗耗時
  不影響契約），前置只會多加每次 ~2.8s 開銷。
- Engine B / PTY——opencode 走 Engine A。
- 部署值（provider／model／key、`OPENCODE_CONFIG` 路徑）——一律由呼叫端
  環境注入，本 repo 不寫死任何部署路徑或憑證。

## Acceptance

1. `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test` 全綠
2. 單元測試（無 LLM）：組裝 argv 形狀、白名單五值鎖、#36413 空輸出分類
   （exit 0 空 → Failed）、fake-agent exit 0/1 e2e
3. Live smoke：真實 `opencode run` → `Completed` + git diff artifact；無效
   model → `Failed`（exit 1，T5 實測 ~20s）；實測 default-format 錯誤輸出落
   `opencode.rs` 註解（error patterns 依證據收窄）
4. 鏡像對帳：lock set == 排程端 `spec.rs` 同名鎖 / 權威 exec-adapter spec
   `["aider", "codex", "opencode", "rho", "zero"]`
