# 上下文契约（context schema · v2）

> L0–L3 的唯一数据源：**L2 只写上下文，L1 只读并校验，L3 只渲染快照**。任何任务不得绕过本契约直接给 UI 传值或发问。

## 1. 结构

```json
{
  "goal":         { "text": "", "template": "T1..T8", "constraints": [] },
  "inputs":       { "chip": "", "project_root": "", "toolchain": "", "probe": "",
                    "serial_port": "", "baud": 115200, "artifact": "", "board": "",
                    "firmware_running": false, "decision": null },
  "missing":      [],
  "pending_ask":  null,
  "pending_confirm": null,
  "index":        { "files": {}, "hash": "", "indexed": false, "indexed_at": "" },
  "requirements": { "hardware": [], "software": [], "conflicts": [], "symptom": null,
                    "hypotheses": [], "impact": null, "ownership": null,
                    "doc_gap": [], "optimization": null, "t6_type": null },
  "hardware_dts": { "path": "hardware.dts", "nodes": [], "pending_todo": [], "before_hash": "" },
  "code":         { "layers": [], "mapping": [], "files": {}, "diff": [], "backups": [] },
  "build":        { "build_ok": false, "errors": [], "warnings": [], "artifact": "", "duration_ms": 0 },
  "flash":        { "flashed": false, "probe": "", "address": "" },
  "debug":        { "debugger": "", "session": "", "breakpoints": [] },
  "serial":       { "port": "", "baud": 115200, "log": [], "checks": [], "verified": false },
  "quality":      { "checklist": [], "issues": [] },
  "docs":         { "framework": "", "framework_raw": "", "code_style": "", "refs": [] },
  "style":        { "samples": [], "rules": [] },
  "execution":    { "chain": [], "current": "", "history": [], "attempts": {},
                    "status": "idle", "error": null, "fatal": false, "verdict": null },
  "snapshot":     null
}
```

## 2. 必填 inputs（缺失即 O8：停止并 `se-ask`）

| 键 | 说明 | 何时必填 | 消费任务 |
|---|---|---|---|
| `chip` | 芯片型号（如 `STM32G474RET6`） | 生成台账/代码、查手册 | `query_docs` `plan_ledger` |
| `project_root` | 工程根绝对路径 | 任何读写/构建 | `index_context` `build` |
| `toolchain` | 构建方式（IDE 工程 / CMake / Make） | `build` | `build` |
| `probe` | 探针类型与端口 | `flash` / `debug_start` | `flash` `debug_start` |
| `serial_port` + `baud` | 串口与波特率 | `serial_cli` | `serial_cli` |
| `artifact` | 固件产物路径（`.elf/.hex/.bin`） | `flash` / `debug_start` | `flash` `debug_start` |
| `board` | 板卡/原理图约束 | 引脚分配、外设规划 | `plan_ledger` |

`firmware_running`：用户确认“新固件已在运行”时置 true，用于 O13 的串口验证放行；任何源文件或构建产物变化后置 false。

## 3. 状态位与不变量

- `build.build_ok` 只能由 `build` 写 `true`；**任何源文件变更（patch_file/write_file/delete_file/diag_code/minimal_fix/apply_or_audit）立即置 `build_ok=false`、`flash.flashed=false`、`serial.verified=false`、`inputs.firmware_running=false`**。
- `flash.flashed` 仅由 `flash` 实测后置 true；`build_ok=false` 时不得为 true。
- `index.indexed`：`index.hash` 与文件实际哈希不一致 → false；任何写操作前必须为 true（O3）。
- `serial.verified`：只有 `serial_cli` 收到真实回传后可置 true；`observe` 只写 `serial.checks`，不置 verified。
- `execution.fatal=true` → 整链终止（O4），只能由 L1 生成新链恢复。
- `execution.status`：`idle | running | suspended | failed | done`；`suspended` 仅因 `pending_ask`/`pending_confirm`。
- `snapshot` 由 `ctx.freeze()` 生成，只读；L3 不得回写。

### 3.1 `execution.error` 对象

| 键 | 说明 |
|---|---|
| `task` | 失败任务 id |
| `code` | `guard_failed` / `accept_failed` / `tool_error` / `conflict` / `timeout` |
| `detail` | 原文、文件:行、回传输出摘要（禁止编造） |
| `recoverable` | bool；false 时置 `execution.fatal=true`（O4） |

## 4. 缺口、授权与回跳

| 情况 | 写入上下文 | L3 输出 |
|---|---|---|
| 缺必填参数 | `missing=[keys]` + `pending_ask={kind:"missing_param",question,options[],default,context_key,reason}` | `se-ask` 补参（每轮 ≤2 问） |
| 需用户授权（删除/替换/烧录/重构） | `pending_confirm={action,scope,risk}` + `pending_ask={kind:"confirm",...}` | `se-ask` 确认卡片 |
| 需用户执行/补设备（观测、烧录、接线） | `pending_ask={kind:"observe",question,options[],context_key,reason}` | `se-ask` 操作指引 |
| 用户确认/补参/换方案 | 回跳处理器写 `inputs.decision` / `inputs.*`，清空 `pending_ask` | 回 L1，生成新链 |

### 4.1 `pending_ask` 字段（L3 唯一可渲染的提问载荷）

| 键 | 说明 |
|---|---|
| `kind` | `missing_param` / `confirm` / `observe` / `choose` |
| `question` | 一句话问题，必须含“为什么需要这个信息或确认” |
| `options[]` | 2–8 项 `{label, value, description}`；确认类必须含“否/取消” |
| `default` | 默认值或默认选项，可空；仅当用户明确同意按默认时才采用 |
| `context_key` | 回答写回 `context.inputs` 的键名 |
| `reason` | 触发依据（如 `O8 缺 chip`、`O12 删除需确认`、`O13 未确认新固件`） |

规则：

- **L1/L2 只写入，L3 只渲染，回跳写回 `inputs`**；三方都不越权。
- `pending_ask` 非空时，禁止下发新链、禁止继续执行队列（`execution.status=suspended`）。
- 每轮最多 2 个问题（硬上限 6），选项不足 2 个时必须允许自由回答。
- 只读工具（`se-list`/`se-read`）不产生 `pending_ask`。

## 5. 文件修改规则

- 每条 `patch_file` / `ledger_patch` 必须记录 `before_hash`、`after_hash`、行范围、`code.backups[]`。
- 同一轮内多次修改同一文件必须串行并更新哈希，禁止并行覆盖。
- 写操作前 guard 检查：目标存在、`index.indexed=true`、已 `read_file`、哈希匹配；新建文件例外，只需目标不存在 + 已索引。
- 文档类写入（`framework_write`/`code_style_write`/`issue_log`）同样受 O3 约束，但允许不产生 `build_ok=false`（仅文档变更）。
