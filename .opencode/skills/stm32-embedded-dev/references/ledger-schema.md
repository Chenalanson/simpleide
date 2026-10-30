# 硬件台账 .dts 结构规范与节点模板

> 目录
> 1. 定位与总体约定
> 2. 文件头规范
> 3. 字段语义表
> 4. 命名规范
> 5. 完整台账范例（可直接套用骨架）
> 6. 各类外设节点模板
> 7. 注释与待核对标注规范
>
> 本文件是台账生成的唯一结构依据。所有手册事实数值（AF 号、IRQn、DMA 映射、地址、极限频率）在未查阅 RM/DS 前一律以 `/* TODO: ... */`（块注释外）或 `// TODO:`（块注释内）占位，禁止编造。

## 1. 定位与总体约定

- 本 `.dts` 是**人工使用的硬件资源台账**，借鉴 DTS 节点/属性风格；不是标准设备树绑定，不参与 DTC/Zephyr 编译。
- **台账是程序代码的参数来源**：后续 C 代码的初始化取值（波特率、通道号、引脚、地址、速率、缓冲规划）以本台账为准，不在代码中另写一套参数；映射关系见 `code-rules.md` 第 2 节，双向核对见 `checklist.md` 第 1 节。
- 自描述：引脚组、通道等定义全部写在本文件内，不引用外部 `*.dtsi`。
- 一个项目一份主台账；多板/多版本用后缀区分（`hardware_nucleo_g474.dts`、`hardware_v2.dts`）。
- 文件编码 UTF-8 无 BOM；缩进 4 空格；注释统一 `/* ... */`；块注释内禁止再嵌套 `/* */`，块内待核对项写 `// TODO:` 文本。

## 2. 文件头规范

文件顶部必须包含块注释，结构如下（值留空处由用户/后续填写）：

```dts
/* ============================================================================
 * 项目硬件资源台账（DTS 风格记录，仅供人工开发对照，不参与编译）
 * 项目名称 : <填写>
 * 芯片型号 : STM32G474RET6            // TODO: 替换为实际型号，后缀决定封装与资源量
 * 封装/板型: LQFP64 / <板型>          // TODO: 核对封装，引脚数以 DS 封装表为准
 * 板版本号 : V0.1 草稿
 * 生成日期 : YYYY-MM-DD
 * 维护说明 : 方案变更先改本台账；带 TODO/待核对 的条目须对照 RM/DS 销项后删除标注
 *
 * 图例：
 *   status = "okay"      已规划使用的资源
 *   status = "disabled"  盘点存在但本版暂不使用
 *   // TODO: ...         待核对/待填写项，禁止凭猜测填值
 * ============================================================================ */
```

> C/DTS 块注释不能嵌套：上面的 `// TODO:` 只是块注释内的文字标记。若写成块外真正的 `//` 注释，必须移到 `/* ... */` 结束之后。

## 3. 字段语义表（台账自定义语义，非标准绑定）

