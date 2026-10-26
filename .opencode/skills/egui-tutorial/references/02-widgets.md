# 基础组件篇


---

第二部分：基础组件篇

﻿# 05 · 文本与标签

文本是 GUI 中最基本的元素。egui 提供了从简单标签到富文本样式的完整文本方案。本章覆盖所有与文本显示相关的 API。

---

## 5.1 纯文本标签 `Label`

最基础的文本显示：

```rust
// 方式一：直接传递字符串
ui.label("这是一段普通文本");

// 方式二：使用 Label 结构体（等价）
ui.add(egui::Label::new("这也是一段文本"));
```

`label()` 是 `ui.add(Label::new(...))` 的便捷方法，二者完全等价。

### 特性

* 文本会自动换行以适应可用宽度
* Label 默认**不可交互**（不会响应鼠标事件）
* 支持多行文本（直接用 `\n` 换行）

```rust
ui.label("第一行\n第二行\n第三行");
```

---

## 5.2 `RichText` 富文本

`RichText` 是 egui 的文本样式引擎，支持颜色、大小、粗细、斜体等：

```rust
use eframe::egui::{self, RichText, Color32};

// 链式调用设置样式
ui.label(
    RichText::new("红色大标题")
        .color(Color32::RED)       // 颜色
        .size(24.0)                // 字号
        .strong()                  // 加粗
);

ui.label(
    RichText::new("灰色小字")
        .color(Color32::GRAY)
        .size(10.0)
        .italics()                 // 斜体
);
```

### RichText 的全部样式方法

|  |  |
| --- | --- |
| 方法 | 效果 |

|  |  |
| --- | --- |
| `.color(Color32)` | 设置文字颜色 |
| `.background_color(Color32)` | 设置文字背景色 |
| `.size(f32)` | 设置字号（像素） |
| `.strong()` | 加粗 |
| `.italics()` | 斜体 |
| `.underline()` | 下划线 |
| `.strikethrough()` | 删除线 |
| `.monospace()` | 等宽字体 |
| `.raised()` | 凸起效果（常用于强调） |
| `.code()` | 代码样式（等宽 + 浅灰背景） |

### 组合使用

```rust
ui.label(
    RichText::new("重要通知")
        .color(Color32::from_rgb(255, 80, 80))
        .size(18.0)
        .strong()
        .underline()
);
```

---

## 5.3 `Heading` 标题

egui 提供了预定义的标题样式——本质上是特定字号的 `RichText`：

```rust
ui.heading("这是大标题");       // 约 20px，加粗
```

`heading()` 内部实际上是：

```rust
ui.label(RichText::new("...").size(20.0).strong());
```

如果你需要更小的标题，用 `RichText` 手动控制字号即可——egui 没有 `H1`/`H2`/`H3` 的概念。

---

## 5.4 中文字体问题

如果你的界面中显示了中文文本，你可能会看到：

```rust
□□□□□  // 或者空白
```

**这是 egui 开发中的头号痛点**——默认字体不包含中文字形。每个字符都变成了方框（tofu）。

### 临时解决方案

在学习阶段（第 22 章之前），有两种临时处理方式：
**方案 A：暂时使用英文**
大多数示例用英文即可运行。
**方案 B：在启动时临时加载一个中文字体**
在 `main` 函数中的 `_cc`（`CreationContext`）里配置：

```rust
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "标题",
        native_options,
        Box::new(|cc| {
            // 临时中文字体配置（详见第 22 章）
            // 这里省略具体代码，知道有这个方法即可
            Ok(Box::new(MyApp::default()))
        }),
    )
}
```

> **第 22 章将完整讲解中文字体配置**。在那之前，本章示例使用英文，避免方框干扰学习。如果你现在就迫不及待想解决，可以直接跳转至第 22 章。

---

## 5.5 混合样式文本（LayoutJob 简介）

有时你需要**在同一行中使用多种样式**：
目前 RichText 已是最佳效果—— "**红色的**警告：操作不可逆"：
这种效果可以用 `RichText::text()` 实现：

```rust
ui.label(
    RichText::new("红色的")
        .color(Color32::RED)
        .text()  // 转换为 WidgetText
);
```

⚠️ 实际 egui 中 `RichText` 通过实现 `Into<WidgetText>` 来支持混排。最常用的方式是将多个 `RichText` 分段：

```rust
use eframe::egui::{self, RichText, Color32};

ui.horizontal(|ui| {
    ui.label(RichText::new("状态: ").strong());
    ui.label(RichText::new("正常").color(Color32::GREEN));
});
```

