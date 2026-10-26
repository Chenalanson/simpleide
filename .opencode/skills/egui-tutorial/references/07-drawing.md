# 自定义绘制


---

第七部分：自定义绘制

﻿# 24 · Painter 与 Shape API

自定义绘制的入口——`Painter` 和 `Shape`。

---

## 24.1 获取 Painter

```rust
// 在 ui() 中获得 Painter
let painter = ui.painter();

// 在指定矩形中使用 Painter
let painter = ui.painter_at(rect);
```

`Painter` 的生命周期是单帧——每帧重新获取。

---

## 24.2 Shape 枚举全览

`Shape` 定义了所有可绘制的图形原语（定义在 `epaint` crate，通过 `egui` re-export）：

```rust
use eframe::egui::{self, epaint};

// 矩形
let rect_shape = epaint::RectShape::new(
    rect,                                    // Rect
    egui::Rounding::same(4.0),              // 圆角
    egui::Color32::RED,                      // 填充色
    epaint::Stroke::new(2.0, egui::Color32::BLACK), // 描边
);
painter.add(egui::Shape::Rect(rect_shape));

// 圆形
let circle = egui::Shape::Circle(epaint::CircleShape {
    center: egui::pos2(100.0, 100.0),
    radius: 30.0,
    fill: egui::Color32::GREEN,
    stroke: epaint::Stroke::new(1.0, egui::Color32::DARK_GREEN),
});
painter.add(circle);

// 线段
painter.line_segment(
    [egui::pos2(10.0, 10.0), egui::pos2(100.0, 100.0)],
    epaint::Stroke::new(2.0, egui::Color32::WHITE),
);

// 路径（多段线）
let points = vec![
    egui::pos2(50.0, 50.0),
    egui::pos2(150.0, 80.0),
    egui::pos2(100.0, 150.0),
];
painter.add(egui::Shape::Path(epaint::PathShape {
    points,
    closed: false,
    fill: egui::Color32::TRANSPARENT,
    stroke: epaint::PathStroke::new(2.0, egui::Color32::YELLOW),
}));

// 文本
painter.text(
    egui::pos2(20.0, 20.0),
    egui::Align2::LEFT_TOP,
    "自定义文本",
    egui::FontId::proportional(16.0),
    egui::Color32::WHITE,
);

// 贝塞尔曲线
painter.add(egui::Shape::CubicBezier(epaint::CubicBezierShape {
    points: [pos2(10.0, 100.0), pos2(60.0, 20.0), pos2(140.0, 180.0), pos2(190.0, 100.0)],
    closed: false,
    fill: egui::Color32::TRANSPARENT,
    stroke: epaint::PathStroke::new(2.0, egui::Color32::CYAN),
}));

// 闭合填充路径
painter.add(egui::Shape::convex_polygon(
    vec![pos2(200.0, 50.0), pos2(300.0, 50.0), pos2(250.0, 150.0)],
    egui::Color32::from_rgba_unmultiplied(255, 100, 100, 128),
    epaint::Stroke::new(1.0, egui::Color32::RED),
));
```

---

## 24.3 坐标系与裁剪

```rust
// 裁剪区域（超出部分不绘制）
painter.with_clip_rect(clip_rect);

// 坐标变换
let mut painter = ui.painter();
painter.translate(egui::vec2(50.0, 50.0)); // 平移原点
painter.rotate(std::f32::consts::PI / 6.0, egui::pos2(100.0, 100.0)); // 旋转
```

---

## 24.4 颜色类型

|  |  |
| --- | --- |
| 类型 | 说明 |

|  |  |
| --- | --- |
| `Color32` | 最常用 — `Color32::RED`, `Color32::from_rgb(r,g,b)`, `Color32::from_rgba_unmultiplied(r,g,b,a)` |
| `Rgba` | 浮点颜色 (0.0~1.0) |
| `Color32::TRANSPARENT` | 透明 |
| `Stroke` | 描边 = 宽度 + 颜色 |
| `PathStroke` | 路径描边 = `Stroke` 增强版 |

