/**
  ******************************************************************************
  * @file    app_echo.h
  * @brief   USART1 回显通道接口（PC 发什么，单片机回什么）。
  *          接收：DMA1_Channel5 循环模式 + USART1 空闲(IDLE)中断，无逐字节中断开销；
  *          发送：DMA1_Channel4 单次模式，主循环只做搬运调度。
  *          对应 hardware.dts 台账 &uart1 / &dma1 节点与 app-bindings.console。
  ******************************************************************************
  */

#ifndef __APP_ECHO_H
#define __APP_ECHO_H

#ifdef __cplusplus
extern "C" {
#endif

#include "main.h"

/* ---- 参数宏（与台账 &uart1 同源，改这里不要改函数体） ---- */
#define APP_ECHO_DMA_RX_SIZE      128U   /* DMA 循环接收缓冲字节数（1 ~ 65535） */
#define APP_ECHO_RX_RING_SIZE     256U   /* 主环形缓冲字节数（实际可用 255 字节） */
#define APP_ECHO_TX_CHUNK_SIZE    64U    /* 单次 TX DMA 最大字节数 */

/**
  * @brief  启动 USART1 回显：启动 DMA 循环接收并使能 IDLE 中断。
  * @retval HAL_OK 成功；其它为 HAL 错误码（此时不应继续使用回显通道）
  * @note   必须在 MX_USART1_UART_Init() 之后调用（DMA 句柄在 MspInit 中已链接）。
  */
HAL_StatusTypeDef app_echo_init(void);

/**
  * @brief  主循环轮询：把接收缓冲中的字节通过 TX DMA 回发给 PC。
  *         无数据或上一块未发完时立即返回，非阻塞。
  */
void app_echo_poll(void);

/**
  * @brief  USART1 空闲中断服务：把 DMA 缓冲中新增的区间搬进主环形缓冲。
  * @note   通过 usart1_set_idle_callback() 注册到 PERIPHERAL 层；由
  *         USART1_IRQHandler → usart1_idle_dispatch() 在 HAL_UART_IRQHandler()
  *         之前调用；函数内部自行判断并清除 IDLE 标志，非 IDLE 触发时立即返回。
  */
void app_echo_idle_isr(void);

/**
  * @brief  获取因缓冲满而被丢弃的字节计数（调试观测用，正常运行应保持 0）。
  */
uint32_t app_echo_overflow_count(void);

#ifdef __cplusplus
}
#endif

#endif /* __APP_ECHO_H */
