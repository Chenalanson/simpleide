# 入门篇


---

第一部分：入门篇

# 01 · egui 是什么

## 1.1 GUI 的两条路：Retained Mode vs Immediate Mode

在开始学习 egui 之前，我们需要先理解一个根本性的概念区分——GUI（图形用户界面）库的两种主要设计模式。

### 用"餐厅点餐"来类比

想象你去一家餐厅：

**Retained Mode（保留模式）**就像一个传统餐厅：

* 服务员来到你桌前，**记住**你点了什么菜（创建控件树）
* 你随时可以对服务员说"我的牛排要七分熟"（修改控件属性）
* 服务员**一直记得**你的所有要求（控件树常驻内存）
* 厨房根据服务员的记录准备菜品（渲染引擎绘制）

当你需要加菜或改单时，服务员只需要**更新某个记录**，不需要重新记录整张桌子。

**Immediate Mode（即时模式）**则像一个自助食堂：

* 每一轮取餐，你都要重新排队，**重新告诉**厨师你要什么（每帧重建 UI 描述）
* 厨师不会"记住"你上一轮点了什么（没有持久的控件树）
* 但好处是——**绝对不会出错**：你当前拿到的就是你当前说的，不存在"服务员记错了"的情况

### 技术对比

|  |  |  |
| --- | --- | --- |
| 特性 | Retained Mode | Immediate Mode |

|  |  |  |
| --- | --- | --- |
| 控件表示 | 控件树常驻内存，可随时修改 | 每帧重新生成 UI 描述，用完即弃 |
| 状态管理 | 控件维护自己的状态（复杂同步） | 状态在你自己的变量里（完全可控） |
| 数据绑定 | 需要专门机制（MVVM、观察者模式） | 直接读写 Rust 变量（零开销抽象） |
| 内存占用 | 控件树 + 状态的开销 | 每帧仅临时的 UI 描述 |
| CPU 开销 | 仅在变化时更新 | 每帧重建（但通常极快） |
| 典型代表 | Qt、GTK、Win32、Flutter、SwiftUI | Dear ImGui（C++）、egui（Rust） |
| 常用场景 | 消费级应用、移动 App | 开发工具、游戏编辑器、调试 UI |

### 为什么游戏开发工具偏爱 Immediate Mode？

游戏行业是 Immediate Mode 的发源地。原因是：

- **游戏中已经有渲染循环**——Immediate Mode 自然地嵌入帧循环
- **状态是"外部的"**——游戏数据（血量、分数、位置）已经存在于你的游戏状态中，不需要再"绑定"到 UI 控件
- **开发效率极高**——改一行代码就能看到 UI 变化，不需要处理"创建控件 → 设置属性 → 注册回调"的繁琐流程
- **不存在状态同步 bug**——不会出现"控件显示的数值和实际数值不一致"的问题

### egui 的代码长什么样？

先看一段"Hello World"级别的代码，建立直观印象：

```rust
// 在 ui() 方法中
if ui.button("点击我").clicked() {
    self.count += 1;  // 直接修改你的数据
}
ui.label(format!("点击次数: {}", self.count));
```

看到关键点了吗？**按钮和标签不是"创建"出来的持久对象**——它们只是在当帧被"描述"了一下。下一帧，同样的代码会再次运行，根据最新的 `self.count` 生成新的 UI 描述。
这在 Retained Mode 中大约等价于（以 Qt 风格为例）：

```rust
// Qt 风格的伪代码（注意：这不是 egui 的写法！仅作对比）
auto* button = new QPushButton("点击我", this);
auto* label = new QLabel("点击次数: 0", this);
connect(button, &QPushButton::clicked, [this, label]() {
    count++;
    label->setText(QString("点击次数: %1").arg(count));
});
```

差异一目了然：Retained Mode 需要创建对象、连接信号、手动更新——Immediate Mode 只需要**描述当前状态**。

---

## 1.2 egui 的定位与设计哲学

### egui 是什么？