要对一行文本的不同部分应用不同样式，使用 `horizontal` 布局将多个 label 并排——这是 immediate mode 的惯用方式。

---

## 5.6 完整示例

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        // 普通文本
        ui.label("1. 普通标签");

        // 大标题
        ui.heading("2. 大标题 (heading)");

        // 富文本样式矩阵
        ui.separator();
        ui.label("3. 富文本样式:");

        ui.label(RichText::new("红色加粗").color(Color32::RED).strong());
        ui.label(RichText::new("蓝色斜体").color(Color32::BLUE).italics());
        ui.label(
            RichText::new("下划线 + 删除线")
                .underline()
                .strikethrough()
        );
        ui.label(RichText::new("代码风格").code());

        // 自定义颜色
        ui.label(
            RichText::new("自定义颜色 #FFAA00")
                .color(Color32::from_rgb(255, 170, 0))
                .size(14.0)
        );

        // 多行文本
        ui.separator();
        ui.label("4. 多行文本:");
        ui.label("这是第一行\n这是第二行\n这是第三行");

        // 混排
        ui.separator();
        ui.label("5. 混排:");
        ui.horizontal(|ui| {
            ui.label(RichText::new("CPU: ").strong());
            ui.label(RichText::new("45%").color(Color32::GREEN));
            ui.label(" | ");
            ui.label(RichText::new("内存: ").strong());
            ui.label(RichText::new("2.1 GB").color(Color32::YELLOW));
        });

        // 多字号对比
        ui.separator();
        ui.label("6. 字号对比:");
        for &size in &[8.0, 12.0, 16.0, 20.0, 28.0, 36.0] {
            ui.label(RichText::new(format!("{}px 文字", size as i32)).size(size));
        }
    });
}
```

---

## 本章小结

* `ui.label()` — 最基础的文本显示
* `RichText` — 链式设置颜色、大小、粗细、斜体、下划线等
* `ui.heading()` — 预定义的大标题样式
* 中文显示需要配置字体（第 22 章解决）
* 混排用 `ui.horizontal()` + 多个 label 实现

**下一章**：按钮与可点击元素——让界面真正可交互。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;
use eframe::egui::{Color32, RichText};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "文本与标签",
        native_options,
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}

struct MyApp;

impl Default for MyApp {
    fn default() -> Self {
        Self
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("egui 文本与标签演示");

        // 1. 普通标签
        ui.separator();
        ui.label("1. 普通标签 (Label):");
        ui.label("This is a plain label.");
        ui.label("Line 1\nLine 2\nLine 3 (multi-line)");

        // 2. 富文本颜色
        ui.separator();
        ui.label("2. RichText 颜色:");
        ui.label(RichText::new("Red text").color(Color32::RED));
        ui.label(RichText::new("Blue text").color(Color32::BLUE));
        ui.label(RichText::new("Green text").color(Color32::GREEN));
        ui.label(RichText::new("Gray text").color(Color32::GRAY));
        ui.label(
            RichText::new("Custom #FFAA00")
                .color(Color32::from_rgb(255, 170, 0)),
        );

        // 3. 富文本样式
        ui.separator();
        ui.label("3. RichText 样式:");
        ui.label(RichText::new("Bold (strong)").strong());
        ui.label(RichText::new("Italic (italics)").italics());
        ui.label(RichText::new("Underlined").underline());
        ui.label(RichText::new("Strikethrough").strikethrough());
        ui.label(RichText::new("Code style text").code());

        // 4. 字号对比
        ui.separator();
        ui.label("4. 字号对比:");
        for &size in &[8.0, 12.0, 16.0, 20.0, 28.0, 36.0] {
            ui.label(RichText::new(format!("{}px text", size as i32)).size(size));
        }

        // 5. 样式组合
        ui.separator();
        ui.label("5. 样式组合:");
        ui.label(
            RichText::new("Bold + Italic + Underline + Red")
                .color(Color32::RED)
                .strong()
                .italics()
                .underline(),
        );
        ui.label(
            RichText::new("Highlighted text")
                .background_color(Color32::from_rgb(255, 255, 100))
                .strong(),
        );

        // 6. Heading 标题
        ui.separator();
        ui.label("6. Heading vs Label:");
        ui.heading("This is a heading");
        ui.label("This is a normal label for comparison");

        // 7. 混排
        ui.separator();
        ui.label("7. 混排 (horizontal):");
        ui.horizontal(|ui| {
            ui.label(RichText::new("CPU: ").strong());
            ui.label(RichText::new("45%").color(Color32::GREEN));
            ui.label(" | ");
            ui.label(RichText::new("Memory: ").strong());
            ui.label(RichText::new("2.1 GB").color(Color32::YELLOW));
        });
    }
}
```

