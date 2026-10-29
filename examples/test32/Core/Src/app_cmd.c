/**
  ******************************************************************************
  * @file    app_cmd.c
  * @brief   串口 ASCII 指令解析（MODULES 层）。
  *
  * 数据来源：app_echo 通道（DMA + IDLE）分块交付的接收字节；
  * 逐字节累积成行，遇 '\n' 整行解析；回复经 app_echo_write() 入发送缓冲。
  *
  * 注意：本模块只在主循环上下文运行，不使用 HAL_Delay，不做阻塞发送。
  ******************************************************************************
  */

/* Includes ------------------------------------------------------------------*/
#include "app_cmd.h"
#include "app_echo.h"
#include "app_led.h"

#include <stdio.h>
#include <string.h>

/* Private constants --------------------------------------------------------*/
static const char CMD_MSG_READY[]   = "System Ready\r\n";
static const char CMD_MSG_LED_ON[]  = "OK:LED ON\r\n";
static const char CMD_MSG_LED_OFF[] = "OK:LED OFF\r\n";
static const char CMD_MSG_ERROR[]   = "ERR:Unknown cmd\r\n";

/* Private variables ---------------------------------------------------------*/
static char     s_line[APP_CMD_LINE_MAX];  /* 行缓冲 */
static uint16_t s_len;                     /* 当前行长度 */
static uint8_t  s_overflow;                /* 1 = 本行超长，已丢弃 */

/* Private functions ---------------------------------------------------------*/

/**
  * @brief  发送一条固定文本（入 TX 环形缓冲，非阻塞）。
  */
static void cmd_send(const char *msg)
{
  (void)app_echo_write((const uint8_t *)msg, (uint16_t)strlen(msg));
}

/**
  * @brief  单字符转大写（避免依赖 ctype，保持最小实现）。
  */
static char cmd_upper(char c)
{
  if ((c >= 'a') && (c <= 'z'))
  {
    return (char)(c - 'a' + 'A');
  }
  return c;
}

/**
  * @brief  大小写不敏感整串比较（upper 参数须全大写）。
  */
static uint8_t cmd_str_eq(const char *s, const char *upper)
{
  while ((*s != '\0') && (*upper != '\0'))
  {
    if (cmd_upper(*s) != *upper)
    {
      return 0U;
    }
    s++;
    upper++;
  }
  return ((*s == '\0') && (*upper == '\0')) ? 1U : 0U;
}

/**
  * @brief  大小写不敏感前缀比较（upper 参数须全大写）。
  */
static uint8_t cmd_prefix_eq(const char *s, const char *upper)
{
  while (*upper != '\0')
  {
    if (*s == '\0')
    {
      return 0U;
    }
    if (cmd_upper(*s) != *upper)
    {
      return 0U;
    }
    s++;
    upper++;
  }
  return 1U;
}

/**
  * @brief  解析 "BLINK:<ms>" 的参数字段。
  * @param  s   整行字符串
  * @param  out 解析成功的周期（ms）
  * @retval 1 = 成功，0 = 格式/范围非法
  */
static uint8_t cmd_parse_blink(const char *s, uint32_t *out)
{
  const char *p = s;
  uint32_t    v = 0U;
  uint8_t     digits = 0U;

  if (cmd_prefix_eq(p, "BLINK:") == 0U)
  {
    return 0U;
  }
  p += 6;                               /* 跳过 "BLINK:" */

  while ((*p >= '0') && (*p <= '9'))
  {
    v = (v * 10U) + (uint32_t)(*p - '0');
    if (v > APP_LED_BLINK_MAX_MS)
    {
      return 0U;                        /* 越界提前退出，避免继续累加 */
    }
    p++;
    digits++;
  }

  if ((digits == 0U) || (*p != '\0'))
  {
    return 0U;                          /* 无参数或含多余字符 */
  }
  if (v < APP_LED_BLINK_MIN_MS)
  {
    return 0U;
  }

  *out = v;
  return 1U;
}

/**
  * @brief  处理一条完整指令（不含换行符）。
  */
static void cmd_handle_line(char *line)
{
  uint32_t period = 0U;
  uint16_t n;
  char     reply[32];
  int      written;

  /* 去掉行尾空白 */
  n = (uint16_t)strlen(line);
  while ((n > 0U) && ((line[n - 1U] == ' ') || (line[n - 1U] == '\t')))
  {
    n--;
    line[n] = '\0';
  }
  if (n == 0U)
  {
    return;                             /* 空行不响应 */
  }

  if (cmd_str_eq(line, "LED_ON") != 0U)
  {
    app_led_on();
    cmd_send(CMD_MSG_LED_ON);
    return;
  }

  if (cmd_str_eq(line, "LED_OFF") != 0U)
  {
    app_led_off();
    cmd_send(CMD_MSG_LED_OFF);
    return;
  }

  if (cmd_parse_blink(line, &period) != 0U)
  {
    app_led_blink_start(period);
    written = snprintf(reply, sizeof(reply), "OK:BLINK %lums\r\n", (unsigned long)period);
    if (written > 0)
    {
      cmd_send(reply);
    }
    return;
  }

  cmd_send(CMD_MSG_ERROR);
}

/**
  * @brief  接收回调：逐字节累积成行，遇 '\n' 触发解析。
  */
static void app_cmd_on_rx(const uint8_t *data, uint16_t len)
{
  uint16_t i;
  uint8_t  byte;

  if (data == 0)
  {
    return;
  }

  for (i = 0U; i < len; i++)
  {
    byte = data[i];

    if (byte == (uint8_t)'\n')
    {
      if (s_overflow != 0U)
      {
        s_overflow = 0U;
        cmd_send(CMD_MSG_ERROR);        /* 超长行按未知指令处理 */
      }
      else
      {
        s_line[s_len] = '\0';
        if (s_len > 0U)
        {
          cmd_handle_line(s_line);
        }
      }
      s_len = 0U;
    }
    else if (byte == (uint8_t)'\r')
    {
      /* 忽略 CR，兼容 CRLF 行尾 */
    }
    else if ((byte >= 0x20U) && (byte <= 0x7EU))
    {
      if (s_len < (APP_CMD_LINE_MAX - 1U))
      {
        s_line[s_len] = (char)byte;
        s_len++;
      }
      else
      {
        s_overflow = 1U;                /* 超出长度，丢弃直到行结束 */
      }
    }
    else
    {
      /* 其它控制字符忽略 */
    }
  }
}

/* Exported functions --------------------------------------------------------*/

void app_cmd_init(void)
{
  s_len      = 0U;
  s_overflow = 0U;
  s_line[0]  = '\0';

  app_echo_set_rx_handler(app_cmd_on_rx);
  cmd_send(CMD_MSG_READY);              /* 上电启动信息 */
}

void app_cmd_poll(void)
{
  /* 解析在接收回调中完成，本接口保留为语义占位，无需周期动作 */
}