[egui](https://github.com/emilk/egui)（发音 "e-gooey" 或 "e-gui"）是一个**纯 Rust 编写的跨平台 Immediate Mode GUI 库**。由瑞典开发者 [Emil Ernerfeldt](https://github.com/emilk)（现就职于 [Rerun](https://rerun.io)）创建并维护。

### egui 解决什么问题？

egui 的目标是：**让你用最少的代码，写出可用的图形界面。**

* 不需要编写 UI 描述文件（XML/JSON/DSL）
* 不需要学习数据绑定框架
* 不需要理解复杂的信号/槽机制
* 不需要手动管理控件生命周期

你只需要在每帧"说出"你想要的 UI，egui 负责渲染它。

### 设计取舍

egui 做了以下核心取舍：

|  |  |
| --- | --- |
| 取舍 | 说明 |

|  |  |
| --- | --- |
| **CPU 换开发体验** | 每帧重新计算 UI 布局，换来零状态同步复杂度 |
| **简单胜过花哨** | 不做复杂的动画引擎，但内置了够用的动画 API |
| **默认性能足够** | 对于 99% 的应用，每帧重建 UI 的 CPU 开销可忽略 |
| **跨平台但不"原生"** | egui 自己绘制一切，外观统一但不使用操作系统的原生控件 |
| **纯 Rust 生态** | 不依赖 C++ 库，编译简单，跨平台无痛 |

### 适用场景

|  |  |
| --- | --- |
| ✅ 非常适合 | ❌ 不太适合 |

|  |  |
| --- | --- |
| 桌面工具/实用程序 | 移动端应用（电池敏感的持续重绘） |
| 游戏引擎编辑器 | 60fps 游戏内 UI（集成复杂） |
| 数据仪表盘/监控面板 | 需要复杂动画的应用 |
| 开发调试 UI | 需要操作系统原生外观的应用 |
| 快速原型和概念验证 | 文字处理/排版密集型应用 |

> **一句话总结**：如果你的目标是"快速做出一个有用的桌面工具"，egui 是目前 Rust 生态中最接近"开箱即用"的选择。

---

## 1.3 egui 生态全景图

egui 不仅仅是一个 crate——它是**一组紧密协作的 crate 家族**。理解每个 crate 的职责，对于后续深入学习至关重要。

```rust
emath  (纯数学：Vec2, Pos2, Rect, Rot2...)
  └─ ecolor  (颜色：Color32, Rgba, Hsva...)
      └─ epaint  (绘制原语：Shape, Tessellator, Fonts, Texture...)
          └─ egui  (UI 构建：Widget, Context, Ui, Response...)
              ├─ eframe  (桌面框架：窗口创建、事件循环、渲染)
              │   ├─ egui-winit  (winit 事件 → egui 输入的桥接)
              │   ├─ egui-wgpu  (wgpu 渲染后端)
              │   └─ egui_glow  (OpenGL/WebGL 渲染后端)
              ├─ egui_extras  (额外 Widget：DatePicker, Table, RetainedImage)
              ├─ egui_plot  (2D 图表：折线图、柱状图、散点图)
              └─ egui_phosphor  (图标字体集成)
```

### 各 crate 职责速查

|  |  |  |
| --- | --- | --- |
| Crate | 一句话职责 | 你是会直接使用的 |

|  |  |  |
| --- | --- | --- |
| **`emath`** | 2D 数学库（Vec2, Pos2, Rect, Rot2） | 间接使用（egui 依赖它） |
| **`ecolor`** | 颜色类型（Color32, Rgba, Hsva） | 间接使用 |
| **`epaint`** | 绘制原语（Shape, Tessellator, Fonts） | 间接使用 |
| **`egui`** | UI 核心库（Widget, Context, Ui） | ✅ **直接使用** |
| **`eframe`** | 桌面框架（窗口、事件循环） | ✅ **本教程默认选择** |
| **`egui-winit`** | 将 winit 事件转换为 egui 输入 | 仅手动集成时使用（第 41 章） |
| **`egui-wgpu`** | wgpu 渲染后端 | 仅手动集成时使用（第 41 章） |
| **`egui_glow`** | OpenGL/WebGL 渲染后端 | 仅手动集成时使用 |
| **`egui_extras`** | 额外 Widget（表格、日期选择器） | ✅ 进阶章节使用（第 27 章） |
| **`egui_plot`** | 2D 图表绘制 | ✅ 进阶章节使用（第 28 章） |
| **`egui_phosphor`** | 图标字体 | ✅ 进阶章节使用（第 29 章） |

### 你只需要记住

作为初学者，你只需要往 `Cargo.toml` 中添加一行：

```rust
eframe = "0.34"
```

这会自动引入 `egui`（核心）、`epaint`（绘制）、`eframe`（框架）以及 `wgpu`（默认渲染后端）。其他 crate 在需要时逐个添加即可。

---

## 1.4 环境准备

在开始编写代码之前，确认你的 Rust 工具链就绪：

```rust
# 检查 Rust 版本（需要 >= 1.96）
rustc --version

# 输出应类似: rustc 1.96.0 (xxxxxxx 2026-xx-xx)

# 检查 cargo

cargo --version

# 输出应类似: cargo 1.96.0 (xxxxxxx 2026-xx-xx)
```

如果你还没有安装 Rust，请访问 <https://rustup.rs> 安装。
本教程使用 **Rust 2024 Edition**，egui 0.34 也默认使用此 edition。如果你运行的是 Rust 1.96+，创建新项目时会自动选择 2024 edition。

### 关于操作系统

本教程在 Windows 11 上进行开发和测试，但 egui 完全跨平台。所有代码在 macOS 和 Linux 上同样可以编译运行——如果有平台特定的注意事项，我们会在相应位置标注。

---

## 本章小结

* **Immediate Mode** = 每帧描述 UI，用完即弃；**Retained Mode** = 控件树常驻内存
* egui 是一个纯 Rust 的 Immediate Mode GUI 库，以简单和开发效率为核心
* egui 最适合工具/编辑器/仪表盘类应用
* egui 生态由多个 crate 组成，你只需要从 `eframe` 开始
* `eframe` 自动引入 `egui` + `epaint`，默认使用 wgpu 渲染

**下一章**：我们将从零创建第一个 egui 应用——一个计数器。你将亲手体验 immediate mode 的代码风格，并看到一个真正可运行的窗口。

---

> 📁 本章无代码示例（纯概念章）

> 📖 延伸阅读：[Dear ImGui 官方介绍](https://github.com/ocornut/imgui)（egui 的精神祖先）

---

第一部分：入门篇

﻿# 02 · 实战入门：构建一个计数器

本章模仿《The Rust Programming Language》第 2 章（猜数字游戏）的做法——**先动手写出一个可运行的程序，建立感性认识，再在后续章节中逐一理解背后的原理。**

在本章中，你将：

* 用 `cargo new` 创建一个新项目
* 添加 `eframe` 依赖
* 写出第一个窗口
* 实现一个可点击的计数器

---

## 2.1 创建项目

打开终端，进入你希望存放项目的目录，执行：

```rust
cargo new counter
cd counter
```

这会创建一个名为 `counter` 的 Rust 项目，包含以下文件：

```rust
counter/
├── Cargo.toml
└── src/
    └── main.rs
```

现在编辑 `Cargo.toml`，添加 eframe 依赖：

```rust
[package]
name = "counter"
version = "0.1.0"
edition = "2024"  # egui 0.34 使用 Rust 2024 edition

[dependencies]
eframe = "0.34"
```

> **说明**：`eframe = "0.34"` 会自动引入 `egui`（UI 核心）和 `wgpu`（默认渲染后端）。你不需要单独列出 `egui`。

保存后，运行以下命令下载依赖：

```rust
cargo build
```

首次编译可能需要几分钟——egui 及其依赖项不少。后续编译会利用缓存，快得多。

---

## 2.2 定义 App 结构体

用你喜欢的编辑器打开 `src/main.rs`，清空内容，写入以下代码：

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    // 创建运行选项，使用默认配置
    let native_options = eframe::NativeOptions::default();

    // 启动应用
    eframe::run_native(
        "我的计数器",                        // 窗口标题
        native_options,                       // 运行选项
        Box::new(|_cc| Ok(Box::new(MyApp::default()))), // 应用创建函数
    )
}

// 应用状态
struct MyApp {
    count: i32, // 计数器当前值
}

impl Default for MyApp {
    fn default() -> Self {
        Self { count: 0 }
    }
}
```

### 逐行解释

**引入 egui**：

```rust
use eframe::egui;
```

这行引入 `egui` 模块——所有的 UI 构建 API 都在其中。
**`main` 函数**：

```rust
fn main() -> eframe::Result<()> {
```

注意返回类型是 `eframe::Result<()>`——这不是标准库的 `Result`，而是 eframe 定义的别名。
**`eframe::run_native`** 是 eframe 的核心启动函数，接受三个参数：

* `"我的计数器"` — 窗口标题（字符串）
* `NativeOptions::default()` — 运行配置（窗口大小、渲染器、vsync 等，这里全用默认值）
* `Box::new(|_cc| Ok(Box::new(MyApp::default())))` — 一个闭包，告诉 eframe **如何创建你的应用实例**

闭包中的 `_cc` 是 `CreationContext`，包含窗口创建时的上下文信息（如 Storage）。我们现在用不到它，所以用 `_` 前缀忽略。
**`MyApp` 结构体** 是你的应用状态的容器——egui 推荐的模式是把所有应用数据放在这个结构体里。目前只有一个 `count: i32`。
**`Default` trait** 是必需的——eframe 需要在启动时创建应用的默认实例。

---

## 2.3 实现 `eframe::App` trait

接下来就是 egui 的核心了。我们需要为 `MyApp` 实现 `eframe::App` trait。这个 trait 包含两个重要的方法：

* `logic()` — 帧逻辑（数据处理、输入检测、后台任务）
* `ui()` — UI 渲染（描述当前帧的界面）

在 `impl Default for MyApp` 块之后，添加：

```rust
impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 帧逻辑：这里可以放数据处理代码
        // 目前不需要，留空
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // UI 渲染：描述当前帧的界面
        // 直接使用 ui 即可——不需要 CentralPanel
        ui.label(format!("点击次数: {}", self.count));

        if ui.button("+1").clicked() {
            self.count += 1;
        }
    }
}
```

### 逐行解释

**`impl eframe::App for MyApp`** — 为我们的应用结构体实现 App trait。
**`fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame)`**：

* `&mut self` — 可修改应用状态（如 count）
* `&egui::Context` — egui 的中央枢纽（输入、内存、样式）
* `&mut eframe::Frame` — 桌面框架提供的帧信息
* 这个方法叫 `logic()`——在 egui 0.34 之前，这个方法和 `ui()` 是合并成一个 `update()` 的。拆分后代码更清晰。

**`fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)`**：

* 这才是"画 UI"的地方
* 0.34 起，`ui` 直接由 eframe 传入并覆盖整个窗口——你无需手动创建面板
* `ui.label(...)` — 显示一行文本（标签）
* `ui.button("+1")` — 显示一个按钮
* `.clicked()` — 返回 `true` 表示**本帧按钮刚被点击**

> ⚠️ **Immediate Mode 的关键思维**：
>
> `ui.button("+1")` 不是在"创建"按钮——它只是在**本帧**描述"这里有个按钮"。如果用户点击了它，`.clicked()` 返回 `true`。下一帧，同样的代码再次运行，按钮再次被描述——**不保留任何持久对象**。

---

## 2.4 完整代码

将以上代码合并，得到完整的 `src/main.rs`：

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "我的计数器",
        native_options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}

struct MyApp {
    count: i32,
}

impl Default for MyApp {
    fn default() -> Self {
        Self { count: 0 }
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 留空：计数器逻辑足够简单，不需要额外的帧逻辑
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label(format!("点击次数: {}", self.count));

        if ui.button("+1").clicked() {
            self.count += 1;
        }
    }
}
```

