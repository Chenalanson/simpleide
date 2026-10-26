# 生态集成


---

第八部分：生态集成

# 27 · egui\_extras

`egui_extras` 是 egui 官方的"附加 Widget"库：DatePicker、Table、RetainedImage。

依赖添加：

```rust
egui_extras = "0.34"
```

---

## 27.1 DatePicker 日期选择器

```rust
use egui_extras::DatePickerButton;

let date_picker = DatePickerButton::new(&mut self.date);
ui.add(date_picker);

// 自定义格式
ui.add(
    DatePickerButton::new(&mut self.date)
        .format("%Y-%m-%d")          // strftime 格式
        .highlight_weekends(true)    // 周末高亮
);
```

点击后弹出日历，选择日期。

---

## 27.2 Table 表格

```rust
use egui_extras::{TableBuilder, Column};

let text_style = egui::TextStyle::Body;

TableBuilder::new(ui)
    .column(Column::auto().resizable(true))  // 第 1 列
    .column(Column::remainder())               // 第 2 列（占满剩余空间）
    .header(20.0, |mut header| {
        header.col(|ui| { ui.strong("名称"); });
        header.col(|ui| { ui.strong("值"); });
    })
    .body(|mut body| {
        for row in &self.data {
            body.row(18.0, |mut row_ui| {
                row_ui.col(|ui| { ui.label(&row.name); });
                row_ui.col(|ui| { ui.label(format!("{}", row.value)); });
            });
        }
    });
```

### Table 要点

* `.striped(true)` — 条纹行
* `.resizable(true)` — 可拖拽列宽
* `.sense(Sense::click())` — 行可点击
* ⚠️ Table 仍是逐行渲染——10 万行会卡死。大数据用分页（参见第 14 章）

---

## 27.3 RetainedImage

```rust
use egui_extras::RetainedImage;

// 从文件字节加载（编译时嵌入）
let img = RetainedImage::from_image_bytes(
    "logo",
    include_bytes!("../assets/logo.png"),
).unwrap();

// 显示
ui.image(img.texture_id(ui.ctx()), img.size_vec2());
```

---

## 27.4 其它工具

```rust
// StripBuilder — 不等高/不等宽的段布局
use egui_extras::StripBuilder;
StripBuilder::new(ui)
    .size(egui_extras::Size::exact(30.0))
    .size(egui_extras::Size::remainder())
    .vertical(|mut strip| {
        strip.cell(|ui| { ui.heading("头部"); });
        strip.cell(|ui| { ui.label("内容"); });
    });
```

---

## 本章小结

* `DatePickerButton` — 日期选择
* `TableBuilder` — 可排序列的表格
* `RetainedImage` — 惰性纹理加载
* `StripBuilder` — 灵活的分段布局

