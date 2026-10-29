# 代码风格学习与极致优化参考（code-style）

本文件用于 T7：从用户已有代码中学习 `.c` / `.h` 写法、模块封装习惯和优化偏好，产出工程根目录的 `code_style.md`，并在后续编码中优先执行。

---

## 1. 优先级与位置

| 项 | 约定 |
|---|---|
| 文件位置 | `<工程根目录>/code_style.md`（固定，不放到 `docs/`） |
| 优先级 | `code_style.md` > `code-rules.md` 默认风格 > 通用习惯；但不得覆盖安全、硬件事实、当前指令和编译器限制 |
| 生成方式 | AI 用 `se-file path=code_style.md` 写入；不要只把风格写在聊天里 |
| 完整性 | 规则必须可执行：写清“怎么做、反例、检查方法”，禁止只写“保持整洁” |
| 样例位置 | 默认扫描 `<工程根目录>/code_style_samples/`；也扫描用户上传到根目录的代码、用户点名的目录/文件 |
| 覆盖范围 | 在文档开头写清样例来源、文件数、未覆盖项（例如没有中断代码、没有 FreeRTOS） |
| 维护 | 用户可随时手改；AI 下次编码前必须重新读取，不得凭记忆 |

`code_style.md` 建议同时包含“给 AI 的规则”和“给用户的自查清单”，方便用户自己找错误。

---

## 2. 样例收集流程（S8）

1. 列出候选来源：
   - `code_style_samples/` 下的 `.c` / `.h`（递归，最多 40 个文件）
   - 用户上传到工程根目录的 `.c` / `.h`
   - 用户明确指定的文件或文件夹
   - 现有 `APP/`、`MID/`、`BSP/`、`Core/Src`、`Core/Inc` 源码（作为补充）
2. 每个来源记录：路径、文件数、是否有头文件、是否有中断/RTOS/协议代码。
3. 逐文件提取特征（见第 3 节），统计“多数写法”和“少数写法”；不要用单个文件代表全部。
4. 冲突时按以下顺序：用户明确说明 > 样例中重复次数最多 > 现有分层架构 > 默认规则。
5. 样例不足时明确写“未覆盖”，并向用户提 1–2 个关键问题（用 `se-ask`），不要编造。
6. 生成 `code_style.md` 后，用 2–3 个正例和反例自检规则是否可执行。

> 样例文件只用于学习，不要复制进业务目录，不要批量改写用户原文件。

---

## 3. 需要提取的特征

### 3.1 文件头与包含

- 文件头注释格式：版权、文件名、简介、作者、日期、版本、修改记录。
- `#ifndef` 宏命名：`__XXX_H`、`XXX_H`、`_XXX_H_` 等。
- 包含顺序：系统头 → HAL/CMSIS → 项目公共头 → 本模块头；是否按字母排序。
- `extern "C"` 是否有。
- 头文件是否只放声明、是否包含实现、是否用前置声明。

### 3.2 命名

- 文件名：`bsp_led.c` / `led.c` / `LED.c`；下划线还是驼峰。
- 类型：`_t` 后缀、`typedef struct` 还是 `struct xxx`。
- 函数：`模块_动作`、驼峰、前缀。
- 变量：`g_` / `s_` / `m_` 前缀、局部变量风格。
- 宏/常量：全大写、`MODULE_` 前缀、枚举还是宏。
- 静态函数：`static` + 模块前缀。

### 3.3 缩进、括号与空行

- 4 空格 / 2 空格 / Tab；Tab 宽度。
- 左花括号同行还是下一行；`else` / `while` 位置。
- 单行 `if` 是否加括号；`switch` 缩进。
- 函数之间空行数；连续变量声明是否对齐。
- 行宽（80/100/120）和换行方式。

### 3.4 注释与文档

- 函数注释：`/** */`、`/* */`、`//`；是否写参数/返回值/单位/范围/线程安全。
- 字段注释是否写在成员右侧或上方。
- 单位是否写入变量名或注释（`_ms`、`_mv`）。
- TODO/FIXME 格式；是否允许注释掉的代码。

