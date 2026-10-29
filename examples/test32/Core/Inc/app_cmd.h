/**
  ******************************************************************************
  * @file    app_cmd.h
  * @brief   串口 ASCII 指令解析（MODULES 层）。
  *
  * 支持指令（以 '\n' 结束，'\r' 忽略）：
  *   LED_ON      → PC13 置高（点亮），回复 "OK:LED ON\r\n"
  *   LED_OFF     → PC13 置低（熄灭），回复 "OK:LED OFF\r\n"
  *   BLINK:<ms>  → PC13 周期翻转，间隔 ms，回复 "OK:BLINK <ms>ms\r\n"
  *   其它        → 回复 "ERR:Unknown cmd\r\n"
  *
  * 台账对应：hardware.dts &uart1（line-terminator / command-set / reply-format /
  *          boot-message 四个字段）
  ******************************************************************************
  */
#ifndef __APP_CMD_H__
#define __APP_CMD_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "main.h"

/* 单行最大长度（含结束符前的字符数，超出按未知指令处理） */
#define APP_CMD_LINE_MAX     64U

/* 初始化：注册接收回调并打印上电启动信息 "System Ready\r\n" */
void app_cmd_init(void);

/* 主循环轮询：解析在接收回调中即时完成，本接口保留为语义占位 */
void app_cmd_poll(void);

#ifdef __cplusplus
}
#endif

#endif /* __APP_CMD_H__ */