---

## 2.5 运行与效果

在终端中执行：

```rust
cargo run
```

你将看到一个窗口：

```rust
┌──────────────────────────┐
│  我的计数器            _ □ × │
│                          │
│  点击次数: 0             │
│  ┌─────┐                 │
│  │ +1  │                 │
│  └─────┘                 │
│                          │
└──────────────────────────┘
```

点击 "+1" 按钮，数字会递增。每次点击，按钮短暂显示蓝色（按下状态），数字加 1。

### 中文界面版本

如果你想把界面文字改成中文，只需要修改字符串：

```rust
ui.label(format!("点击次数: {}", self.count));
if ui.button("递增").clicked() {
//        ^^^^ 改这里
```

> ⚠️ 按钮上的中文可能显示为方框（□□□）或空白——这是因为 egui 默认字体不包含中文字形。我们在第 22 章会详细讲解如何配置中文字体。目前可以先用英文，或者接受暂时的方框——第 22 章会完美解决。

---

## 2.6 直觉与回顾

在进入下一章之前，花一点时间感受这段代码中的关键模式：

### 🤔 问题 1：按钮为什么在 `if` 判断里？

```rust
if ui.button("+1").clicked() {
    self.count += 1;
}
```

这不是"创建按钮，给它注册回调"。而是"本帧画一个按钮，如果它刚被点击了，就执行 {} 中的代码"。按钮和点击检查是同一个表达式的两部分。

