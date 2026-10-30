---
name: stm32-embedded-dev
description: "STM32 嵌入式开发四层执行器：L0 执行总则（硬守卫/拦截器）→ L1 任务编排（模板匹配或 AI 编排，只生成并校验任务链）→ L2 内部任务（索引、读写、补丁、台账、分层代码、构建、烧录、调试、串口等原子任务，只读写上下文）→ L3 外部输出（只读快照，渲染为编辑器 UI/日志/确认框/报告，模板 A-H）。覆盖 T1 整机开发、T2 单外设增补、T3 逻辑修改、T4 故障排查、T5 台账维护、T6 其他操作、T7 代码风格、T8 系统整体感知；hardware.dts 台账先行、分层 C 代码、模块化与调度设计、framework.md、构建/烧录/串口验证与回跳重排。触发词：STM32 写代码/写程序/配置外设/串口回显 echo/ADC 采集/PWM 输出/I2C/SPI/CAN/DMA/FreeRTOS 任务/分层架构/模块驱动/设备管理器/状态机调度/framework 架构图/现有工程续写补全/编译报错/HardFault/运行不工作/硬件台账 hardware.dts 维护/改参数改逻辑/删除文件/重命名/移动文件/清理旧文件/工程维护/代码风格/code_style.md/学习我的风格/模块封装/极致优化/代码审查/诊断代码/LED 指示/阅读别人的工程/分析工程架构/系统整体感知/工程归属/替换 framework.md/重构目录/烧录/调试。"
---

# STM32 嵌入式开发执行器（四层固定版）

**总则管能不能干；编排管按什么顺序干；内部任务真的去干；外部输出只告诉你干成什么样。**

## 0. 执行总则（Ordinance · 全程锁定）

> 硬宪法：L1/L2/L3 每次动作前先过本层。模板、AI 编排、用户临时指令、evolution 规则都不得覆盖；违规直接拦截并给出条款号。

| # | 条款 | 拦截点 |
|---|---|---|
| O1 | 烧录：必须先 `build` 且 `build.build_ok == true`；禁止烧旧产物 | flash |
| O2 | 调试：必须有合法 `build.artifact`，且 `flash.flashed == true` 或用户显式允许调试既有固件 | debug_start |
| O3 | **修改**已有文件（含 `se-file` 整份重写）：`index_context` → `read_file` → `patch_file`（或 `write_file`）；**新建**文件：`index_context` → 目标不存在确认 → `write_file`（新建无需 read_file）；禁止用 `se-file` 绕过已读要求 | 任何写操作 |
| O4 | 任一任务返回不可恢复错误：立即终止整条链，不再编排、不再执行后续任务 | 任务失败 |
| O5 | 模板链失败不得静默切到 AI 自由编排；必须 `se-ask` 用户确认或显式标注降级 | 编排回退 |
| O6 | L2 不得直接写 UI/弹窗/日志面板/进度条，也不得直接发问；展示与提问只经 L3 | L2 执行中 |
| O7 | L3 只渲染快照，不得修改上下文；用户回答由回跳处理器写入 `inputs`，再回 L1 | L3 输出 |
| O8 | 上下文缺必要参数（芯片、工程路径、工具链、探针/串口）：停止并 `se-ask` 补全，禁止猜测 | 编排前 |
| O9 | 硬件事实（AF/IRQn/DMA 请求/寄存器地址/电气极限）未核实：用宏或 `<TODO>` 承载并注明查阅位置，禁止猜值 | 台账/代码 |
| O10 | 功率/电机/逆变：首轮输出即给安全警示；代码含刹车、使能安全态、故障动作链（故障→停 PWM→断使能） | 模板展开 |
| O11 | 未授权不改既有逻辑、时钟、引脚、生成代码；改动逐处登记（文件/位置/原值/新值/原因） | 写文件 |
| O12 | 删除只走 `delete_file`（`se-delete`）：用户明确要求、工程内普通文件、非目录/生成目录/`.simple-embedded`/`.se-workspace`、一次 ≤10 个、先备份 | 删除 |
| O13 | 串口/运行验证前：`build.build_ok == true` 且（`flash.flashed == true` 或用户确认已运行新固件）；不得把旧固件输出当本轮结果 | serial_cli / regress |
| O14 | **执行落盘**：用户已确认修改/落盘后，禁止只输出计划、承诺或重复读取；已拿到正文的文件必须直接 `patch_file`/`write_file`，同一文件不得连续 `se-read` 两次（读取被截断或文件已变化除外）；读完后的下一轮必须落盘；缺正文的文件不得猜写，先落盘已知文件并明确列出待读项，不得把部分完成说成全部 | patch_file / write_file / se-read |

