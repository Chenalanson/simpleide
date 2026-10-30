# 编排模板库（T1–T8 · v2）

> L1 唯一允许「写场景差异」的地方。每个模板 = 匹配条件 + 必备上下文 + 任务链 + 输出模板。任务 id 必须来自 `task-registry.md`；模板未命中时由 AI 从任务库编排，同样要过 `plan_validate`（O1–O14）。

## 通用约定

- `→` 强依赖；`?` 按上下文插入；`[confirm]` = 暂停点（L1 写 `pending_ask`/`pending_confirm`，L3 渲染，回答后生成新链）。
- **O3 自动前置**：`plan_validate` 在写操作前补 `index_context`，修改前补 `read_file`；补入位置与原因写 `execution.history`，无法补则 REJECT。
- **O14 执行优先**：用户已确认执行/落盘后，禁止只输出计划、承诺或重复读取；已有正文的文件直接进入 `patch_file`，仍缺正文的才 `read_file`；同一文件不得连续 `se-read` 两次，读完后的下一轮必须落盘。
- **O13**：`serial_cli`/`regress` 前必须 `build.build_ok=true` 且（`flash.flashed=true` 或 `inputs.firmware_running=true`）；否则先 `flash` 或 `pending_ask`。
- 每步验收标准（`accept`）见 `task-registry.md` 对应行；不通过 = 该步失败，按 L2 失败策略处理，不得带病继续。
- `flash` / `debug_start` 当前为“预览 + 用户执行”，不得写成自动完成。

---

## T1 整机/新功能开发 → 模板 A

**匹配**：新板级设计、≥2 类外设、从需求到完整程序。

**必备上下文**：`chip`、`project_root`、`board`；`toolchain` 缺失时必须在编排前补问（O8），不得默认。

**任务链**

1. `collect_requirements` — 需求/约束/复用资源；缺关键项先问（≤2 问）
2. `index_context` — 建索引（后续读/写前提）
3. `query_docs?` — 查 AF/IRQn/DMA/寄存器事实，查不到写 `<TODO>`
4. `read_context?` — 复用既有工程台账/架构（新工程可跳过）
5. `plan_ledger` — 硬件资源表 + 软件需求表 + 冲突预判
6. `write_ledger` — 完整 `hardware.dts`
7. `plan_architecture` — 层级表 + 目录骨架 + 文件落点
8. `codegen_by_layer` — APP→MID→MODULES→PERIPHERAL
9. `framework_write` — 三章：分层架构 / 软件架构五维度 / 运行视图（耗时 `[预估]`）
10. `consistency_check` — 8 维一致性表
11. `build` — 真实构建
12. `serial_cli?` — 用户确认新固件已在运行后验证（O13）

**输出**：模板 A（上下文收集 / 需求拆解 / 台账 / 架构与代码 / framework / 核对 / 构建与串口）。

---

## T2 单外设增补 → 模板 B

**匹配**：现有工程加一类外设/一路功能（串口回显、一路 ADC、一个 PWM、一个传感器驱动）。

**必备上下文**：`chip`、`project_root`；既有台账与目录结构（缺失则先 `index_context`）。

**任务链**

1. `index_context` — 建索引
2. `read_context` — 现有台账/架构/复用句柄
3. `query_docs?` — 新增外设的 AF/IRQn/DMA 事实
4. `plan_ledger`（片段）— 受影响节点规划
5. `ledger_patch` — 只改受影响节点
6. `codegen_by_layer` — 仅改动层
7. `framework_patch?` / `framework_write?` — 有文档走增量；无文档先 `framework_write` 建档
8. `consistency_check` — 8 维（不适用写 N/A，不删行）
9. `build` — 0 error
10. `serial_cli?` — 目标命令验证（O13）

**输出**：模板 B（架构落点/复用表/新增改动表/台账片段/代码/核对/构建）。

---

## T3 现有逻辑修改 → 模板 C

**匹配**：硬件资源不变，只改参数、状态机、协议解析、已定位 bug。

**必备上下文**：目标文件与位置；**不进入**需求拆解与台账设计（发现硬件要变 → 升级 T2）。

**任务链**

1. `index_context` → `read_context?` → `read_file`
2. `patch_plan` — 目标行为量化 + 语义/假设确认 + 改动点表
3. `patch_file` — diff 风格最小修改
4. `framework_patch?` — 任务/中断/调用链/周期变化时更新，否则标“无变更”
5. `build` — 0 error
6. `serial_cli?` — 正常路径 + 边界分支验证（O13）

**输出**：模板 C（现状 / 方案 / 代码 diff / 影响面 / 验证）。

---

## T4 故障排查 → 模板 D

**匹配**：编译/链接报错、HardFault、运行现象（不工作/乱码/死机/读数异常）。

**必备上下文**：报错原文或现象、复现条件、相关文件；无串口时需可用观测手段（可后补）。

**任务链**

