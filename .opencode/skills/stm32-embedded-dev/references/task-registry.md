# 任务注册库（task registry · v3）

> **唯一可被 L1 编排引用的任务集合。** 任务只读写 `context`（见 `context-schema.md`），不直接面向用户/UI、不直接发问（O6）。未注册任务禁止出现在任务链中。

三种条件缺一不可：

| 条件 | 回答的问题 | 不过的后果 |
|---|---|---|
| `guard` | 能不能开工？ | 不下发/不执行（O1/O2/O3/O13） |
| `accept` | 干完算不算合格？ | **该任务失败**，按 `on_error` 处理 |
| `on_error` | 失败怎么办？ | 写错误/`pending_ask` 进上下文，交回 L1 |

> `accept` 是唯一验收来源；领域文件只提供细节，不另立标准。任何源文件变更任务都必须同时置 `build.build_ok=false`、`flash.flashed=false`、`serial.verified=false`、`inputs.firmware_running=false`（下表简写为 `失效链`）。

## 1. 需求与索引（4）

| id | 用途 | 工具/协议 | 参考文件 | context_in → context_out | guard | accept | on_error |
|---|---|---|---|---|---|---|---|
| `collect_requirements` | 归集目标/约束/缺失/复用资源 | 推理 | — | `goal` → `requirements`, `missing` | 请求已表达 | 目标与约束可量化；缺失项全入 `missing`；复用资源带 `文件:对象` | 信息不足 → 写 `pending_ask`，`status=suspended` |
| `index_context` | 建文件索引与哈希 | `se-list` `se-read` | — | `inputs.project_root` → `index{files,hash,indexed}` | 工程目录可读 | 覆盖构建文件/入口/源码目录；hash 写入；`indexed=true` | 目录不可读 → `fatal` |
| `read_context` | 读既有台账/架构/文档 | `se-read` | — | `inputs.project_root`, `index` → `code.mapping`, `hardware_dts`, `docs.framework_raw` | `index.indexed=true` | 台账/架构/文档 三项各有结论（有/无/路径） | 缺文件记 `missing`，不阻断只读 |
| `query_docs` | 查手册事实（AF/IRQn/DMA/寄存器） | 推理（查 RM/DS） | — | `inputs.chip`, `requirements.hardware` → `docs.refs`, `hardware_dts.pending_todo` | 有 `chip` 或 `board` | 每个事实有章节/页码出处；查不到写 `<TODO>` | 无法查证 → 记 `pending_todo`，禁止猜值（O9） |

## 2. 读写与补丁（5）

| id | 用途 | 工具/协议 | 参考文件 | context_in → context_out | guard | accept | on_error |
|---|---|---|---|---|---|---|---|
| `read_file` | 读目标文件 | `se-read` | — | `index`, `target` → `code.files[target]` | `indexed=true`；文件存在 | 返回完整内容或明确错误；记录 hash | 不存在 → 失败，新建改用 `write_file` |
| `patch_plan` | 生成最小改动清单 | 推理 | — | `code.files`, `requirements` → `code.diff` | 目标已读取 | 每处含 文件/位置/原值/新值/原因；越权项标 `need_confirm` | 无法定位 → 回 `read_file` |
| `patch_file` | 按定位最小修改 | `se-replace` | — | `code.diff[target]`, `before_hash` → `code.files`, `code.backups`, `失效链` | `indexed=true`；已读；hash 匹配；改动在用户请求/已确认范围内 | diff 与计划一致；无越界；hash 更新；失效链已置 | 哈希冲突 → 失败，重读重规划 |
| `write_file` | 新建文件 | `se-file` | — | `target`, `content` → `code.files`, `失效链` | `indexed=true`；目标不存在；落在允许层 | 路径合规；内容符合模板；索引更新 | 已存在 → 改用 `patch_file` |
| `delete_file` | 删除普通文件（先备份） | `se-delete` | `other-operations.md` | 用户确认 + `targets` → `code.backups`, `失效链` | O12 全部满足；已索引；已确认；引用已闭合；目标 mtime/size 与索引记录一致；回退可行 | 备份存在；禁区检查通过；≤10；删除前复核 mtime/size 未变；删除后验证不存在；IDE 自动从 Makefile `C_SOURCES` 移除（Makefile 有未保存修改时跳过并提示） | 任一不满足 → 拒绝执行 |