### 🤔 问题 2：为什么 `count` 的值没有丢失？

`MyApp` 结构体由 eframe **持有**在内存中。它不会被每帧销毁重建。每次调用 `ui()` 时，`self` 指向的是同一个 `MyApp` 实例。所以 `count` 的值自然保留。
那么，Widget（如 slider、text edit）的值是怎么跨帧保留的？它们没有存在 `MyApp` 中——答案是 egui 内部的 `Memory` 存储。第 18 章会深入讲解。

### 🤔 问题 3：`logic()` 和 `ui()` 有什么区别？

当前计数器过于简单，`logic()` 是空的。在实际应用中：

* **`logic()`** — 数据处理、快捷键检测、网络请求、定时器
* **`ui()`** — 纯粹的 UI 描述

这种分离是 egui 0.34 的重要改进。第 19 章会详细讲解数据流的最佳实践。

---

## 本章小结

* 在 `Cargo.toml` 中添加 `eframe = "0.34"` 即可开始 egui 开发
* 应用状态存储在 `MyApp` 结构体中，需要实现 `Default` 和 `eframe::App`
* `eframe::App` 有两个核心方法：`logic()`（帧逻辑）和 `ui()`（UI 渲染）
* `ui.button("...").clicked()` 在按钮被点击时返回 `true`
* 计数器值保留在 `MyApp.count` 中——因为 `MyApp` 由 eframe 持有