**校验只有两种结论**：`PASS` → 下发；`REJECT` → 终止，输出「违规条款 + 违规步骤 + 修复建议」。

## 1. 任务编排（Orchestration · 只规划不干活）

### 1.1 固定流程

```
输入：context 快照 + 用户目标 + 模板库(orchestration-templates.md) + 任务库(task-registry.md)
  ↓
[1] 读上下文：必要参数齐全？(O8) 否 → 写 missing + pending_ask → L3 渲染提问 → 本轮终止
  ↓
[2] 匹配：命中模板 T1–T8 → 展开任务链并填充参数
            未命中 → AI 编排：只能从任务库选 id，输出 {chain[], reason, assumptions}
  ↓
[3] plan_validate（L1 内置校验器）：逐条过 O1–O14，检查每步 guard、顺序、副作用
            PASS → [4]；REJECT → 终止并说明违规条款与修复建议，不得下发
  ↓
[4] 下发待执行任务队列（每步声明 context_in[] / context_out[] / guard / accept）
```

### 1.2 模板库（**场景差异只允许写在这里**）

| 模板 | 匹配条件 | 任务链骨架 | 输出模板 |
|---|---|---|---|
| T1 整机/新功能 | 新板级 / ≥2 类外设 / 需求→完整程序 | collect_requirements → index_context → query_docs? → read_context? → plan_ledger → write_ledger → plan_architecture → codegen_by_layer → framework_write → consistency_check → build → serial_cli? | A |
| T2 单外设增补 | 现有工程加一类外设/一路功能 | index_context → read_context → query_docs? → plan_ledger → ledger_patch → codegen_by_layer → framework_patch?/framework_write? → consistency_check → build → serial_cli? | B |
| T3 逻辑修改 | 硬件不变，改参数/状态机/协议/已定位 bug | index_context → read_context? → read_file → patch_plan → patch_file → framework_patch? → build → serial_cli? | C |
| T4 故障排查 | 编译/链接/HardFault/运行现象 | index_context → read_context? → collect_symptom → diag_hypotheses → diag_code → build → flash? → debug_start? → serial_cli?/observe? → issue_log(open) → minimal_fix → framework_patch? → build → regress → issue_log(resolved) → diag_cleanup → build | D |
| T5 台账维护 | 只改 `hardware.dts`，不出代码 | index_context → read_ledger → ledger_patch → ledger_validate | E |
| T6 其他操作 | 删除/重命名/移动/配置/文档/咨询设计 | index_context → read_context? → classify → impact_analysis → [confirm] → delete_file?/execute_or_preview → verify_rollback | F |
| T7 代码风格 | `code_style.md` 学习/审计/极致优化 | index_context → collect_samples → style_rules → code_style_write → apply_or_audit → optimize_plan? → build → serial_cli? | G |
| T8 系统感知 | 外来工程/架构判断/framework 替换/重构 | index_context → inventory → ownership_judge → diff_docs → [confirm] → framework_write/framework_patch | H |

判定规则：

