/* USER CODE BEGIN Header */
/**
  ******************************************************************************
  * @file           : main.h
  * @brief          : Header for main.c file.
  *                   This file contains the common defines of the application.
  ******************************************************************************
  * @attention
  *
  * Copyright (c) 2026 STMicroelectronics.
  * All rights reserved.
  *
  * This software is licensed under terms that can be found in the LICENSE file
  * in the root directory of this software component.
  * If no LICENSE file comes with this software, it is provided AS-IS.
  *
  ******************************************************************************
  */
/* USER CODE END Header */

/* Define to prevent recursive inclusion -------------------------------------*/
#ifndef __MAIN_H
#define __MAIN_H

#ifdef __cplusplus
extern "C" {
#endif

/* Includes ------------------------------------------------------------------*/
#include "stm32f1xx_hal.h"

/* Private includes ----------------------------------------------------------*/
/* USER CODE BEGIN Includes */

/* USER CODE END Includes */

/* Exported types ------------------------------------------------------------*/
/* USER CODE BEGIN ET */

/* USER CODE END ET */

/* Exported constants --------------------------------------------------------*/
/* USER CODE BEGIN EC */

/* USER CODE END EC */

/* Exported macro ------------------------------------------------------------*/
/* USER CODE BEGIN EM */

/* USER CODE END EM */

/* Exported functions prototypes ---------------------------------------------*/
void Error_Handler(void);

/* USER CODE BEGIN EFP */

/* USER CODE END EFP */

/* Private defines -----------------------------------------------------------*/

/* USER CODE BEGIN Private defines */

/* ---- LED 引脚（与 hardware.dts 台账 pin_led_pc13 / led_status 节点同源） ----
 * 闪烁周期、闪烁使能与极性映射统一由 MODULES 层 app_led 模块管理
 * （参数宏见 Core/Inc/app_led.h）；main.c 不再定义闪烁周期宏，也不保留第二条节拍。
 * 此处仅保留「上电初始熄灭」所需的引脚与极性定义。
 */
#define LED_GPIO_PORT              GPIOC
#define LED_GPIO_PIN               GPIO_PIN_13
#define LED_ACTIVE_LOW             0      /* 与台账 led_status.polarity="active-high" 同源；
                                           * 若实测点亮/熄灭相反，须同时改台账 polarity、
                                           * app_led.c 的 led_write() 映射与本宏，不可只改一处 */

/* USER CODE END Private defines */

#ifdef __cplusplus
}
#endif

#endif /* __MAIN_H */
