# 代码风格（blink）

> **来源**：`src/main.c`、`src/board.c`、`include/board.h`（3 个文件，44 行）、`Makefile`、`blink.resources.json`（build_options）
> **未覆盖**：中断/ISR、调度器、状态机、错误码体系、日志、动态内存、RTOS、通信协议、多模块依赖、单元测试
> **标注约定**：`[样例观察]` = 从现有代码直接归纳，必须遵守；`[默认约定·待确认]` = 样例未覆盖，按嵌入式 C 通用做法补齐，用户确认后固化
> **最后更新**：2026-10-28（依据 blink.resources.json generated_at）

## 1. 文件与目录

| 规则 | 正例 | 反例 |
|---|---|---|
| `[样例观察]` 实现放 `src/`，头文件放 `include/`；现有工程沿用不迁移 | `src/board.c` + `include/board.h` | `board.c` 放工程根目录 |
| `[样例观察]` 模块 `.c` 与 `.h` 同名配对 | `board.c` / `board.h` | `board_impl.c` / `board_api.h` |
| `[样例观察]` 业务源码不进 `build/` 等生成目录 | `src/main.c` | `build/main.c` |
| `[默认约定·待确认]` 扩展为分层工程时按映射落位：APP→`src/`，BSP→`src/`+`include/`，MID 新建 `src/mid/` | `src/mid/task_sched.c` | 调度器塞进 `main.c` |

## 2. 文件头与包含

| 规则 | 正例 | 反例 |
|---|---|---|
| `[样例观察]` 包含顺序：标准库头（`<>`）→ 空行 → 项目头（`""`） | 见下方骨架 | 项目头写在标准库头之前 |
| `[样例观察]` 头文件保护宏 = 大写文件名 + `_H`，`#endif` 不带尾注释 | `#ifndef BOARD_H` / `#define BOARD_H` / `#endif` | `#ifndef __BOARD_H__` |
| `[样例观察]` 头文件只放 `#include`、函数声明、宏 | `void led_set(uint32_t on);` | `uint32_t g_led;` 定义在 `.h` |
| `[样例观察]` 纯 C 工程不加 `extern "C"` | 无 | 无谓的 `#ifdef __cplusplus` |
| `[默认约定·待确认]` 新增 `.c` 首行写一行职责注释，不强制 Doxygen 版权块 | `/* 板级 LED 与延时封装 */` | 空模板/大段版权头 |

包含骨架：

```c
#include <stdint.h>
#include "board.h"
```

## 3. 命名

| 规则 | 正例 | 反例 |
|---|---|---|
| `[样例观察]` 文件名/函数名/变量名：小写 + 下划线 | `board_init`、`delay_ms` | `BoardInit`、`delayMs` |
| `[样例观察]` 函数名 `模块_动作` | `led_set` | `set_led`、`LedSet` |
| `[样例观察]` 宏与保护宏全大写 | `BOARD_H` | `board_h` |
| `[样例观察]` 宽度敏感量用固定宽度类型 | `uint32_t on`、`uint8_t i` | `unsigned int on` |
| `[样例观察]` 无符号布尔语义参数用 `uint32_t`（0=关，非0=开） | `void led_set(uint32_t on)` | 同模块混用 `bool`/`uint8_t` |
| `[样例观察]` 时间量单位写入名字后缀 | `delay_ms(uint32_t ms)` | `delay(uint32_t t)` |
| `[默认约定·待确认]` 文件级全局 `g_`，文件内静态 `s_` | `static uint32_t s_tick;` | 裸名全局 `tick` |
| `[默认约定·待确认]` 类型名 `_t` 后缀 | `board_cfg_t` | `boardcfg` |

## 4. 缩进、括号、空行

| 规则 | 示例 |
|---|---|
| `[样例观察]` 4 空格缩进，禁用 Tab | `    led_set(1);` |
| `[样例观察]` 左花括号与语句同行（K&R） | `void board_init(void) {` |
| `[样例观察]` 空函数体 `{`、`}` 各占一行 | `void board_init(void) {` 换行 `}` |
| `[样例观察]` 函数之间空一行 | `board_init` 与 `led_set` 之间 |
| `[样例观察]` 无限循环写 `for (;;)` | `for (;;) {` |
| `[样例观察]` 二元运算符两侧、逗号后各一个空格 | `led_set(i + 1);` |
| `[默认约定·待确认]` 行宽 ≤ 100 列 | 超长表达式换行并缩进 4 空格 |
| `[默认约定·待确认]` 单行 `if/for/while` 一律加花括号 | `if (on) {` … `}` |

## 5. 注释与文档

| 规则 | 示例 |
|---|---|
| `[样例观察]` 说明性注释用块注释 `/* ... */`，多行续行 `*` 对齐 | `/* Minimal blink example ...` / ` * Open this folder ... */` |
| `[样例观察]` `//` 只用于 TODO/临时标记 | `//TODO: 说明待办` |
| `[样例观察]` 公共函数上方一行注释说明职责，含参数时写清含义与取值 | `/* LED 开(非0)/关(0) */` |
| `[默认约定·待确认]` TODO 必须写清待办内容与去向 | `//TODO: 接真实 GPIO 后删除 no-op` |
| `[默认约定·待确认]` 手册待核实用 `/* TODO: 依 RM xx 章节 */`，不填猜测值 | 禁止凭记忆填 AF/IRQn |