| 字段 | 台账含义 | 取值约定 |
|---|---|---|
| `compatible` | 外设类型标签，便于分类检索 | `stm32,usart`、`stm32,adc`、`stm32,pwm-timer`、`stm32,spi`、`stm32,i2c`、`stm32,can`、`stm32,gpio`、`stm32,watchdog`、`ext,i2c-device`、`ext,spi-device` |
| `status` | 规划状态 | `"okay"` / `"disabled"` |
| `usage` | 该资源的中文业务用途 | 字符串，一句话说明 |
| `note` | 补充说明、约束、风险提示 | 字符串；待核对内容优先放 TODO 注释 |
| `pinctrl-0` | 引用的引脚组 label 列表 | `<&pin_xxx &pin_yyy>`；引脚组定义见第 5.3 节 |
| `pinctrl-names` | 引脚组用途名 | `"default"`；多状态预留时可加 `"sleep"` 注释说明 |
| `clocks` | 时钟规划（来源、总线、目标频率） | 用注释 + 数值属性描述，不写寄存器位 |
| `interrupts` | 建议中断向量号与优先级 | `<IRQn 符号 优先级>`；IRQn 待核对 RM 向量表，未知写 `<IRQ_TODO 优先级>` |
| `dmas` / `dma-names` | 建议 DMA 通道/请求与方向命名 | `<&dma1 DMA_REQ_xxx_TODO>`；名称 `"tx"/"rx"/"rx-1"` 等；F1 固定通道映射，F4/G4/H7 需另记 DMAMUX request |
| `current-speed` | 串口/总线速率（bps） | 整数 |
| `reg`（子节点） | ADC 通道号 / 总线器件地址 | ADC 通道整数（未知写 `<ADC_CH_xxx_TODO>`）；I2C 为 7bit 地址（不含 R/W 位） |
| `pin` / `af` | 引脚组物理位置与复用号 | 字符串 `"PA2"`；AF 未查到写 `<AF_<外设>_TODO>`，禁止写 `<0>` 猜测值 |
| `io-direction` | 信号方向 | `"in"/"out"/"analog"/"in-out"` |
| `polarity` | 有效电平/有效方式 | `"active-high"`、`"active-low"`、`"rising-edge"` 等 |
| `safe-state` | 复位/故障时输出应有的安全状态 | `"low"`、`"high"`、`"inactive"`、`"off"`；必须与故障动作方向一致 |
| `period-ns` / `frequency-hz` | 周期/频率规划值 | 整数；优先用人类可读注释并列算式 |

> 允许按需追加描述性属性（如 `supply = "3v3";`、`pull = "up";`），但同一语义全文件统一命名，避免同义词混用。

## 4. 命名规范

1. **实例 label**：沿用芯片实例名，便于对照 RM：`uart1`、`uart2`、`adc1`、`spi1`、`i2c1`、`can1`、`timers1`、`dma1`。
2. **引脚组 label**：`<实例>_<功能>_<引脚>`，如 `uart2_tx_pa2`、`adc_ch_vin_pa1`、`pwm_motor_pa8`；纯 GPIO 用 `pin_<功能>`，如 `pin_led_green_pc13`、`pin_brake_bkin_pb6`。
3. **通道 label**：`ch_<用途>`，如 `ch_vin`、`ch_temp`、`ch_is_a`；被 `app-bindings` 引用的节点必须有 label。
4. **外部器件 label**：`<用途>_<型号缩写>`，如 `temp_sensor_stts75`、`flash_w25q`。
5. **命名用小写字母与下划线**；不使用中文 label（中文放 `usage` 注释/属性）。

## 5. 完整台账范例（骨架，数值待核对）

### 5.1 全局节点（时钟 + 绑定视图）

```dts
/ {
    /* 系统时钟规划：来源与频率为规划值，最终以 RCC 配置与 RM 时钟树核对为准 */
    system-clock {
        usage = "系统时钟规划";
        source = "hse";                 /* hse=外部晶振 / hsi=内部RC / pll 由 HSE/HSI 倍频 */
        hse-frequency-hz = <0>;        /* TODO: 填板载晶振实际值（如 8MHz/24MHz），查原理图 */
        sysclk-hz = <0>;                /* TODO: 目标主频，对照 DS 最大额定值 */
        ahb-hz = <0>;
        apb1-hz = <0>;
        apb2-hz = <0>;
        note = "APB 预分频非 1 时定时器时钟为 PCLK 的 2 倍，PWM/定时参数复算时注意";
    };

    /* 业务用途绑定视图：用途 → 引用资源，保证台账内外一致、可检索 */
    app-bindings {
        usage = "业务用途到硬件资源的绑定视图";
        console      = <&uart1>;        /* 调试串口 */
        sensor_vin   = <&adc1 &adc1 ch_vin>;  /* 母线电压采集 */
        motor_pwm    = <&timers1 &pwm_motor>; /* 电机 PWM 输出 */
        key_user     = <&pin_key_pc13>;
    };
};
```

