/**
  ******************************************************************************
  * @file    app_led.c
  * @brief   PC13 状态指示灯控制（MODULES 层）。非阻塞：闪烁节拍由
  *          HAL_GetTick() 时间戳差值驱动，主循环调用 app_led_poll() 推进。
  *
  * 注意：不使用 HAL_Delay；PC13 输出在 app_led_init() 中置为熄灭，
  *       闪烁期间收到 on/off 立即切换并停止闪烁。
  ******************************************************************************
  */

/* Includes ------------------------------------------------------------------*/
#include "app_led.h"

/* Private types ------------------------------------------------------------*/
typedef enum
{
  APP_LED_MODE_OFF = 0,   /* 常灭 */
  APP_LED_MODE_ON,        /* 常亮 */
  APP_LED_MODE_BLINK      /* 周期闪烁 */
} app_led_mode_t;

/* Private variables ---------------------------------------------------------*/
static volatile app_led_mode_t s_mode;        /* 当前模式 */
static volatile uint32_t       s_period_ms;   /* 闪烁翻转间隔（ms） */
static uint32_t                s_last_tick;   /* 上次翻转时刻 */

/* Private functions ---------------------------------------------------------*/

/**
  * @brief  输出电平：1 = 点亮（高电平），0 = 熄灭（低电平）。
  * @note   极性来源：hardware.dts led_status.polarity = "active-high"。
  *         若实测为低有效，只需把 GPIO_PIN_SET/RESET 对调并同步台账。
  */
static void led_write(uint8_t on)
{
  HAL_GPIO_WritePin(APP_LED_GPIO_PORT, APP_LED_GPIO_PIN,
                    (on != 0U) ? GPIO_PIN_SET : GPIO_PIN_RESET);
}

/* Exported functions --------------------------------------------------------*/

void app_led_init(void)
{
  s_mode      = APP_LED_MODE_OFF;
  s_period_ms = 0U;
  s_last_tick = HAL_GetTick();
  led_write(0U);                        /* 上电默认熄灭 */
}

void app_led_on(void)
{
  s_mode = APP_LED_MODE_ON;
  led_write(1U);
}

void app_led_off(void)
{
  s_mode = APP_LED_MODE_OFF;
  led_write(0U);
}

void app_led_blink_start(uint32_t period_ms)
{
  if (period_ms < APP_LED_BLINK_MIN_MS)
  {
    period_ms = APP_LED_BLINK_MIN_MS;
  }
  if (period_ms > APP_LED_BLINK_MAX_MS)
  {
    period_ms = APP_LED_BLINK_MAX_MS;
  }

  s_period_ms = period_ms;
  s_last_tick = HAL_GetTick();
  s_mode      = APP_LED_MODE_BLINK;
  led_write(1U);                        /* 进入闪烁先点亮，便于观察 */
}

void app_led_poll(void)
{
  if (s_mode != APP_LED_MODE_BLINK)
  {
    return;
  }
  if ((uint32_t)(HAL_GetTick() - s_last_tick) >= s_period_ms)
  {
    s_last_tick = HAL_GetTick();
    HAL_GPIO_TogglePin(APP_LED_GPIO_PORT, APP_LED_GPIO_PIN);
  }
}
