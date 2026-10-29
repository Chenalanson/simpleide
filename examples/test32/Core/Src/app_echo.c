/**
  ******************************************************************************
  * @file    app_echo.c
  * @brief   USART1 回显实现：DMA 循环接收 + 空闲(IDLE)中断 → 环形缓冲 → TX DMA 回发。
  *
  * 数据流：
  *   PC → PA10(USART1_RX) → DMA1_Channel5 循环写入 s_dma_rx_buf[]
  *      → 一帧结束触发 IDLE 中断 → app_echo_idle_isr() 把新增区间搬进 s_rx_ring[]
  *   主循环 app_echo_poll() → 取出一块 → HAL_UART_Transmit_DMA
  *      → DMA1_Channel4 → PA9(USART1_TX) → PC
  *
  * 注意：
  *   1) 本文件不重新初始化 USART1：时钟/引脚/波特率沿用 MX_USART1_UART_Init()；
  *      DMA 句柄在 HAL_UART_MspInit() 的 USER CODE 区创建并链接（usart.c）。
  *   2) 中断侧只做「搬运字节」，不发送、不打印、不阻塞等待。
  *   3) TX 同一时刻只允许一块数据在途，由 s_tx_busy 串行化；
  *      单帧数据超过 APP_ECHO_TX_CHUNK_SIZE 时，下一轮 poll 自动续发。
  ******************************************************************************
  */

/* Includes ------------------------------------------------------------------*/
#include "app_echo.h"
#include "usart.h"

/* Private variables ---------------------------------------------------------*/
static uint8_t           s_dma_rx_buf[APP_ECHO_DMA_RX_SIZE]; /* DMA 循环接收缓冲（硬件写入） */
static volatile uint16_t s_dma_last;          /* 上次 IDLE 时 DMA 已写到的位置 */
static volatile uint8_t  s_rx_ring[APP_ECHO_RX_RING_SIZE]; /* 主环形缓冲 */
static volatile uint16_t s_rx_head;           /* 写索引：仅 IDLE 中断修改 */
static volatile uint16_t s_rx_tail;           /* 读索引：仅主循环修改 */
static volatile uint32_t s_rx_overflow;       /* 缓冲满丢弃计数 */
static uint8_t           s_tx_chunk[APP_ECHO_TX_CHUNK_SIZE]; /* 回发中转块（TX DMA 源） */
static volatile uint8_t  s_tx_busy;           /* 1 = 上一块 TX DMA 仍在发送 */

/* Private functions ---------------------------------------------------------*/

/**
  * @brief  压入一个接收字节（中断上下文调用）。
  * @param  byte 待入缓冲的字节
  */
static void echo_ring_push(uint8_t byte)
{
  uint16_t next = (uint16_t)((s_rx_head + 1U) % APP_ECHO_RX_RING_SIZE);

  if (next == s_rx_tail)                /* 缓冲已满：丢弃并计数，不阻塞中断 */
  {
    s_rx_overflow++;
    return;
  }
  s_rx_ring[s_rx_head] = byte;
  s_rx_head = next;
}

/**
  * @brief  把 DMA 缓冲区间 [from, to) 逐字节压入环形缓冲（中断上下文调用）。
  * @param  from 起始索引（含）
  * @param  to   结束索引（不含），最大为 APP_ECHO_DMA_RX_SIZE
  * @note   用索引而非指针，避免 DMA 计数恰好等于缓冲长度时产生越界指针。
  */
static void echo_ring_push_range(uint16_t from, uint16_t to)
{
  uint16_t i;

  for (i = from; i < to; i++)
  {
    echo_ring_push(s_dma_rx_buf[i]);
  }
}

/**
  * @brief  从接收缓冲取出最多 max_len 字节（主循环上下文调用）。
  * @param  dst     目标缓冲区
  * @param  max_len 本次最多取出字节数
  * @retval 实际取出字节数
  */
static uint16_t echo_ring_pop(uint8_t *dst, uint16_t max_len)
{
  uint16_t n = 0U;

  while ((n < max_len) && (s_rx_tail != s_rx_head))
  {
    dst[n] = s_rx_ring[s_rx_tail];
    n++;
    s_rx_tail = (uint16_t)((s_rx_tail + 1U) % APP_ECHO_RX_RING_SIZE);
  }
  return n;
}

/* Exported functions --------------------------------------------------------*/