### 5.2 外设实例节点（示例见第 6 节）

### 5.3 引脚组定义区（台账内自包含）

```dts
/* 引脚组定义：被各实例 pinctrl-0 引用；查 RM“引脚复用定义表”核对 af */
&pinctrl {
    uart1_tx_pa9: uart1_tx_pa9 {
        pin = "PA9";
        af = <AF_USART1_TX_TODO>;        /* TODO: 查 RM 复用表，替换为真实 AF 号 */
        io-direction = "in-out";
        usage = "调试串口 TX";
    };
    uart1_rx_pa10: uart1_rx_pa10 {
        pin = "PA10";
        af = <AF_USART1_RX_TODO>;        /* TODO: 查 RM 复用表，替换为真实 AF 号 */
        io-direction = "in-out";
        usage = "调试串口 RX";
    };
    pin_led_green_pc13: pin_led_green_pc13 {
        pin = "PC13";
        io-direction = "out";
        polarity = "active-low";         /* 板载 LED 常见低有效，待核对原理图 */
        usage = "运行状态指示";
        note = "PC13 与 §6.7 按键模板冲突，同一工程只能二选一";
    };
};
```

> `AF_*_TODO` 不是真实 AF 号，必须在第 5.4 节待核对汇总登记；禁止保留 `<0>` 这类猜测值。
>
> `PC13` 在本节被 LED 占用，§6.7 按键模板也使用 `PC13`；同一工程只能二选一，另一功能改引脚后同步更新 label 与台账。

### 5.4 文件末尾：待核对汇总块

```dts
/* ============================================================================
 * 待核对汇总（销项后删除对应行）
 * [ ] uart1 TX/RX AF 号（AF_USART1_TX_TODO / AF_USART1_RX_TODO）—— 依据：RM 复用表
 * [ ] uart1 IRQn / 优先级              —— 依据：RM USART 章节 + 中断向量表
 * [ ] uart1 DMA 通道/请求号（F1 固定映射；F4/G4/H7 查 DMAMUX）—— 依据：RM DMA request mapping
 * [ ] adc1 ch_vin 通道号与 PA1 映射    —— 依据：DS 引脚-通道对照表
 * [ ] adc1 内部温度通道/分辨率/校准     —— 依据：对应系列 DS/RM
 * [ ] 晶振 HSE 实际频率                —— 依据：原理图晶振位号
 * [ ] 刹车脚 BKIN 极性、滤波与安全态     —— 依据：原理图 + RM 高级定时器刹车章节
 * [ ] 看门狗超时/窗口与喂狗点           —— 依据：RM 看门狗章节 + 任务周期
 * [ ] CAN 终端电阻与收发器供电          —— 依据：原理图 + 收发器手册
 * ============================================================================ */
```

## 6. 各类外设节点模板

### 6.1 串口（USART/UART/LPUART）

```dts
&uart1 {
    compatible = "stm32,usart";
    status = "okay";
    usage = "调试日志/上位机命令通道";
    current-speed = <115200>;
    data-bits = <8>;
    parity = "none";
    stop-bits = <1>;
    pinctrl-0 = <&uart1_tx_pa9 &uart1_rx_pa10>;
    pinctrl-names = "default";
    interrupts = <USART1_IRQn 3>;        /* TODO: 确认本系列 IRQn 符号（LPUART 等不同）；优先级 3 为规划值 */
    /* 大数据量时预留 DMA，不用则整段注释 */
    dmas = <&dma1 DMA_REQ_USART1_TX_TODO>, <&dma1 DMA_REQ_USART1_RX_TODO>;
    dma-names = "tx", "rx";
    note = "F1 为固定 DMA 通道映射；F4/G4/H7 经 DMAMUX，需分别核对控制器/通道与 request 编号。波特率误差需在 DS 允许范围内；fPCLK 以时钟规划实际值复算";
};
```

### 6.2 ADC（控制器 + 通道子节点）

