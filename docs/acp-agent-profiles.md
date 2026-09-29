# ACP Agent Profiles — 實測證據（per-agent ACP facts）

> Status: **已實測**（本機 `opencode 1.18.32`，ACP over stdio，實際 NDJSON transcript）。
> 探測腳本與原始證據：`/tmp/acp_probe/`（`probe.py`、`probe2.py`、`transcript.ndjson`、
> `transcript2.ndjson`、`NN_*.ndjson` 逐步存證；不隨 repo 發佈）。本文件每個事實
> 標 **verified**（附一行 repro）或 **unverified**（附 blocker）。NDJSON 皆為逐字
> 擷取，非憑空編造。

## 0. 候選 agent 與環境

| 項目 | 值 |
|------|-----|
| Agent | `/usr/bin/opencode` v1.18.32（`agentInfo.name = "OpenCode"`） |
| 子命令 | `opencode acp`（"start ACP (Agent Client Protocol) server"） |
| 傳輸 | stdio、line-delimited JSON-RPC 2.0（NDJSON，無 Content-Length header） |
| 本機授權 | 已配置（initialize 未回 `auth_required`；`authMethods` 僅列出登入方法） |
| LLM | 預設 `opencode/big-pickle`（session configOptions `currentValue`） |

```sh
opencode acp --help   # 選項：--print-logs --log-level --pure --port(0) --hostname --mdns --cors --cwd
```

## 1. Handshake — `initialize` ✅ verified

請求（client → agent）：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize",
 "params":{"protocolVersion":1,
           "clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}}}
```

回應（逐字，節錄）：`protocolVersion` 為**整數 1**；`agentCapabilities` 含
`loadSession:true`、`mcpCapabilities{http,sse}`、`promptCapabilities{embeddedContext,image}`、
`sessionCapabilities{close,fork,list,resume}`；`authMethods:[{"id":"opencode-login",…}]`；
`agentInfo:{"name":"OpenCode","version":"1.18.32"}`。

```json
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,
 "agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true,"sse":true},
  "promptCapabilities":{"embeddedContext":true,"image":true},
  "sessionCapabilities":{"close":{},"fork":{},"list":{},"resume":{}}},
 "authMethods":[{"description":"Run `opencode auth login` in the terminal",
                 "name":"Login with opencode","id":"opencode-login"}],
 "agentInfo":{"name":"OpenCode","version":"1.18.32"}}}
```

- 冷啟動耗時：**9–16 s**（stderr：plugin 載入、LSP server 探測為大宗）。
  repro：`python3 /tmp/acp_probe/probe2.py a`（transcript2 計時欄位）。

## 2. `session/new` ✅ verified

```json
{"jsonrpc":"2.0","id":2,"method":"session/new",
 "params":{"cwd":"/tmp/acp_probe/sandbox","mcpServers":[]}}
→ {"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_f143ce210ffedTVIIFQ7X1fxeZ",
   "configOptions":[{"id":"model","type":"select","currentValue":"opencode/big-pickle","options":[…]},
                    {"id":"mode","type":"select","currentValue":"orchestrator",
                     "options":[{"value":"build",…},{"value":"plan",…},…]}]}}
```

- `sessionId` 形狀：`ses_` + 隨機字串（本例 `ses_f143ce210ffedTVIIFQ7X1fxeZ`）。
- 回應附 `configOptions`（model / mode 可切換；`mode` 選項 `build`/`plan` 由
  opencode 內建，`orchestrator` 來自本機 plugin 設定）。repro：同 §1，transcript
  `02_session_new.ndjson`。

## 3. `session/prompt` 與完成信號 ✅ verified

```json
{"jsonrpc":"2.0","id":3,"method":"session/prompt",
 "params":{"sessionId":"ses_f143…","prompt":[{"type":"text","text":"Reply with exactly the single word: pong."}]}}
```

完成信號 = **`session/prompt` 的 response 本身**（非通知）：

```json
{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn",
 "usage":{"inputTokens":47317,"outputTokens":2,"totalTokens":51246,"cachedReadTokens":3927},"_meta":{}}}
