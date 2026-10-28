# 系统安装、右键菜单和工程资源报告

## 安装到 Windows

入口：

```text
配置 → 其他 → 系统集成 → 安装到系统
```

默认安装位置：

```text
%LOCALAPPDATA%\Programs\SimpleEmbedded
```

安装会：

- 复制或硬链接当前 exe 和旁边的 `tools/`
- 跳过 `.compiler.tmp` / `.make.tmp`
- 创建开始菜单快捷方式
- 写入 Windows 应用列表卸载项
- 注册文件夹右键菜单：
  - 文件夹：**用 Simple embedded 打开**
  - 文件夹空白处：**在此处用 Simple embedded 打开**
- 注册 `.se-workspace` 关联

安装是 HKCU 用户级，不需要管理员权限，不写入 `Program Files`。Windows 11 默认精简菜单可能在 **显示更多选项** 中。

命令行：

```powershell
simple-embedded.exe --install-system
simple-embedded.exe --install-system --silent
simple-embedded.exe --uninstall-system
```

卸载：

- Windows 设置 → 应用 → 已安装的应用 → Simple embedded → 卸载
- 或 `simple-embedded.exe --uninstall-system`
- 会删除右键菜单、开始菜单快捷方式、`.se-workspace` 关联和安装目录；保留 `%APPDATA%\SimpleEmbedded` 用户设置和最近工程。

设置页的 **移除右键菜单与关联** 只移除系统集成，保留安装文件。

如果安装目录中的 exe 正在运行，覆盖会失败；先关闭已安装版本。卸载时运行中的 exe 通过 `%TEMP%` 下的延迟 PowerShell 脚本在退出后删除。

## 工程资源报告

入口：

```text
工程信息 → 资源报告 → 生成资源报告
工作区菜单 → 生成资源报告...
```

输出：

```text
<工程名>.resources.json
```

例如 `blink.resources.json`。后台分析，完成后 Toast 提示并刷新文件树。

### 顶层字段

- `schema` / `schema_version`
- `generated_at`
- `application`
- `project`
- `hardware`
- `software`
- `system`
- `notes`
- `warnings`

### application

- 软件名称、版本、作者、debug/release、可执行文件路径。

### project

- 工程名、根目录、工程配置文件、工作区文件
- 构建系统、工具链、构建位置（local/ssh）
- 目标名、芯片、编译器、优化选项、输出目录、产物
- 有效包含目录、库目录、宏
- 完整 `build` 配置（命令、SSH、烧录、串口、固件、内存、构建选项）
- `debug` 配置（程序、gdb、watch）

### hardware

- 芯片、CPU、FPU、内存布局、链接脚本
- Flash/RAM 起始、大小和构建占用率
- 固件 ELF：
  - 文件大小、修改时间
  - `data` / `bss`
  - 栈顶、栈大小、堆大小、堆类型
  - 最大 20 个全局变量
- SVD：
  - 外设数量、寄存器数量、来源
  - 每个外设的寄存器：名称、绝对地址、偏移、大小、访问、说明、位域数量
- `.ioc`：
  - 设备、组件（`Mcu.IP*`）
  - RCC 时钟键值
  - 引脚 `*.Signal` 和 `*.GPIO_Label`
- `used_components`

### software

- 语言
- 源码、头文件、Python、链接脚本、启动文件、库文件清单
- 文件数、总字节、源码/头文件行数
- 组件识别：FreeRTOS、CMSIS、STM32 HAL/LL、USB、FatFS、LittleFS、LwIP、mbedTLS、LVGL、u8g2、FreeModbus、SEGGER RTT、Arduino、ESP-IDF、Zephyr、RT-Thread、ChibiOS、ThreadX、MicroPython、Make/CMake/Cargo/Keil/CubeMX
- 符号统计
- 调用图统计（如果调用/引用索引已就绪）
- 支持包列表

### system

- OS、架构、debug/release
- 可执行文件
- 报告文件名
- 扫描文件数、字节数、源码/头文件行数、是否截断

### 限制

- 静态启发式分析，不是电路网表或 BOM。
- SVD 外设是芯片能力清单，不代表全部使用；实际使用的 CubeMX 组件和引脚来自 `.ioc`。
- 没有 `.ioc` 时引脚为空。
- 组件识别依赖文件路径关键字；外部器件、网络和实际连线无法发现。
- 扫描跳过 `build` / `Debug` / `Release` / `out` / `target` / `Objects` / `Listings` / `tools` / `dist` / `.git` / `.simple-embedded` / `node_modules`。
- 最多扫描 2 万文件、单文件 2 MB；源码/头文件列表各最多 500 个。
- 使用已保存文件和当前索引；未保存编辑不会进入清单。
- 同名报告会覆盖。
- 不替代原理图、BOM 或链接映射文件。

### 常见 warnings

- 未识别芯片型号
- 未解析 Flash / RAM 布局
- 未找到 SVD / 寄存器定义
- 未找到 `.ioc`，未分析引脚
- 未找到固件产物
- 调用/引用索引未就绪
- 文件清单超过扫描上限
- 支持包解析失败
