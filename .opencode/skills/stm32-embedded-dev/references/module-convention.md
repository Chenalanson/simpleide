# 模块化编程规范（新模块开发强制参考）

> 目录
> 1. 设计原则（高内聚低耦合）
> 2. `.h` / `.c` 文件职责与骨架
> 3. 私有数据封装：static 与 get/set
> 4. 句柄结构体两种封装方式
> 5. 依赖与接口规则
> 6. 新模块开发步骤（编号流程）
> 7. 源码标记约定（ps / cubemx配置）
>
> 适用：在 MODULES（器件驱动）与 PERIPHERAL（外设驱动）新建模块、为新器件（新 SPI/I2C/UART 协议器件）写驱动时执行。

## 1. 设计原则（高内聚低耦合）

### 1.1 耦合类型对照

| 耦合方式 | 说明 | 本规程态度 |
|---|---|---|
| 非直接耦合 | 模块间无直接联系 | 目标状态 |
| 数据耦合 | 通过函数参数交换数据 | 推荐主方式 |
| 标记耦合 | 传递结构体记录 | 允许，传指针 |
| 控制耦合 | 用标志/开关/名字控制他模块 | 限制使用，接口内收敛 |
| 外部耦合 | 多模块直接访问同一全局变量 | 禁止直接访问，改走 get/set |

### 1.2 降耦四法

1. 接口设计：隐藏内部变量与私有类型，只引出必要 API。
2. 全局变量最小化：模块状态用 `static`，对外用读写接口暴露。
3. 功能单一：一个模块对应一个器件/一类外设，接口少而简单。
4. 依赖单向：模块间只做上层调下层，禁止相互调用（衔接 `architecture.md` 第 4 节 D1–D3）。

## 2. `.h` / `.c` 文件职责与骨架

| 文件 | 允许内容 | 禁止内容 |
|---|---|---|
| `xxx.h` | 头文件保护宏、`extern "C"` 包裹、结构体类型定义、枚举/错误码、对外函数声明、句柄获取函数声明、必要宏 | 变量定义、函数实现、`#include` 业务层头（防循环包含） |
| `xxx.c` | `static` 私有变量/函数、对外函数实现、私有配置表 | 在头文件中落地定义、暴露内部细节 |

`.h` 骨架：

```c
#ifndef __BSP_XXX_H__
#define __BSP_XXX_H__

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
/* 仅包含对外接口所需类型头，不包含他模块 .c */

typedef struct {
    /* 句柄成员：端口/外设基址/引脚/状态等 */
} bsp_xxx_t;

/* 对外接口声明 */
uint8_t bsp_xxx_init(void);
uint8_t bsp_xxx_read(uint16_t *out_val);
bsp_xxx_t *bsp_xxx_get_handle(void);

#ifdef __cplusplus
}
#endif

#endif /* __BSP_XXX_H__ */
```

`.c` 骨架：

```c
#include "bsp_xxx.h"
#include "bsp_spi.h"            /* 只向下依赖 PERIPHERAL */

static bsp_xxx_t s_dev;         /* 模块私有实例，外部不可见 */
static uint8_t s_init_done = 0; /* 私有状态标志 */

bsp_xxx_t *bsp_xxx_get_handle(void) { return &s_dev; }

uint8_t bsp_xxx_init(void)
{
    if (s_init_done) return ERR_OK;
    /* 参数绑定与外设时序初始化 */
    s_init_done = 1;
    return ERR_OK;
}
```

## 3. 私有数据封装：static 与 get/set

1. 模块变量（状态、缓冲指针、上次读数、错误计数）一律 `static` 于 `.c`；禁止在 `.h` 直接定义全局变量。
2. 对外访问通过函数接口，如同私有成员：

```c
static volatile uint32_t s_tick_ms;
uint32_t app_system_time(void) { return s_tick_ms; }
void bsp_xxx_set_enable(uint8_t on) { s_dev.enable = on; }
```

3. `static` 修饰的函数不得在 `.h` 声明；作用域仅限本文件。
4. 中断与主循环共享的 `static` 标志/计数加 `volatile`；多字节共享数据读取在短临界区内完成。

## 4. 句柄结构体两种封装方式

### 方式 A：外地函数初始化结构体实例（需要灵活换绑时用）

```c
/* .h 定义类型 */
typedef struct {
    SPI_TypeDef *spi;
    GPIO_TypeDef *cs_port;
    uint32_t     cs_pin;
} soft_spi_t;

/* .c 提供初始化函数绑定参数 */
void soft_spi_init(soft_spi_t *dev, SPI_TypeDef *spi,
                   GPIO_TypeDef *cs_port, uint32_t cs_pin);
```