```

- 實測 `stopReason` 只出現 `"end_turn"`（兩次純文字 + 兩次含工具的 turn 皆然）。
  `max_tokens`/`refusal` 等其他值：**unverified**（blocker：需人為觸發，本輪未產生）。
- 小任務單 turn 實測延遲 **6–8 s**（big-pickle；gateway 排隊另計）。
  repro：`python3 /tmp/acp_probe/probe.py`（transcript.ndjson t 欄位：prompt→resp ≈6.3 s）。

## 4. `session/update` 通知變體（實測到的形狀）✅ verified

所有通知都沒有 `id`；`params.sessionId` 必出現；判別欄位是
**`params.update.sessionUpdate`**（不是 JSON-RPC 的 method，method 固定為 `session/update`）。
以下逐字節錄（完整行見 `03_prompt_rid3.ndjson`、`04_prompt_rid4.ndjson`）：

| sessionUpdate 值 | 實測形狀（欄位） |
|------------------|------------------|
| `available_commands_update` | `availableCommands:[{name,description,…}]`（每次 prompt 前重發） |
| `agent_message_chunk` | `messageId:"msg_…"`, `content:{type:"text",text:"pong"}` |
| `agent_thought_chunk` | `messageId:"prt_…"`, `content:{type:"text",text:…}` |
| `user_message_chunk` | 同上；**僅在 session/load 重播時觀測到** |
| `tool_call` | `toolCallId`, `title:"write"`, `kind:"edit"`, `status:"pending"`, `locations:[]`, `rawInput:{}` |
| `tool_call_update` | 同 id 帶 `status:"in_progress"`（`rawInput{filePath,content}`、`locations[{path}]`）→ `status:"completed"`（`content:[{type:"content",content:{…}}]`、`rawOutput{output,metadata}`）或 `status:"failed"`（`rawOutput{error:"…"}`，見 §6） |
| `usage_update` | `used:51244, size:200000, cost:{amount:0,currency:"USD"}` |

```json
{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"ses_f143…",
 "update":{"sessionUpdate":"tool_call","toolCallId":"call_function_84pyzjrffvxe_1",
           "title":"write","kind":"edit","status":"pending","locations":[],"rawInput":{}}}}
```

- **未觀測到的變體**（ACP 規格存在、本環境未觸發）：`plan`、`current_mode_update`、
  `file_system_tree_update`… — **unverified**，blocker：本輪任務未產生對應活動；
  實作時不可發明形狀，遇到未知 `sessionUpdate` 值應原樣保留（pass-through）。

## 5. Permission semantics ✅ verified（含一個行為異常）

實測結論：

1. **預設（無任何 permission 設定）= 自動允許，ACP 上不會出現權限請求。**
   兩個獨立 session 的檔案寫入皆直接執行，全程零 `session/request_permission`；
   stderr 揭示決策來源：`evaluated permission=edit pattern=tmp/…/perm.txt
   action.permission=* action.action=allow action.pattern=*`（萬用 allow 規則）。
   全域/使用者設定檔（`opencode.json`、`oh-my-opencode-slim.json`、`config.json`）
   皆**無** `permission` 鍵 → 萬用 allow 是 opencode 的預設行為。
2. **`permission.edit = "ask"`（project 級 `opencode.json`）→ 觸發 server→client
   request：`session/request_permission`**，完整實測形狀：

```json
{"jsonrpc":"2.0","id":0,"method":"session/request_permission",
 "params":{"sessionId":"ses_f142…",
  "toolCall":{"toolCallId":"call_function_b8soq9wnryui_1",
    "title":"/tmp/acp_probe/ask_proj/perm.txt","kind":"edit","status":"pending",
    "locations":[{"path":"/tmp/acp_probe/ask_proj/perm.txt"}],
    "rawInput":{"filepath":"/tmp/acp_probe/ask_proj/perm.txt","diff":"Index: …\n+++ …\n@@ -0,0 +1,1 @@\n+ok\n"},
    "content":[{"type":"diff","path":"/tmp/acp_probe/ask_proj/perm.txt","oldText":"","newText":"ok\n"}]},
  "options":[{"optionId":"once","kind":"allow_once","name":"Allow once"},
             {"optionId":"always","kind":"allow_always","name":"Always allow"},
             {"optionId":"reject","kind":"reject_once","name":"Reject"}]}}
