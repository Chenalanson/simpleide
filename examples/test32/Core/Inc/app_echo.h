/**
  ******************************************************************************
  * @file    app_echo.h
  * @brief   USART1 收发通道（MODULES 层）：DMA 循环接收 + IDLE 分段搬运，
  *          发送侧为 TX 环形缓冲 + 分块 DMA。
  *
  *          文件名沿用（工程既有 app_echo_init / app_echo_poll 调用点不变），
  *          职责由「自动回显」改为「通用收发通道」，供 app_cmd 使用。
  *
  * 台账对应：hardware.dts &uart1（rx-dma-buffer-size / rx-ring-size /
  *          tx-ring-size / tx-chunk-size 四项与本文件宏一一对应，改值先改台账）
  ******************************************************************************
  */
#ifndef __APP_ECHO_H__
#define __APP_ECHO_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "main.h"

/* 参数宏：与 hardware.dts &uart1 同源 */
#define APP_ECHO_DMA_RX_SIZE      128U   /* DMA 循环接收缓冲（字节） */
#define APP_ECHO_RX_RING_SIZE     256U   /* 接收环形缓冲（字节） */
#define APP_ECHO_TX_RING_SIZE     256U   /* 发送环形缓冲（字节） */
#define APP_ECHO_TX_CHUNK_SIZE     64U   /* 单次 TX DMA 最大字节数 */
#define APP_ECHO_POLL_CHUNK        64U   /* 单轮 poll 交给上层的最多字节数 */

/* 接收数据回调：由业务层（app_cmd）注册，主循环上下文调用 */
typedef void (*app_echo_rx_handler_t)(const uint8_t *data, uint16_t len);

/* 初始化：启动 RX DMA + IDLE 中断，并挂接 app_led / app_cmd */
HAL_StatusTypeDef app_echo_init(void);

/* 主循环轮询：分发接收数据、推进 TX 分块发送、驱动 LED 闪烁节拍 */
void app_echo_poll(void);

/* 发送 len 字节：写入 TX 环形缓冲，由 app_echo_poll 分块 DMA 发出，不阻塞
 * 返回值：实际入队的字节数（缓冲满时小于 len） */
uint16_t app_echo_write(const uint8_t *data, uint16_t len);

/* 注册接收数据回调（在 app_echo_init 之后调用） */
void app_echo_set_rx_handler(app_echo_rx_handler_t handler);

/* IDLE 中断入口：由 PERIPHERAL 层（usart.c）分发表调用 */
void app_echo_idle_isr(void);

/* 观测接口：RX 缓冲溢出次数 / TX 丢弃字节数（正常应恒为 0） */
uint32_t app_echo_overflow_count(void);
uint32_t app_echo_tx_drop_count(void);

#ifdef __cplusplus
}
#endif

#endif /* __APP_ECHO_H__ */
