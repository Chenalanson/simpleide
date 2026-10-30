# 分层架构规范（plan_architecture 专用）

> 目录
> 1. 架构确认流程（编码前强制）
> 2. 默认架构层级表（用户未自定义时使用）
> 3. 工程根目录分层骨架（写代码前强制落地：标准骨架 / 目录清单 / 落地规则 / 映射表）
> 4. 单向调用规则与依赖红线
> 5. 文件归属决策表（新文件放哪一层）
> 6. APP 初始化骨架（app_init 三步法）
> 7. 自定义架构适配规则
>
> 用法：编码前先完成架构确认（第 1 节），后续所有代码块必须标注所属层与目录；违反第 4 节依赖红线即返工。

## 1. 架构确认流程（编码前强制）

| 步骤 | 动作 | 产出 |
|---|---|---|
| A1 判定 | 检查用户是否给出自定义分层（层级名、目录、职责、调用方向）；现有工程是否已有目录结构 | 架构来源结论：`用户自定义` / `现有工程沿用` / `默认架构` |
| A2 确认表 | 输出架构确认表（层 \| 目录 \| 职责 \| 典型文件 \| 可调用下层）；默认架构直接套用第 2 节，自定义按用户描述重排 | 架构确认表 |
| A3 目录骨架落地 | 新建工程：按第 3 节标准骨架输出**根目录分层文件夹创建清单**与目录树；现有工程：输出「默认层 → 现目录」映射表，缺失目录列为补齐建议；每个待写文件标注层内相对路径 | 目录树代码块 + 目录创建/补齐清单 + 文件落点路径表 |
| A4 归属规则 | 按第 5 节决策表登记本次新增文件落点（含完整相对路径）；现有工程沿用其既有目录，不强行迁移 | 文件落点清单（文件 \| 层 \| 相对路径 \| 动作：新增/沿用） |
| A5 验收 | 按 `plan_architecture.accept` 逐条核对：①每层职责无重叠 ②调用方向单向无环 ③每个待写文件有明确落点与相对路径 ④新建工程目录骨架清单完整 / 现有工程目录映射表完整 ⑤源码文件全部落在层目录内，根目录无散落业务 `.c/.h` | 验收通过；任一条不满足 → 本任务 `accept` 失败，回 L1 重新编排，不得续跑后续任务 |

默认架构在用户未反对时直接采用，并在确认表首行注明「采用默认架构，可在编码开始前提出替换」。

## 2. 默认架构层级表

| 层 | 默认目录 | 职责 | 典型文件 | 允许调用 |
|---|---|---|---|---|
| 应用层 APP | `APP/` | 初始化编排；调度线程；循环线程；业务状态机任务 | `app.c`、`app.h` | MID、BSP/MODULES |
| 中间件层 MID-业务解释层 | `MID/` | 高级 API 封装与协议解释（如 `power_set("SC8701",8.20)` → `dev_write("SC8701", 0x21, buf, len)`；单字节命令可简写 `dev_write("SC8701", 0x21)`） | `service_*.c` | MID 设备管理器、MODULES |
| 中间件层 MID-调度/设备/日志 | `MID/` | 非抢占调度器；设备注册与回调绑定；日志与错误码定义 | `task_sched.c`、`dev_manager.c`、`log.c`、`err_code.h` | MODULES、PERIPHERAL |
| 驱动层 BSP-MODULES（模块驱动） | `BSP/MODULES/` | 具体器件：显示屏、传感器、计量芯片、编码器、按键、显示模组 | `bsp_st7789v.c/.h`、`bsp_sc8701.c`、`bsp_ina199.c`、`bsp_key.c` | PERIPHERAL；仅可包含 `err_code.h` / `log.h` 叶头文件（不调用 MID 服务） |
| 驱动层 BSP-PERIPHERAL（外设驱动） | `BSP/PERIPHERAL/` | MCU 片上外设封装：SPI/I2C/ADC/PWM/UART/TIME/GPIO 基础操作 | `bsp_spi.c`、`bsp_adc.c`、`bsp_pwm.c`、`bsp_usart.c`、`bsp_time.c` | LL/HAL 库 |
| 硬件抽象层 HAL/LL | 库源码（不改动） | 寄存器统一封装、句柄管理 | CubeMX 生成代码、LL 源码 | 硬件层 |
| 硬件层 | — | 寄存器与实际电路 | — | — |