**下一章**：`egui_plot` 图表库。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("egui_extras", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { items: Vec<(String, i32)> }

impl Default for MyApp {
    fn default() -> Self { Self { items: vec![("苹果".into(), 5), ("香蕉".into(), 3), ("橙子".into(), 7)] } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("egui_extras::Table 演示");

        use egui_extras::{TableBuilder, Column};

        let mut to_remove = None;
        TableBuilder::new(ui)
            .column(Column::auto().resizable(true))
            .column(Column::auto())
            .column(Column::remainder())
            .header(20.0, |mut h| {
                h.col(|ui| { ui.strong("名称"); });
                h.col(|ui| { ui.strong("数量"); });
                h.col(|ui| { ui.strong("操作"); });
            })
            .body(|mut b| {
                for (i, (name, qty)) in self.items.iter().enumerate() {
                    b.row(18.0, |mut r| {
                        r.col(|ui| { ui.label(name); });
                        r.col(|ui| { ui.label(format!("{}", qty)); });
                        r.col(|ui| { if ui.button("删除").clicked() { to_remove = Some(i); } });
                    });
                }
            });

        if let Some(i) = to_remove { self.items.remove(i); }
    }
}
```

---

第八部分：生态集成

# 28 · egui\_plot 图表库

`egui_plot` 是 egui 官方的 2D 图表库。依赖：

```rust
egui_plot = "0.34"
```

---

## 28.1 基础图表

```rust
use egui_plot::{Plot, Line, PlotPoints, Bar, Legend};

// 折线图
let sin_points: PlotPoints = (0..1000)
    .map(|i| {
        let x = i as f64 * 0.01;
        [x, x.sin()]
    })
    .collect();
let sin_line = Line::new(sin_points).name("sin(x)");

Plot::new("my_plot")
    .legend(Legend::default())
    .show(ui, |plot_ui| {
        plot_ui.line(sin_line);
    });
```

---

## 28.2 柱状图

```rust
use egui_plot::BarChart;

let bars = BarChart::new(
    (0..10)
        .map(|i| Bar::new(i as f64, (i * i) as f64).name(format!("{}²", i)))
        .collect(),
)
.name("平方数")
.color(egui::Color32::LIGHT_BLUE);

Plot::new("bar_plot")
    .show(ui, |plot_ui| plot_ui.bar_chart(bars));
```

---

## 28.3 交互配置

```rust
Plot::new("plot")
    .x_axis_label("时间 (s)")
    .y_axis_label("值")
    .allow_zoom(true)          // 滚轮缩放（默认 true）
    .allow_drag(true)           // 拖拽平移（默认 true）
    .show_axes([true, true])    // 显示坐标轴
    .show_grid([true, true])    // 显示网格线
    .legend(Legend::default())  // 显示图例
    .show(ui, |plot_ui| { / ... / });
```

---

## 28.4 光标联动

多个图表共享十字光标：

```rust
use egui_plot::LinkedCursorsGroup;

let mut cursors = LinkedCursorsGroup::new(true, true); // 联动 X 和 Y

Plot::new("plot1")
    .link_cursor(cursors.clone(), 0)
    .show(ui, |plot_ui| { / ... / });

Plot::new("plot2")
    .link_cursor(cursors, 0)  // 同一组 → 光标联动
    .show(ui, |plot_ui| { / ... / });
```

---

## 本章小结

* `Plot::new("id").show(ui, |plot_ui| {...})` — 创建图表
* `Line::new(points)` — 折线图
* `BarChart::new(bars)` — 柱状图
* `LinkedCursorsGroup` — 多图表光标联动
* `.allow_zoom()` / `.allow_drag()` — 交互控制

**下一章**：`egui_phosphor` 图标库。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("egui_plot 图表", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("egui_plot 图表演示");

        // egui_plot 是 egui 官方的 2D 图表库
        // Cargo.toml 中添加: egui_plot = "0.34"
        // 完整 API 参考: https://docs.rs/egui_plot/

        ui.label("基础用法:");
        ui.label("  use egui_plot::{Plot, Line, PlotPoints, Legend};");
        ui.label("  let points = PlotPoints::new(vec![...]);");
        ui.label("  let line = Line::new(points).name(\"series\");");
        ui.label("  Plot::new(\"id\").show(ui, |plot_ui| { plot_ui.line(line); });");

        ui.separator();
        ui.label("可用图表类型:");
        ui.label("  • Line  — 折线图");
        ui.label("  • Points — 散点图");
        ui.label("  • Bar / BarChart — 柱状图");
        ui.label("  • Arrows — 向量场");
        ui.label("  • Polygon — 填充区域");
        ui.label("  • Text — 图表标注");

        ui.separator();
        ui.label("注意: egui_plot 的 API 随版本演进，请以 docs.rs 为准");
        ui.label("参见官方示例: github.com/emilk/egui_plot");
    }
}
```

---

第八部分：生态集成

# 29 · egui\_phosphor 图标库

Phosphor 是一套开源图标集（~1000+ 图标），通过 `egui_phosphor` 集成到 egui。

```rust
egui_phosphor = "0.34"
```

---

## 29.1 基本用法

```rust
use eframe::egui::{self, RichText};

// 在按钮中使用图标
if ui.add(egui::Button::new(
    RichText::new(format!("{} 保存", egui_phosphor::regular::FLOPPY_DISK))
)).clicked() {
    // 保存文件
}

// 纯图标按钮
if ui.add(egui::Button::new(
    RichText::new(egui_phosphor::regular::PLUS).size(18.0)
)).clicked() {
    // 新建
}
```

---

## 29.2 四种粗细变体

```rust
egui_phosphor::regular::CAMERA    // 常规线条
egui_phosphor::bold::CAMERA       // 粗线条
egui_phosphor::fill::CAMERA       // 实心填充
egui_phosphor::duotone::CAMERA    // 双色调
```

---

## 29.3 常用图标速查

|  |  |  |
| --- | --- | --- |
| 图标 | 名称 | 用途 |

|  |  |  |
| --- | --- | --- |
| 💾 | `FLOPPY_DISK` | 保存 |
| ➕ | `PLUS` | 新建/添加 |
| 🔍 | `MAGNIFYING_GLASS` | 搜索 |
| ⚙ | `GEAR` | 设置 |
| ▶ | `PLAY` | 播放 |
| ✕ | `X` | 关闭/删除 |
| ✓ | `CHECK` | 确认 |
| 📋 | `CLIPBOARD` | 复制 |
| 📂 | `FOLDER` | 文件夹 |

---

## 本章小结

* `egui_phosphor::regular::ICON_NAME` — 图标字符
* 配合 `RichText` 在 Label/Button 中使用
* 四种变体：regular / bold / fill / duotone

**下一章**：第三方 crate 集成。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("图标库", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("图标演示");

        // egui 支持 emoji 作为内置 icons，也可集成第三方图标库如 egui_phosphor
        ui.horizontal(|ui| {
            if ui.button("💾 保存").clicked() { }
            if ui.button("➕ 新建").clicked() { }
            if ui.button("🔍 搜索").clicked() { }
            if ui.button("⚙ 设置").clicked() { }
        });

        ui.separator();
        ui.label("提示: egui_phosphor crate (0.34) 提供专业图标集");
        ui.label("用法: egui_phosphor::regular::FLOPPY_DISK 等");
        ui.label("参考 crates.io/crates/egui_phosphor");
    }
}
```

---

第八部分：生态集成

﻿# 30 · 第三方 crate 集成

本章讲解如何将 egui 与 Rust 生态中的常用 crate 配合使用。

---

## 30.1 图片加载与显示

```rust
image = { version = "0.25", features = ["png", "jpeg"] }
```

```rust
use eframe::egui::{self, ColorImage, TextureOptions};

