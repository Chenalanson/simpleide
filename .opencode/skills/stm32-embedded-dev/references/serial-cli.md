# 串口 CLI 调试参考（serial-cli）

本文件规定固件侧串口 CLI 的实现方式，以及 AI 通过 IDE `se-cli` 协议与开发板交互时的约定。
目标是：**同一条命令，人能在 IDE 的 CLI 窗口里敲，AI 也能用 `se-cli` 发**，输出格式一致、可解析、可复现。

---

## 1. 边界与前置条件

| 项 | 约定 |
|---|---|
| 传输 | UART，8N1，默认 115200；波特率必须与工程 CLI 配置一致 |
| 行尾 | 默认 `CRLF`；固件同时接受 `\r`、`\n`、`\r\n` |
| 编码 | ASCII；中文说明放在注释里，不放在响应中 |
| 单行长度 | ≤ 200 字节，超出直接丢弃并回 `ERR 2 line-too-long` |
| 响应 | 每条命令至少一行；成功 `OK ...`，失败 `ERR <code> <message>` |
| 非阻塞 | `cli_poll()` 单次执行 ≤ 5 ms（按 `CLI_POLL_MAX_BYTES` 限量）；禁止在 ISR 里解析命令 |
| 动态内存 | 禁止 `malloc`；命令表和缓冲区全部静态分配 |
| 复位 | `reset` 只做 `NVIC_SystemReset()`；擦除/写 Flash 必须二次确认，默认不提供 |
| 调试口 | CLI 占用一路 UART；不要和业务协议共用同一路，除非协议明确多路复用 |

> 烧录后先确认运行的是本轮新固件（O13），再确认串口能收到 `help` 的响应，然后才让 AI 发 `se-cli`。没有响应时不要假设命令执行成功。

---

## 2. 文件落点（默认分层架构）

| 文件 | 层 | 职责 |
|---|---|---|
| `BSP/PERIPHERAL/bsp_cli.h` | BSP/PERIPHERAL | 命令结构体、注册/初始化/轮询接口 |
| `BSP/PERIPHERAL/bsp_cli.c` | BSP/PERIPHERAL | 行缓冲、分词、命令查找、内置命令 |
| `BSP/PERIPHERAL/bsp_usart.c/.h` | BSP/PERIPHERAL | USART 初始化、收发、RX 环形缓冲（若已存在则沿用） |
| `APP/app.c` | APP | `app_init()` 中 USART 初始化后调用 `cli_init()`，注册业务命令 |
| `APP/app.c` | APP | `app_sched_thread()` 或 `app_loop_thread()` 周期调用 `cli_poll()` |

现有工程已有 UART 封装时**沿用**，只补 CLI 层；不要为 CLI 再初始化一路 UART。

---

## 3. 最小实现骨架

### 3.1 `bsp_cli.h`

```c
#ifndef BSP_CLI_H
#define BSP_CLI_H

#include <stdint.h>
#include <stddef.h>

#define CLI_LINE_MAX      200u
#define CLI_ARGC_MAX      8u

typedef struct {
    const char *name;                                   /* 命令名，区分大小写 */
    const char *usage;                                  /* help 中显示的一行用法 */
    void (*handler)(int argc, char *argv[]);            /* 命令处理函数 */
} cli_command_t;

void cli_init(void);
void cli_register(const cli_command_t *commands, size_t count);
void cli_poll(void);

/* 供命令实现输出；内部只做有界写，不阻塞等发送完成 */
void cli_write(const char *text);
void cli_write_line(const char *text);
void cli_printf(const char *fmt, ...);

#endif /* BSP_CLI_H */
```

### 3.2 `bsp_cli.c`（核心逻辑）