HAL_StatusTypeDef app_echo_init(void)
{
  HAL_StatusTypeDef st;

  s_dma_last    = 0U;
  s_rx_head     = 0U;
  s_rx_tail     = 0U;
  s_rx_overflow = 0U;
  s_tx_busy     = 0U;

  /* 把 IDLE 中断处理注册到 PERIPHERAL 层（usart.c），由 USART1_IRQHandler 经
   * usart1_idle_dispatch() 调用，避免 it.c(PERIPHERAL) 直接依赖 MODULES（D1） */
  usart1_set_idle_callback(app_echo_idle_isr);

  /* 启动 DMA 循环接收（circular 模式不会自动停止，由 IDLE 中断分段搬运） */
  st = HAL_UART_Receive_DMA(&huart1, s_dma_rx_buf, APP_ECHO_DMA_RX_SIZE);
  if (st != HAL_OK)
  {
    return st;
  }

  /* 使能 USART1 空闲中断：一帧结束、线路空闲 1 个字符时间后触发 */
  __HAL_UART_ENABLE_IT(&huart1, UART_IT_IDLE);

  return HAL_OK;
}

void app_echo_poll(void)
{
  uint16_t len;

  if (s_tx_busy != 0U)                  /* 上一块还在发，等完成回调释放 */
  {
    return;
  }

  len = echo_ring_pop(s_tx_chunk, APP_ECHO_TX_CHUNK_SIZE);
  if (len == 0U)
  {
    return;
  }

  s_tx_busy = 1U;
  if (HAL_UART_Transmit_DMA(&huart1, s_tx_chunk, len) != HAL_OK)
  {
    s_tx_busy = 0U;                     /* 启动失败：下一轮重试，不卡死主循环 */
  }
}

void app_echo_idle_isr(void)
{
  volatile uint32_t tmp;
  uint16_t pos;

  if (__HAL_UART_GET_FLAG(&huart1, UART_FLAG_IDLE) == RESET)
  {
    return;                             /* 非 IDLE 触发，交由 HAL_UART_IRQHandler 处理 */
  }

  /* 清除 IDLE 标志：F1 规定序列为「先读 SR 再读 DR」 */
  tmp = huart1.Instance->SR;
  tmp = huart1.Instance->DR;
  (void)tmp;

  /* 循环模式下 DMA 已写入位置 = 缓冲长度 - DMA 剩余计数 */
  pos = (uint16_t)(APP_ECHO_DMA_RX_SIZE - __HAL_DMA_GET_COUNTER(huart1.hdmarx));

  if (pos != s_dma_last)
  {
    if (pos > s_dma_last)
    {
      echo_ring_push_range(s_dma_last, pos);
    }
    else
    {
      /* DMA 写指针已回绕：先搬缓冲尾部，再从 0 搬到 pos */
      echo_ring_push_range(s_dma_last, APP_ECHO_DMA_RX_SIZE);
      echo_ring_push_range(0U, pos);
    }
    s_dma_last = pos;
  }
}

uint32_t app_echo_overflow_count(void)
{
  return s_rx_overflow;
}

/* HAL callbacks -------------------------------------------------------------*/

/**
  * @brief  TX DMA 发送完成回调：释放发送权，允许主循环启动下一块。
  */
void HAL_UART_TxCpltCallback(UART_HandleTypeDef *huart)
{
  if (huart->Instance == USART1)
  {
    s_tx_busy = 0U;
  }
}

/**
  * @brief  串口错误回调：溢出/噪声/帧错误会使 HAL 中止 DMA 接收，
  *         必须恢复状态并重启 DMA + IDLE，否则通道会静默失效。
  * @note   仅当 RX 已不在搬运状态时才重启（避免 TX 侧错误误重启 RX）。
  */
void HAL_UART_ErrorCallback(UART_HandleTypeDef *huart)
{
  if (huart->Instance == USART1)
  {
    if (huart->RxState != HAL_UART_STATE_BUSY_RX)
    {
      huart->RxState = HAL_UART_STATE_READY;   /* 防止 HAL 把状态留在 BUSY_RX */
      s_dma_last = 0U;
      (void)HAL_UART_Receive_DMA(&huart1, s_dma_rx_buf, APP_ECHO_DMA_RX_SIZE);
      __HAL_UART_ENABLE_IT(&huart1, UART_IT_IDLE);
    }
  }
}