// 从文件加载图片到 CPU 端
let img = image::open("photo.jpg").unwrap().to_rgba8();
let size = [img.width() as _, img.height() as _];
let pixels = img.into_raw();

// 创建 ColorImage
let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);

// 上传到 GPU，获取纹理
let texture = ui.ctx().load_texture(
    "my_photo",
    color_image,
    TextureOptions::LINEAR, // LINEAR = 平滑缩放, NEAREST = 像素风
);

// 显示图片
ui.image(texture.id(), texture.size_vec2());
```

### RetainedImage 简化版

```rust
use egui_extras::RetainedImage;

let img = RetainedImage::from_image_bytes("photo", &std::fs::read("photo.jpg").unwrap())
    .unwrap()
    .with_options(egui::TextureOptions::NEAREST); // 像素风纹理

ui.image(img.texture_id(ui.ctx()), img.size_vec2());
```

---

## 30.2 rfd — 原生文件对话框

```rust
rfd = "0.15"
```

```rust
fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // 打开文件
    if self.open_file_requested {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg"])
            .pick_file()
        {
            self.load_image_from_path(&path);
        }
        self.open_file_requested = false;
    }
}

fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    if ui.button("打开图片").clicked() {
        self.open_file_requested = true;  // 设置标志，在 logic 中处理
    }
}
```

---

## 30.3 log / env\_logger 调试日志

```rust
log = "0.4"
env_logger = "0.11"
```

```rust
fn main() {
    env_logger::init(); // 初始化
    log::info!("应用启动");
    // eframe::run_native(...)
}

// 在应用中任意位置
fn logic(&mut self, ...) {
    log::debug!("帧逻辑: count={}", self.count);
}
```

```rust
# 运行
RUST_LOG=debug cargo run
```

---

## 30.4 chrono — 时间格式化

```rust
chrono = "0.4"
```

```rust
use chrono::Local;

fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    let now = Local::now();
    ui.label(format!("当前时间: {}", now.format("%Y-%m-%d %H:%M:%S")));
}
```

---

## 本章小结

* `image` + `ctx.load_texture()` → 显示图片
* `rfd::FileDialog` → 原生文件打开/保存对话框
* `log` + `env_logger` → 调试输出
* `chrono` → 时间格式化和显示

**下一章**：tokio 与 egui —— 异步编程入门。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("第三方crate集成", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { time_str: String, log_output: String }

impl Default for MyApp { fn default() -> Self { Self { time_str: String::new(), log_output: String::new() } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 使用 chrono 更新时间
        self.time_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("第三方 crate 集成演示");

        ui.label(format!("当前时间: {}", self.time_str));
        ui.separator();

        // rfd 文件对话框
        if ui.button("打开文件对话框").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                self.log_output = format!("选择了: {}", path.display());
            }
        }

        if ui.button("保存文件对话框").clicked() {
            if let Some(path) = rfd::FileDialog::new().save_file() {
                self.log_output = format!("保存到: {}", path.display());
            }
        }

        ui.separator();
        ui.label(format!("日志: {}", self.log_output));

        ui.separator();
        ui.label("提示: 日志输出可通过 RUST_LOG=debug cargo run 查看");
        log::info!("UI 已渲染");
    }
}
```

---

第八部分：生态集成

﻿# 31 · tokio 与 egui