## 3. 台账与架构（7）

| id | 用途 | 工具/协议 | 参考文件 | context_in → context_out | guard | accept | on_error |
|---|---|---|---|---|---|---|---|
| `read_ledger` | 读 `hardware.dts` | `se-read` | `ledger-schema.md` | `inputs.project_root` → `hardware_dts` | `indexed=true` | 节点解析完整；语法错误记录 | 无台账 → `nodes=[]` 并标注 |
| `plan_ledger` | 规划节点/参数/冲突 | 推理 | `ledger-schema.md`, `field-map.md` | `requirements`, `inputs.chip` → `hardware_dts.nodes`, `requirements.conflicts` | 需求已拆解 | 功能点全落表；冲突给 ≥2 替代；参数有来源或 `<TODO>` | 缺关键参数 → `pending_ask` |
| `write_ledger` | 写完整台账（T1） | `se-file` | `ledger-schema.md` | `hardware_dts.nodes` → `hardware_dts`, `index` | 规划完成；已有台账时先 `read_ledger`（新工程无台账可跳过） | TODO 集中；`app-bindings` 闭合；无重复占用；参数有来源；已有台账时记录 `before_hash`/备份 | 校验不过 → 退回 `plan_ledger` |
| `ledger_patch` | 增量改台账（T2/T5） | `se-replace` | `ledger-schema.md` | 受影响节点 + `before_hash` → `hardware_dts`, `code.backups` | `read_ledger` 完成；hash 匹配 | 仅改受影响节点；未涉及节点不变；引用闭合 | 冲突 → 失败并保留原文件 |
| `ledger_validate` | 台账自洽校验 | 推理 | `ledger-schema.md` | `hardware_dts` → `quality.checklist` | 台账已更新 | 引用/占用/TODO 三项逐条结论 | 不通过 → 阻断编码 |
| `plan_architecture` | 层级/目录/落点 | 推理 | `architecture.md` | `requirements`, `code.mapping`（可空） → `code.layers`, `code.mapping` | 需求/台账就绪 | 层级表+目录树+落点；依赖单向无环；根目录无业务源码；新工程给默认骨架 | 冲突 → 写 `pending_ask`（2 方案） |
| `codegen_by_layer` | 按层产码 | `se-file` / `se-replace` | `architecture.md`, `code-rules.md`, `module-convention.md`, `software-arch.md` | 台账/架构/风格 → `code.files`, `code.diff`, `失效链` | 架构/台账就绪 | 每文件标层+相对路径；参数回指台账；范式/调度/设计三结论齐全；非授权改动=0；安全动作链落地 | 编译错误 → 转 T4 |

## 4. 构建、烧录与调试（5）

| id | 用途 | 工具/协议 | 参考文件 | context_in → context_out | guard | accept | on_error |
|---|---|---|---|---|---|---|---|
| `build` | 触发真实构建 | `se-build` | — | `inputs.project_root`, `inputs.toolchain` → `build{build_ok,errors,warnings,artifact}` | 源码就绪；已索引 | 真实回传；0 error；warning 列出；`artifact` 存在 | 失败 → 按错误修复后重跑（≤2） |
| `flash` | 烧录固件 | **预览（未落地）** / 用户执行 | — | `build.build_ok`, `build.artifact`, `inputs.probe`；用户确认 → `flash{flashed,probe,address}` | **O1** + 用户确认 | 烧录后校验 ID/校验和/运行；`flashed` 仅实测后 true | 失败 → 保持 false 并报错 |
| `debug_start` | 启动调试会话 | **预览（未落地）** / 用户执行 | — | `build.artifact`, `inputs.probe` → `debug{debugger,session}` | **O2** | 会话建立；断点/暂停可用；artifact 与 build 一致 | 无探针 → 写 `pending_ask` |
| `serial_cli` | 串口命令收发 | `se-cli` | `serial-cli.md` | `serial.port/baud`, `build.build_ok`, `flash.flashed` 或 `inputs.firmware_running` → `serial.log`, `serial.checks`, `serial.verified` | **O13**；端口可用 | 有真实收发；命令有 `OK`/`ERR`；无响应记录排查 | 无响应 → 排查或 `pending_ask`，禁写“通过” |
| `observe` | 无串口观测（LED/GPIO/watch/寄存器） | 推理 + 用户执行 | `diagnostics.md` | 观测方案、`debug` → `serial.checks` | 至少一种手段且用户可执行 | 观测值有记录（次数/电平/watch）；能区分假设 | 无手段 → `pending_ask(kind=observe)` |