使用处定义实例并初始化：`soft_spi_t sst7789_spi_dev; soft_spi_init(&sst7789_spi_dev, SPI1, GPIOB, GPIO_PIN_0);`

### 方式 B：本地定义实例 + 获取句柄函数（默认推荐，解耦更好）

```c
/* .c */
static bsp_485_t s_rs485_dev = {
    .p_usart_x       = USART1,
    .p_gpio_dir_port = RS485_DIR1_GPIO_Port,
    .gpio_dir_pin    = RS485_DIR1_Pin,
};
bsp_485_t *bsp_rs485_get_handle(void) { return &s_rs485_dev; }
```

上层不写 `extern`，调用 `bsp_rs485_get_handle()` 获取单例句柄。

### 传参规则（强制）

| 规则 | 说明 |
|---|---|
| 传指针，不传结构体值 | 结构体按值拷贝开销大，成员多时可能撑爆栈；统一传首地址 |
| 参数已是指针则不再取址 | `bsp_gd60932_t *device` 内部直接 `device->member`；禁止 `&device` 变成二级指针 |
| 句柄为空先判 | 使用前 `if (handle == NULL) return ERR_NULL;` |

### 外部句柄依赖场景

器件驱动（如 ST7789V 写屏需要 SPI 发送句柄）必须使用外设驱动初始化后的句柄时：在外设驱动 `.c` 定义并初始化实例并暴露 `get_handle`；器件模块 `.h` 只包含外设模块头，通过句柄获取函数取得实例，不重复声明定义。

## 5. 依赖与接口规则

1. 接口返回统一错误码（`ERR_OK / ERR_BUSY / ERR_TIMEOUT / ERR_NULL / ERR_HW`，集中定义于 MID 错误码头文件）。
2. 等待类接口带 `timeout_ms`，超时返回错误码，不裸等。
3. 模块对外接口命名：`<模块前缀>_<动作>`（`sc8701_read_voltage`、`st7789v_draw_char`）；内部函数 `static` 小写。
4. 模块间数据传递：参数传值/传指针为主；共享状态经 get/set；禁止跨模块直接改他模块 `static` 变量。
5. 头文件最小包含原则：`.h` 只放接口必需类型；实现依赖放 `.c`，降低耦合与编译时间。
6. 结构体成员含义在注释中写明单位与量程（如 `voltage_mv`、`temp_x10_c`）。

## 6. 新模块开发步骤（M-DEV，按序执行）

| 步骤 | 动作 | 产出 |
|---|---|---|
| M1 定位归属 | 按 `architecture.md` 第 5 节决策表确定 MODULES 或 PERIPHERAL；确认向下依赖的外设接口已存在 | 落点目录与依赖清单 |
| M2 设计句柄与接口表 | 列出初始化/读写/启停/错误状态接口；确定句柄成员 | 接口表（函数原型+用途+返回码） |
| M3 编写 `.h` | 保护宏、extern "C"、类型与接口声明；无私有变量定义 | 模块头文件 |
| M4 编写 `.c` | `static` 私有变量；初始化绑定；接口实现；错误码返回；超时保护 | 模块实现文件 |
| M5 挂接初始化 | MODULES 初始化加入 `app_init` 步骤 2；PERIPHERAL 加入步骤 1；顺序依赖正确 | app.c 调用点（最小改动表登记） |
| M6 注册（如用设备管理器） | 按 `software-arch.md` 4.3 节绑定 name/write/read；读函数不适用时传 NULL | 注册代码片段 |
| M7 登记 framework | 新模块写入 framework.md 依赖矩阵与调用链；任务/中断变化同步更新运行视图 | framework 增量片段 |

## 7. 源码标记约定（沿用工程习惯，生成代码时保留）

| 标记 | 含义 | 使用位置 |
|---|---|---|
| `//ps:` | 用户需要按板级实际修改的自定义值（引脚、地址、参数、结构体实例） | 初始化结构体、宏定义处 |
| `//cubemx配置:` | 提示需要在 CubeMX/工程中开启的外设、时钟、DMA、NVIC 项及配置要点 | 模块初始化函数上方 |

示例：

```c
//cubemx配置：开启 SPI1（主模式、全双工、8 位、CPOL=0/CPHA=0）、PA5/PA6/PA7 复用；CS 用普通 GPIO 软件控制
//ps: 修改为本板实际 SPI 实例与 CS 引脚
static soft_spi_t s_lcd_spi = {
    .spi   = SPI1,
    .cs_port = GPIOB,
    .cs_pin  = GPIO_PIN_0,
};
```
