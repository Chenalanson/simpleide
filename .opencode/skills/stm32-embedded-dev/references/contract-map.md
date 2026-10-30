# 契约总表（contract map）

> 本文件不是知识库，是**防漏清单**：任何任务、模板、协议、上下文键的改动，都必须按本表同步。改契约前先读这里，改完按第 6 节自检。

## 1. 四层与文件职责

| 层 | 文件 | 职责 | 不允许 |
|---|---|---|---|
| L0 | `SKILL.md` §0 | O1–O14 硬规则、拦截点 | 被模板/AI/用户临时指令/evolution 覆盖 |
| L1 | `SKILL.md` §1、`orchestration-templates.md`、`task-registry.md` | 匹配模板、生成链、`plan_validate`、下发 | 读写文件、构建、烧录、发问 |
| L2 | `task-registry.md` + 15 本领域文件 | 执行原子任务，只写 context | 写 UI、直接发问、越过 guard/accept |
| L3 | `SKILL.md` §3、`templates.md` | 冻结快照、渲染模板/日志/`se-ask` | 修改 context、补步骤、执行任务 |

## 2. 任务 ↔ 参考文件 ↔ 主要输出

| 任务组 | 任务 | 参考文件 | 主要输出键 |
|---|---|---|---|
| 需求/索引 | collect_requirements / index_context / read_context / query_docs | —（query_docs 查 RM/DS） | `requirements` `index` `docs.refs` |
| 读写/补丁 | read_file / patch_plan / patch_file / write_file / delete_file | `other-operations.md`（删除） | `code.files` `code.diff` `code.backups` |
| 台账/架构 | read_ledger / plan_ledger / write_ledger / ledger_patch / ledger_validate | `ledger-schema.md` `field-map.md` | `hardware_dts` `requirements.conflicts` |
| 架构/编码 | plan_architecture / codegen_by_layer | `architecture.md` `code-rules.md` `module-convention.md` `software-arch.md` | `code.layers` `code.mapping` `code.files` |
| 构建/烧录/调试 | build / flash / debug_start / serial_cli / observe | `serial-cli.md` `diagnostics.md` | `build` `flash` `debug` `serial` |
| 诊断/质量 | collect_symptom / diag_hypotheses / diag_code / minimal_fix / regress / diag_cleanup / consistency_check / issue_log | `diagnostics.md` `debug-playbook.md` `checklist.md` | `requirements.symptom` `requirements.hypotheses` `quality` |
| 文档/风格/其他 | framework_write / framework_patch / collect_samples / style_rules / code_style_write / apply_or_audit / optimize_plan | `framework-template.md` `code-style.md` | `docs.framework` `docs.code_style` `style` |
| T6 | classify / impact_analysis / execute_or_preview / verify_rollback | `other-operations.md` `consulting-design.md` | `requirements.t6_type` `requirements.impact` |
| T8 | inventory / ownership_judge / diff_docs | `system-perception.md` §2/§3/§4 | `code.mapping` `requirements.ownership` `requirements.doc_gap` |
| L1/L3 内置 | plan_validate（L1）/ snapshot_render（L3） | — | `execution.verdict` / 无 |

## 3. 模板 ↔ 任务 ↔ 输出小节

| 模板 | 链（与 SKILL §1.2 一致） | 主要产出小节 |
|---|---|---|
| A | T1 链 | 1 上下文 / 2 需求 / 3 台账 / 4 dts / 5 架构与代码 / 6 framework / 7 核对 / 8 构建与串口 |
| B | T2 链 | 1 架构复用 / 2 台账片段 / 3 代码 / 4 framework 增量 / 5 一致性 / 6 待核对 / 7 构建与串口 |
| C | T3 链 | 1 现状 / 2 方案 / 3 diff / 4 影响面 / 5 验证 |
| D | T4 链 | 1 现象 / 2 诊断 / 3 定位 / 4 最小修复 / 5 验证 / 6 清理与预防 |
| E | T5 链 | 1 变更 / 2 dts 片段 / 3 一致性复核 / 4 代码衔接 |
| F | T6 链 | 1 清单 / 2 影响 / 3 预览或方案 / 4 结果 / 5 验证与回退 |
| G | T7 链 | 1 来源 / 2 规则 / 3 code_style / 4 应用与审计 / 5 优化 / 6 验证 / 7 交付 |
| H | T8 链 | 1 现状 / 2 归属 / 3 方案 / 4 framework 处理 / 5 重构 / 6 结论 |

## 4. 协议白名单

