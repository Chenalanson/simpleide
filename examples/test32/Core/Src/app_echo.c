/**
  ******************************************************************************
  * @file    app_echo.c
  * @brief   USART1 收发通道（MODULES 层）。
  *
  * 接收链路：
  *   PC → PA10(USART1_RX) → DMA1_Channel5 循环写 s_dma_rx_buf[]
  *      → IDLE 中断 app_echo_idle_isr() 把新增区间搬进 s_rx_ring[]
  *      → app_echo_poll() 分块取出，交给已注册回调 app_cmd_on_rx()
  *
  * 发送链路：
  *   app_echo_write() 写入 s_tx_ring[] → app_echo_poll() 取 ≤64 B 到 s_tx_chunk[]
  *      → HAL_UART_Transmit_DMA → DMA1_Channel4 → PA9(USART1_TX) → PC
  *
  * 注意：
  *   1) 不重新初始化 USART1：时钟/引脚/波特率沿用 MX_USART1_UART_Init()。
  *   2) 中断内只搬运字节，不发送、不打印、不阻塞。
  *   3) 本文件同时是 app_led / app_cmd 的挂接点：main.c 为 CubeMX 生成文件，
  *      现有调用（app_echo_init / app_echo_poll）保持不变。
  ******************************************************************************
  */

/* Includes ------------------------------------------------------------------*/
#include "app_echo.h"
#include "usart.h"
#include "app_led.h"
#include "app_cmd.h"

/* Private variables ---------------------------------------------------------*/
static uint8_t              s_dma_rx_buf[APP_ECHO_DMA_RX_SIZE]; /* DMA 循环接收缓冲 */
static volatile uint16_t    s_dma_last;                          /* 上次 IDLE 时 DMA 已写位置 */

static volatile uint8_t     s_rx_ring[APP_ECHO_RX_RING_SIZE];    /* 接收环形缓冲 */
static volatile uint16_t    s_rx_head;                           /* 写索引：仅 ISR 修改 */
static volatile uint16_t    s_rx_tail;                           /* 读索引：仅主循环修改 */
static volatile uint32_t    s_rx_overflow;                       /* RX 缓冲满丢弃计数 */

static volatile uint8_t     s_tx_ring[APP_ECHO_TX_RING_SIZE];    /* 发送环形缓冲 */
static volatile uint16_t    s_tx_head;                           /* 写索引：主循环修改 */
static volatile uint16_t    s_tx_tail;                           /* 读索引：主循环修改 */
static volatile uint32_t    s_tx_drop;                           /* TX 丢弃字节数 */

static uint8_t              s_tx_chunk[APP_ECHO_TX_CHUNK_SIZE];  /* TX DMA 源块 */
static volatile uint8_t     s_tx_busy;                           /* 1 = 上一块仍在发送 */

static uint8_t              s_rx_tmp[APP_ECHO_POLL_CHUNK];       /* 主循环搬运临时块 */
static app_echo_rx_handler_t s_rx_handler;                       /* 业务层接收回调 */

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
  * @param  to   结束索引（不含）
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

/**
  * @brief  压入一个发送字节（主循环上下文调用）。
  * @param  byte 待发送字节
  */
static void echo_tx_push(uint8_t byte)
{
  uint16_t next = (uint16_t)((s_tx_head + 1U) % APP_ECHO_TX_RING_SIZE);

  if (next == s_tx_tail)                /* 发送缓冲已满：丢弃并计数 */
  {
    s_tx_drop++;
    return;
  }
  s_tx_ring[s_tx_head] = byte;
  s_tx_head = next;
}

/**
  * @brief  从发送缓冲取出最多 max_len 字节（主循环上下文调用）。
  */
static uint16_t echo_tx_pop(uint8_t *dst, uint16_t max_len)
{
  uint16_t n = 0U;

  while ((n < max_len) && (s_tx_tail != s_tx_head))
  {
    dst[n] = s_tx_ring[s_tx_tail];
    n++;
    s_tx_tail = (uint16_t)((s_tx_tail + 1U) % APP_ECHO_TX_RING_SIZE);
  }
  return n;
}

/**
  * @brief  重新启动 DMA 循环接收并使能 IDLE 中断（错误恢复用）。
  * @retval HAL 状态
  */
static HAL_StatusTypeDef echo_rx_restart(void)
{
  HAL_StatusTypeDef st;

  s_dma_last = 0U;
  st = HAL_UART_Receive_DMA(&huart1, s_dma_rx_buf, APP_ECHO_DMA_RX_SIZE);
  if (st == HAL_OK)
  {
    __HAL_UART_ENABLE_IT(&huart1, UART_IT_IDLE);
  }
  return st;
}

/* Exported functions --------------------------------------------------------*/

