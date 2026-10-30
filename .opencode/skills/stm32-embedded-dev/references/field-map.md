# 需求映射与参数计算速查（plan_ledger 专用）

> 目录
> 1. 需求 → 台账节点 + 代码落点映射表
> 2. 参数计算公式（Q 编号与 checklist.md 对应）
> 3. 规划易错点速查（按外设）
>
> 用法：plan_ledger 查表确定资源类型；write_ledger 按台账规范落笔；参数计算对照第 2 节；易错点用于冲突预判与 consistency_check 核对。

## 1. 需求 → 台账节点 + 代码落点映射表

| 需求类别 | 台账节点/字段 | 代码落点（code-rules.md 章节 + 架构层） |
|---|---|---|
| 调试串口/日志 | uart 实例 + TX/RX 引脚组 + `app-bindings.console` | 5.2；PERIPHERAL 层 `bsp_usart` 封装 + MID 打印封装 |
| 上位机不定长收发 | uart 实例 + DMA tx/rx 预留 + 帧格式 note | 5.2 DMA 循环 + 空闲中断；环形缓冲；解析状态机在 APP/MID 层 |
| 单路电压/温度采集 | adc 控制器 + `channel@N` 子节点 | 5.3 单次轮询；换算函数在 MODULES 采集模块内 |
| 多路周期采集 | adc 多通道子节点 + 节拍 note | 5.3 扫描 DMA；数组顺序对齐通道顺序；滤波在 APP/MID 层 |
| 电机 PWM（单路/三相） | timers 实例 + 通道子节点 + 刹车/使能/看门狗安全节点 | 5.4 PWM（PERIPHERAL）；5.1 安全 GPIO；故障动作链在 APP 层调用安全接口 |
| 相电流采样 | adc 通道 + 同步触发 note + 相别 usage | 5.3 采样 + `software-arch.md` §3.3/§7（任务触发类型与耗时约束）；换算系数集中宏 |
| I2C 传感器 | i2c 总线 + 器件子节点（地址/型号） | 5.6 寄存器读写封装（PERIPHERAL `bsp_i2c`；软件模拟时显式命名 `bsp_i2c_soft`）；器件接口在 MODULES |
| SPI Flash/显示 | spi 总线 + 器件子节点（片选/频率/模式） | 5.5 收发封装（PERIPHERAL `bsp_spi`）；片选时序与帧在 MODULES 器件驱动 |
| CAN 通信 | can 实例 + 引脚组 + 速率/采样点 | 5.7 发送/过滤器/接收回调（PERIPHERAL）；解析在 APP/MID |
| 按键/LED/指示 | gpio 节点（方向/电平/触发） | 5.1 GPIO；电平翻转集中在 MODULES 器件模块（如 `bsp_key`） |
| 外部中断（故障/限位） | gpio 节点 + 优先级 note | 5.1 EXTI 回调；仅置标志/提交任务，处理在任务层 |
| 看门狗 | 安全节点 + 喂狗窗口 note | APP 层周期喂狗任务；窗口约束核对 |
| 低功耗产品 | 未用脚 note + 时钟 note + 唤醒脚节点 | PERIPHERAL 引脚处理；APP 层进入/退出点预留 |
| 器件复位/使能 | gpio 节点 + 时序 note | MODULES 初始化序列按手册时序实现 |
| 预留扩展 | `status="disabled"` 备用节点 | 暂不生成代码；台账保留待启用 |

## 2. 参数计算公式（Q1–Q7 与 checklist.md 第 4 节一一对应；资源预算另列，不占 Q 编号）

| 编号 | 场景 | 公式与判据 |
|---|---|---|
| Q1 | 串口波特率 | 过采样16：`USARTDIV = fPCLK/(16×Baud)`；BRR 整数 = floor(USARTDIV)，小数 = (USARTDIV−整数)×16；误差按 DS 限值验收；fPCLK 取时钟规划实际值 |
| Q2 | PWM/定时 | `f = fTIMCLK/((PSC+1)×(ARR+1))`；占空比 `D = CCR/(ARR+1)×100%`；APB 预分频 ≠1 时 `fTIMCLK = 2×PCLK`；中心对齐周期 ≈ `2×(ARR+1)` |
| Q3 | ADC | `Tconv = 采样时间(cycles) + 转换时间(cycles，查 DS)`；单通道最高率 ≈ `fADC_max/Tconv`；多通道串行节拍逐通道累加；分辨率与数组宽度匹配 |
| Q4 | SPI | `fSCLK ≤ fSMAX(从机)`；CPOL/CPHA 四模式以从机时序图为准；NSS 建议软件管理 |
| Q5 | I2C | standard 100k / fast 400k；fast-plus 1M 仅在 RM/DS 明确支持且从机、上拉与总线电容均满足时启用，默认不启用；地址为 7bit |
| Q6 | CAN | 位时间 = 同步段 + 传播段 + 相位段1 + 相位段2；采样点建议 75%~87.5%；全节点参数一致 |
| Q7 | 外部时序 | 上电稳定等待、复位脉宽、片选建立/保持时间对齐器件手册；不同器件分别列表 |
| — | 资源预算（非 Q 项） | 引脚占用 ≤ 封装可用 GPIO（预留 10%~20% 扩展）；DMA 通道按实时性排序；EXTI 线号同线号不可双源；RAM/Flash 只做量级提示（缓冲/查表/协议栈） |

## 3. 规划易错点速查（按外设）

| 分组 | 易错点 | 处置 |
|---|---|---|
| 时钟 | HSI 当精确通信时钟；APB 倍频漏算；主频超 DS 上限 | 台账时钟规划列全总线频率；通信优先 HSE；PWM 参数按 Q2 复算 |
| 引脚复用 | AF 号凭记忆填；ADC 通道-引脚错位；I2C 漏记上拉；同脚双引用；SWD 脚被挪用 | 全部 TODO 占位查 RM/DS；台账交叉核对占用表 |
| 串口/总线 | 波特率误差超标；I2C 地址写成 8bit；CAN 漏记采样点与终端电阻；SPI 模式配反 | 按 Q1/Q4/Q5/Q6 复算；台账字段显式记录 |
| ADC | 参考源选错；采样时间过短；电流采样漏记偏置增益；DMA 宽度不匹配 | 台账 `reference`/`acquisition-time` 备注依据；换算系数入宏 |
| PWM/安全 | 漏建刹车/使能/看门狗节点；未记互补/死区/故障动作；复位默认电平误导功率级 | 安全节点必含 `safe-state` 与故障动作注释（checklist C6/S7 + ledger-schema §6.3） |
| DMA/中断 | 通道映射随系列变化；缓冲缺 `volatile`；慢任务挂高优先级中断 | 台账只写建议并 TODO；回调轻量，慢逻辑任务化 |
| 台账工程性 | 命名混用同义词；绑定视图引用不闭合；TODO 散落无汇总；台账落后于原理图变更 | 按 ledger-schema 命名规范；引用闭合检查；变更先改台账 |