### 3.5 类型、宏与错误处理

- 固定宽度类型 `uint8_t` / `uint32_t` 的使用。
- `bool` / `uint8_t` / 自定义 `status_t`。
- 错误码枚举命名、成功值、错误传播方式（返回码 / 全局错误 / 断言）。
- 参数检查：`NULL`、范围、状态机状态。
- 日志宏：`LOG_D` / `printf` / 自定义 `bsp_log`；是否在 ISR 中打印。
- 断言和 `Error_Handler` 的使用。

### 3.6 模块封装（重点）

- 是否有句柄结构体（`xxx_handle_t`），字段是否私有。
- 初始化函数：`xxx_init(&handle, &config)` 还是 `xxx_init()` 单例。
- 读写接口：`get/set`、`read/write`、`start/stop`。
- `static` 私有变量/函数的使用。
- 模块间依赖：是否直接访问其他模块全局变量；是否通过接口。
- 目录归属：`APP/`、`MID/`、`BSP/MODULES/`、`BSP/PERIPHERAL/`。
- 中断与主循环的分工：ISR 只置标志/搬运，主循环处理业务。
- 是否存在 `volatile`、临界区、原子操作习惯。

### 3.7 优化偏好

- 默认优化等级（`-O0/-O2/-Os`）、LTO、函数段、`-ffunction-sections`。
- 是否使用查表、定点数、避免浮点、DMA、双缓冲、循环展开。
- RAM/Flash 优先级；是否愿意用空间换时间。
- 日志/断言是否在 release 中关闭。
- 是否使用 `inline`、`__attribute__((section))`、对齐、缓存。

---

## 4. code_style.md 推荐结构

```markdown
# 代码风格（项目）

> 来源：code_style_samples/（12 个文件）、APP/、BSP/PERIPHERAL/
> 未覆盖：FreeRTOS 任务、CAN 协议、低功耗
> 最后更新：YYYY-MM-DD

## 1. 文件与目录
| 规则 | 正例 | 反例 |
|---|---|---|
| 业务源码只能放 APP/MID/BSP 层 | `BSP/PERIPHERAL/bsp_led.c` | `led.c` 放根目录 |

## 2. 文件头与包含
...

## 3. 命名
...

## 4. 缩进、括号、空行
...

## 5. 注释与文档
...

## 6. 类型、宏、错误码、日志
...

## 7. 模块封装模板
（给一个 20–40 行的 .h/.c 骨架，严格按用户习惯）

## 8. 中断、调度与共享数据
...

## 9. 优化偏好
| 目标 | 规则 | 验证 |
|---|---|---|
| Flash 优先 | 默认 -Os，关闭不必要日志 | size 对比 |

## 10. 自查清单
- [ ] 文件头完整
- [ ] 头文件保护宏符合项目风格
- [ ] 函数名/类型名符合前缀约定
- [ ] 错误码和参数检查齐全
- [ ] 模块只通过接口访问其他模块
- [ ] ISR 中没有阻塞/打印/长循环
- [ ] 新增文件已加入 C_SOURCES
```

---

## 5. 代码模板（按用户风格替换占位符）

### 5.1 头文件骨架

```c
/**
 * @file    bsp_xxx.h
 * @brief   <一句话说明>
 * @author  <用户/团队>
 * @date    YYYY-MM-DD
 */
#ifndef BSP_XXX_H
#define BSP_XXX_H

#include <stdint.h>

typedef enum {
    BSP_XXX_OK = 0,
    BSP_XXX_ERR_PARAM = 1,
    BSP_XXX_ERR_STATE = 2,
    BSP_XXX_ERR_TIMEOUT = 3,
} bsp_xxx_status_t;

typedef struct {
    uint32_t example;
} bsp_xxx_config_t;

typedef struct bsp_xxx bsp_xxx_t;

bsp_xxx_status_t bsp_xxx_init(bsp_xxx_t *self, const bsp_xxx_config_t *config);
bsp_xxx_status_t bsp_xxx_start(bsp_xxx_t *self);
bsp_xxx_status_t bsp_xxx_stop(bsp_xxx_t *self);
uint32_t bsp_xxx_get_value(const bsp_xxx_t *self);

#endif /* BSP_XXX_H */
```