```c
#include "bsp_cli.h"
#include "bsp_usart.h"       /* 工程现有 USART 封装：bsp_usart_read_byte / bsp_usart_write */
#include "main.h"           /* CubeMX 工程；裸机工程改为对应 CMSIS 设备头 */
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdbool.h>

#define CLI_CMD_MAX       24u
#define CLI_PROMPT        "cli> "
#define CLI_POLL_MAX_BYTES 64u   /* 单次 cli_poll 最多处理的字节数，按波特率/主频核算 ≤5 ms */

static cli_command_t s_commands[CLI_CMD_MAX];
static size_t s_command_count;
static char s_line[CLI_LINE_MAX];
static size_t s_line_len;
static uint8_t s_ignore_lf;

static void cli_builtin_help(int argc, char *argv[]);
static void cli_builtin_info(int argc, char *argv[]);
static void cli_builtin_echo(int argc, char *argv[]);
static void cli_builtin_reset(int argc, char *argv[]);

static const cli_command_t s_builtins[] = {
    { "help",  "help                列出所有命令", cli_builtin_help },
    { "info",  "info                输出版本、时钟、复位状态（原始 RCC_CSR）", cli_builtin_info },
    { "echo",  "echo <text>         原样回显，用于链路自检", cli_builtin_echo },
    { "reset", "reset               软件复位 MCU", cli_builtin_reset },
};

void cli_init(void)
{
    s_command_count = 0u;
    s_line_len = 0u;
    s_ignore_lf = 0u;
    cli_register(s_builtins, sizeof(s_builtins) / sizeof(s_builtins[0]));
    cli_write_line("");
    cli_write_line("Simple embedded CLI ready. Type 'help'.");
    cli_write(CLI_PROMPT);
}

void cli_register(const cli_command_t *commands, size_t count)
{
    if (commands == NULL) {
        return;
    }
    for (size_t i = 0u; i < count && s_command_count < CLI_CMD_MAX; i++) {
        if (commands[i].name == NULL || commands[i].handler == NULL) {
            continue;
        }
        s_commands[s_command_count++] = commands[i];
    }
}

void cli_write(const char *text)
{
    if (text != NULL) {
        bsp_usart_write((const uint8_t *)text, strlen(text));
    }
}

void cli_write_line(const char *text)
{
    cli_write(text);
    cli_write("\r\n");
}

void cli_printf(const char *fmt, ...)
{
    char buffer[160];
    va_list args;
    va_start(args, fmt);
    int written = vsnprintf(buffer, sizeof(buffer), fmt, args);
    va_end(args);
    if (written > 0) {
        cli_write(buffer);
    }
}

static void cli_execute(char *line)
{
    char *argv[CLI_ARGC_MAX];
    int argc = 0;
    char *cursor = line;

    while (*cursor != '\0' && argc < (int)CLI_ARGC_MAX) {
        while (*cursor == ' ' || *cursor == '\t') {
            cursor++;
        }
        if (*cursor == '\0') {
            break;
        }
        argv[argc++] = cursor;
        while (*cursor != '\0' && *cursor != ' ' && *cursor != '\t') {
            cursor++;
        }
        if (*cursor != '\0') {
            *cursor++ = '\0';
        }
    }
    if (argc == 0) {
        return;
    }
    for (size_t i = 0u; i < s_command_count; i++) {
        if (strcmp(s_commands[i].name, argv[0]) == 0) {
            s_commands[i].handler(argc, argv);
            return;
        }
    }
    cli_printf("ERR 1 unknown-command %s\r\n", argv[0]);
}

void cli_poll(void)
{
    uint8_t byte;
    uint16_t budget = CLI_POLL_MAX_BYTES;   /* 单次限量；剩余字节留在环形缓冲，下次再处理 */

    while (budget > 0u && bsp_usart_read_byte(&byte)) {
        budget--;
        if (byte == '\r') {
            s_ignore_lf = 1u;
        } else if (byte == '\n') {
            if (s_ignore_lf != 0u) {
                s_ignore_lf = 0u;
                continue;
            }
        } else {
            s_ignore_lf = 0u;
        }

        if (byte == '\r' || byte == '\n') {
            s_line[s_line_len] = '\0';
            if (s_line_len > 0u) {
                cli_execute(s_line);
            }
            s_line_len = 0u;
            cli_write(CLI_PROMPT);
        } else if (byte == 0x08u || byte == 0x7Fu) {
            if (s_line_len > 0u) {
                s_line_len--;
            }
        } else if (byte >= 0x20u && byte < 0x7Fu) {
            if (s_line_len + 1u < CLI_LINE_MAX) {
                s_line[s_line_len++] = (char)byte;
            } else {
                s_line_len = 0u;
                cli_write_line("ERR 2 line-too-long");
                cli_write(CLI_PROMPT);
            }
        }
    }
}

static void cli_builtin_help(int argc, char *argv[])
{
    (void)argc;
    (void)argv;
    cli_write_line("commands:");
    for (size_t i = 0u; i < s_command_count; i++) {
        cli_write("  ");
        cli_write_line(s_commands[i].usage);
    }
    cli_write_line("OK help");
}

static void cli_builtin_info(int argc, char *argv[])
{
    (void)argc;
    (void)argv;
    cli_printf("build: %s %s\r\n", __DATE__, __TIME__);
    cli_printf("clock: %lu Hz\r\n", (unsigned long)SystemCoreClock);
    /* TODO: 按系列 RM 核对 RCC_CSR 复位标志位定义；未核实前只输出原始值，不解释 */
    cli_printf("reset-csr: 0x%08lX\r\n", (unsigned long)RCC->CSR);
    cli_write_line("OK info");
}

static void cli_builtin_echo(int argc, char *argv[])
{
    cli_write("echo:");
    for (int i = 1; i < argc; i++) {
        cli_write(" ");
        cli_write(argv[i]);
    }
    cli_write_line("");
    cli_write_line("OK echo");
}

static void cli_builtin_reset(int argc, char *argv[])
{
    (void)argc;
    (void)argv;
    cli_write_line("OK reset");
    NVIC_SystemReset();
}
```