---

第二部分：基础组件篇

﻿# 06 · 按钮与可点击元素

按钮是最基本的交互控件。egui 提供了多种可点击元素：普通按钮、链接文本、图片按钮，以及键盘快捷键处理。

---

## 6.1 `Button` 基本用法

```rust
// 基本按钮
if ui.button("点击我").clicked() {
    // 用户点击了按钮
}

// 使用 add 而非 button（效果相同）
if ui.add(egui::Button::new("点击我")).clicked() {
    // ...
}
```

### `Response::clicked()` vs 其他检测方式

```rust
let response = ui.button("按钮");

if response.clicked() {
    // 本帧按钮被左键点击（最常用）
}
if response.clicked_by(egui::PointerButton::Secondary) {
    // 本帧被右键点击
}
if response.double_clicked() {
    // 本帧被双击
}
if response.hovered() {
    // 当前鼠标悬停在按钮上
}
```

### 按钮样式

```rust
// 填充按钮（带背景色）
ui.button("默认填充按钮");

// 轮廓按钮
ui.add(egui::Button::new("轮廓按钮").fill(egui::Color32::TRANSPARENT));

// 最小尺寸按钮
ui.add(egui::Button::new("小按钮").small());

// 使用 Sense 禁用按钮
ui.add_enabled(false, egui::Button::new("禁用按钮"));
// 或者
ui.add(egui::Button::new("禁用按钮").sense(egui::Sense::hover()));
```

### 按钮响应状态演示

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        // 正常按钮
        if ui.button("正常按钮").clicked() {
            self.log.push("点击了正常按钮".into());
        }

        // 禁用按钮：不响应任何点击
        ui.add_enabled(false, egui::Button::new("禁用按钮"));
        // 或
        ui.add(egui::Button::new("禁用按钮2").sense(egui::Sense::hover()));

        ui.separator();

        // 检测不同点击类型
        let resp = ui.button("右键/双击测试");
        if resp.clicked() { self.log.push("左键点击".into()); }
        if resp.clicked_by(egui::PointerButton::Secondary) {
            self.log.push("右键点击".into());
        }
        if resp.double_clicked() { self.log.push("双击".into()); }

        ui.separator();

        // 按钮大小
        ui.add(egui::Button::new("小按钮 (small)").small());
        ui.button("默认大小");
        ui.add(egui::Button::new("自定义宽度").min_size(egui::vec2(200.0, 0.0)));

        // 悬停判断
        if ui.button("悬停变色 (hover)").hovered() {
            ui.label("你正在悬停在按钮上!");
        }
    });
}
```

---

## 6.2 `Link` 链接文本

外观类似网页超链接，适合"关于"、"帮助"等次要导航：

```rust
use eframe::egui;

// 最简形式
ui.hyperlink("https://www.rust-lang.org");

// 自定义显示文本
ui.hyperlink_to("Rust 官网", "https://www.rust-lang.org");

