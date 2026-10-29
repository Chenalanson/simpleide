/**
  ******************************************************************************
  * @file    task_sched.c
  * @brief   非抢占时间片轮询调度器实现（MID 层）。
  *
  * 依赖方向：MID -> HAL（HAL_GetTick 时基），不依赖 APP/MODULES。
  * 数据结构：定长数组，无动态内存；任务按注册顺序线性遍历。
  ******************************************************************************
  */

/* Includes ------------------------------------------------------------------*/
#include "task_sched.h"
#include "main.h"

#include <stddef.h>

/* Private types ------------------------------------------------------------*/
typedef struct
{
  task_func_t func;        /* 任务函数 */
  uint32_t    period_ms;   /* 周期，单位 ms */
  uint32_t    last_tick;   /* 上次执行时刻（HAL_GetTick） */
} task_item_t;

/* Private variables ---------------------------------------------------------*/
static task_item_t s_tasks[TASK_SCHED_MAX_TASKS];
static uint8_t     s_task_count;

/* Exported functions --------------------------------------------------------*/

err_code_t task_sched_register(task_func_t func, uint32_t period_ms)
{
  if (func == NULL)
  {
    return ERR_NULL;
  }
  if (period_ms == 0U)
  {
    return ERR_PARAM;
  }
  if (s_task_count >= TASK_SCHED_MAX_TASKS)
  {
    return ERR_FULL;
  }

  s_tasks[s_task_count].func      = func;
  s_tasks[s_task_count].period_ms = period_ms;
  s_tasks[s_task_count].last_tick = HAL_GetTick();
  s_task_count++;

  return ERR_OK;
}

void task_sched_poll(void)
{
  uint32_t now;
  uint8_t  i;

  now = HAL_GetTick();

  for (i = 0U; i < s_task_count; i++)
  {
    if (s_tasks[i].func == NULL)
    {
      continue;
    }
    if ((uint32_t)(now - s_tasks[i].last_tick) >= s_tasks[i].period_ms)
    {
      s_tasks[i].last_tick = now;
      s_tasks[i].func();
    }
  }
}

void task_sched_clear(void)
{
  uint8_t i;

  for (i = 0U; i < TASK_SCHED_MAX_TASKS; i++)
  {
    s_tasks[i].func      = NULL;
    s_tasks[i].period_ms = 0U;
    s_tasks[i].last_tick = 0U;
  }
  s_task_count = 0U;
}

uint8_t task_sched_count(void)
{
  return s_task_count;
}