> `bsp_usart_read_byte` 返回 1 表示取到字节，0 表示缓冲为空。若工程现有 UART 只有中断回调，请在回调里写入环形缓冲，再由 `bsp_usart_read_byte` 出队；ISR 内不要调用 `cli_*`。

`bsp_usart.h` 至少需要提供给 CLI 使用的三个接口：

```c
void    bsp_usart_init(void);
void    bsp_usart_write(const uint8_t *data, size_t len);
uint8_t bsp_usart_read_byte(uint8_t *byte);   /* 1 = 取到，0 = 当前无数据 */
```

### 3.3 APP 侧接入

```c
/* APP/app.c — app_init() */
void app_init(void)
{
    bsp_usart_init();
    cli_init();
    /* 注册业务命令，例如 LED、ADC、PWM */
    cli_register(app_commands, app_command_count);
}

/* APP/app.c — app_sched_thread()：周期任务，CLI 轮询放这里，周期 ≤ 10 ms */
void app_sched_thread(void)
{
    cli_poll();
    task_time_sched();      /* 工程现有非抢占调度器 */
}

/* APP/app.c — app_loop_thread()：连续状态机任务 */
void app_loop_thread(void)
{
    multi_menu_ui_task();   /* 工程现有连续状态机任务，按实际替换 */
}
```

---

## 4. 业务命令示例

### 4.1 LED

```c
static void cmd_led(int argc, char *argv[])
{
    if (argc != 2) {
        cli_write_line("ERR 10 usage: led on|off");
        return;
    }
    if (strcmp(argv[1], "on") == 0) {
        bsp_led_set(1);
    } else if (strcmp(argv[1], "off") == 0) {
        bsp_led_set(0);
    } else {
        cli_write_line("ERR 11 usage: led on|off");
        return;
    }
    cli_printf("led: %s\r\nOK led\r\n", argv[1]);
}
```

### 4.2 ADC

```c
static void cmd_adc(int argc, char *argv[])
{
    (void)argc;
    (void)argv;
    uint32_t mv = 0u;
    if (bsp_adc_read_mv(&mv) != 0) {
        cli_write_line("ERR 20 adc-not-ready");
        return;
    }
    cli_printf("adc: %lu mV\r\n", (unsigned long)mv);
    cli_write_line("OK adc");
}
```

### 4.3 寄存器读写（谨慎）

```c
/* 默认只读；量产固件保持 0，调试版才允许写。 */
#define CLI_REG_WRITE_ENABLE   0

typedef struct {
    uint32_t base;
    uint32_t size;
    const char *name;
} reg_window_t;

#if !defined(REG_WIN_APB1_BASE) || !defined(REG_WIN_APB1_SIZE) || \
    !defined(REG_WIN_APB2_BASE) || !defined(REG_WIN_APB2_SIZE) || \
    !defined(REG_WIN_AHB_BASE)  || !defined(REG_WIN_AHB_SIZE)
#error "TODO: 先按本芯片 RM Memory map 定义 REG_WIN_* 白名单窗口"
#endif

/* TODO: 按本芯片 RM 的 Memory map 在 bsp_conf.h 定义窗口；不同系列基址/长度不同，禁止照抄。 */
static const reg_window_t s_reg_windows[] = {
    { REG_WIN_APB1_BASE, REG_WIN_APB1_SIZE, "apb1" },
    { REG_WIN_APB2_BASE, REG_WIN_APB2_SIZE, "apb2" },
    { REG_WIN_AHB_BASE,  REG_WIN_AHB_SIZE,  "ahb"  },
};

static bool reg_addr_allowed(uint32_t addr)
{
    if ((addr & 0x3u) != 0u) {              /* 只允许 32 位对齐访问 */
        return false;
    }
    for (size_t i = 0u; i < sizeof(s_reg_windows) / sizeof(s_reg_windows[0]); i++) {
        uint32_t end = s_reg_windows[i].base + s_reg_windows[i].size;
        if (addr >= s_reg_windows[i].base && addr < end) {
            return true;
        }
    }
    return false;
}

static void cmd_reg(int argc, char *argv[])
{
    if (argc < 3) {
        cli_write_line("ERR 30 usage: reg read|write <addr> [value]");
        return;
    }
    uint32_t addr = (uint32_t)strtoul(argv[2], NULL, 0);
    if (!reg_addr_allowed(addr)) {
        cli_write_line("ERR 31 address-not-allowed");
        return;
    }
    if (strcmp(argv[1], "read") == 0) {
        cli_printf("reg[0x%08lX] = 0x%08lX\r\n", (unsigned long)addr,
                   (unsigned long)*(volatile uint32_t *)addr);
        cli_write_line("OK reg");
    } else if (strcmp(argv[1], "write") == 0 && argc == 4) {
#if CLI_REG_WRITE_ENABLE
        *(volatile uint32_t *)addr = (uint32_t)strtoul(argv[3], NULL, 0);
        cli_write_line("OK reg");
#else
        cli_write_line("ERR 33 reg-write-disabled");
#endif
    } else {
        cli_write_line("ERR 32 usage: reg read|write <addr> [value]");
    }
}
```

