/**
  ******************************************************************************
  * @file    err_code.h
  * @brief   统一错误码定义（MID 层）。
  *
  * 分层归属：MID/ 公共机制头文件，被 APP、MID、BSP/MODULES、BSP/PERIPHERAL 引用；
  *           本文件不包含任何 HAL 头，避免向上反向依赖。
  *
  * 约定：
  *   - 成功统一返回 ERR_OK（值为 0），便于 if (ret != ERR_OK) 判定；
  *   - 需要与 HAL 状态转换时在具体模块内完成，不在此处引入 HAL 类型。
  ******************************************************************************
  */
#ifndef __ERR_CODE_H__
#define __ERR_CODE_H__

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>

typedef enum
{
  ERR_OK = 0,       /* 成功 */
  ERR_PARAM,        /* 参数非法（越界、长度 0、周期 0 等） */
  ERR_NULL,         /* 空指针 */
  ERR_BUSY,         /* 资源忙（正在发送、缓冲区占用） */
  ERR_TIMEOUT,      /* 超时 */
  ERR_FULL,         /* 缓冲区/注册表已满 */
  ERR_EMPTY,        /* 缓冲区/注册表为空 */
  ERR_HW,           /* 硬件或底层外设返回错误 */
  ERR_STATE         /* 当前状态不允许该操作 */
} err_code_t;

#ifdef __cplusplus
}
#endif

#endif /* __ERR_CODE_H__ */