1. `index_context` → `read_context?` — 建索引；读既有台账/文档（映射与 framework 数据）
2. `collect_symptom` — 现象/条件/最近改动/供电时钟复位
2. `diag_hypotheses` — 按概率排序可能位置 + 证据 + 排除方法
3. `diag_code` — 最小诊断代码（`#if DIAG`，可移除）
4. `build` — 0 error
5. `flash?` — 需要运行新诊断代码时（O1 + 用户确认）
6. `debug_start?` — 需要断点/watch 时（O2）
7. `serial_cli?` / `observe?` — 真实验证（O13）；无串口走 LED/GPIO/watch
8. `issue_log(open)` — 记录真实问题与现象
9. `minimal_fix` — 最小修复
10. `framework_patch?` — ISR/时序/调用链变化时同步 framework.md
11. `build` → `regress` — 修复后回归
12. `issue_log(resolved)` — 同标题补原因与方案
13. `diag_cleanup` — 移除 `#if DIAG`
14. `build` — 清理后再次 0 error

**输出**：模板 D（现象 / 诊断 / 定位结论 / 最小修复 / 验证 / 清理与预防）。

---

## T5 台账维护 → 模板 E

**匹配**：只要求增删改 `hardware.dts` 节点，不出代码。

**任务链**

1. `index_context` → `read_ledger`
2. `ledger_patch`（diff 式片段，带备份与哈希）
3. `ledger_validate` — 引用闭合 / 占用冲突 / TODO 汇总

**输出**：模板 E（变更清单 / dts 片段 / 一致性复核 / 代码衔接提示）。

---

## T6 其他操作 → 模板 F

**匹配**：删除/重命名/移动/工程配置/文档维护/清理，以及咨询、方案设计等不属于 T1–T5/T7/T8 的受控请求。

**任务链**

1. `index_context` → `read_context?` — 建索引；读既有台账/文档（提供 mapping）
2. `classify` — T6-O（操作）或 T6-C（咨询/设计）
3. `impact_analysis` — 构建/包含目录/链接脚本/工程配置/可恢复性
4. `[confirm]` — `se-ask` 确认（禁止跳过）
5. 执行分支：
   - 删除 → `delete_file`（O12：备份、≤10、禁区检查）
   - 其他已落地操作 → `execute_or_preview`
   - 未落地协议（重命名/移动/目录创建）→ 只给命令预览，不声称完成
   - T6-C → 只给方案对比，用户明确要求写文档时才走 `write_file`（需 O3 + 确认）
6. `verify_rollback` — 验证 + 回退方式

**输出**：模板 F（清单 / 影响 / 预览或方案对比 / 结果或结论 / 验证与回退）。

**红线**：咨询类不产生文件修改；删除不碰工程根/目录/`.simple-embedded`/`.git`/生成目录；一次 ≤10 个文件；未落地协议不得声称已完成。

---

## T7 代码风格与优化 → 模板 G

**匹配**：「按我的风格写 / 学习代码风格 / 生成或更新 `code_style.md` / 风格审计 / 极致优化」。

**任务链**

1. `index_context` → `collect_samples` — `code_style_samples/`、根目录代码、用户指定文件/目录、现有 APP/MID/BSP/Core
2. `style_rules` — 文件头/命名/缩进/注释/类型宏/错误码/日志/模块封装/调度写法/优化偏好
3. `code_style_write` — 正例 + 反例 + 用户自查清单
4. `apply_or_audit` — 新码逐条对照；旧码只列偏差建议（未确认不批量改）
5. `optimize_plan?` — 目标（速度/尺寸/RAM/Flash/功耗/实时性）→约束→方案→测量→风险→回退
6. `build` — 0 error 收尾
7. `serial_cli?` — 仅在声称性能/行为改善且 O13 满足时验证

**输出**：模板 G（来源 / 规则 / code_style.md / 应用与审计 / 优化 / 验证 / 交付）。

---

## T8 系统整体感知 → 模板 H

**匹配**：「阅读/理解这个工程 / 分析架构 / 这是别人的工程 / 生成或替换 `framework.md`」，或工程明显不是默认 `APP/MID/BSP`。

**任务链**

1. `index_context` → `inventory` — 按 `system-perception.md` §2 盘点：构建文件、目录树、入口、启动/链接脚本、RTOS/厂商代码、现有文档
2. `ownership_judge` — 按 §3 判 默认 / 兼容 / 外来 / 混合 + 证据（不得把“与默认不同”判为错误）
3. `diff_docs` — 按 `framework-template.md` 三章结构与实际代码对比，列缺失/过期/冲突
4. `[confirm]` — `se-ask`：保留现状 / 只补全 / 替换 framework.md / 重构
5. `framework_write`（保留/替换）或 `framework_patch`（只补全）— 写真实架构 + 默认层映射；不确定标 TODO
6. 若用户要求重构：先给迁移方案（影响/验证/回退），实际改码转 T1/T2/T3，移动删除转 T6

**输出**：模板 H（现状感知 / 归属判断 / 处理方案 / framework 处理 / 重构迁移 / 结论与下一步）。

**红线**：默认不改业务代码、不移动目录、不覆盖 `framework.md`；外来工程保留原结构是合格结论。

---

## 未命中模板时的 AI 编排约束

- 只能从 `task-registry.md` 选 id，输出 `{chain[], reason, assumptions}`。
- 必须声明每步 `context_in`/`context_out`，并保证 O1–O14 可校验。
- 任何写操作前必须有 `index_context`；修改前还必须有 `read_file`；任何 flash 前必须有 `build`。
- `plan_validate` 返回 REJECT 时不得执行，按 O5 交给用户决定是否降级。