- 多模板命中按「用户最终目标」归类；置信度低先给 ≤2 句理由，用户纠正后重排。
- T3 发现要改硬件 → 升 T2；T5 要求顺带实现 → 升 T2。
- T7/T8 与 T1–T5 同时出现时，不另起模板：T7 作为编码前置任务（先读或生成 `code_style.md`），T8 作为前置感知或后置文档任务插入链中。
- T8 默认只感知/对比/给方案，不改业务代码、不移动目录、不覆盖 `framework.md`；替换/重构必须确认后再生成新链。
- T2 若无现有 `framework.md`：`framework_patch?` 改走 `framework_write`。
- 模板链中途失败（O5）：`se-ask` 三选一「按建议降级 / 换模板 / 终止」，禁止自行续编。

### 1.3 编排语法与自动前置

- `→` 强依赖；`?` 按上下文条件插入；`[confirm]` = **暂停点**：L1 写 `pending_ask` + `pending_confirm`，链暂停，L3 渲染确认卡；用户回答后由回跳写入 `inputs`，L1 生成**新链**（剩余任务 + 新参数），不自动续跑旧队列。
- **O3 自动前置**：`plan_validate` 检查任何写操作前是否有 `index_context`；修改前还必须有 `read_file`；缺失则自动插入并记入 `execution.history`，无法插入则 REJECT。
- 每轮只允许一条活动链；用户确认、补参、换方案都生成新链，但 `context` 里的工程状态（索引、台账、构建结果）继续有效。

### 1.4 编排器禁止项

- 禁止写 `if 芯片==STM32 … else if 芯片==ESP32`：差异压进模板条件或上下文键。
- 禁止创造未注册任务 id；只能引用 `task-registry.md`。
- 禁止把「编排结果」当「执行结果」输出；未执行的任务不得写“已完成”。
- **执行优先（O14）**：用户已确认执行/落盘后，禁止只输出计划、承诺或重复读取；已有正文的文件直接 `patch_file`，仍缺的才 `read_file`。同一文件不得连续 `se-read` 两次（读取被截断或文件已变化除外）；读完后的下一轮必须落盘，不得再写“接下来修改/我将落盘”却不输出实际块。

### 1.5 缺正文时的落盘顺序（O3/O14 冲突消解）

用户催落盘不等于可以猜写。按以下顺序处理：

1. **先列目标表**：`文件 | 是否已有全文 | 动作`，不要凭记忆假设“应该读过”。
2. **有全文的**：立即 `patch_file`（`se-replace`）；只有新建或整份重写才 `write_file`（`se-file`），整份重写也必须有旧文件全文。
3. **缺全文的**：先 `read_file`（`se-read`，可带 `lines`）；已经读过且未变化的不重复读。
4. **删除源文件**：IDE 删除后会自动从 Makefile `C_SOURCES` 移除对应条目（Makefile 有未保存修改时跳过并提示），不需要先读 Makefile 才能删；但如果被删文件仍被某个未读文件引用（例如 `stm32f1xx_it.c` 引用 `huart1`），必须先读该文件，不能只删不改。
5. **部分完成**：本轮可以先落盘所有已读且安全的文件；未读文件明确列为“待读/未落盘”，说明依赖和下一步。禁止用 `se-file` 猜写未读文件，禁止把部分完成说成全部完成。
6. **验证**：落盘后 `se-build`；构建失败按真实错误修，不要用“应该能编译”结束。

## 2. 内部任务（Inner Tasks · 只改上下文）

### 2.1 任务契约

每个任务必须声明：`id` / `context_in[]` / `context_out[]` / `tool` / `ref` / `guard` / `accept` / `on_error`。

