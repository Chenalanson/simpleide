# Simple embedded AI 问题记录：blink

> 本文件由 Simple embedded 自动维护，记录 AI 协作中遇到的问题和解决方案。
> 状态为“未解决”的问题会在后续对话中用同标题更新。

<!-- simple-embedded-ai-issue title="构建失败：Makefile MKDIR 在 sh 下使用 cmd.exe 语法" status="open" -->
## 构建失败：Makefile MKDIR 在 sh 下使用 cmd.exe 语法

- 状态：未解决
- 现象：mingw32-make 报 /usr/bin/sh: -c: line 2: syntax error: unexpected end of file，Makefile:25 Error 2
- 原因：OS=Windows_NT 分支给 MKDIR/CLEAN 赋了 cmd.exe 语法，而实际执行配方的 SHELL 是 /usr/bin/sh，sh 无法解析 if 语句
- 相关文件：Makefile:11-18、Makefile:25
<!-- /simple-embedded-ai-issue -->

