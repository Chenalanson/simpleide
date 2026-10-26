# 高级特性


---

第九部分：高级特性

# 32 · 动画与过渡效果

---

## 32.1 `ctx.animate_bool()` — 布尔动画

```rust
// 返回值: f32 (0.0 → 1.0)
let anim = ctx.animate_bool(
    egui::Id::new("panel_open"),
    self.panel_open,  // 当前目标值
);

// 用法: 透明度渐变
let alpha = anim;  // 0.0 ~ 1.0
ui.add_enabled(anim > 0.5, egui::Label::new("面板内容"));
```

内部自动调用 `ctx.request_repaint()` 维持动画。

---

## 32.2 `ctx.animate_value_with_time()` — 数值动画

```rust
// ⚠️ 只接受 f32！
let animated_val = ctx.animate_value_with_time(
    egui::Id::new("position"),
    self.target_x,    // f32 目标值
    0.5,              // 动画持续时间（秒）
);

// 用于 Widget 水平移动
let x_offset = animated_val;
```

---

## 32.3 手动插值动画（自定义类型）

`animate_value` 只支持 `f32`。对于颜色或位置，需要手动 `lerp`：

```rust
// 颜色渐变
fn lerp_color(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgba_unmultiplied(
        lerp_i32(from.r() as i32, to.r() as i32, t) as u8,
        lerp_i32(from.g() as i32, to.g() as i32, t) as u8,
        lerp_i32(from.b() as i32, to.b() as i32, t) as u8,
        lerp_i32(from.a() as i32, to.a() as i32, t) as u8,
    )
}

// 位置移动
fn lerp_pos(from: egui::Pos2, to: egui::Pos2, t: f32) -> egui::Pos2 {
    egui::pos2(
        egui::emath::lerp(from.x..=to.x, t),
        egui::emath::lerp(from.y..=to.y, t),
    )
}
```

---

## 32.4 使用 `request_repaint_after` 驱动动画

```rust
// 手动动画循环
fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let now = std::time::Instant::now();
    let elapsed = (now - self.anim_start).as_secs_f32();
    self.anim_t = (elapsed / 2.0).min(1.0); // 2秒动画

    if self.anim_t < 1.0 {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        // ~60 FPS，降低 CPU 占用
    }
}
```

---

## 32.5 进阶：社区 crate `egui_animation`

如需 easing 函数（ease-in-out、elastic、bounce），可使用第三方：

```rust
// egui_animation crate
use egui_animation::{
    animate_bool_eased,
    ease_in_out_quad,  // 缓入缓出
};

let t = animate_bool_eased(
    ctx,
    egui::Id::new("my_anim"),
    self.visible,
    ease_in_out_quad,
    0.5,  // 动画时长
);
```

---

## 本章小结

* `ctx.animate_bool(id, value)` — 返回 f32 (0→1)
* `ctx.animate_value_with_time(id, val, time)` — f32 值动画
* 颜色/位置动画需手动 `lerp`
* `ctx.request_repaint_after(duration)` — 控制重绘频率
* 复杂 easing 用 `egui_animation` 社区 crate

**下一章**：拖拽与放置。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("动画", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { panel_open: bool, target_val: f32 }

impl Default for MyApp { fn default() -> Self { Self { panel_open: false, target_val: 0.0 } } }

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("动画演示");

        // animate_bool: 面板开关动画
        let anim = ui.ctx().animate_bool(egui::Id::new("panel"), self.panel_open);
        if ui.checkbox(&mut self.panel_open, "展开面板").changed() || anim > 0.0 && anim < 1.0 {
            // 动画进行中，持续重绘
        }

        ui.add_enabled(anim > 0.1, egui::Label::new(
            egui::RichText::new(format!("面板内容 (透明度: {:.2})", anim)).color(
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, (anim * 255.0) as u8)
            )
        ));

        ui.separator();

        // animate_value_with_time: 数值动画
        ui.add(egui::Slider::new(&mut self.target_val, 0.0..=100.0).text("目标值"));
        let animated = ui.ctx().animate_value_with_time(
            egui::Id::new("val_anim"), self.target_val, 0.3
        );
        ui.label(format!("平滑值: {:.1}", animated));
    }
}
```

---

第九部分：高级特性

﻿# 33 · 拖拽与放置

---

## 33.1 Widget 级别的 Drag Source

```rust
// 标记 Widget 为可拖拽
let response = ui.button("拖拽我");
let drag_id = egui::Id::new("my_drag");