```

- 注意：agent 主動發起的 request **id 是整數，從 0 起算**（與 client 發起側的 id
  空間獨立）——這是 `RequestId` 需要 untagged `u64/i64/String` 的直接證據之一。
- 回覆形狀（client → agent）：`{"jsonrpc":"2.0","id":0,
  "result":{"outcome":"selected","optionId":"once"}}`。
3. **拒絕路徑（verified）**：選 `reject_once` → 工具以
   `tool_call_update status:"failed"` 收場（無 JSON-RPC error）：
   `rawOutput.error = "The user rejected permission to use this specific tool call."`，
   之後 agent 回覆文字並以 `stopReason:"end_turn"` 正常結束 prompt。
   → **失敗要從 `tool_call_update.status=="failed"` 偵測，不能等 error response。**
4. **異常（verified，照實記）**：選 `allow_once`（optionId `"once"`）在 v1.18.32
   **也被記為拒絕**（同上訊息、檔案未建立）。allow 決策在 ACP headless 模式下
   目前無法生效 —— 對「v1 一律 deny」的 wrapper 策略無影響（deny 路徑 verified）。
   repro：`cd /tmp/acp_probe && PERM_ACTION=reject python3 probe2.py b /tmp/acp_probe/ask_proj`。
5. **CLI 旗標**：`opencode acp --help` 無任何 allow/bypass 權限旗標（`--pure` 僅停
   用外部 plugin）。permission 由 config 決定（`~/.config/opencode/*` 全域、
   `<project>/opencode.json` 專案級）。
   repro：`opencode acp --help 2>&1 | grep -i perm`（無輸出）。

## 6. Session persistence（跨行程 `session/load`）✅ verified

行程結束後另起新 process、重新 `initialize`、以原 `sessionId` 送：

```json
{"jsonrpc":"2.0","id":2,"method":"session/load",
 "params":{"sessionId":"ses_f143ce210ffedTVIIFQ7X1fxeZ","cwd":"/tmp/acp_probe/sandbox","mcpServers":[]}}
```

- **成功**：response 為 configOptions（同 session/new）；response **之前**會把整段
  歷史以 `session/update` 重播回來（`user_message_chunk` + `agent_message_chunk` +
  `agent_thought_chunk` + `tool_call`/`tool_call_update`，<0.1 s 內一次傾瀉）。
- **記憶驗證**：load 後問「我之前要你回什麼單字？」→ `agent_message_chunk`
  `"pong"`、`stopReason:"end_turn"`。跨行程記憶成立。
  repro：`python3 /tmp/acp_probe/probe.py`（transcript.ndjson 第 20–33 幀）。
- **不存在的 sessionId（verified）**：回 JSON-RPC error（非自訂碼）：

```json
{"jsonrpc":"2.0","id":2,
 "error":{"code":-32603,"message":"Internal error: OpenCode service failure",
          "data":{"service":"session"}}}
```

  repro：`python3 /tmp/acp_probe/probe2.py a`。

## 7. Exit behavior ✅ verified

- Client 關閉 stdin（EOF）→ agent **優雅退出，exit code 0**（兩個獨立 process 均驗證，
  收尾 <15 s）。repro：`python3 /tmp/acp_probe/probe.py`（`05_exit_eof.ndjson`：
  `{"exit_code": 0}`）。
- SIGTERM 路徑：**unverified**（blocker：EOF 已足夠，未刻意觸發 SIGTERM）。
- 無 shutdown/close 方法呼叫需求（agentCapabilities 的 `sessionCapabilities.close`
  存在但本輪未測：**unverified**，blocker：非 T1 必要路徑）。

## 8. 對 wrapper 實作（T2+）的直接含義

| 事實 | 含義 |
|------|------|
| 完成信號 = prompt response 的 `stopReason` | client 需 request/response 對應表（id → pending） |
| 失敗在 `tool_call_update.status=="failed"`（非 error response） | TaskOutcome 映射（T4）必須掃 update 流 |
| 權限預設 auto-allow；request 僅在 ask 設定下出現 | wrapper 不需主動觸發權限；收到 `session/request_permission` 時回 deny（id:0 整數起算） |
| `session/load` 可跨行程恢復 + 重播 | 長任務可恢復；重播期間的通知要可辨識（`user_message_chunk` 混雜出現） |
| exit code 0 on stdin EOF | 收尾 = 關 stdin + wait，不需要 kill |

## 9. 探測預算與方法記錄

- LLM 往返共 6 次（pong×1、預設設定下寫檔×2〔兩個獨立 session 驗證 auto-allow〕、
  load 後 replay 提問×1、ask-mode 寫檔×2），單次最長 ≈8 s，遠低於 15 分鐘 blocker 線。
- 全部 NDJSON 證據在 `/tmp/acp_probe/transcript*.ndjson` 與逐步存檔；本文件引用
  的每一行皆逐字取自該處。
- `/tmp/openab_ref/` 參考檔在探測當下不存在（目錄缺席）→ T2 未參考任何上游
  實作，無 MIT attribution 必要性（結構皆為本 repo 自訂）。