```dts
&adc1 {
    compatible = "stm32,adc";
    status = "okay";
    usage = "模拟量周期采集";
    resolution = <12>;                  /* TODO: 16 位 ADC 系列按 DS 改为 16；默认 12 位需核对 */
    reference = "vdda";                 /* vdda / vrefint / 外部基准，待核对硬件设计 */
    pinctrl-0 = <&adc_ch_vin_pa1>;      /* TODO: 引脚组需在 §5.3 定义并核对通道映射 */
    pinctrl-names = "default";
    note = "ADC 实例数、通道编号、内部温度通道、分辨率与校准流程随 F1/F4/G4/H7 不同，逐项查对应 DS/RM";

    ch_vin: channel@TODO {              /* 单元地址仅占位，真实通道号见 reg */
        reg = <ADC_CH_VIN_TODO>;        /* TODO: 查 DS 引脚-通道表；不同系列编号不同 */
        usage = "母线电压采样（经分压网络）";
        pin = "PA1";                    /* TODO: 与通道号一起核对，禁止只凭引脚名填 reg */
        range-note = "分压比与运放增益决定换算系数，原理图核对";
        acquisition-time = "<TODO>";   /* 采样周期，源阻抗高需加大，查 DS 推荐值 */
    };
    ch_temp: channel@TEMP_TODO {        /* 内部温度传感器通道号随系列不同，必查 DS */
        reg = <ADC_CH_TEMP_TODO>;
        usage = "内部温度监测";
        reference = "vrefint";
    };
};
```

### 6.3 PWM / 定时器（含电机安全预留）

```dts
&timers1 {
    compatible = "stm32,pwm-timer";
    status = "okay";
    usage = "三相/单路电机或功率输出 PWM";
    pinctrl-0 = <&pwm_motor_pa8>;
    pinctrl-names = "default";
    frequency-hz = <20000>;             /* 规划频率，复算见 field-map.md Q2 */
    note = "高级定时器含互补输出/刹车/死区；安全参数待 RM 章节核对，禁止臆造";

    pwm_motor: channel@0 {
        reg = <0>;                      /* 通道号 */
        usage = "电机高边桥臂 PWM";
        complementary = "yes";          /* 是否互补输出，按拓扑填写 yes/no */
    };
};

/* 安全资源：刹车、使能与看门狗单独成节点 */
&safety {
    brake_bkin: brake_bkin_pb6 {
        compatible = "stm32,gpio";
        status = "okay";
        usage = "硬件刹车输入（急停/过流保护）";
        pin = "PB6";
        polarity = "active-high";       /* TODO: 对照驱动电路核对有效电平 */
        safe-state = "inactive";        /* 正常/复位默认未触发；触发即停 PWM + 断使能 */
        note = "建议硬件 RC 滤波；滤波参数与有效电平查原理图；故障时 PWM 必须立即关闭";
    };
    en_power: en_power_pa4 {
        compatible = "stm32,gpio";
        status = "okay";
        usage = "功率级使能输出，上电默认关断";
        pin = "PA4";
        polarity = "active-high";
        safe-state = "low";              /* 复位/故障默认安全电平 */
    };
    watchdog: watchdog {
        compatible = "stm32,watchdog";
        status = "disabled";             /* 启用改 "okay"；不启用时必须在待核对汇总写明 N/A 理由 */
        usage = "程序跑飞/主循环卡死复位保护";
        timeout-ms = <2000>;             /* TODO: 规划值，按最长任务周期与最坏阻塞核算 */
        window = "none";                 /* 窗口看门狗填窗口上下限；查 RM */
        refresh-context = "app_sched_thread 或独立低优先级任务；禁止在 ISR 喂狗";
        note = "启用后喂狗点必须覆盖所有长阻塞路径；调试阶段可 N/A，但需在待核对汇总说明";
    };
};
```

### 6.4 SPI

