/**
  ******************************************************************************
  * @file    app_led.h
  * @brief   PC13 状态指示灯控制（MODULES 层）：常亮 / 常灭 / 非阻塞周期闪烁。
  *
  * 台账对应：hardware.dts &general-gpios -> led_status
  *          polarity = "active-high"（置高点亮）、safe-state = "low"（上电熄灭）
  ******************************************************************************
  */
#ifndef __APP_LED_H__
#define __APP_LED_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "main.h"

/* 硬件绑定（与台账 pin_led_pc13 同源） */
#define APP_LED_GPIO_PORT        GPIOC
#define APP_LED_GPIO_PIN         GPIO_PIN_13

/* BLINK:<ms> 允许范围（与台账 blink-period-ms-range 同源） */
#define APP_LED_BLINK_MIN_MS     1U
#define APP_LED_BLINK_MAX_MS     60000U

/* 初始化：输出安全态（熄灭） */
void app_led_init(void);

/* 常亮 / 常灭：若正处于闪烁中，立即停止闪烁并切换 */
void app_led_on(void);
void app_led_off(void);

/* 开始周期闪烁：每 period_ms 翻转一次；参数越界自动钳位 */
void app_led_blink_start(uint32_t period_ms);

/* 主循环轮询：非阻塞翻转节拍（HAL_GetTick 时间戳差值） */
void app_led_poll(void);

#ifdef __cplusplus
}
#endif

#endif /* __APP_LED_H__ */