// 结合条件判断
if ui.hyperlink_to("设置", "settings://").clicked() {
    // 自定义协议：自己处理点击逻辑
    self.show_settings = true;
}
```

> ⚠️ 注意：API 名是 `ui.hyperlink()` 和 `ui.hyperlink_to()`，不是 `Link::new()`。这在 0.34 中是标准用法。

---

## 6.3 `ImageButton` 图片按钮

用图片替代文本的按钮（需要先加载纹理，详见第 30 章）：

```rust
// 概念代码（需配合纹理加载）
let texture = / 加载纹理 /;
let image_button = egui::ImageButton::new(
    egui::Image::new(&texture, texture.size_vec2())
        .tint(egui::Color32::WHITE) // 图片 tint
);
if ui.add(image_button).clicked() {
    // 图片按钮被点击
}
```

---

## 6.4 键盘快捷键

快捷键应在 `logic()` 中处理，而非 `ui()`：

```rust
fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    ctx.input(|i| {
        // 单键
        if i.key_pressed(egui::Key::Escape) {
            self.show_menu = false; // 关闭菜单
        }

        // 修饰键组合
        if i.modifiers.ctrl && i.key_pressed(egui::Key::S) {
            self.save_file(); // Ctrl+S 保存
        }
        if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z) {
            self.redo(); // Ctrl+Shift+Z 重做
        }
    });
}
```

### 常用 Key 枚举

|  |  |
| --- | --- |
| Key | 说明 |

|  |  |
| --- | --- |
| `Key::A` ~ `Key::Z` | 字母键 |
| `Key::Num0` ~ `Key::Num9` | 数字键 |
| `Key::Space` | 空格 |
| `Key::Enter` | 回车 |
| `Key::Escape` | Esc |
| `Key::Tab` | Tab |
| `Key::Backspace` | 退格 |
| `Key::Delete` | Delete |
| `Key::ArrowUp/Down/Left/Right` | 方向键 |
| `Key::F1` ~ `Key::F12` | 功能键 |

---

## 本章小结

* `ui.button("...").clicked()` — 最常用模式
* 按钮样式：`.small()`、`.fill(...)`、`ui.add_enabled(false, ...)`
* `ui.hyperlink_to("文本", "URL")` — 链接文本
* 快捷键在 `logic()` 中通过 `ctx.input(|i| i.key_pressed(...))` 处理

**下一章**：输入框与文本编辑。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
﻿use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "按钮示例",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(MyApp::default()))),
    )
}

struct MyApp { clicks: i32, log: Vec<String> }

impl Default for MyApp {
    fn default() -> Self { Self { clicks: 0, log: Vec::new() } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("按钮示例");

        // 基本按钮
        if ui.button("点击我").clicked() {
            self.clicks += 1;
            self.log.push("左键点击".into());
        }

        // 右键点击
        let resp = ui.button("右键点击测试");
        if resp.clicked_by(egui::PointerButton::Secondary) {
            self.log.push("右键点击".into());
        }

        // 双击
        if resp.double_clicked() {
            self.log.push("双击".into());
        }

        ui.label(format!("按钮被点击了 {} 次", self.clicks));
        ui.separator();

        // 按钮样式
        ui.horizontal(|ui| {
            let _ = ui.button("默认按钮");
            ui.add(egui::Button::new("轮廓按钮").fill(egui::Color32::TRANSPARENT));
            ui.add(egui::Button::new("小按钮").small());
            ui.add_enabled(false, egui::Button::new("禁用按钮"));
        });

        ui.separator();

        // 链接文本
        ui.hyperlink_to("egui 官网", "https://github.com/emilk/egui");

        // 事件日志
        ui.separator();
        ui.heading("事件日志:");
        egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
            for entry in &self.log {
                ui.label(entry);
            }
        });
    }
}
```

---

第二部分：基础组件篇

﻿# 07 · 输入框与文本编辑

`TextEdit` 是 egui 中的文本输入控件，支持单行、多行、密码、只读等模式。

---

## 7.1 `TextEdit::singleline` 单行输入

```rust
// 绑定到一个 String 变量
ui.add(egui::TextEdit::singleline(&mut self.name));

// 带占位提示
ui.add(
    egui::TextEdit::singleline(&mut self.name)
        .hint_text("请输入姓名")
);

// 限制宽度
ui.add(
    egui::TextEdit::singleline(&mut self.name)
        .desired_width(200.0)
);

// 限制字符数
ui.add(
    egui::TextEdit::singleline(&mut self.name)
        .char_limit(20)
);
```

**Enter 行为**：在单行模式下，按 Enter 键会使输入框失去焦点。

---

## 7.2 `TextEdit::multiline` 多行输入

```rust
ui.add(
    egui::TextEdit::multiline(&mut self.description)
        .desired_rows(5)               // 显示 5 行
        .desired_width(f32::INFINITY)   // 撑满可用宽度
);
```

多行模式下 Enter 创建换行，Ctrl+Enter 失去焦点。内容超出可视区域时自动出现滚动条。
**固定尺寸**：

```rust
ui.add_sized(
    egui::vec2(400.0, 200.0),           // 固定宽 400, 高 200
    egui::TextEdit::multiline(&mut text),
);
```

---

## 7.3 文本选择的读写

```rust
let mut output = egui::TextEdit::singleline(&mut self.text);
let response = ui.add(output);

// 检测内容变化
if response.changed() {
    println!("文本变为: {}", self.text);
}
```

`changed()` 与 `clicked()` 的区别：

* `clicked()` — 本帧**刚被按下并释放**
* `changed()` — 本帧**内容发生了变化**（输入、粘贴、删除均触发）

---

## 7.4 高级控制

### 只读模式

```rust
ui.add(
    egui::TextEdit::singleline(&mut text)
        .interactive(false)  // 不可编辑，但可以选中和复制
);
```

### 密码输入

```rust
ui.add(
    egui::TextEdit::singleline(&mut self.password)
        .password(true)       // 显示圆点，阻止复制
        .hint_text("请输入密码")
);
```