### 5.2 源文件骨架

```c
#include "bsp_xxx.h"
#include "board.h"

struct bsp_xxx {
    uint32_t value;
    uint8_t started;
};

static bsp_xxx_status_t bsp_xxx_check(const bsp_xxx_t *self)
{
    return (self == NULL) ? BSP_XXX_ERR_PARAM : BSP_XXX_OK;
}

bsp_xxx_status_t bsp_xxx_init(bsp_xxx_t *self, const bsp_xxx_config_t *config)
{
    if (self == NULL || config == NULL) {
        return BSP_XXX_ERR_PARAM;
    }
    self->value = config->example;
    self->started = 0u;
    return BSP_XXX_OK;
}
```

> 这只是骨架；必须按用户样例替换命名、注释、缩进、错误码和接口风格。不要直接照搬默认模板。

---

## 6. 风格审计

用户要求“找错误/审查风格”时输出：

| 文件:位置 | 偏差 | 用户规则 | 建议 | 是否本次修改 |
|---|---|---|---|---|

规则：

- 先列高影响问题（错误码、越界、ISR 阻塞、模块越层），再列格式问题。
- 不擅自批量改旧代码；用户确认后才修改。
- 修改后跑 `se-build`；必要时用 `se-cli` 或 LED/GPIO 验证行为。
- 把重复出现的偏差补充进 `code_style.md` 的自查清单。

---

## 7. 极致优化

### 7.1 先定目标

| 目标 | 典型手段 | 代价 |
|---|---|---|
| Flash 最小 | `-Os`、LTO、函数段 + `--gc-sections`、去日志、查表、避免浮点 | 可读性、调试信息 |
| 速度最快 | `-O2/-O3`、内联、循环展开、DMA、双缓冲、缓存、减少分支 | Flash、功耗、复杂度 |
| RAM 最小 | 复用缓冲、位域、静态分配、避免大栈、压缩数据 | 速度、可读性 |
| 功耗最低 | 休眠、事件驱动、降低主频、外设时钟门控、DMA | 实时性、唤醒延迟 |
| 实时性 | 中断优先级、临界区缩短、无阻塞、固定周期 | 设计复杂度 |

### 7.2 极致优化清单

1. 编译/链接：`-Os/-O2`、LTO、`-ffunction-sections -fdata-sections`、`--gc-sections`、`-fno-unwind-tables`、关闭未用库。
2. 数据：定点数替代浮点；查表替代三角函数；位域/位带；结构体对齐与打包；常量放 Flash。
3. 循环：减少内层分支；固定边界；必要时展开；避免在循环内 `strlen`/`malloc`。
4. 外设：DMA 替代 CPU 搬运；双缓冲；中断只搬运；批量读写；时钟门控。
5. 中断：短 ISR；优先级分组；临界区最小；避免在 ISR 打印/阻塞。
6. 日志/断言：release 关闭调试日志；错误码保留；避免 `printf` 浮点。
7. 启动/内存：合理栈堆；`_Min_Stack_Size`；避免大局部数组；检查 map 文件。
8. 测量：构建前后 `size`/map 对比；GPIO 翻转或 DWT 计时；功耗表；不能用“感觉更快”。

### 7.3 优化 Gate

- 必须写目标、约束、测量方法、预期收益和回退。
- 不得牺牲正确性、安全性、硬件时序或可维护性换性能。
- 未测量不得宣称“极致优化完成”；只可写“已按清单优化，待实测”。
- 优化后必须 `se-build` 0 error，并做回归验证。
