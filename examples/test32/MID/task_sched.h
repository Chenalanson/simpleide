/**
  ******************************************************************************
  * @file    task_sched.h
  * @brief   非抢占时间片轮询调度器（MID 层）。
  *
  * 执行范式：前后台（foreground/background）+ 非抢占时间片轮询。
  *   前台 main() 的 while(1) 调用 task_sched_poll()；
  *   后台中断只做搬运与置标志，不调用本模块。
  *
  * 时基：直接复用 HAL_GetTick()（SysTick 1ms），不新增定时器中断。
  * 触发类型：本模块只支持"周期性"任务（到点执行后重载下一次到期时刻）；
  *           "一次性/条件性"任务请用状态机 + 事件标志实现，不注册到本调度器。
  *
  * 注意：任务函数必须短小非阻塞，单次执行时间应远小于最短周期；
  *       内部用无符号差值比较，HAL_GetTick() 回绕时仍成立。
  ******************************************************************************
  */
#ifndef __TASK_SCHED_H__
#define __TASK_SCHED_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "err_code.h"

/* 最大可注册任务数（按实际需要调整，占用 RAM = 任务数 x 12 字节） */
#define TASK_SCHED_MAX_TASKS     8U

/* 任务函数原型：无参数、无返回值、非阻塞 */
typedef void (*task_func_t)(void);

/**
  * @brief  注册一个周期任务。
  * @param  func      任务函数（不可为 NULL）
  * @param  period_ms 周期，单位 ms（不可为 0）
  * @retval ERR_OK    注册成功
  * @retval ERR_PARAM func 为 NULL 或 period_ms 为 0
  * @retval ERR_FULL  任务表已满（>= TASK_SCHED_MAX_TASKS）
  * @note   注册顺序即为同一节拍内的执行顺序；首个周期从注册时刻起算。
  */
err_code_t task_sched_register(task_func_t func, uint32_t period_ms);

/**
  * @brief  调度轮询：检查全部任务，到点则执行。
  * @note   在主循环中反复调用；单次调用耗时与任务数成正比。
  */
void task_sched_poll(void);

/**
  * @brief  清空任务表（复位或重新编排时使用）。
  */
void task_sched_clear(void);

/**
  * @brief  返回当前已注册任务数。
  */
uint8_t task_sched_count(void);

#ifdef __cplusplus
}
#endif

#endif /* __TASK_SCHED_H__ */