寄存器写命令默认编译关闭（`CLI_REG_WRITE_ENABLE 0`）；只读命令也必须过按芯片定义的地址白名单与 4 字节对齐检查，禁止用单一 `0x4000xxxx` 通配。

---

## 5. 响应格式（AI 解析依赖）

| 场景 | 固件输出 |
|---|---|
| 成功 | 数据行（可选）+ `OK <command>` |
| 参数错误 | `ERR 10 usage: ...` |
| 未就绪 | `ERR 20 adc-not-ready` |
| 未知命令 | `ERR 1 unknown-command <name>` |
| 超长行 | `ERR 2 line-too-long` |
| 寄存器写被关闭 | `ERR 33 reg-write-disabled` |
| 复位前 | `OK reset` |

约定：

1. 每条命令的最后一个成功行必须是 `OK <command>`；失败必须是 `ERR <code> <message>`。
2. 数据行使用 `key: value`，一行一个值，便于 AI 引用。
3. 不要输出 `\0`、ANSI 颜色或非 ASCII；不要用 `printf` 直接打印浮点（newlib-nano 默认不支持 `%f`）。
4. 响应总长 ≤ 2 KB；长列表用分页命令（`help` 最多一次列完，命令总数 ≤ 24）。

---

## 6. AI / IDE 交互流程

1. 满足 O13 才能发 `se-cli`：`build.build_ok == true`，且 `flash.flashed == true` 或用户确认正在运行的是本轮新固件；`build.artifact` 的哈希/时间戳与烧录产物一致。不满足时先 `flash`（预览 + 用户执行）或 `pending_ask`，不得用旧固件输出冒充本轮结果。
2. AI 输出 `se-cli` 块：每行一条命令，先 `help` 确认命令表，再发目标命令。
3. IDE 按工程 CLI 配置（端口、波特率、行尾、等待时间）逐条发送，等待每条命令的超时后收集输出。
4. IDE 把真实输出作为下一条消息发回；AI 只能依据回传内容判断，禁止编造。
5. 无响应时依次检查：端口/波特率 → TX/RX 接线 → 固件是否包含 `cli_poll()` → 是否与业务协议抢占同一 UART；仍无响应 → 转 `observe`（LED/GPIO/watch）或 `pending_ask(kind=observe)`，验证状态保持未验证，禁止写“通过”。
6. 问题及解决方案用 `se-issue` 写入问题记录（`quality.issues` / IDE 问题面板）；不要另建第二份问题文件。

示例：

`````text
````se-cli
help
info
adc read
````
`````

---

## 7. 集成检查表

| 检查项 | 要求 |
|---|---|
| 文件 | `bsp_cli.c/.h`、`bsp_usart.c/.h` 落在 `BSP/PERIPHERAL/`，不散落根目录 |
| 构建 | 新 `.c` 已加入 `C_SOURCES`（或 Keil 工程组） |
| 初始化 | `cli_init()` 在 UART 初始化之后调用 |
| 轮询 | 主循环/调度任务周期调用 `cli_poll()`，周期 ≤ 10 ms |
| 缓冲 | RX 环形缓冲 ≥ 256 字节，溢出置错误标志 |
| 中断 | ISR 只搬运字节，不解析、不打印、不阻塞 |
| 安全 | 写寄存器/Flash/复位命令有范围限制或编译开关 |
| 文档 | `hardware.dts` 记录调试串口节点；`framework.md` 更新调用链 |
| 验证 | 构建 0 error；O13 满足（新固件已运行）；`help` 有响应；每条命令有 `OK`/`ERR` |

---

## 8. 常见故障对照

| 现象 | 优先检查 |
|---|---|
| 完全无输出 | 波特率、TX 引脚、`cli_poll()` 是否被调用、UART 是否初始化 |
| 乱码 | 系统时钟与波特率计算、晶振频率、`BRR` 配置 |
| 命令无响应但 `help` 有 | 命令名拼写、命令是否注册、`CLI_CMD_MAX` 是否已满 |
| 只收到前半行 | 行缓冲太小、ISR 丢字节、发送阻塞导致溢出 |
| 复位后 CLI 消失 | `cli_init()` 未在启动流程调用，或进入了错误处理死循环 |
| 与业务协议冲突 | 调试 UART 与业务 UART 复用，改为独立调试口 |