### 其他选项

```rust
egui::TextEdit::singleline(&mut text)
    .hint_text("提示文字")       // 空时显示的占位文字
    .char_limit(100)            // 最大字符数
    .clip_text(true)            // 超宽时裁剪（默认 true）
    .font(egui::FontId::monospace(12.0)) // 使用等宽字体
```

---

## 7.5 完整示例

```rust
struct MyApp {
    username: String,
    password: String,
    bio: String,
    log: Vec<String>,
}

fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.heading("用户信息表单");

        // 用户名：单行 + 占位提示
        ui.label("用户名:");
        ui.add(
            egui::TextEdit::singleline(&mut self.username)
                .hint_text("请输入用户名")
                .desired_width(200.0),
        );

        // 密码：密码模式
        ui.label("密码:");
        ui.add(
            egui::TextEdit::singleline(&mut self.password)
                .password(true)
                .hint_text("请输入密码")
                .desired_width(200.0),
        );

        // 简介：多行
        ui.label("简介:");
        ui.add(
            egui::TextEdit::multiline(&mut self.bio)
                .hint_text("写点什么...")
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );

        // 实时显示输入的字符数
        ui.label(format!("简介: {} 字符", self.bio.len()));

        ui.separator();

        // 只读预览
        ui.label("预览（只读）:");
        ui.add(
            egui::TextEdit::singleline(
                &mut format!("{} - {}", self.username, self.password)
            )
            .interactive(false),
        );
    });
}
```

---

## 本章小结

* `TextEdit::singleline()` — 单行，Enter 失去焦点
* `TextEdit::multiline()` — 多行，Enter 换行
* `.hint_text()` — 空时占位提示
* `.password(true)` — 密码遮罩 + 阻止复制
* `.interactive(false)` — 只读（可选中复制）
* `response.changed()` — 检测内容变化