---

## 24.5 绘制示例：形状画廊

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        // 为绘制分配空间
        let (resp, painter) =
            ui.allocate_painter(egui::vec2(400.0, 300.0), egui::Sense::hover());

        let rect = resp.rect;

        // 绘制各种形状
        // ... (上述 Shape 示例)
    });
}
```

---

## 本章小结

* `ui.painter()` 获取当前帧绘制器
* `Shape` = 绘制原语（Rect、Circle、Line、Path、Text、Bezier、Mesh）
* `painter.add(Shape)` 添加绘制命令
* `painter.with_clip_rect()` 设置裁剪
* 坐标变换：`translate()`、`rotate()`

**下一章**：自定义 Widget 开发。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("自定义绘制", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("Painter 与 Shape API 演示");

        let (resp, painter) = ui.allocate_painter(egui::vec2(300.0, 150.0), egui::Sense::hover());
        let r = resp.rect;

        // 绘制矩形 — 通过 Shape 添加到 painter
        let shape = egui::Shape::rect_filled(
            egui::Rect::from_min_size(r.min + egui::vec2(10.0, 10.0), egui::vec2(80.0, 40.0)),
            4.0,  // 圆角半径
            egui::Color32::RED,
        );
        painter.add(shape);

        // 绘制文字
        painter.text(
            r.min + egui::vec2(10.0, 70.0),
            egui::Align2::LEFT_TOP,
            "由 Painter 直接绘制的文本",
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );

        // 绘制线段
        let line_shape = egui::Shape::line_segment(
            [r.min + egui::vec2(10.0, 110.0), r.min + egui::vec2(200.0, 110.0)],
            egui::Stroke::new(2.0, egui::Color32::GREEN),
        );
        painter.add(line_shape);

        if resp.hovered() { ui.label("鼠标在绘制区域中"); }
    }
}
```

---

第七部分：自定义绘制

# 25 · 自定义 Widget 开发

从使用内置 Widget 到创造自己的控件。

---

## 25.1 自定义 Widget 的完整流程

```rust
① allocate_ui 获取空间
② 获取 Painter
③ 绘制外观
④ 处理交互（hit test）
⑤ 返回 Response
```

---

## 25.2 案例：圆形按钮

```rust
pub fn circle_button(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    radius: f32,
    color: egui::Color32,
) -> egui::Response {
    let rect = egui::Rect::from_center_size(
        center,
        egui::vec2(radius  2.0, radius  2.0),
    );

    // ① 分配空间
    let (id, rect) = ui.allocate_space(rect.size());
    let response = ui.interact(rect, id, egui::Sense::click());

    // ② 绘制
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();

    painter.circle(
        center,
        radius,
        if response.hovered() {
            visuals.hovered.bg_fill
        } else {
            color
        },
        egui::Stroke::NONE,
    );

    // ③ 返回 Response
    response
}
```

---

## 25.3 实现 `Widget` trait

让自定义组件可用 `ui.add()` 调用：

```rust
struct CircleButton {
    center: egui::Pos2,
    radius: f32,
    color: egui::Color32,
}

impl egui::Widget for CircleButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        circle_button(ui, self.center, self.radius, self.color)
    }
}

// 使用
ui.add(CircleButton {
    center: egui::pos2(100.0, 100.0),
    radius: 25.0,
    color: egui::Color32::RED,
});
```

---

## 本章小结

* 5 步构建自定义 Widget
* `ui.interact(rect, id, sense)` 处理交互
* `ui.style().interact(&response)` 获取状态相关样式
* `impl Widget` 实现 `ui.add()` 集成

