/* USER CODE BEGIN Header */
/**
  ******************************************************************************
  * @file    usart.h
  * @brief   This file contains all the function prototypes for
  *          the usart.c file
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
#ifndef __USART_H__
#define __USART_H__

#ifdef __cplusplus
extern "C" {
#endif

/* Includes ------------------------------------------------------------------*/
#include "main.h"

/* USER CODE BEGIN Includes */

/* USER CODE END Includes */

extern UART_HandleTypeDef huart1;

/* USER CODE BEGIN Private defines */

/* USART1 空闲(IDLE)中断回调类型：由上层 MODULES 注册。
 * 目的：使 PERIPHERAL 层(stm32f1xx_it.c)不直接调用 MODULES 层函数，
 *       符合 architecture.md D1（只允许上层调用下层）。 */
typedef void (*usart1_idle_cb_t)(void);

/* USER CODE END Private defines */

void MX_USART1_UART_Init(void);

/* USER CODE BEGIN Prototypes */

/**
  * @brief  注册 USART1 IDLE 中断回调（传 NULL 可取消注册）。
  * @param  cb 回调函数；将在 USART1_IRQHandler 中经 usart1_idle_dispatch() 调用
  * @note   回调内只做搬运/置标志，禁止阻塞、打印、长循环。
  */
void usart1_set_idle_callback(usart1_idle_cb_t cb);

/**
  * @brief  USART1 IDLE 中断分发：由 USART1_IRQHandler 在 HAL_UART_IRQHandler() 之前调用。
  * @note   仅调用已注册的回调；未注册时立即返回。
  */
void usart1_idle_dispatch(void);

/* USER CODE END Prototypes */

#ifdef __cplusplus
}
#endif

#endif /* __USART_H__ */