**下一章**：复选框与开关。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("输入框示例", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { username: String, password: String, bio: String }

impl Default for MyApp {
    fn default() -> Self { Self { username: String::new(), password: String::new(), bio: String::new() } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("用户信息表单");

        ui.label("用户名:");
        ui.add(egui::TextEdit::singleline(&mut self.username).hint_text("请输入用户名").desired_width(200.0));

        ui.label("密码:");
        ui.add(egui::TextEdit::singleline(&mut self.password).password(true).hint_text("请输入密码").desired_width(200.0));

        ui.label("简介:");
        ui.add(egui::TextEdit::multiline(&mut self.bio).hint_text("写点什么...").desired_rows(4).desired_width(f32::INFINITY));

        ui.label(format!("简介: {} 字符", self.bio.len()));

        ui.separator();
        let preview = format!("{} - {}", self.username, if self.password.is_empty() { "(空)" } else { "***" });
        ui.label("预览（只读）:");
        ui.add(egui::TextEdit::singleline(&mut preview.clone()).interactive(false));
    }
}
```

---

第二部分：基础组件篇

﻿# 08 · 复选框与开关

复选框、开关、可选中标签——三种控制布尔状态或选项选择的控件。

---

## 8.1 `Checkbox` 复选框

```rust
ui.checkbox(&mut self.enable_audio, "启用音频");
//        ^^^^^^^^^^^^^^^^^^^^^^^^    ^^^^^^^^
//        绑定的 bool 变量              标签文本
```

* 点击复选框或标签文字都可切换
* 返回 `Response`，可用 `.changed()` 检测变化

### 不带文字

```rust
ui.checkbox(&mut self.checked, "");       // 空字符串
// 或直接用 add
ui.add(egui::Checkbox::without_text(&mut self.checked));
```

### RichText 标签

```rust
ui.checkbox(
    &mut self.dangerous,
    RichText::new("危险操作").color(Color32::RED).strong(),
);
```

---

## 8.2 开关（Toggle）

外观类似移动端的 toggle 开关。egui 0.34 中 `Switch` 已移除，可以使用 `selectable_label` 或 `Checkbox` 模拟：

```rust
// 使用 selectable_label 模拟开关效果
ui.horizontal(|ui| {
    ui.label("Wi-Fi:");
    if ui.selectable_label(self.wifi_enabled, if self.wifi_enabled { "● 开" } else { "○ 关" }).clicked() {
        self.wifi_enabled = !self.wifi_enabled;
    }
});

// 或直接用 Checkbox（功能等价，外观不同）
ui.checkbox(&mut self.wifi_enabled, "Wi-Fi");
```

与 Checkbox 的区别（概念层面）：

* Toggle/Switch — 视觉上更现代，适合设置页
* Checkbox — 更传统，适合表单/多选列表

---

## 8.3 `SelectableLabel` 可选中标签

用于列表选择或单选/多选场景：

```rust
// 单选模式（模仿 radio）
for (i, item) in self.items.iter().enumerate() {
    if ui.selectable_label(self.selected == i, item).clicked() {
        self.selected = i;
    }
}
```

* 点击时切换选中状态
* 选中状态的标签有高亮背景色
* 适合"选项列表"而非表单

---

## 8.4 分组依赖逻辑（级联控制）

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.heading("设置面板");

        // 主开关
        ui.checkbox(&mut self.enable_feature, "启用高级功能");

        // 依赖子选项（仅在主开关启用时显示和可用）
        ui.add_enabled(self.enable_feature, egui::Label::new("高级选项:"));
        ui.add_enabled(self.enable_feature,
            egui::Checkbox::new(&mut self.advanced_option, ""),
        );

        // 全选/取消全选
        ui.separator();
        ui.heading("权限设置");
        let all_checked = self.perms.iter().all(|&p| p);
        let any_checked = self.perms.iter().any(|&p| p);

        // 三态全选（勾选表示全选，横线表示部分选）
        if ui.checkbox(&mut all_checked, "全选").changed() {
            let new_state = !all_checked;
            for p in &mut self.perms { *p = new_state; }
        }

        for (i, p) in self.perms.iter_mut().enumerate() {
            ui.checkbox(p, format!("权限 {}", i + 1));
        }
    });
}
```

---

## 8.5 三种控件选择指南

|  |  |
| --- | --- |
| 场景 | 推荐控件 |

|  |  |
| --- | --- |
| 表单中的单个选项 | `Checkbox` |
| 设置页的开关 | `Switch` |
| 列表/菜单中的选项 | `SelectableLabel` |
| 多选列表 | 多个 `Checkbox` |
| 单选列表 | `SelectableLabel` 带状态管理 |

---

## 本章小结

* `ui.checkbox(&mut bool, "标签")` — 复选框
* Toggle 开关 — 用 `selectable_label` 或 `Checkbox` 模拟
* `ui.selectable_label(selected, text)` — 可选中标签
* `ui.add_enabled(cond, widget)` — 级联控制启用/禁用

**下一章**：数值输入 — Slider 和 DragValue。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("复选框与开关", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { enable_audio: bool, wifi: bool, perms: [bool; 3] }

impl Default for MyApp {
    fn default() -> Self { Self { enable_audio: true, wifi: false, perms: [false; 3] } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("设置面板");

        ui.checkbox(&mut self.enable_audio, "启用音频");

        // 开关风格的 toggle
        ui.horizontal(|ui| {
            ui.label("Wi-Fi:");
            if ui.selectable_label(self.wifi, if self.wifi { "● 开" } else { "○ 关" }).clicked() {
                self.wifi = !self.wifi;
            }
        });
        ui.label(format!("Wi-Fi 状态: {}", if self.wifi { "开" } else { "关" }));

        ui.separator();
        ui.heading("权限设置");

        let mut all_check = self.perms.iter().all(|&p| p);
        if ui.checkbox(&mut all_check, "全选").changed() {
            let new = !all_check;
            for p in &mut self.perms { *p = new; }
        }

        for (i, p) in self.perms.iter_mut().enumerate() {
            ui.checkbox(p, format!("权限 {}", i + 1));
        }

        ui.separator();
        ui.add_enabled(false, egui::Button::new("仅在全部选中时可用"));
    }
}
```

---

第二部分：基础组件篇

# 09 · 数值输入

egui 提供了两种数值输入方式：`Slider`（滑块）和 `DragValue`（拖拽数值）。本章覆盖它们的完整 API。

---

## 9.1 `Slider` 滑块

```rust
// 整数滑块
ui.add(egui::Slider::new(&mut self.volume, 0..=100));

// 浮点滑块
ui.add(egui::Slider::new(&mut self.opacity, 0.0..=1.0));

// 带标签
ui.add(egui::Slider::new(&mut self.volume, 0..=100).text("音量"));

// 步长
ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).step_by(0.5));
```

### Slider 全部选项

```rust
egui::Slider::new(&mut value, range)
    .text("标签")                    // 滑块左侧标签
    .step_by(0.1)                    // 步长
    .logarithmic(true)               // 对数刻度
    .clamp_to_range(true)            // 强制值在范围内（默认 true）
    .show_value(true)                // 显示数值（默认 true）
    .custom_formatter(|n, _| format!("{:.1}%", n))
    .custom_parser(|s| s.trim_end_matches('%').parse().ok())
    .trailing_fill(true)             // 滑轨尾部填充
    .orientation(egui::SliderOrientation::Vertical)