```dts
&spi1 {
    compatible = "stm32,spi";
    status = "okay";
    usage = "外部 Flash / 显示屏等串行总线";
    frequency-hz = <10000000>;          /* 不超从机上限，查从机手册 */
    cpol = <0>; cpha = <0>;             /* 模式 CPOL/CPHA，从机手册为准 */
    pinctrl-0 = <&spi1_sck_pa5 &spi1_miso_pa6 &spi1_mosi_pa7>;
    pinctrl-names = "default";

    flash_w25q: flash@0 {               /* reg=片选索引 */
        compatible = "ext,spi-device";
        status = "okay";
        usage = "外部 SPI Flash";
        cs-pin = "PB0";
        max-frequency-hz = <10000000>;
        note = "片选电平与上电时序查器件手册";
    };
};
```

### 6.5 I2C（总线 + 从设备子节点）

```dts
&i2c1 {
    compatible = "stm32,i2c";
    status = "okay";
    usage = "板上低速传感器总线";
    clock-frequency-hz = <400000>;     /* standard=100k / fast=400k，上拉与电容匹配核对 */
    pinctrl-0 = <&i2c1_scl_pb8 &i2c1_sda_pb9>;  /* TODO: 与 §6.6 CAN 模板的 PB8/PB9 冲突，二选一 */
    pinctrl-names = "default";
    note = "SCL/SDA 需外部上拉；PB8/PB9 与 §6.6 CAN 模板冲突，同一工程二选一；替代引脚（如 PB6/PB7）以 RM 复用表为准";

    temp_sensor: temp_sensor@48 {       /* reg=7bit 地址示例；按器件手册核对，不含 R/W 位 */
        compatible = "ext,i2c-device";
        status = "okay";
        usage = "环境温度传感器";
        chip = "<器件型号，待填>";
        reg = <0x48>;
    };
};
```

### 6.6 CAN/FDCAN

```dts
&fdcan1 {
    compatible = "stm32,can";
    status = "okay";
    usage = "工业总线通信";
    bus-speed-bps = <125000>;
    sample-point-permille = <875>;      /* 采样点 87.5%，总线上所有节点须一致 */
    pinctrl-0 = <&can_tx_pb9 &can_rx_pb8>;  /* TODO: 与 §6.5 I2C1 的 PB8/PB9 冲突，二选一；替代引脚查 RM */
    pinctrl-names = "default";
    termination = "TODO: 核对原理图（常见 120Ω，仅总线两端节点）";
    note = "终端电阻阻值/位置与收发器供电、共地均以原理图为准；120Ω 不是所有板卡通用值";
};
```

### 6.7 普通 GPIO / 预留资源

```dts
&general-gpios {
    pin_key_pc13: pin_key_pc13 {
        compatible = "stm32,gpio";
        status = "okay";
        usage = "用户按键（与 §5.3 LED 模板互斥）";
        pin = "PC13";
        io-direction = "in";
        pull = "up";
        trigger = "falling-edge";
        note = "PC13 与 §5.3 LED 模板冲突，同一工程二选一；按键改其他 GPIO 时同步改 label 与台账";
    };
    spare_pa15: spare_pa15 {
        compatible = "stm32,gpio";
        status = "disabled";
        usage = "预留扩展脚";
        pin = "PA15";
        note = "未用管脚建议按 DS 推荐配置（模拟输入/上拉）降低功耗与误触发";
    };
};
```

## 7. 注释与待核对标注规范

1. 每个被 `app-bindings` 引用的节点，`usage` 必须填写。
2. 数值不确定时**不填整数猜测值**：属性保留并加 `/* TODO: ... */`，或属性值写 `<TODO>` 文字注释占位。
3. 注释写清「依据来源」类型即可，不必复制原文：`/* 依据：DS 引脚-通道表 */`、`/* 依据：RM 定时器章节 */`、`/* 依据：原理图 U3 网络 */`。
4. 安全相关节点（刹车、使能、过流反馈）注释必须包含故障动作提示，不只写接线描述。
5. 台账演进：版本变更在文件头追加 `/* [V0.2] YYYY-MM-DD 变更摘要 */`；删除节点时注释保留一行历史说明而非直接抹除。