层间调用总方向：

```
APP → MID(解释层) → MID(设备管理器/调度/日志) → BSP/MODULES → BSP/PERIPHERAL → LL/HAL → 寄存器
```

## 3. 工程根目录分层骨架（写代码前强制落地）

### 3.1 标准骨架（新建工程照建；现有工程按 3.3 映射沿用）

```text
<工程根目录>/
├── hardware.dts                  # 硬件台账（台账产物，根目录文档位）
├── framework.md                  # 架构与运行视图（编码产物，根目录文档位）
├── code_style.md                 # 代码风格（T7 产物，可选，根目录文档位）
├── APP/                          # 应用层
│   ├── app.c / app.h
│   └── tasks/                    # 可选：业务任务状态机分文件时存放
├── MID/                          # 中间件层
│   ├── task_sched.c / task_sched.h    # 调度器
│   ├── dev_manager.c / dev_manager.h  # 设备管理器
│   ├── err_code.h                     # 统一错误码
│   ├── log.c / log.h                  # 日志（按需）
│   └── service_*.c / service_*.h      # 业务解释层
├── BSP/
│   ├── MODULES/                  # 模块驱动（器件级）
│   │   ├── bsp_st7789v.c/.h
│   │   ├── bsp_sc8701.c/.h
│   │   ├── bsp_ina199.c/.h
│   │   └── bsp_key.c/.h
│   └── PERIPHERAL/               # 外设驱动（片上外设级）
│       ├── bsp_spi.c/.h
│       ├── bsp_adc.c/.h
│       ├── bsp_pwm.c/.h
│       ├── bsp_usart.c/.h
│       └── bsp_time.c/.h
├── Drivers/                      # HAL/LL/标准库只读源码（CubeMX 生成，目录名可沿用原名）
├── LIB/                          # 第三方只读库（可选；适配封装层放 MID/）
├── EIDE-GCC/                     # 构建工程（可选，按实际 IDE 保留目录名）
└── MDK-ARM/                      # 构建工程（可选）
```

### 3.2 目录职责与初始内容清单

| 目录 | 作用 | 允许放置 | 新建工程最小初始内容 |
|---|---|---|---|
| `APP/` | 应用编排、调度/循环线程、业务任务 | `app.c/.h`、`tasks/*.c/.h` | `app.c/app.h` 骨架（`app_init`、`app_sched_thread`、`app_loop_thread` 三个函数位） |
| `MID/` | 中间件机制与解释层 | 调度器、设备管理器、错误码、日志、`service_*` | `err_code.h`；按需先建 `task_sched.c/.h` |
| `BSP/MODULES/` | 器件模块驱动 | `bsp_<器件>.c/.h` | 空目录或首个模块文件 |
| `BSP/PERIPHERAL/` | 片上外设封装 | `bsp_<外设>.c/.h` | 空目录或首个外设文件 |
| `Drivers/` | 只读 HAL/LL/SPL 库、CMSIS 设备头与 `Drivers/ThirdParty/` | 库源码、`stm32xx_hal_*.c`、启动文件、原样第三方库 | 不手写；由 CubeMX/库包管理 |
| `LIB/` | 第三方开源库只读目录 | 协议栈、开源库源码（原样导入） | 不手写；适配封装层放 `MID/` |
| 构建目录（`EIDE-GCC/`、`MDK-ARM/` 等） | IDE 工程配置与编译产物 | `.uvprojx`、EIDE/CMake 配置、obj 输出 | 由 IDE 生成，源码层不混入产物 |
| 根目录 | 文档与台账 | `hardware.dts`、`framework.md`、`code_style.md` | **禁止**放业务 `.c/.h`；`Drivers/`、`LIB/`、`third_party/`、构建目录与文档不属于散落源码 |

### 3.3 落地规则