本章针对不熟悉异步编程的读者——提供足以在 egui 中使用的**最小必要知识**。

---

## 31.1 为什么 GUI 需要异步？

网络请求、文件 I/O、长计算——**不能阻塞 UI 线程**，否则界面卡死。

---

## 31.2 tokio 极小入门

```rust
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
```

```rust
// #[tokio::main] 创建一个异步运行时
#[tokio::main]
async fn main() -> eframe::Result<()> {
    eframe::run_native(/ ... /)
}

// async fn：异步函数
async fn fetch_data(url: &str) -> Result<String, reqwest::Error> {
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}
```

关键：`.await` 是"等待点"——在此期间 tokio 可以处理其他任务。

---

## 31.3 推荐方案：mpsc 通道

不需要引入额外 crate——标准库 `std::sync::mpsc` 足够：

```rust
use std::sync::mpsc;

enum AsyncResult {
    Data(String),
    Error(String),
}

struct MyApp {
    async_rx: mpsc::Receiver<AsyncResult>,
    response_text: String,
    loading: bool,
    request_tx: mpsc::Sender<String>,
}

impl MyApp {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        // 启动后台线程处理网络请求
        std::thread::spawn(move || {
            // 这里可以使用阻塞版本的 reqwest
        });
        Self {
            async_rx: rx,
            request_tx: tx,
            response_text: String::new(),
            loading: false,
        }
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 轮询异步结果（非阻塞）
        if let Ok(result) = self.async_rx.try_recv() {
            match result {
                AsyncResult::Data(text) => {
                    self.response_text = text;
                    self.loading = false;
                }
                AsyncResult::Error(e) => {
                    self.response_text = format!("错误: {}", e);
                    self.loading = false;
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("输入 URL")
                        .desired_width(300.0),
                );
                if ui.button("获取").clicked() && !self.loading {
                    let url = self.url.clone();
                    let tx = self.request_tx.clone();
                    self.loading = true;
                    std::thread::spawn(move || {
                        match reqwest::blocking::get(&url) {
                            Ok(resp) => {
                                let text = resp.text().unwrap_or_default();
                                tx.send(AsyncResult::Data(text)).ok();
                            }
                            Err(e) => {
                                tx.send(AsyncResult::Error(e.to_string())).ok();
                            }
                        }
                    });
                }
            });

            if self.loading {
                ui.spinner(); // 加载动画
            }
            ui.label(&self.response_text);
        });
    }
}
```

---

## 31.4 备选方案

|  |  |  |
| --- | --- | --- |
| 方案 | 复杂度 | 适用 |

|  |  |  |
| --- | --- | --- |
| `std::thread::spawn` + `mpsc` | 低 | 绝大多数情况（推荐） |
| `tokio::spawn` + `tokio::sync::mpsc` | 中 | 需要真正的异步 I/O |
| `poll_promise` crate | 低 | 简单的一次性异步任务 |

---

## 本章小结

* 异步 = 不阻塞 UI 线程
* 推荐 `std::thread::spawn` + `mpsc::channel` + `rx.try_recv()` 轮询
* `ui.spinner()` 显示加载状态
* 不要将 `ui()` 标记为 `async fn`

**下一章**：动画与过渡效果。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;
use std::sync::mpsc;

fn main() -> eframe::Result<()> {
    eframe::run_native("tokio与egui", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::new()))))
}

#[allow(dead_code)]
enum AsyncResult { Data(String), Error(String) }

struct MyApp { rx: mpsc::Receiver<AsyncResult>, tx: mpsc::Sender<String>, response: String, loading: bool, url: String }

impl MyApp {
    fn new() -> Self {
        let (_tx_resp, rx) = mpsc::channel();
        let (tx_req, _rx_req) = mpsc::channel::<String>();
        Self { rx, tx: tx_req, response: String::new(), loading: false, url: String::new() }
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(result) = self.rx.try_recv() {
            match result {
                AsyncResult::Data(text) => { self.response = text; self.loading = false; }
                AsyncResult::Error(e) => { self.response = format!("错误: {}", e); self.loading = false; }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("异步请求演示");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.url).hint_text("输入URL").desired_width(250.0));
            if ui.button("发送").clicked() && !self.loading {
                self.loading = true;
                self.response.clear();
                let _tx = self.tx.clone();
                let _url = self.url.clone();
                std::thread::spawn(move || { /* 用 _tx.send(AsyncResult::Data(...)) 发送结果 */ });
            }
        });

        if self.loading { ui.spinner(); }
        if !self.response.is_empty() { ui.label(&self.response); }
        ui.label("提示: 此示例仅展示 mpsc 通道模式骨架");
    }
}
```