| 协议 | 用途 | 约束 |
|---|---|---|
| `se-list` / `se-read` | 列目录/读文件 | 只读，不需授权，不产生 `pending_ask` |
| `se-file` | 新建文件 / 整份重写 | 必须先 `index_context`；修改已有文件前必须 `read_file`（O3）；能局部改就不要整份覆盖 |
| `se-replace` | 局部修改已有文件 | 必须先 `read_file`；`SEARCH` 必须唯一匹配；只改必要片段（O3、最小改动 M1–M7） |
| `se-delete` | 删除文件 | 只能由 `delete_file` 使用；O12 全部满足 |
| `se-build` | 真实构建 | 必须引用回传输出；0 error 才是 `build_ok=true` |
| `se-cli` | 串口收发 | 必须满足 O13；引用真实 `OK/ERR` |
| `se-ask` | 提问/确认卡 | **只能由 L3 渲染**，内容必须来自 `pending_ask` |
| `se-issue` | 记录问题 | 只记真实问题；open→resolved 同标题 |
| `se-evolve` | 提议长期规则 | 最多 5 条；用户点击保存才写入 |

**未落地（只能预览 + 用户执行）**：`flash`、`debug_start`、重命名、移动、目录创建、批量清理。不得声称自动完成。

## 5. 上下文键所有权

| 命名空间 | 写入者 | 只读者 |
|---|---|---|
| `goal` / `inputs` / `missing` / `pending_*` | L1 + 回跳处理器 | 所有层 |
| `index` | `index_context` | 所有写任务 |
| `requirements` | collect / plan / diag / classify / impact | L1/L3 |
| `hardware_dts` | read/plan/write/ledger_patch | codegen / check |
| `code` | read_file / patch / codegen / apply_or_audit | build / check |
| `build` | `build` | flash / serial / L3 |
| `flash` | `flash` | serial / debug / L3 |
| `debug` | `debug_start` | observe / L3 |
| `serial` | `serial_cli` / `observe` | L3 |
| `quality` | consistency / regress / verify | L3 |
| `docs` / `style` | framework / code_style / collect / style | codegen / L3 |
| `execution` | L1（编排器 + `plan_validate`） | L2 循环 / L3 |

## 6. 一致性自检（每次改完必跑）

1. 链中每个任务 id 都能在 `task-registry.md` 找到；没有孤立任务（注册但从不被引用，除 `?` 备选）。
2. `task-registry.md` 每个 `ref` 指向的文件存在，`§N` 指向的章节存在。
3. 每个写任务有 O3 前置：`index_context` +（修改时）`read_file`。
4. 每个 `flash` 前有 `build`；每个 `debug_start` 前有合法 `artifact` + `flashed` 或用户允许。
5. 每个 `serial_cli`/`regress` 满足 O13；无串口时有 `observe` 或 `pending_ask(kind=observe)`。
6. 删除只走 `delete_file`；`execute_or_preview` 不承担删除。
7. L2 不出现直接 `se-ask`；所有提问来自 `pending_ask`，所有授权来自 `pending_confirm`。
8. `SKILL.md` §1.2、`orchestration-templates.md`、`templates.md` 的 0 节判定行，三者链完全一致。
9. 注册表里的 `context_in/out` 键都存在于 `context-schema.md`；schema 中每个键至少有一个写入者。
10. 没有未声明的 `se-*`；预览协议有明确标注。
11. 全库不含旧阶段词汇（stage gate 时代的编号与术语）。
12. 源文件变更任务必须置 `build_ok=false`、`flashed=false`、`serial.verified=false`、`firmware_running=false`；纯文档写入不得置 `build_ok=false`。
13. 用户已确认执行的写任务不得只输出计划或重复读取：已有正文的文件直接 `patch_file`，同一文件不得连续 `se-read` 两次，读完后的下一轮必须落盘（O14）。

## 7. 改动同步清单

| 你要改什么 | 必须同步的位置 |
|---|---|
| 新增/删除任务 | `task-registry.md`、`SKILL.md` §2.2 计数、本文件 §2、相关模板链、`context-schema.md`（若新键） |
| 新增/修改模板 | `SKILL.md` §1.2、`orchestration-templates.md`、`templates.md` 0 节判定行、本文件 §3 |
| 新增上下文键 | `context-schema.md`、`task-registry.md` 的 in/out、本文件 §5 |
| 新增协议 | 本文件 §4、`SKILL.md` §0（若涉及安全）、相关任务 tool 列 |
| 新增领域文件 | `SKILL.md` §4 路由、任务 `ref` 列、本文件 §2 |
| 修改 O 条款 | `SKILL.md` §0、受影响的 guard/accept、本文件 §6 自检项 |
| 修改目录/命名规范 | `architecture.md`、`code-rules.md`、`checklist.md`、本文件 §2 |