1. **新建工程**：先输出完整目录骨架与文件夹创建清单（推荐创建顺序：`APP/` → `MID/` → `BSP/MODULES/` → `BSP/PERIPHERAL/`），再逐个文件给代码块；每个代码块前标注完整相对路径（如 `【BSP/MODULES/bsp_sc8701.c】`）。
2. **目录缺失处理**：待使用但尚不存在的目录列入「需新建清单」；AI 输出骨架与文件内容，由用户在 IDE/文件系统实际建目录后落盘；不假设目录已存在。
3. **现有工程续写**：沿用现有目录名与文件位置；必须输出「默认层 → 现工程目录」映射表（格式见 3.4）；不迁移、不复制、不重排已有源码；新文件按映射落到对应现有目录。
4. **根目录红线**：业务 `.c/.h` 不得直接放工程根目录；根目录仅允许 `hardware.dts`、`framework.md`、`code_style.md`、只读库目录（`Drivers/`、`LIB/`、`third_party/`）与构建工程目录；第三方库源码保持只读、不复制进业务目录；编译产物与 `.o/.hex/.axf` 不进入源码层目录。
5. 源码按层放目录，不按 IDE 工程文件归类；构建工程只引用目录，不复制源码；第三方开源库统一放只读 `LIB/` 或 `Drivers/ThirdParty/`，不修改库源码，适配封装层放 `MID/`。

### 3.4 现有工程映射表格式（模板 A 5.1 / 模板 B 1.1 直接套用）

| 默认层目录 | 现工程实际目录 | 映射结论（沿用 / 新建 / 合并） | 本次落点文件 |
|---|---|---|---|
| `APP/` | （如 `User/`、`Application/`） |  |  |
| `MID/` |  |  |  |
| `BSP/MODULES/` |  |  |  |
| `BSP/PERIPHERAL/` |  |  |  |

## 4. 单向调用规则与依赖红线

| 编号 | 规则 | 禁止行为 |
|---|---|---|
| D1 | 调用只能沿第 2 节方向向下；上层可直接使用下层暴露接口，不反向引用内部实现 | PERIPHERAL `#include` MODULES；MODULES `#include` APP |
| D2 | 同层依赖优先走 MID 公共服务（设备管理器/日志），不跨模块直接操作他模块私有变量 | MODULES-A 直接访问 MODULES-B 的 static/extern 变量 |
| D3 | 禁止循环包含：`.h` 之间不得形成 include 环；必要时用前向声明或句柄指针 | A.h include B.h，B.h 再 include A.h |
| D4 | APP 不直接操作片上外设寄存器/HAL 句柄细节；跨片外设访问经 MODULES 或 PERIPHERAL 接口 | app.c 直接调 `HAL_SPI_Transmit` 驱动具体器件时序 |
| D5 | 第三方库原样放只读 `LIB/` 或 `Drivers/ThirdParty/`，只被上层调用；适配封装层放 `MID/`；不反向依赖工程业务头文件 | 开源库 include `app.h`；直接在库源码里加业务补丁 |
| D6 | 依赖关系登记到 framework 增量片段，由 `framework_patch` 写入 framework.md 第 1 节依赖矩阵；新增依赖先确认不违反 D1–D5 | 口头默认，无记录 |

## 5. 文件归属决策表（新文件落点）

| 待写对象 | 落点层 / 相对路径 | 判断依据 |
|---|---|---|
| 主初始化、调度循环、业务任务编排 | `APP/app.c`、`APP/tasks/xxx_task.c` | 只做调用编排，无器件时序细节 |
| 面向用户语义的封装（设置电压、保护阈值配置） | `MID/service_xxx.c/.h` | 名字是业务语义，不是寄存器/器件名 |
| 调度器、设备注册、日志、错误码 | `MID/task_sched.*`、`MID/dev_manager.*`、`MID/err_code.h` | 跨模块公共机制 |
| 某具体器件驱动（屏、传感器、计量芯片、温感、按键板） | `BSP/MODULES/bsp_<器件>.c/.h` | 对应一颗外部芯片或功能板；内部可调用多个外设驱动 |
| 某片上外设封装（SPI1 收发、ADC 扫描、PWM 通道、UART 收发、时基） | `BSP/PERIPHERAL/bsp_<外设>.c/.h` | 对应 MCU 外设实例，不含外部器件协议 |
| 器件协议帧解析与状态机 | 跟随器件放 `BSP/MODULES/`；通用协议框架放 `MID/` | 专用解析随器件，通用机制独立 |
| 第三方开源库、协议栈 | 只读 `LIB/` 或 `Drivers/ThirdParty/`（适配层放 MID） | 不修改库源码，仅封装适配层 |