HAL_StatusTypeDef app_echo_init(void)
{
  HAL_StatusTypeDef st;

  s_dma_last    = 0U;
  s_rx_head     = 0U;
  s_rx_tail     = 0U;
  s_rx_overflow = 0U;
  s_tx_head     = 0U;
  s_tx_tail     = 0U;
  s_tx_drop     = 0U;
  s_tx_busy     = 0U;
  s_rx_handler  = 0;

  /* 把 IDLE 中断处理注册到 PERIPHERAL 层（usart.c），由 USART1_IRQHandler 经
   * usart1_idle_dispatch() 调用，避免 it.c(PERIPHERAL) 直接依赖 MODULES（D1） */
  usart1_set_idle_callback(app_echo_idle_isr);

  st = HAL_UART_Receive_DMA(&huart1, s_dma_rx_buf, APP_ECHO_DMA_RX_SIZE);
  if (st != HAL_OK)
  {
    return st;
  }
  __HAL_UART_ENABLE_IT(&huart1, UART_IT_IDLE);

  /* 业务挂接：LED 先回安全态（熄灭），命令模块随后注册接收回调并打印启动信息 */
  app_led_init();
  app_cmd_init();

  return HAL_OK;
}

void app_echo_poll(void)
{
  uint16_t n;

  /* 1) LED 非阻塞闪烁节拍（始终推进，与串口数据无关） */
  app_led_poll();

  /* 2) 接收数据交给业务层（整行解析在 app_cmd 内完成） */
  if (s_rx_handler != 0)
  {
    n = echo_ring_pop(s_rx_tmp, APP_ECHO_POLL_CHUNK);
    if (n > 0U)
    {
      s_rx_handler(s_rx_tmp, n);
    }
  }

  /* 3) 推进发送：同一时刻只有一块在途 */
  if (s_tx_busy != 0U)
  {
    return;
  }
  n = echo_tx_pop(s_tx_chunk, APP_ECHO_TX_CHUNK_SIZE);
  if (n == 0U)
  {
    return;
  }
  s_tx_busy = 1U;
  if (HAL_UART_Transmit_DMA(&huart1, s_tx_chunk, n) != HAL_OK)
  {
    s_tx_busy = 0U;
    s_tx_drop += n;                     /* 本块未发出，计入丢弃用于观测 */
  }
}

uint16_t app_echo_write(const uint8_t *data, uint16_t len)
{
  uint16_t i;

  if (data == 0)
  {
    return 0U;
  }
  for (i = 0U; i < len; i++)
  {
    echo_tx_push(data[i]);
  }
  return len;
}

void app_echo_set_rx_handler(app_echo_rx_handler_t handler)
{
  s_rx_handler = handler;
}

uint32_t app_echo_overflow_count(void)
{
  return s_rx_overflow;
}

uint32_t app_echo_tx_drop_count(void)
{
  return s_tx_drop;
}

/* HAL 回调 ----------------------------------------------------------------*/

void app_echo_idle_isr(void)
{
  uint16_t pos;
  uint16_t from;
  uint16_t to;

  if (__HAL_UART_GET_FLAG(&huart1, UART_FLAG_IDLE) == RESET)
  {
    return;                             /* 非 IDLE 事件，交给 HAL_UART_IRQHandler 处理 */
  }
  /* 清 IDLE：读 SR 后读 DR（RM USART「Idle line detection」，台账已核对） */
  __HAL_UART_CLEAR_IDLEFLAG(&huart1);

  pos = (uint16_t)(APP_ECHO_DMA_RX_SIZE - (uint16_t)__HAL_DMA_GET_COUNTER(huart1.hdmarx));
  if (pos >= APP_ECHO_DMA_RX_SIZE)
  {
    pos = 0U;
  }

  from = s_dma_last;
  to   = pos;

  if (from < to)
  {
    echo_ring_push_range(from, to);
  }
  else if (from > to)                   /* 跨过缓冲尾部：分两段搬运 */
  {
    echo_ring_push_range(from, APP_ECHO_DMA_RX_SIZE);
    echo_ring_push_range(0U, to);
  }
  else
  {
    /* 无新增字节 */
  }

  s_dma_last = to;
}

void HAL_UART_TxCpltCallback(UART_HandleTypeDef *huart)
{
  if (huart->Instance != USART1)
  {
    return;
  }
  s_tx_busy = 0U;                       /* 释放发送权，允许下一块 */
}

void HAL_UART_ErrorCallback(UART_HandleTypeDef *huart)
{
  if (huart->Instance != USART1)
  {
    return;
  }
  /* ORE/NE/FE 时 HAL 会中止 DMA 接收，必须重启，否则通道静默失效 */
  if (huart->RxState != HAL_UART_STATE_BUSY_RX)
  {
    (void)echo_rx_restart();
  }
}
