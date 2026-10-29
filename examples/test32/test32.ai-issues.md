# Simple embedded AI 问题记录：test32

> 本文件由 Simple embedded 自动维护，记录 AI 协作中遇到的问题和解决方案。
> 状态为“未解决”的问题会在后续对话中用同标题更新。

<!-- simple-embedded-ai-issue title="链接错误：undefined reference to app_led_init / app_cmd_init / app_led_poll" status="resolved" -->
## 链接错误：undefined reference to app_led_init / app_cmd_init / app_led_poll

- 状态：已解决
- 现象：构建退出码 2，链接行缺少 build/app_led.o 与 build/app_cmd.o，app_echo.c:180/181/191 报未定义引用，并伴随 Unknown destination type (ARM/Thumb) 与 dangerous relocation 派生告警
- 原因：新增的 Core/Src/app_led.c、Core/Src/app_cmd.c 未加入构建源文件列表，未参与编译与链接
- 解决方案：在 IDE 工程中将 Core/Src/app_led.c、Core/Src/app_cmd.c 加入源文件列表后重建，链接行收录两个对象文件，复验构建通过（0 error / 0 warning，text 11228 / data 92 / bss 2996）
- 相关文件：Core/Src/app_echo.c、Core/Src/app_led.c、Core/Src/app_cmd.c、Makefile
<!-- /simple-embedded-ai-issue -->

<!-- simple-embedded-ai-issue title="编译错误：main.c LED_BLINK_INTERVAL_MS undeclared" status="open" -->
## 编译错误：main.c LED_BLINK_INTERVAL_MS undeclared

- 状态：未解决
- 现象：Core/Src/main.c:117 'LED_BLINK_INTERVAL_MS' undeclared，构建退出码 2
- 原因：main.h 已按方案删除该周期宏，但 main.c 的旧 LED 闪烁节拍与 s_led_last_tick 未同步删除，形成悬空引用
- 相关文件：Core/Src/main.c（USER CODE PV、USER CODE 3）、Core/Inc/main.h
<!-- /simple-embedded-ai-issue -->

<!-- simple-embedded-ai-issue title="编译错误：main.c 'LED_BLINK_INTERVAL_MS' undeclared" status="resolved" -->
## 编译错误：main.c 'LED_BLINK_INTERVAL_MS' undeclared

- 状态：已解决
- 现象：main.h 删除 LED_BLINK_INTERVAL_MS 后 main.c:117 仍引用旧闪烁节拍，报 undeclared
- 原因：main.h 侧宏已删，但 main.c USER CODE 3 旧节拍与 PV 区 s_led_last_tick 未同批删除
- 解决方案：删除 main.c USER CODE 3 整段旧节拍 + 删除 PV 区 s_led_last_tick；main.h 保持已删态。构建 0 error / 0 warning / 7.2s；串口 LED_ON / LED_OFF / BLINK:500 / ABC 全部按预期回 OK/ERR
- 相关文件：Core/Src/main.c、Core/Inc/main.h
<!-- /simple-embedded-ai-issue -->