if response.drag_started() {
    // 拖拽开始
    ui.memory_mut(|mem| mem.data.insert_temp(drag_id, "my_data"));
}

// 检查是否有拖入
if ui.label("放在这里").dropped_file() {
    // ...
}

// 拖拽预览
if ui.ctx().is_being_dragged(drag_id) {
    // 显示拖拽中的视觉反馈
}
```

---

## 33.2 文件拖入

从系统文件管理器拖文件到 egui 窗口：

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // 检测是否有文件正悬停在窗口上
    if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
        // 显示"松开以打开"的提示
        ui.label("释放文件以打开");
    }

    // 检测文件已放下
    if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        for file in &files {
            if let Some(path) = &file.path {
                self.load_file(path);
            }
        }
    }
}
```

---

## 33.3 可排序列表

```rust
for i in 0..self.items.len() {
    let response = ui.label(&self.items[i]);

    // 声明此标签可拖拽
    let drag_id = egui::Id::new("item").with(i);
    if response.drag_started() {
        ui.memory_mut(|mem| mem.data.insert_temp(drag_id, i));
    }

    // 检测拖拽到此位置上方/下方
    if response.hovered() && ui.ctx().is_being_dragged(drag_id) {
        // 插入指示线
        let rect = response.rect;
        ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(2.0, egui::Color32::RED));
    }
}
```

---

## 本章小结

* `response.drag_started()` / `dragged()` / `drag_stopped()` — 拖拽生命周期
* `ctx.input(|i| i.raw.dropped_files)` — 文件拖入
* 可排序列表 = drag\_id + hover 检测 + 视觉指示