## 5. 诊断与质量（8）

| id | 用途 | 工具/协议 | 参考文件 | context_in → context_out | guard | accept | on_error |
|---|---|---|---|---|---|---|---|
| `collect_symptom` | 采集现象 | 推理 | `diagnostics.md` | `goal` → `requirements.symptom` | 有报错/现象 | 现象/复现/最近改动/供电时钟复位 齐全 | 不足 → `pending_ask`，不臆断 |
| `diag_hypotheses` | 排序可能位置 | 推理 | `diagnostics.md`, `debug-playbook.md` | `requirements.symptom`, `code.files` → `requirements.hypotheses` | 现象已采集 | 每条有证据+排除方法；按概率排序 | 无证据 → 标低概率 |
| `diag_code` | 诊断代码 | `se-replace` | `diagnostics.md` | `requirements.hypotheses` → `code.diff`, `失效链` | 假设已定 | 可观测/可移除/不改业务；标插入与移除方式 | 影响业务 → 拒绝生成 |
| `minimal_fix` | 最小修复 | `se-replace` | `code-rules.md` | 命中假设 + 观测 → `code.files`, `code.diff`, `code.backups`, `失效链` | 已定位 | 改动最小；有原因；回归点明确；诊断代码保留到回归后 | 需重构 → 升级 T1/T2 |
| `regress` | 回归验证 | `se-build` + `serial_cli`/`observe` | — | 修复后代码 → `quality.checklist` | `build.build_ok=true`；O13 满足 | 原现象复测通过；无新增 error/warning | 未通过 → 回 `diag_hypotheses` |
| `diag_cleanup` | 移除诊断代码 | `se-replace` | `diagnostics.md` | `code.diff` → `code.files`, `失效链` | `regress` 通过 | 无 `#if DIAG` 残留；记录移除方式；随后由 `build` 验证 0 error | 残留 → 失败并报告位置 |
| `consistency_check` | 8 维一致性核对 | 推理 | `checklist.md` | `hardware_dts`, `code.files` → `quality.checklist` | 代码/台账就绪 | 8 行逐行有状态；待改附动作；未核实带 TODO | 不过 → 退回对应任务 |
| `issue_log` | 记录问题与解决 | `se-issue` | `other-operations.md` | 真实问题/修复 → `quality.issues` | 有真实问题 | open/resolved 状态正确；原因与方案完整 | 非真实问题 → 不记录 |

## 6. 文档、风格与非代码操作（14）