**下一章**：深入了解 `App` trait、帧循环的工作流程，以及 `Context` 和 `Frame` 的完整 API。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "我的计数器",
        native_options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}

/// 应用状态：存储所有需要在帧之间保留的数据
struct MyApp {
    count: i32,
}

impl Default for MyApp {
    fn default() -> Self {
        Self { count: 0 }
    }
}

impl eframe::App for MyApp {
    /// 帧逻辑：数据处理、快捷键检测、后台任务
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 计数器逻辑足够简单，这里不需要额外处理
    }

    /// UI 渲染：每帧描述界面
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("计数器示例");

        ui.separator();

        // 显示当前计数
        ui.label(format!("点击次数: {}", self.count));

        // 按钮：检测点击并修改状态
        if ui.button("+1").clicked() {
            self.count += 1;
        }

        // 添加一个递减按钮
        if ui.button("-1").clicked() {
            self.count -= 1;
        }

        // 添加一个重置按钮
        if ui.button("重置").clicked() {
            self.count = 0;
        }
    }
}
```

---

第一部分：入门篇

﻿# 03 · 理解 App trait 与帧循环

第 2 章我们快速实现了一个计数器，但只知其然。本章将深入讲解 **egui 如何运行**——帧循环的结构、`App` trait 的方法职责、以及 `Context` 和 `Frame` 的角色。

---

## 3.1 帧循环全景图

egui 应用的生命周期可以用一个词概括：**帧（Frame）**。

每一帧，egui 执行以下步骤：

```rust
        ┌─────────────┐
        │  收集输入事件 │  ← 键盘、鼠标、触控、IME...
        └──────┬──────┘
               ▼
        ┌─────────────┐
        │  logic()     │  ← 数据处理、快捷键、后台任务（你的代码）
        └──────┬──────┘
               ▼
        ┌─────────────┐
        │  ui()        │  ← 构建 UI 描述（你的代码）
        └──────┬──────┘
               ▼
        ┌─────────────┐
        │  布局计算     │  ← egui 自动完成
        └──────┬──────┘
               ▼
        ┌─────────────┐
        │  Tessellation│  ← Shapes → 三角形网格（epaint）
        └──────┬──────┘
               ▼
        ┌─────────────┐
        │  GPU 渲染    │  ← wgpu/glow 提交绘制命令
        └──────┬──────┘
               │
        等待下一帧 ← （有输入变化或 request_repaint 时继续）
```

> **关键事实**：egui **只在有输入变化时才绘制新帧**——这是性能优化的核心。如果你不碰鼠标键盘，界面不会重绘。如果你需要持续的动画，需要调用 `ctx.request_repaint()`（第 32 章）。

---

## 3.2 `logic()` 详解

`logic()` 是 egui 0.34 从旧 `update()` 拆分出来的**逻辑专用方法**：

```rust
fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // 你的逻辑代码
}
```

### 职责范围

|  |  |
| --- | --- |
| ✅ 适合放 logic() | ❌ 不适合放 logic() |

|  |  |
| --- | --- |
| 读取键盘输入并更新状态 | 调用 `ui.button()` 等 UI 构建 |
| 异步任务结果轮询 | 修改 Widget 的外观 |
| 网络请求 | 创建/关闭窗口 |
| 定时器、计时器逻辑 | 任何依赖 `ui()` 方法的事情 |
| 数据计算与转换 |

### 为什么需要分离？

在 0.33 及之前，所有代码都在 `update(ctx, frame)` 中——逻辑和 UI 混合在一起，导致：

* 代码难以阅读：数据处理和 UI 渲染交织
* 测试困难：测试数据逻辑时必须模拟整个 UI 上下文
* 职责不清：容易在 UI 代码中夹带副作用

0.34 的分离解决了这些问题——第 19 章会深入讲解数据流模式。

### 示例：在 logic() 中处理快捷键

```rust
fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // 检查是否按下了 Ctrl+Z（撤销）
    ctx.input(|i| {
        if i.modifiers.ctrl && i.key_pressed(egui::Key::Z) {
            // 处理撤销逻辑
            self.undo();
        }
    });
}
```

`ctx.input(|i| {...})` 是一个获取只读输入的惯用方式——传入闭包，闭包接收 `&InputState`。

---

## 3.3 `ui()` 详解

`ui()` 是**描述界面**的地方：

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // 你的 UI 代码
}
```

### 核心原则

- **UI 描述是每帧重新建立的**——`ui.button()` 不会"创建"持久按钮，只是在当帧"说"这里有个按钮
- **`ui()` 中不应有副作用**——除了通过 `Response`（如 `.clicked()`）获取用户交互的结果来修改 `self`
- **通过 `ui.ctx()` 访问 `Context`**——输入、样式、内存、字体等都通过 `ui.ctx()` 获取