```

---

## 9.2 对数滑块

用于跨越多个数量级的值（如频率 20Hz ~ 20,000Hz）：

```rust
ui.add(
    egui::Slider::new(&mut self.frequency, 20.0..=20000.0)
        .logarithmic(true)
        .text("频率"),
);
```

对数滑块让你在低频区和高频区都能精细调节。

---

## 9.3 `DragValue` 拖拽数值

更紧凑的数值输入——拖拽改变数值，也可直接输入：

```rust
// 基础用法
ui.add(egui::DragValue::new(&mut self.speed));

// 带单位后缀
ui.add(egui::DragValue::new(&mut self.angle).suffix("°"));

// 前缀 + 速度控制
ui.add(
    egui::DragValue::new(&mut self.price)
        .prefix("¥")
        .speed(0.5)  // 拖拽速度（默认为 1.0）
);

// 小数位数
ui.add(
    egui::DragValue::new(&mut self.ratio)
        .fixed_decimals(2)  // 固定 2 位小数
        .speed(0.01),
);
```

### DragValue vs Slider

|  |  |  |
| --- | --- | --- |
| 特性 | Slider | DragValue |

|  |  |  |
| --- | --- | --- |
| 空间占用 | 较大（滑轨） | 紧凑（数值框） |
| 精确操作 | 滑块拖动 | 拖拽 + 可输入数值 |
| 固定范围 | ✅ `range` 强制 | ✅ `range` |
| 对数刻度 | ✅ | ❌（0.34 不支持） |
| 适合场景 | 粗调、直观 | 微调、精确 |

**组合使用**：用 Slider 粗调 + DragValue 显示和精确输入：

```rust
ui.horizontal(|ui| {
    ui.label("亮度:");
    ui.add(egui::Slider::new(&mut self.brightness, 0.0..=100.0));
    ui.add(egui::DragValue::new(&mut self.brightness).suffix("%"));
});
```

---

## 9.4 自定义格式化

```rust
// 时间格式
ui.add(
    egui::DragValue::new(&mut self.seconds)
        .range(0..=86399) // 一天的秒数
        .custom_formatter(|n, _| {
            let h = n as i32 / 3600;
            let m = (n as i32 % 3600) / 60;
            let s = n as i32 % 60;
            format!("{:02}:{:02}:{:02}", h, m, s)
        })
        .custom_parser(|s| {
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() == 3 {
                Some(
                    parts[0].parse::<f64>().unwrap_or(0.0) * 3600.0
                    + parts[1].parse::<f64>().unwrap_or(0.0) * 60.0
                    + parts[2].parse::<f64>().unwrap_or(0.0)
                )
            } else { None }
        }),
);
```

---

## 本章小结

* `Slider::new(&mut val, range)` — 滑块，支持对数、步长、自定义格式
* `DragValue::new(&mut val)` — 紧凑数值，支持拖拽+输入、前后缀、小数位
* Slider 粗调 + DragValue 微调 = 最佳组合
* `custom_formatter` / `custom_parser` 实现自定义显示格式

**下一章**：下拉框与列表选择。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("数值输入", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { volume: i32, opacity: f32, speed: f64, frequency: f32 }

impl Default for MyApp {
    fn default() -> Self { Self { volume: 50, opacity: 0.8, speed: 1.0, frequency: 440.0 } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("数值输入控件");

        ui.label("音量:");
        ui.add(egui::Slider::new(&mut self.volume, 0..=100));
        ui.label("不透明度:");
        ui.add(egui::Slider::new(&mut self.opacity, 0.0..=1.0).step_by(0.05));
        ui.label("频率 (Hz):");
        ui.add(egui::Slider::new(&mut self.frequency, 20.0..=20000.0).logarithmic(true));

        ui.separator();
        ui.label("速度:");
        ui.add(egui::DragValue::new(&mut self.speed).speed(0.1).suffix("x"));
        ui.label("不透明度:");
        ui.add(egui::DragValue::new(&mut self.opacity).speed(0.01).fixed_decimals(3).prefix("α="));

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("亮度:");
            ui.add(egui::Slider::new(&mut self.volume, 0..=100));
            ui.add(egui::DragValue::new(&mut self.volume).suffix("%"));
        });
    }
}
```

---

第二部分：基础组件篇

# 10 · 下拉框与列表选择