| id | 用途 | 工具/协议 | 参考文件 | context_in → context_out | guard | accept | on_error |
|---|---|---|---|---|---|---|---|
| `framework_write` | 生成 `framework.md` 三章 | `se-file` | `framework-template.md`, `system-perception.md §4` | `code.layers`, `code.mapping`, `docs.framework_raw` → `docs.framework` | 架构/代码就绪；存在旧文档时已读；替换已确认 | 三章齐全；五维度匹配；耗时 `[预估]`；实测列留空 | 缺实测 → 留空不编造 |
| `framework_patch` | 增量更新点 | `se-replace` | `framework-template.md` | 变更项 + `docs.framework_raw` → `docs.framework` | 变更明确；旧文档已读 | 仅增量小节；标「无变更」项齐全 | 冲突 → `pending_confirm` |
| `collect_samples` | 收集风格样例 | `se-list` `se-read` | `code-style.md` | `inputs.project_root` → `style.samples` | 工程可读 | 来源/文件数/覆盖率写清；不足说明 | 无样例 → `pending_ask` |
| `style_rules` | 归纳风格规则 | 推理 | `code-style.md` | `style.samples` → `style.rules` | 样例已收集 | 每条可检查；有正例/反例；无空泛口号 | 规则不可执行 → 重写 |
| `code_style_write` | 写 `code_style.md` | `se-file` | `code-style.md` | `style.rules` → `docs.code_style` | 规则已归纳 | 章节完整；含自查清单；路径正确 | 覆盖旧文件 → 先备份 |
| `apply_or_audit` | 按风格编码/审计 | `se-replace` | `code-style.md` | `docs.code_style`, `code.files` → `code.diff`, `quality.checklist`, `失效链`（有代码变更时） | 风格已就绪；旧码改动已确认 | 新码逐条对照；旧码只列建议；偏差表完整 | 未确认 → 不批量改旧码 |
| `optimize_plan` | 优化方案 | 推理 | `code-style.md` | `code.files`, 目标 → `requirements.optimization` | 有明确目标 | 目标/约束/测量/风险/回退 齐全 | 无测量方法 → 不实施 |
| `classify` | T6-O/T6-C 分类 | 推理 | `other-operations.md`, `consulting-design.md` | `goal` → `requirements.t6_type` | 请求已明确 | 类型明确；咨询类承诺“不自动修改” | 模糊 → `pending_ask` |
| `impact_analysis` | 影响面分析 | 推理 | `other-operations.md` | `goal`, `code.mapping`, `requirements.t6_type` → `requirements.impact` | 已分类；已索引 | 构建/包含/链接/工程配置 四项结论；可恢复性 | 缺分析不得执行 |
| `execute_or_preview` | 受控执行或预览 | 预览 / 已落地协议 | `other-operations.md` | 确认 + `requirements.impact` → `code.backups`（非删除） | 已确认；回退可行（预检） | 执行有备份；未落地只预览，不声称完成 | 未确认 → 拒绝 |
| `verify_rollback` | 验证与回退 | `se-build` / `se-read` | `other-operations.md` | 变更 + `code.backups` → `quality.checklist` | 变更已记录回退方案（预检） | 验证步骤+回退命令；可恢复 | 无回退 → 拒绝执行（应在执行前拦） |
| `inventory` | 工程盘点 | `se-list` `se-read` | `system-perception.md §2` | `inputs.project_root`, `index` → `code.mapping`, `docs.framework_raw` | `indexed=true` | 构建/目录/入口/RTOS（或裸机结论）/文档 五项有证据；不臆测 | 证据不足 → 标 TODO |
| `ownership_judge` | 归属判断 | 推理 | `system-perception.md §3` | `code.mapping`, `docs.framework_raw` → `requirements.ownership` | 盘点完成 | 四类之一+证据；给“不做什么” | 无法判定 → 列不确定项 |
| `diff_docs` | 文档与代码对比 | 推理 | `framework-template.md`, `system-perception.md §4` | `docs.framework_raw`, `code.mapping` → `requirements.doc_gap` | 有现有文档（无则记缺失后继续） | 差异逐条（缺失/过期/冲突）；不覆盖旧文档 | 无文档 → 建议新建 |

## 7. L1/L3 内置函数（不是 L2 任务，不得进任务链）

| id | 层 | 用途 | context_in → context_out | 规则 |
|---|---|---|---|---|
| `plan_validate` | L1 | 总则校验 O1–O14 | `execution.chain`, `context` → `execution.verdict`, `execution.history` | 每条 O 有结论；REJECT 必须给条款号 + 违规步骤 + 修复建议 |
| `snapshot_render` | L3 | 快照渲染（模板 A–H / `pending_ask` 卡片） | `snapshot`, `pending_ask` → 无（只读） | 不得回写 context（O7）；模板小节齐全；提问卡必须来自 `pending_ask` |

## 8. 通用约束

- 任务失败必须写 `execution.status`、`execution.error{task,code,detail,recoverable}`；`accept` 不通过视同失败。
- 重试上限 2 次；不得跳过 `guard`；不得在 `se-file` / `se-replace` 之外改文件。
- 任务不得输出面向用户的措辞、不得直接发问；需要输入一律写 `pending_ask`，交 L3。
- 任务不得自行切换模板或升级任务类型；类型变化只能由 L1 重新编排。
- `write_file` 是底层写任务，由 `codegen_by_layer`、`apply_or_audit`、T6-C 文档写入内部调用，不单独出现在模板链。
- `flash` / `debug_start` 为可选任务，仅在 T4 或用户明确要求时插入；当前只有“预览 + 用户执行”，未落地自动协议；不得声称已自动完成。
