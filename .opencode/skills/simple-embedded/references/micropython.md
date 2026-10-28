# MicroPython / ESP32

## 启用

配置 → 构建 → 工具链选 **MicroPython**。

构建页会隐藏构建位置、SSH、Build/Clean/Rebuild/Flash、并行任务和工具链路径，改为：

- 串口端口（可手动输入 `COM3` 或 `/dev/ttyUSB0`）
- 波特率（默认 115200）
- **自动检测**

配置保存在工程 `.simple-embedded/settings.json` 的 `serial.port` / `serial.baud`。

端口扫描优先识别 Espressif `0x303A`、CP210x、CH340/CH910x、FTDI、PL2303 等 USB 桥接芯片；不是每个 ESP32 板都能被识别为“ESP32”，必要时手动填写。

## 右侧操作

MicroPython 模式下右侧操作栏变为：

### 运行

- 把当前 `.py` 内容通过 raw REPL 上传为 `main.py`。
- 然后发送 Ctrl-D 软复位。
- 适合“改完当前脚本马上运行”。

### 上传

- 把当前文件上传到芯片。
- 如果本地文件来自芯片下载，会优先写回原芯片路径，即使本地保存成了“副本”。
- 当前文件名为 `main.py` 时写回 `main.py`。

### 固件

- 通过 esptool 烧录 MicroPython `.bin`。
- 需要系统安装 esptool。
- 配置：
  - `firmware.esptool`
  - `firmware.file`
  - `firmware.chip`
  - `firmware.address`
  - `firmware.erase`
  - `firmware.verify`
- 默认地址：
  - ESP32 / ESP32-S2：`0x1000`
  - ESP32-S3 / C3 / C6 / H2：`0x0`
- 整片擦除会清除板上文件系统。
- esptool 4.x 使用 `erase_flash` / `write_flash`；5.x 使用 `erase-flash` / `write-flash`。关闭写入校验时，5.x 会提示该开关被忽略。

### 复位

- 只发送 Ctrl-D 软复位，不重新上传。

## 芯片文件

MicroPython 模式的左栏把“工程信息”替换为 **芯片文件**：

- 展开后通过 raw REPL 执行 `os.ilistdir`，遍历根目录和最多两层子目录。
- 显示相对路径、目录标记和文件大小。
- 点击文件会读取芯片内容（上限 128 KB），保存到工程对应相对路径，并打开到编辑器。
- 同名文件自动加“副本”后缀。
- 下载时记录本地路径到芯片原路径的映射；之后 **上传** 会优先写回原芯片路径。
- **刷新** 固定在该分区底部；读取会暂停板上正在运行的程序。
- 切换工程或保存串口配置后列表失效并自动重读。

## 输出

运行、上传、固件、复位、串口错误和芯片文件读写都会写入底部输出面板的 **MicroPython 输出** 页。

## 限制

- 上传和读取上限 128 KB。
- 不支持交互式 REPL、目录上传、`boot.py` 特殊流程或自动下载固件。
- 上传期间串口被占用、开发板未运行 MicroPython 或超时会报错。
- 固件烧录需要用户自行准备合并后的 `.bin` 和 esptool。
- 端口自动检测是基于 USB VID / 产品名的启发式排序。