当用户在多个选项中选择一个或多个值时，egui 提供 `ComboBox`、`RadioButton`、`SelectableLabel`。

---

## 10.1 `ComboBox` 下拉框

```rust
egui::ComboBox::from_label("选择语言")
    .selected_text(format!("{:?}", self.language))
    .show_ui(ui, |ui| {
        ui.selectable_value(&mut self.language, Language::Rust, "Rust");
        ui.selectable_value(&mut self.language, Language::Python, "Python");
        ui.selectable_value(&mut self.language, Language::JavaScript, "JavaScript");
    });
```

### 要点

* `.from_label("标签")` — 下拉框标题
* `.selected_text(...)` — 当前选中项的显示文本
* `.show_ui(ui, |ui| {...})` — 下拉列表中显示的 UI
* `ui.selectable_value(&mut enum, variant, text)` — 单选枚举值

### 用 id\_source 而非 label

```rust
// 当同一标签需要多个 ComboBox 时，使用 id_source 区分
egui::ComboBox::from_id_source("language_selector")
    .selected_text(...)
    .show_ui(ui, |ui| { ... });
```

### 自定义列表项

```rust
egui::ComboBox::from_label("选择")
    .selected_text(self.choice)
    .show_ui(ui, |ui| {
        for item in &self.items {
            ui.selectable_value(&mut self.choice, item.clone(), item);
        }
    });
```

---

## 10.2 `RadioButton` 单选按钮

```rust
ui.radio_value(&mut self.theme, Theme::Dark, "深色主题");
ui.radio_value(&mut self.theme, Theme::Light, "浅色主题");
ui.radio_value(&mut self.theme, Theme::Auto, "自动");
```

* 同一枚举值的 RadioButton 自动编组（互斥）
* 点击标签文字也可选中
* 适合选项较少（2-5 个）的场景

---

## 10.3 `SelectableLabel` 多选场景

```rust
for (i, item) in self.items.iter().enumerate() {
    let mut selected = self.selected_items.contains(&i);
    if ui.selectable_label(selected, item).clicked() {
        if selected {
            self.selected_items.retain(|&x| x != i);
        } else {
            self.selected_items.push(i);
        }
    }
}
```

---

## 10.4 选择控件决策树

```rust
选项数量？
  ├─ ≤ 5 个   → RadioButton（全可见，一键选中）
  ├─ 5 ~ 20 个 → ComboBox（紧凑，点击展开）
  └─ > 20 个  → ComboBox + 搜索过滤

选择模式？
  ├─ 单选      → ComboBox / RadioButton
  └─ 多选      → 多个 Checkbox / SelectableLabel 列表
```

---

## 本章小结

* `ComboBox` — 下拉框，适合中量选项
* `RadioButton` — 单选按钮，适合少量选项
* `SelectableLabel` — 可选中标签，适合列表选择
* 使用 `id_source` 区分同标签的多个 ComboBox

**下一章**：布局模型——理解 egui 如何组织界面空间。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("下拉框与列表选择", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

#[derive(PartialEq, Clone)]
enum Theme { Dark, Light, Auto }

struct MyApp { language: String, theme: Theme, items: Vec<String>, selected: Vec<usize> }

impl Default for MyApp {
    fn default() -> Self {
        Self {
            language: "Rust".into(), theme: Theme::Dark,
            items: vec!["选项A".into(), "选项B".into(), "选项C".into()],
            selected: vec![],
        }
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("选择控件");

        // ComboBox
        egui::ComboBox::from_label("编程语言")
            .selected_text(&self.language)
            .show_ui(ui, |ui| {
                for lang in &["Rust", "Python", "JavaScript", "Go"] {
                    ui.selectable_value(&mut self.language, lang.to_string(), *lang);
                }
            });

        // RadioButton
        ui.separator();
        ui.label("主题:");
        ui.radio_value(&mut self.theme, Theme::Dark, "深色");
        ui.radio_value(&mut self.theme, Theme::Light, "浅色");
        ui.radio_value(&mut self.theme, Theme::Auto, "自动");

        // SelectableLabel (多选)
        ui.separator();
        ui.label("多选列表:");
        let mut to_remove = vec![];
        for (i, item) in self.items.iter().enumerate() {
            let sel = self.selected.contains(&i);
            if ui.selectable_label(sel, item).clicked() {
                if sel { to_remove.push(i); } else { self.selected.push(i); }
            }
        }
        for &i in &to_remove { self.selected.retain(|&x| x != i); }

        ui.label(format!("已选: {:?}", self.selected.iter().map(|&i| &self.items[i]).collect::<Vec<_>>()));
    }
}
```