- **guard 管开工**：前置条件不满足就不执行（O1/O2/O3/O13）。
- **accept 管验收**：执行完逐条检查；不通过 = 该任务失败（重试 ≤2 次或 `fatal`→O4），不得带病进入下一步。
- **入参只从 context 取**：外部零散输入先由 L1 归一化写入 context，任务不直接收裸参数。
- **出参只写 context**：`status`、`data`、错误码/详情、`artifacts[]`、追加 `history`；任何源文件变更同时置 `build.build_ok=false`、`flash.flashed=false`。
- **不直接给 UI、不直接发问（O6）**：需要用户输入时写 `pending_ask`，交 L3 渲染。
- **写文件必须带定位**：目标行范围或期望哈希，冲突即失败，禁止整文件盲覆盖。

### 2.2 任务注册表

完整表（43 个 L2 任务 + 2 个内置函数）见 `references/task-registry.md`，逐行给 `guard / accept / on_error / ref`。本处只给分类计数，避免摘要与全表漂移：

| 类别 | 任务数 | 代表任务 |
|---|---|---|
| 需求与索引 | 4 | collect_requirements / index_context / read_context / query_docs |
| 读写与补丁 | 5 | read_file / patch_plan / patch_file / write_file / delete_file |
| 台账与架构 | 7 | read_ledger / plan_ledger / write_ledger / ledger_patch / ledger_validate / plan_architecture / codegen_by_layer |
| 构建/烧录/调试 | 5 | build / flash / debug_start / serial_cli / observe |
| 诊断与质量 | 8 | collect_symptom / diag_hypotheses / diag_code / minimal_fix / regress / diag_cleanup / consistency_check / issue_log |
| 文档/风格/其他 | 14 | framework_write / framework_patch / collect_samples / style_rules / code_style_write / apply_or_audit / optimize_plan / classify / impact_analysis / execute_or_preview / verify_rollback / inventory / ownership_judge / diff_docs |
| L1/L3 内置（非 L2） | 2 | plan_validate（L1 校验器）/ snapshot_render（L3 渲染器） |

### 2.3 失败与重试策略

| 场景 | 处理 |
|---|---|
| 可重试（网络抖动、串口占用、构建产物锁） | 同任务重试 ≤2 次，仍失败 → 置 `fatal` → O4 终止 |
| 不可恢复（编译错误未定位、文件哈希冲突、探针/端口缺失） | 立即置 `fatal=true`，终止整链，转 L3 报错 |
| 部分成功（多文件补丁只落一部分） | 记录已改文件 + 备份/回退方式，终止并报告，不继续 |
| 需用户授权/补参（删除、替换、烧录、观测设备） | 写 `pending_confirm` + `pending_ask`，暂停链，转 L3 渲染 |

### 2.4 执行循环

```
plan_validate(chain, ctx)                    # L1 内置，PASS 才继续
for task in queue:
    if not task.guard(ctx): write_error("guard_failed"); break      # 含 O1/O2/O3/O13
    ctx = task.run(ctx)                       # 只写上下文，不碰 UI、不发问
    if not task.accept(ctx): task_failed(task); break               # 验收不过 = 失败
    if ctx.execution.fatal: break
snapshot = ctx.freeze() → L3
```

## 3. 外部输出（Outer Output · 只读快照）

### 3.1 渲染契约

- **只读**：渲染对象是 `ctx.freeze()` 快照，禁止写回 context（O7）。
- **只翻译不决策**：不补步骤、不跳条款、不执行任务。
- 输出目标：编辑器 UI（进度/日志/变更预览）、用户（模板小节/报告/确认框）、外部调用方（MCP/OpenCode/其他工具）。

### 3.2 输出形态

| 场景 | 形态 |
|---|---|
| 链开始/进行中 | 判定行 + 进度（当前任务 id + 阶段 + 已用步骤） |
| 链结束 | 对应模板小节 A–H（骨架见 `references/templates.md`） |
| 需授权/补参/选择 | `se-ask` 卡片：读取 `context.pending_ask` 渲染；L3 不改上下文，回答由回跳写入 `inputs` |
| 需观测设备/无法验证 | `se-ask` 卡片（`kind=observe`）；禁止静默跳过验证 |
| 构建/串口证据 | 必须引用真实回传（`error`/`warning` 行、`OK`/`ERR`），禁止编造 |
| 失败 | 错误码 + 失败任务 + 已完成项 + 回退方式 + 建议下一步 |
| 只读工具 | `se-list`/`se-read` 直接执行，不弹确认框、不产生 `pending_ask` |