**下一章**：交互式自定义绘制——画布缩放、拖拽、绘图板。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("自定义Widget", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { rating: u8 }

impl Default for MyApp { fn default() -> Self { Self { rating: 3 } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("自定义星级评分组件");

        // 星级评分
        ui.horizontal(|ui| {
            for i in 0..5 {
                let (resp, painter) = ui.allocate_painter(egui::vec2(30.0, 30.0), egui::Sense::click());
                let filled = i < self.rating;
                let color = if filled { egui::Color32::YELLOW } else { egui::Color32::DARK_GRAY };
                painter.circle(resp.rect.center(), 12.0, color, egui::Stroke::new(1.0, egui::Color32::WHITE));
                if resp.clicked() { self.rating = i + 1; }
            }
        });

        ui.label(format!("评分: {}/5", self.rating));

        if ui.button("重置").clicked() { self.rating = 0; }
    }
}
```

---

第七部分：自定义绘制

﻿# 26 · 交互式自定义绘制

---

## 26.1 拖拽检测

```rust
let response = ui.interact(rect, id, egui::Sense::drag());

if response.dragged() {
    // 正在拖拽中
    let delta = response.drag_delta(); // 本帧位移
}
if response.drag_started() {
    // 拖拽刚启动
}
if response.drag_stopped() {
    // 拖拽结束
}
```

---

## 26.2 画布缩放与平移

```rust
struct Canvas {
    offset: egui::Vec2,    // 平移偏移
    scale: f32,            // 缩放比例
    strokes: Vec<Vec<egui::Pos2>>, // 笔画数据
}

fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let (resp, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::drag());

        // 滚轮缩放
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        self.scale = (1.0 + scroll  0.01).max(0.1).min(10.0);

        // 拖拽平移
        if resp.dragged_by(egui::PointerButton::Secondary) {
            self.offset += resp.drag_delta();
        }

        // 应用变换
        let to_screen = egui::emath::TSTransform::from_translation(self.offset)

* egui::emath::TSTransform::from_scaling(self.scale);

// 绘制笔画（变换后的点）
        for stroke in &self.strokes {
            let screen_points: Vec<egui::Pos2> = stroke
                .iter()
                .map(|p| to_screen  p)
                .collect();
            painter.add(egui::Shape::line(screen_points, egui::Stroke::new(...)));
        }

        // 左键拖拽添加笔画
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer_pos) = resp.hover_pos() {
                let canvas_pos = to_screen.inverse() * pointer_pos;
                self.strokes.last_mut().unwrap().push(canvas_pos);
            }
        }
    });
}
```

---

## 本章小结

* `response.dragged_by(button)` — 检测特定按键拖拽
* `response.drag_delta()` — 拖拽增量
* `TSTransform` — 组合平移/缩放/旋转变换

**下一章**：`egui_extras` — 额外的 Widget。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("简易绘图板", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { strokes: Vec<Vec<egui::Pos2>>, current: Vec<egui::Pos2>, color: egui::Color32 }

impl Default for MyApp { fn default() -> Self { Self { strokes: vec![], current: vec![], color: egui::Color32::WHITE } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("简易绘图板");
        ui.horizontal(|ui| {
            if ui.button("清除").clicked() { self.strokes.clear(); }
            ui.label("颜色:");
            for &c in &[egui::Color32::WHITE, egui::Color32::RED, egui::Color32::GREEN, egui::Color32::BLUE, egui::Color32::YELLOW] {
                if ui.add(egui::Button::new("●").fill(c).small()).clicked() { self.color = c; }
            }
        });

        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::drag());

        // 绘制历史笔画
        for stroke in &self.strokes {
            if stroke.len() >= 2 {
                for w in stroke.windows(2) {
                    painter.line_segment([w[0], w[1]], egui::Stroke::new(2.0, self.color));
                }
            }
        }

        // 拖拽绘制
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = resp.hover_pos() {
                self.current.push(pos);
                if self.current.len() >= 2 {
                    for w in self.current.windows(2) {
                        painter.line_segment([w[0], w[1]], egui::Stroke::new(2.0, self.color));
                    }
                }
            }
        } else if !self.current.is_empty() {
            self.strokes.push(std::mem::take(&mut self.current));
        }
    }
}
```