## 6. APP 初始化骨架（app_init 三步法）

固定顺序，不得跳步；步骤内为默认示例，参数以台账为准。

```c
/*
 * 软件架构链：应用层 → 中间件（日志/错误码/设备管理） → 驱动层(模块驱动→外设驱动) → 硬件抽象层(LL/HAL) → 硬件层
 */

void app_init(void)
{
    LL_SYSTICK_EnableIT();                 // 滴答中断：非抢占调度时基（1ms）

    // 步骤1：外设使能（PERIPHERAL 层初始化，依赖硬件时钟已开）
    bsp_spi_init(SPI1);                    // SPI1 初始化
    bsp_adc_init_dma(ADC1, 1);             // ADC1 + DMA 初始化
    bsp_pwm_init(TIM1);                    // PWM 定时器初始化
    bsp_usart_init();                      // 串口初始化（实例由台账绑定）
    bsp_time_init();                       // 软件时基/周期定时器初始化

    // 步骤2：设备初始化（MODULES 层，必须在外设初始化完成之后）
    bsp_st7789v_init();                    // 屏幕器件初始化
    pid_init();                            // 算法/补偿器实例初始化
    bsp_sc8701_init();

    // 步骤3：任务时间变量/调度注册（调度器层）
    freedom_set_time(&multi_menu_timer, 5); // 参考实现，需适配：周期任务时间变量，单位 ms
    // 任务分类：周期性（每次到点重赋值）/ 一次性（事件触发后再赋值）/ 条件性（满足条件赋值）

    LL_mDelay(200);                         // 器件上电稳定等待（仅初始化阶段，任务循环内禁止长阻塞延时）
}

/* 调度线程：非抢占式时间片触发，检查时间变量到点后调用任务 */
void app_sched_thread(void)
{
    freedom_task_sched();                  // 参考实现，需适配：按工程调度器 API 替换
}

/* 循环线程：连续执行状态机任务（瞬时判断，不挂时间间隔） */
void app_loop_thread(void)
{
    multi_menu_ui_task();                  // 菜单状态机
    bsp_sc8701_task(run_flag);             // 器件状态机
    ovp_task();                            // 过压保护
    ocp_task();                            // 过流保护
    otp_task();                            // 过温保护
}
```

规则：

- `app_init` 只编排调用，不实现器件细节；每步注释标明所属层。
- 步骤 1 失败（初始化返回错误）进入错误上报与安全态，不继续步骤 2。
- 新增外设时先补 PERIPHERAL 初始化，再在步骤 2 补对应 MODULES 初始化，顺序不可反。
- 调度器 API 名称以工程实际为准；`freedom_*` / `task_time_sched` 仅为参考实现，落地时适配；APP 对外固定 `app_init` / `app_sched_thread` / `app_loop_thread`。

## 7. 自定义架构适配规则

| 情形 | 处理 |
|---|---|
| 用户给出层级名（如 HAL/BSP/APP/OS 层、driver/app/service） | 按用户命名生成层级表与目录树；保留「自上而下单向依赖」原则 |
| 用户现有工程已有目录（如 `User/`、`Drivers/`、`Application/`） | 沿用现有目录名，将默认层职责映射到现有目录；输出映射表（默认层 → 现工程目录），不新建平行目录 |
| 用户指定 RTOS 架构（任务文件独立、队列/互斥件目录） | 在 MID 层增加 OS 机制目录；调度章节引用 `software-arch.md` 抢占式模型；APP 层为任务函数集合 |
| 层级数少于默认（无 MID 层） | 将解释层与设备管理器职责并入 MODULES 层公共头；架构表如实记录，不强行补层 |
| 架构变更（中途调整目录/分层） | AI 只输出迁移清单（文件 \| 原路径 \| 新路径 \| 影响 \| 验证/回退），不执行移动/重命名；实际搬迁由用户手工完成，或走 T6 `execute_or_preview` 仅给预览；framework.md 由 `framework_patch` 增量更新，不直接改代码 |