**下一章**：输入处理深入。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("拖拽与放置", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { dropped_files: Vec<String>, items: Vec<String> }

impl Default for MyApp {
    fn default() -> Self { Self { dropped_files: vec![], items: (0..6).map(|i| format!("项目 {}", i)).collect() } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let files = ctx.input(|i| i.raw.dropped_files.clone());
            for f in &files {
                if let Some(path) = &f.path {
                    self.dropped_files.push(path.display().to_string());
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("拖拽与放置演示");
        ui.label("从文件管理器拖文件到此处");

        if !self.dropped_files.is_empty() {
            ui.separator();
            ui.heading("已拖入的文件:");
            for f in &self.dropped_files { ui.label(f); }
            if ui.button("清除列表").clicked() { self.dropped_files.clear(); }
        }

        ui.separator();
        ui.label("文件悬停中...");

        ui.separator();
        ui.heading("可排序列表:");
        for (_i, item) in self.items.clone().iter().enumerate() {
            let resp = ui.label(item);
            if resp.drag_started() { /* 开始拖拽 */ }
            if resp.dragged() { /* 拖拽中 */ }
            if resp.drag_stopped() { /* 拖拽结束 */ }
        }
    }
}
```

---

第九部分：高级特性

﻿# 34 · 输入处理深入

---

## 34.1 `ctx.input()` API 全景

```rust
ctx.input(|i| {
    // 键盘
    i.key_down(egui::Key::A)         // 按键正在按下
    i.key_pressed(egui::Key::Enter)  // 本帧刚按下
    i.key_released(egui::Key::Escape)// 本帧刚释放
    i.modifiers                       // Ctrl/Shift/Alt/Win

    // 鼠标
    i.pointer                         // 指针状态
    i.pointer.button_down(PointerButton::Primary)
    i.pointer.latest_pos()              // 当前鼠标位置
    i.pointer.delta()                 // 鼠标位移

    // 其他
    i.scroll_delta                     // 滚轮增量
    i.time                             // egui 内部时钟
    i.viewport_rect                    // 视口矩形
});
```

---

## 34.2 键盘事件 vs 字符输入

```rust
// ❌ 错误：用 Key::A 检测输入 'a'
if i.key_pressed(egui::Key::A) {
    // 只能检测到英文键盘的 A
}

// ✅ 正确：用 Event::Text 检测字符输入
for event in &i.events {
    if let egui::Event::Text(text) = event {
        // text 是用户输入的字符（支持 IME 组合）
        self.insert_text(text);
    }
}
```

**为什么重要**？`Event::Text` 支持 IME（输入法），`Key::A` 不支持。对于中文输入，`Event::Text` 是正确的选择。

---

## 34.3 消费输入事件

```rust
// 消费快捷键，防止被其他 Widget 处理
ctx.input_mut(|i| {
    if i.consume_key(egui::Modifiers::CTRL, egui::Key::S) {
        // Ctrl+S 被消费，TextEdit 不会收到 'S'
    }
});
```

---

## 34.4 IME 与中文输入

egui 0.34 对 IME（输入法）的支持通过 `ImeEvent`：

```rust
// TextEdit 内置了 IME 支持——无需额外处理
ui.add(egui::TextEdit::singleline(&mut text));

// 中文输入法的候选窗口由操作系统管理
// egui 负责接收最终的字符
```

---

## 本章小结

* `ctx.input(|i| {...})` — 只读输入
* `ctx.input_mut(|i| {...})` — 消费事件
* `Event::Text` — 支持 IME 的字符输入（优于 Key 检测）
* `consume_key` — 防止事件冒泡
* TextEdit 内置 IME 支持——中文输入开箱可用

**下一章**：多窗口应用。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("输入处理深入", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { text: String, ctrl_s_pressed: bool }

impl Default for MyApp { fn default() -> Self { Self { text: String::new(), ctrl_s_pressed: false } } }

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::S) {
                self.ctrl_s_pressed = true;
            }
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("输入处理演示");

        let mouse_pos = ui.ctx().input(|i| i.pointer.latest_pos());
        ui.label(format!("鼠标位置: {:?}", mouse_pos));
        ui.label(format!("Ctrl+S: {}", if self.ctrl_s_pressed { "已按下" } else { "未按下" }));
        if self.ctrl_s_pressed { self.ctrl_s_pressed = false; }

        ui.separator();
        ui.label("文本输入（支持 IME 中文输入）:");
        ui.add(egui::TextEdit::singleline(&mut self.text).hint_text("在此输入中文...").desired_width(250.0));
    }
}
```

---

第九部分：高级特性

﻿# 35 · 多窗口应用

egui 0.34 支持多个原生操作系统窗口（通过 Viewport 系统）。

---

## 35.1 Viewport 基础概念

```rust
// 标准方式：每帧声明子窗口
ctx.show_viewport_deferred(
    viewport_id,                     // 唯一标识
    egui::ViewportBuilder::default()
        .with_title("工具窗口")
        .with_inner_size([300.0, 400.0]),
    move |ctx, class| {
        match class {
            egui::ViewportClass::Deferred => {
                // 独立窗口 — 使用 CentralPanel
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.label("工具窗口内容");
                });
            }
            egui::ViewportClass::Embedded => {
                // 回退 — 嵌入父窗口内的 Window
                egui::Window::new("工具").show_inside(ui, |ui| {
                    ui.label("工具窗口内容");
                });
            }
            _ => {}
        }
    },
);
```

---

## 35.2 窗口间通信

```rust
// 共享状态（通过 Arc）
use std::sync::{Arc, Mutex};

struct SharedState {
    color: egui::Color32,
}

// 主窗口
struct MyApp {
    shared: Arc<Mutex<SharedState>>,
}
```

---

## 35.3 Deferred vs Immediate

|  |  |  |  |
| --- | --- | --- | --- |
| 类型 | 创建方式 | 性能 | 通信复杂度 |

|  |  |  |  |
| --- | --- | --- | --- |
| Deferred | `show_viewport_deferred` | 高（独立重绘） | 高（需 Arc/Mutex） |
| Immediate | `show_viewport_immediate` | 低（所有窗口同步重绘） | 低（直接访问状态） |

---

## 本章小结

* `ctx.show_viewport_deferred(id, builder, callback)` — 创建子窗口
* `ViewportClass::Deferred` — 原生窗口
* `ViewportClass::Embedded` — 回退方案
* 窗口间通信需 `Arc<Mutex<T>>`
* 多窗口 API 仍为实验性

**下一章**：性能优化。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("多窗口", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { show_tool: bool }

impl Default for MyApp { fn default() -> Self { Self { show_tool: false } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("多窗口演示（Viewport）");

        // 在实际项目中，创建独立窗口的代码：
        // ui.ctx().show_viewport_deferred(
        //     egui::ViewportId::from_hash_of(&"tool_window"),
        //     egui::ViewportBuilder::default()
        //         .with_title("工具窗口")
        //         .with_inner_size([300.0, 250.0]),
        //     |ctx, _class| {
        //         egui::CentralPanel::default().show(ctx, |ui| {
        //             ui.label("独立工具窗口");
        //         });
        //     },
        // );

        if ui.button("打开工具面板").clicked() {
            self.show_tool = true;
        }

        let mut tool_open = self.show_tool;
        if tool_open {
            egui::Window::new("工具")
                .open(&mut tool_open)
                .resizable(true)
                .default_size([300.0, 250.0])
                .show(ui, |ui| {
                    ui.label("这是一个嵌入式工具窗口。");
                    ui.label("在支持多视口的平台上，可以变成独立窗口。");
                    if ui.button("关闭").clicked() {
                        // 由 open 标志处理
                    }
                });
        }
        if !tool_open { self.show_tool = false; }

        ui.separator();
        ui.label("eframe 多窗口相关 API:");
        ui.label("  • show_viewport_deferred — 创建独立视口（实验性）");
        ui.label("  • ViewportBuilder — 配置窗口大小/位置/标题");
        ui.label("  • ViewportId::from_hash_of — 生成视口 ID");
        ui.label("  • egui::Window — 单窗口内浮动（兼容所有平台）");
    }
}
```

---

第九部分：高级特性

# 36 · 性能优化

---

## 36.1 理解重绘成本

egui 的性能瓶颈通常是 **CPU**（UI 构建 + 布局 + tessellation），而非 GPU（渲染）。

---

## 36.2 `request_repaint()` 的合理使用

```rust
// ✅ 仅在需要时请求重绘
ctx.request_repaint();

// ✅ 延迟重绘（降低 CPU 占用）
ctx.request_repaint_after(std::time::Duration::from_millis(100));
```

---

## 36.3 `Sense` 的性能影响

```rust
// 便宜：仅 hover，不需点击
ui.interact(rect, id, egui::Sense::hover());

// 贵：需要检测点击
ui.interact(rect, id, egui::Sense::click());

// 更贵：需要追踪拖拽
ui.interact(rect, id, egui::Sense::drag());

// 最便宜：不需要交互
egui::Sense::focusable_noninteractive()
```

> **核心原则**：给不接收点击的区域用 `hover()`，给不可交互区域用 `focusable_noninteractive()`。

---

## 36.4 裁剪与可见性

```rust
// 只在可见区域绘制
painter.with_clip_rect(clip_rect);

// ScrollArea 自动裁剪可见内容
egui::ScrollArea::vertical().show(ui, |ui| {
    // ScrollArea 内自动裁剪
});

// 跳过不可见 Widget 的布局计算
ui.skip_ahead(n);
```

---

## 36.5 大数据集的优化策略

```rust
1. 分页：只渲染当前页 → 最简单的方案- 裁剪：只渲染可见行（手动管理 visible range）
- 懒渲染：只在用户滚到新区域时才构建 UI
```

---

## 36.6 Profiling 工具

```rust
# 启用 puffin profiler
eframe = { version = "0.34", features = ["puffin"] }
```

```rust
let native_options = eframe::NativeOptions {
    renderer: eframe::Renderer::Wgpu,
    wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
        // profiling 配置
        ..Default::default()
    },
    ..Default::default()
};
```

---

## 本章小结

* `request_repaint_after` — 降低动画帧率
* `Sense::hover()` — 给不需要点击的区域用便宜的 Sense
* 裁剪 + `skip_ahead` — 减少不必要的计算
* 分页/裁剪 — 解决大数据集渲染

**下一章**：常见陷阱与调试。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
﻿use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("性能优化", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { item_count: usize }

impl Default for MyApp { fn default() -> Self { Self { item_count: 100 } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("性能优化演示");
        ui.add(egui::Slider::new(&mut self.item_count, 1..=5000).text("列表项数"));

        ui.separator();
        // 大量 Widget 渲染——如无分层/裁剪会导致性能问题
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for i in 0..self.item_count.min(1000) {
                ui.label(format!("Item #{i}"));
            }
            if self.item_count > 1000 {
                ui.label(format!("... 还有 {} 项（已裁剪）", self.item_count - 1000));
            }
        });

        ui.separator();
        ui.label("提示: Sense::hover() 比 Sense::click() 更轻量");
        ui.label("不使用交互的区域应用 Sense::focusable_noninteractive()");
    }
}
```

---

第九部分：高级特性

﻿# 37 · 常见陷阱与调试

---

## 37.1 ID 冲突 — 头号陷阱

**症状**：两个同名 Widget 状态互串、窗口漂移、Checkbox 状态联动。

**解决**：

```rust
// ❌ 冲突
ui.add(egui::TextEdit::singleline(&mut self.a));
ui.add(egui::TextEdit::singleline(&mut self.b));

// ✅ 解决方案
ui.push_id("a", |ui| { ui.add(egui::TextEdit::singleline(&mut self.a)); });
ui.push_id("b", |ui| { ui.add(egui::TextEdit::singleline(&mut self.b)); });

// ✅ 或：显式 id_source
ui.add(egui::TextEdit::singleline(&mut self.a).id_salt("a"));
```

---

## 37.2 状态意外丢失

**原因**：在 `ui()` 中用局部变量初始化 Widget 的值：

```rust
// ❌ 值在每帧重置
let mut value = 0.0;
ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
// value 每帧都重置为 0.0！
```

**解决**：用 `MyApp` 字段或 `Memory`。

---

## 37.3 布局不符合预期

**常见原因**：

* 忘了包裹 `ScrollArea` → 内容被裁剪
* `allocate_space` 返回的 rect 理解错误
* `ui.set_min_size()` 和 `ui.set_max_size()` 的陷阱

---

## 37.4 调试工具

```rust
// 屏幕叠加文本（调试用）
ctx.debug_text(format!("帧: {}", self.frame_count));

// 隐藏部分 UI 排查问题
ui.set_visible(false);
ui.button("这不会显示");
ui.set_visible(true);

// 激活 egui 的隐藏调试面板
egui::debug_ui(ctx); // 显示 Memory 内容、Shapes 等信息
```

---

## 37.5 参考 egui\_demo\_app

```rust
cargo run -p egui_demo_app
```

这是官方演示应用——**学习 egui 的最佳参考**。所有 Widget 都有源码对照。

---

## 37.6 常见编译错误

|  |  |  |
| --- | --- | --- |
| 错误 | 原因 | 解决 |

|  |  |  |
| --- | --- | --- |
| `does not live long enough` | 闭包中的生命周期 | 用 `move` 或 `clone` |
| `trait bound not satisfied` | 缺少 trait 实现 | 添加 `#[derive(...)]` |
| `cannot borrow as mutable` | 借用冲突 | `logic` 和 `ui` 都获取 `&mut self`——但它们是先后调用的，不会冲突。如果在 `ui` 内部需要可变借用，用 `RefCell` |

---

## 37.7 辅助功能（AccessKit）

egui 0.34 通过 AccessKit 支持屏幕阅读器：

```rust
// eframe 默认启用 accesskit feature
// 每个 Widget 自动注册到无障碍树

// 获取当前无障碍树
ctx.accesskit_handle();

// 使用 kittest（基于 AccessKit）查找 Widget
// 见第 43 章
```

⚠️ 无障碍功能在 web 后端上仍是实验性的。

---

## 本章小结

* ID 冲突 = `push_id` / `id_source` 解决
* 状态丢失 = 用 MyApp 字段而非局部变量
* `ctx.debug_text()` / `egui::debug_ui()` — 内建调试工具
* `egui_demo_app` — 最佳学习参考

**下一章**：Immediate Mode 设计哲学。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("陷阱与调试", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { name_a: String, name_b: String, frame: u64 }

impl Default for MyApp { fn default() -> Self { Self { name_a: String::new(), name_b: String::new(), frame: 0 } } }

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame = ctx.input(|i| i.time as u64);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("常见陷阱演示");

        // ✅ 修复 ID 冲突（也可用 ScrollArea::id_salt、Widget::id_source 等方案）
        ui.label("两个输入框——使用 push_id 避免冲突:");
        ui.push_id("a", |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.name_a).hint_text("输入 A"));
        });
        ui.push_id("b", |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.name_b).hint_text("输入 B"));
        });

        ui.label(format!("A: {}, B: {}", self.name_a, self.name_b));

        ui.separator();

        // 调试叠加文本
        ui.ctx().debug_text(format!("帧: {}, A={}, B={}", self.frame, self.name_a, self.name_b));

        ui.label("按下 Ctrl+Shift+D 查看 egui 内置调试面板");
    }
}
```