### 3.3 模板索引（骨架全文：`references/templates.md`）

`T1→A｜T2→B｜T3→C｜T4→D｜T5→E｜T6→F｜T7→G｜T8→H`

小节编号、标题、表格行列写死；不适用写 `N/A` 不删行；空单元格填 `—`；模板未预设但必需的内容并入最接近小节，不新增编号。增量更新未涉及的小节写「无变更」，初次生成无内容写 `N/A`。

### 3.4 回跳回路（唯一重入入口）

```
L3 输出结果 → 用户确认/改意图/补参数 → 回跳处理器写入 context.inputs → 回 L1 重新编排 → L2 执行 → L3 再输出
```

禁止：UI 确认直接改文件、跳过构建、直接烧录、绕过编排续跑旧链（O7）。

### 3.5 状态行格式（供 UI/日志解析）

```
[L1] T2 单外设增补 | chain: index_context→read_context→… | template B
[L2] task=build status=ok build_ok=true warnings=3 artifact=build/app.hex
[L3] 构建通过（0 error / 3 warning）| 待串口验证
```

## 4. 附录：契约与路由（非执行层）

**一致性总表：`references/contract-map.md`**（任务↔参考文件↔上下文键↔模板、协议声明、审计清单）。改任何契约前先读它。

| 使用层 | 文件 | 查什么 |
|---|---|---|
| 全层 | `references/contract-map.md` | 契约总表、协议白名单、一致性审计清单 |
| L1 | `references/orchestration-templates.md` | T1–T8 匹配条件、任务链、红线 |
| L1/L2 | `references/task-registry.md` | 任务 id、guard/accept/on_error、工具与参考文件 |
| L0–L3 | `references/context-schema.md` | 上下文键、必填参数、状态位、缺口处理 |
| L3 | `references/templates.md` | 模板 A–H 输出骨架 |
| L2 | `references/ledger-schema.md` | 台账语法、字段语义、节点模板、文件头 |
| L2 | `references/field-map.md` | 需求→资源映射、参数公式、易错点 |
| L2 | `references/architecture.md` | 分层架构、目录树、文件归属、依赖红线 |
| L2 | `references/code-rules.md` | 外设封装要点、编码规则 R1–R9、最小改动 M1–M7 |
| L2 | `references/module-convention.md` | 新模块句柄/接口/static/依赖（M-DEV） |
| L2 | `references/software-arch.md` | 执行范式、调度机制、设计模式、共享数据保护 |
| L2 | `references/framework-template.md` | framework.md 三章结构、Mermaid 规范、实测数据规则 |
| L2 | `references/checklist.md` | 一致性 8 维、适用矩阵、手册查阅位置 |
| L2 | `references/debug-playbook.md` | 报错/排查编号、HardFault 定位 |
| L2 | `references/diagnostics.md` | 诊断假设、诊断代码、串口/无串口观测 |
| L2 | `references/serial-cli.md` | CLI 固件骨架、命令表、响应格式、无响应排查 |
| L2 | `references/code-style.md` | 风格样例收集、code_style.md 模板、极致优化 |
| L2 | `references/system-perception.md` | 盘点清单（inventory）、归属标准（ownership_judge）、framework 替换/迁移 |
| L2 | `references/other-operations.md` | 删除/重命名/移动/配置/文档操作分级与回退 |
| L2 | `references/consulting-design.md` | 咨询/设计：方案对比、约束、决策记录 |
| 全局 | `evolution.md` | 长期偏好（只影响风格/流程偏好，不得覆盖 O1–O14） |
