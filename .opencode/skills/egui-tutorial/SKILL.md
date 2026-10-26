---
name: egui-tutorial
description: 基于《EGUI 全面教程》(egui 0.34.3) 的完整中文参考。当用户需要用 Rust/egui/eframe 构建 GUI 桌面应用、查询 egui API 用法、学习即时模式 GUI 概念、调试 egui 布局/状态/样式问题，或需要 Widget/布局/状态管理/样式主题/自定义绘制/生态集成/高级特性的代码示例时使用。覆盖入门到工程实践全链路。
---

# EGUI 全面教程参考（egui 0.34.3）

> 来源：https://deapskf.github.io/AI-egui-tutorial/
> 版本：egui/eframe 0.34.3 · Rust 2024 Edition

## 核心心智模型（必须先理解）

egui 是 **Immediate Mode（即时模式）GUI**：
- 每一帧都重新"描述"整个 UI，没有持久控件树；
- 状态存在你自己的 Rust 变量里（`App` 结构体字段），不在控件里；
- 没有数据绑定/信号槽，直接读写变量即可；
- `ui.button("OK").clicked()` 返回 `Response`，每帧重新求值。

## 最小骨架

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "app",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

#[derive(Default)]
struct App { count: i32 }

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("点击 +1").clicked() {
                self.count += 1;
            }
            ui.label(format!("计数: {}", self.count));
        });
    }
}
```

## 常用 API 速查

### 控件

```rust
ui.label("文本");
ui.heading("标题");
ui.button("按钮").clicked();
ui.text_edit_singleline(&mut s);
ui.checkbox(&mut b, "选项");
ui.add(egui::Slider::new(&mut v, 0.0..=100.0).text("滑条"));
ui.add(egui::DragValue::new(&mut v));
egui::ComboBox::from_label("选择").show_ui(ui, |ui| { /* 选项 */ });
ui.hyperlink_to("链接", "https://...");
ui.separator();
ui.spinner();
```

### 布局

```rust
ui.horizontal(|ui| { /* 水平排列 */ });
ui.vertical(|ui| { /* 垂直排列 */ });
egui::Grid::new("grid_id").show(ui, |ui| { ui.label("单元格"); ui.end_row(); });
egui::ScrollArea::vertical().show(ui, |ui| { /* 可滚动内容 */ });
egui::CentralPanel::default().show(ctx, |ui| { /* 中央面板 */ });
egui::TopBottomPanel::top("top").show(ctx, |ui| { /* 顶部栏 */ });
egui::SidePanel::left("left").show(ctx, |ui| { /* 侧边栏 */ });
```

### 窗口与弹窗

```rust
egui::Window::new("浮动窗口").show(ctx, |ui| { ui.label("内容"); });
widget.context_menu(|ui| { ui.button("右键菜单项"); });
```

### 输入处理

```rust
ctx.input(|i| {
    if i.key_pressed(egui::Key::Escape) { /* Esc */ }
    if i.modifiers.ctrl && i.key_pressed(egui::Key::S) { /* Ctrl+S */ }
    for event in &i.events {
        if let egui::Event::Text(t) = event { /* 字符输入（支持 IME 中文） */ }
    }
});
```

### 样式与主题

```rust
// 暗色/亮色
ctx.style_mut(|s| s.visuals = egui::Visuals::dark());
ctx.style_mut(|s| s.visuals = egui::Visuals::light());

// 自定义配色
let mut v = egui::Visuals::dark();
v.widgets.hovered.bg_fill = egui::Color32::from_rgb(30, 60, 120);
v.selection.bg_fill = egui::Color32::from_rgb(40, 80, 180);
ctx.set_visuals(v);
```

### 持久化

```rust
// 在 App::new 里从 storage 读
fn new(cc: &eframe::CreationContext<'_>) -> Self {
    if let Some(storage) = cc.storage {
        eframe::get_value(storage, "app_state").unwrap_or_default()
    } else { Self::default() }
}
// 保存
fn save(&mut self, storage: &mut dyn eframe::Storage) {
    eframe::set_value(storage, "app_state", self);
}
```

## 参考文件导航

按主题深入时读取对应文件：

| 文件 | 内容 |
|---|---|
| `references/01-getting-started.md` | egui 概念、App trait、帧循环、项目结构 |
| `references/02-widgets.md` | Label/Button/TextEdit/Checkbox/Slider/ComboBox 全部基础控件 |
| `references/03-layout.md` | 布局模型、horizontal/vertical、Grid、ScrollArea、面板系统 |
| `references/04-windows.md` | 浮动窗口、右键菜单、弹窗、Area |
| `references/05-state.md` | Id 系统、Memory、logic/ui 分离、serde 持久化 |
| `references/06-styling.md` | Style/Visuals、中文字体配置、明暗主题切换 |
| `references/07-drawing.md` | Painter/Shape 绘图原语、自定义 Widget、交互画布 |
| `references/08-ecosystem.md` | egui_extras(Table/DatePicker)、egui_plot 图表、egui_phosphor 图标、tokio 集成 |
| `references/09-advanced.md` | 动画、拖拽、输入处理(IME)、多窗口、性能优化、常见陷阱 |
| `references/10-internals.md` | Immediate Mode 哲学、egui 架构剖析、手写 IMGUI、渲染管线 |
| `references/11-engineering.md` | 测试(kittest)、打包分发(WebAssembly)、bevy_egui |
| `references/12-appendix.md` | Widget 速查表、Style/Visuals 字段、常见错误、社区资源 |

## 关键注意事项

1. **中文字体**：egui 默认不含 CJK 字体，中文会显示为方框 □。必须加载自定义字体（见 `06-styling.md`）。
2. **状态在 App 里**：不要用局部变量保存 Slider 值，否则每帧重置。所有 UI 状态必须是 `App` 的字段。
3. **ID 冲突**：同位置出现两个同类型控件会状态互串，用 `ui.push_id()` 或 `.id_salt()` 区分。
4. **性能**：egui 按需重绘，空闲时不占 CPU；需要持续刷新时调 `ctx.request_repaint()` 或 `request_repaint_after(duration)`。
5. **中文输入**：用 `Event::Text` 检测字符输入（支持 IME），不要用 `Key::A` 检测字母。
