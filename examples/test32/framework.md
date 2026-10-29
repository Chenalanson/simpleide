# test32 Framework 设计说明

> 硬件参数来源：`hardware.dts`（同目录，本次无变更）。
> 本文档描述当前已实现代码：CubeMX 生成工程骨架 + USART1 DMA 回显（`Core/Src/app_echo.c`）+ PC13 LED 非阻塞闪烁（`Core/Src/main.c` USER CODE 3）。
> 标注约定：`[预估]` = 按 72MHz / Cortex-M3 / Flash 2WS 估算的量级值；`[待回填]` = 需实测填入，禁止编造；`TODO` = 手册事实待核对，见 `hardware.dts` 待核对汇总块。

## 1. 分层架构

### 1.1 层级表

| 层 | 目录 | 职责 | 典型文件 | 允许调用下层 |
|---|---|---|---|---|
| 应用层 APP | `Core/Src`（USER CODE 2/3 区） | 初始化编排、主循环调度、LED 节拍、回显轮询 | `main.c` | BSP/MODULES、PERIPHERAL、HAL |
| 中间件层 MID（调度/日志/错误码） | 未建立 | N/A（任务仅 2 个，用主循环时间戳比较替代调度器；无日志与统一错误码） | — | — |
| 驱动层 BSP-MODULES（业务搬运模块） | `Core/Src`、`Core/Inc` | 回显通道：DMA/IDLE 分段搬运、环形缓冲、TX 分块串行化、错误恢复 | `app_echo.c`、`app_echo.h` | PERIPHERAL 实例、HAL |
| 驱动层 BSP-PERIPHERAL（外设驱动） | `Core/Src`、`Core/Inc` | USART1 初始化与 DMA 句柄链接、PC13 GPIO、中断向量入口 | `usart.c`、`gpio.c`、`stm32f1xx_it.c`、`stm32f1xx_hal_msp.c` | HAL |
| 硬件抽象层 HAL/LL | `Drivers/STM32F1xx_HAL_Driver`、`Drivers/CMSIS` | 寄存器封装、句柄管理 | `stm32f1xx_hal_*.c` | 硬件层 |
| 硬件层 | — | 寄存器、8MHz 晶振、USB-TTL 接线、PC13 LED | — | — |

### 1.2 目录树