## 6. 类型、宏、错误码、日志

| 规则 | 示例 |
|---|---|
| `[样例观察]` 固定宽度类型配 `<stdint.h>` | `uint32_t`、`uint8_t` |
| `[样例观察]` 未使用参数用 `(void)param;` 显式抑制警告 | `(void)ms;` |
| `[默认约定·待确认]` 参数检查：指针判空、范围检查，失败返回错误码 | `if (p == NULL) { return BOARD_ERR_PARAM; }` |
| `[默认约定·待确认]` 错误码用枚举、成功为 0 | `typedef enum { BOARD_OK = 0, BOARD_ERR_PARAM, BOARD_ERR_HW } board_err_t;` |
| `[默认约定·待确认]` 魔法数宏化 | `#define LED_BLINK_MS 250` |
| `[默认约定·待确认]` 日志不在 ISR 内打印，可用宏编译期关闭 | `#if LOG_EN` … `#endif` |

## 7. 模块封装模板

| 规则 | 正例 | 反例 |
|---|---|---|
| `[样例观察]` 一个模块 = `.h` 声明 + `.c` 实现，成对出现 | `board.h` / `board.c` | 实现写进头文件 |
| `[样例观察]` 私有状态 `static` 留在 `.c` | `static uint32_t s_state;` | `.h` 里 `extern uint32_t state;` |
| `[样例观察]` 依赖单向，下层不 include 上层 | `main.c` → `board.c` | `board.c` include `main.h` |
| `[样例观察]` 平台相关空实现集中在一个模块便于替换 | `board.c` 三个 no-op | 桩函数散落多文件 |
| `[默认约定·待确认]` 有状态模块用 `模块_t` + `模块_get_handle()` 单例 | `board_t *board_get_handle(void);` | 到处 `extern board_t g_board;` |
| `[默认约定·待确认]` 跨模块只走接口，不直读他模块私有变量 | `board_get_state()` | 直接访问对方 `static` 变量（不可行）/全局变量 |

骨架（严格按本工程风格）：

```c
/* board.h */
#ifndef BOARD_H
#define BOARD_H

#include <stdint.h>

void board_init(void);
uint32_t board_get_state(void);

#endif
```

```c
/* board.c */
#include <stdint.h>
#include "board.h"

static uint32_t s_state;

void board_init(void) {
    s_state = 0;
}

uint32_t board_get_state(void) {
    return s_state;
}
```

## 8. 中断、调度与共享数据

| 规则 | 示例 | 禁止项 |
|---|---|---|
| `[默认约定·待确认]` ISR 只置标志/搬运数据 | `static volatile uint8_t s_rx_flag; s_rx_flag = 1;` | ISR 内 `printf`、`delay_ms`、长循环、阻塞发送 |
| `[默认约定·待确认]` 共享变量加 `volatile` | `static volatile uint32_t s_tick;` | 共享变量裸定义 |
| `[默认约定·待确认]` 多字节共享数据在短临界区读写 | 关中断只包住搬运语句 | 临界区内调用阻塞接口 |
| `[默认约定·待确认]` 正式业务用非阻塞节拍，`delay_ms` 只用于初始化/示例 | SysTick 计数 + 主循环判断到点 | 主循环长时间 `delay_ms` |

## 9. 优化偏好

| 规则 | 说明 |
|---|---|
| `[样例观察]` 编译选项 `-std=c11 -Wall -Wextra -O2 -Iinclude` | 以 Makefile 为准，新增代码保持 0 warning |
| `[样例观察]` `function_sections = true`（resources.json） | 便于后续 `--gc-sections` 裁剪未用代码 |
| `[样例观察]` `debug_info = enable` | 保留调试信息，不在开发期关闭 |
| `[默认约定·待确认]` 常量加 `const` 落 Flash；查表代替运行时计算 | 大数组/字串必须 `const` |
| `[默认约定·待确认]` 非必要不用浮点；使用前评估精度与库体积 | newlib-nano 下避免 `printf("%f")` |
| `[默认约定·待确认]` 优化前先测量（`size`/map、GPIO 翻转计时） | 不凭感觉宣称优化完成 |

> **待确认冲突**：`Makefile` 使用 `-std=c11`，`blink.resources.json` 的 `build_options.c_standard` 为 `c99`，两者不一致，需统一（影响 `for (uint8_t i = 0; ...)` 这类声明位置）。

## 10. 自查清单

- [ ] 新文件 `.c/.h` 同名配对并落在 `src/`、`include/`
- [ ] 头文件保护宏 `XXX_H`，只放声明
- [ ] 包含顺序：标准库 → 项目头，中间空一行
- [ ] 4 空格缩进，无 Tab；左花括号同行
- [ ] 函数名 `模块_动作`，时间量带 `_ms` 后缀
- [ ] 未使用参数写 `(void)param;`
- [ ] `-Wall -Wextra` 下 0 warning（无未使用变量）
- [ ] 魔法数已宏化
- [ ] 模块私有状态 `static`，头文件无变量定义
- [ ] 依赖单向，下层不 include 上层
- [ ] ISR 无打印/阻塞/长循环；共享变量 `volatile`
- [ ] TODO 写清内容，无空 TODO
- [ ] `mingw32-make` 构建 0 error、0 warning