### ui() 获取用户交互的正确方式

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // ✅ 正确：通过 Response 获取交互结果
    if ui.button("确认").clicked() {
        self.confirmed = true;
    }

    // ✅ 正确：通过 changed() 检测输入变化
    let response = ui.add(egui::TextEdit::singleline(&mut self.name));
    if response.changed() {
        self.name_changed = true;
    }
}
```

```rust
// ❌ 错误：在 ui() 中直接做网络请求
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // 这会在每帧都发起网络请求！应该放在 logic() 中
    // let data = reqwest::blocking::get("https://...").unwrap();
}
```

---

## 3.4 `Frame` 与渲染控制

`Frame` 是 eframe 提供的**桌面框架上下文**，通过 `logic()` 的第二个参数访问：

```rust
fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
    // frame 提供以下能力：
}
```

### Frame 的主要方法

|  |  |
| --- | --- |
| 方法 | 作用 |

|  |  |
| --- | --- |
| `frame.info()` | 获取窗口信息（native\_pixels\_per\_point 等） |
| `frame.storage()` | 获取持久化存储的引用（第 20 章） |
| `frame.request_repaint()` | 请求下一帧重绘（同 `ctx.request_repaint()`） |
| `frame.wgpu_render_state()` | 获取 wgpu 渲染状态（高级用法） |
| `frame.glow_render_state()` | 获取 glow 渲染状态（若使用 glow 后端） |
| `frame.set_window_title("...")` | 动态修改窗口标题 |

### 示例：在标题栏显示帧信息

```rust
fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
    // 在标题栏显示当前帧的时间戳
    frame.set_window_title(
        format!("我的应用 - 时间: {:.1}", ctx.input(|i| i.time))
    );
}
```

---

## 3.5 `Context` 是什么

`Context` 是 egui 的**中央枢纽**——几乎所有操作都通过它进行。

```rust
// Context 的核心访问方法
ctx.input(|i| { ... });          // 读取输入状态（键盘、鼠标、IME）
ctx.input_mut(|i| { ... });      // 可变访问（消费输入事件）
ctx.style();                      // 读取当前样式
ctx.global_style_mut(|style| { ... });   // 修改样式
ctx.memory();                     // 读取 Memory（持久化状态）
ctx.memory_mut(|mem| { ... });    // 修改 Memory
ctx.request_repaint();            // 请求下一帧重绘
ctx.request_repaint_after(dur);   // 延迟请求重绘
ctx.output_mut(|o| { ... });      // 写入系统命令（复制、打开 URL 等）
ctx.fonts(|f| { ... });           // 访问字体系统
ctx.content_rect();                // 获取屏幕空间矩形
```

### Context 是"帧级临时对象"

Context 的生命周期是一帧。你不能将它存储到 `MyApp` 中跨帧使用。每次 `logic()` 和 `ui()` 调用都传入一个新的 Context。

### 对比：Context vs Frame vs Ui

|  |  |  |
| --- | --- | --- |
| 对象 | 来源 | 职责 |

|  |  |  |
| --- | --- | --- |
| `Context` | `fn logic(ctx)` / `ui.ctx()` | egui 核心功能（输入、样式、内存、输出） |
| `Frame` | `fn logic(ctx, frame)` / `fn ui(ui, frame)` | eframe 特定功能（窗口管理、持久化存储） |
| `Ui` | `fn ui(ui, frame)` — 直接由 eframe 传入 | 布局和 Widget 构建 |

> 简单记忆：`Context` 是 egui 的，"`Frame`" 是 eframe 的，"`Ui`" 是布局的。在 `ui()` 中，`ui.ctx()` 获取 Context。

---

## 3.6 验证你的理解

在你的计数器项目中添加帧计数，验证理解：

```rust
struct MyApp {
    count: i32,
    frame_count: u64,      // 新增：帧计数器
    last_input_time: f64,   // 新增：上次输入时间
}

fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // 每帧递增帧计数（注意：只在有输入变化时重绘，所以这不是"每秒帧数"）
    self.frame_count += 1;

    // 记录最新的输入时间
    self.last_input_time = ctx.input(|i| i.time);
}

fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // 获取 Context 信息（必须在借用 ui 之前）
    let screen_size = ui.ctx().content_rect().size();

    ui.heading("帧循环验证");

    ui.label(format!("计数器: {}", self.count));
    if ui.button("+1").clicked() {
        self.count += 1;
    }

    ui.separator();

    // 显示帧信息
    ui.label(format!("帧序号: {}", self.frame_count));
    ui.label(format!("当前时间: {:.2}", self.last_input_time));
    ui.label(format!("屏幕大小: {:?}", screen_size));
}
```

你可以观察到：只有当你点击按钮或移动鼠标时，帧序号才增长——这验证了"无输入则无重绘"的行为。

---

## 本章小结

* 每帧执行流程：输入收集 → `logic()` → `ui()` → 布局 → Tessellation → GPU 渲染
* `logic()` — 数据和后台逻辑；`ui()` — 纯粹的 UI 描述
* `Frame` — eframe 的桌面框架能力（窗口管理、存储）
* `Context` — egui 的中央枢纽（输入、样式、内存、输出）
* egui 默认只在输入变化时重绘——这是性能优化的基础

**下一章**：开发工作流与项目结构——如何组织你的 egui 项目代码，以及提升开发效率的工具和配置。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "帧循环验证",
        native_options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}

/// 应用状态
struct MyApp {
    count: i32,
    frame_count: u64,      // 帧序号：证明"无输入不重绘"
    last_input_time: f64,   // egui 内部时钟（秒）
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            count: 0,
            frame_count: 0,
            last_input_time: 0.0,
        }
    }
}

impl eframe::App for MyApp {
    /// 帧逻辑：跟踪帧计数和时间
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        self.last_input_time = ctx.input(|i| i.time);
    }

    /// UI 渲染：显示帧信息和计数器
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 从 Context 获取信息（在 ui 被借用之前）
        let (width, height) = {
            let ctx = ui.ctx();
            let r = ctx.content_rect();
            (r.width(), r.height())
        };

        ui.heading("帧循环验证");
        ui.separator();

        // --- 交互区 ---
        ui.horizontal(|ui| {
            if ui.button("+1").clicked() {
                self.count += 1;
            }
            if ui.button("-1").clicked() {
                self.count -= 1;
            }
            if ui.button("重置").clicked() {
                self.count = 0;
            }
        });
        ui.label(format!("计数器当前值: {}", self.count));
        ui.separator();

        // --- 帧信息区 ---
        ui.label(format!("帧序号: {}", self.frame_count));
        ui.label(format!("egui 时钟: {:.3} 秒", self.last_input_time));
        ui.label(format!("窗口尺寸: {:.0} x {:.0}", width, height));
        ui.separator();

        // --- 说明区 ---
        ui.label("提示：仅在移动鼠标或点击按钮时帧序号才增长");
        ui.label("这证明 egui 默认只在有输入事件时才重绘");
    }
}
```

---

第一部分：入门篇

﻿# 04 · 开发工作流与项目结构

前两章我们写了简单的计数器应用，所有代码都在 `main.rs` 中。随着项目增长，你需要组织代码结构、优化编译配置、使用效率工具。本章覆盖 egui 开发的实践知识。

---

## 4.1 推荐的 Cargo.toml 配置

### 最小配置

```rust
[package]
name = "my-egui-app"
version = "0.1.0"
edition = "2024"

[dependencies]
eframe = "0.34"
```

### 选择渲染后端

egui 0.34 起，**wgpu 是默认渲染器**。如果你想使用 OpenGL（如兼容老显卡或特定平台需求）：

```rust
# 使用 OpenGL (glow) 后端
eframe = { version = "0.34", default-features = false, features = ["glow"] }
```

|  |  |  |  |
| --- | --- | --- | --- |
| 后端 | Feature | 底层 | 适合 |

|  |  |  |  |
| --- | --- | --- | --- |
| wgpu（默认） | `default-features` | Vulkan / Metal / DX12 / WebGPU | 现代平台 |
| glow | `features = ["glow"]` | OpenGL 3.3+ / WebGL | 兼容性优先 |

> 对初学者来说，保持默认（wgpu）即可。只有在默认后端出问题时才考虑切换。

### Release 构建优化

在 `Cargo.toml` 末尾添加 profile 配置：

```rust
[profile.release]
opt-level = 3      # 最高优化级别
lto = "fat"        # 链接时优化（略微增加编译时间）
codegen-units = 1  # 单代码生成单元（更好的优化，略慢编译）
strip = true       # 移除调试符号（减小文件体积）
```

这能将 `.exe` 文件从 ~20MB 减小到 ~5MB，启动速度也更快。

---

## 4.2 典型项目目录结构

随着功能增多，单文件 `main.rs` 会变得难以维护。推荐的演进路径是：

### 小型项目（< 500 行）

```rust
my-app/
├── Cargo.toml
└── src/
    └── main.rs          # 包含 MyApp + logic + ui
```

### 中型项目（500 ~ 2000 行）

```rust
my-app/
├── Cargo.toml
└── src/
    ├── main.rs          # main 函数 + NativeOptions 配置
    └── app.rs           # MyApp 结构体 + App trait 实现
```

**`src/main.rs`**：

```rust
mod app;

use app::MyApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "我的应用",
        native_options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}
```

**`src/app.rs`**：

```rust
use eframe::egui;

pub struct MyApp {
    // 你的状态字段...
}

impl Default for MyApp {
    fn default() -> Self { / ... / }
}

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ...
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ...
    }
}
```

### 大型项目（> 2000 行）

```rust
my-app/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── app.rs           # MyApp 结构体
    ├── panels/          # 按界面区域拆分
    │   ├── mod.rs
    │   ├── sidebar.rs
    │   ├── content.rs
    │   └── status_bar.rs
    ├── widgets/         # 自定义 Widget
    │   ├── mod.rs
    │   └── color_picker.rs
    └── utils/           # 工具函数
        ├── mod.rs
        └── theme.rs
```

---

## 4.3 开发效率工具

### cargo watch 自动重编译

安装 `cargo-watch`：

```rust
cargo install cargo-watch
```

然后替代 `cargo run` 使用：

```rust
cargo watch -x run
```

每次保存文件，自动重新编译和运行。对于 UI 开发——频繁小修改、立即看效果——非常实用。

### 环境变量与日志

egui 使用 `log` crate 输出内部日志。启动应用时设置环境变量：

```rust
# PowerShell (Windows)
$env:RUST_LOG = "debug"
cargo run

# bash (Linux/macOS)

RUST_LOG=debug cargo run
```

egui 会输出纹理加载、字体图集更新、帧时间等调试信息。
如果你使用 `log` crate 在自己的代码中打印日志，配合 `env_logger`：

```rust
[dependencies]
eframe = "0.34"
log = "0.4"
env_logger = "0.11"
```

```rust
fn main() {
    env_logger::init(); // 初始化日志系统
    log::info!("应用启动");
    // ...
}
```

### eframe::NativeOptions 调试开关

```rust
let native_options = eframe::NativeOptions {
    follow_system_theme: false,  // 不跟随系统主题（方便调试自定义主题）
    vsync: true,                  // 垂直同步（默认 true，一般保留）
    renderer: eframe::Renderer::Wgpu, // 显式指定渲染器
    ..Default::default()
};
```

|  |  |
| --- | --- |
| 字段 | 说明 |

|  |  |
| --- | --- |
| `follow_system_theme` | 是否跟随系统明/暗主题 |
| `vsync` | 垂直同步（减少撕裂，但可能有轻微延迟） |
| `renderer` | `Renderer::Wgpu` / `Renderer::Glow` |
| `viewport` | 自定义窗口大小、位置等（详见第 35 章） |
| `depth_buffer` | 深度缓冲位数（3D 内容需要） |
| `multisampling` | MSAA 抗锯齿采样数 |

---

## 4.4 跨平台注意事项

egui 的代码在 Windows、macOS、Linux 上**几乎完全一致**。以下是你需要知道的主要区别：

### 路径差异

```rust
// ❌ 不要硬编码路径分隔符
let path = "data\\config.json";  // Windows 特有

// ✅ 使用 std::path::Path
let path = std::path::Path::new("data").join("config.json");
```

### 文件对话框

`rfd`（Rust File Dialog）crate 在各平台调用原生文件选择对话框（第 30 章会详细讲解）。

### 字体配置

* **Windows**：系统字体在 `C:\Windows\Fonts\`
* **macOS**：系统字体在 `/System/Library/Fonts/` 和 `/Library/Fonts/`
* **Linux**：系统字体在 `/usr/share/fonts/`

推荐的方式是将字体文件通过 `include_bytes!` 嵌入应用，确保跨平台一致性（第 22 章）。

### 窗口行为

* Windows：窗口行为最标准
* macOS：菜单栏在屏幕顶部，不是窗口内部
* Linux：不同桌面环境有细微差异

egui 为你处理了大部分差异，你通常不需要写平台特定代码。

---

## 4.5 从本章开始的约定

在后续章节中，每个代码示例都遵循以下约定：- **完整的可运行项目**放在 `examples/chXX-name/` 目录中
- **教程中的代码片段**展示核心逻辑，可能省略 `main` 函数等样板
- **代码注释使用中文**，变量和函数命名使用英文
- **Rust 2024 edition** — `Cargo.toml` 中 `edition = "2024"`
如果你想运行任何示例：

```rust
cd examples/ch02-counter
cargo run
```

---

## 本章小结

* `Cargo.toml` 中只需 `eframe = "0.34"`，默认使用 wgpu 渲染
* 项目结构随规模演进：单体 → `main.rs + app.rs` → 多模块
* 使用 `cargo watch` 提升开发效率
* 使用 `RUST_LOG=debug` 查看 egui 内部日志
* egui 跨平台表现一致，少数差异已由 eframe 处理

**下一章**：开始学习 egui 的第一个基础组件——文本与标签。

---

> 📁 本章无独立代码示例（工具与概念章）