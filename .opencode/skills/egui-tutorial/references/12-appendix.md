# 附录 · 速查与参考

## 附录 A · Widget 速查表

### 显示类

| Widget | 代码 | 交互 |
|---|---|---|
| Label | `ui.label("text")` | 无 |
| Heading | `ui.heading("title")` | 无 |
| RichText | `RichText::new("t").color(c).size(s)` | 无 |
| Image | `ui.image(tex_id, size)` | 无 |
| Separator | `ui.separator()` | 无 |
| Spinner | `ui.spinner()` | 无 |
| ProgressBar | `ui.add(ProgressBar::new(p))` | 无 |

### 交互类

| Widget | 代码 | 返回值 |
|---|---|---|
| Button | `ui.button("OK")` | `Response` |
| Checkbox | `ui.checkbox(&mut b, "label")` | `Response` |
| Toggle | `ui.selectable_label(b, "●/○")` | `Response` |
| RadioButton | `ui.radio_value(&mut e, v, "label")` | `Response` |
| SelectableLabel | `ui.selectable_label(sel, "label")` | `Response` |
| Hyperlink | `ui.hyperlink_to("label", "url")` | `Response` |

### 输入类

| Widget | 代码 | 绑定 |
|---|---|---|
| TextEdit 单行 | `ui.add(TextEdit::singleline(&mut s))` | `&mut String` |
| TextEdit 多行 | `ui.add(TextEdit::multiline(&mut s))` | `&mut String` |
| Slider | `ui.add(Slider::new(&mut v, 0..=100))` | `&mut f64/f32/i32` |
| DragValue | `ui.add(DragValue::new(&mut v))` | `&mut f64/f32/i32` |
| ComboBox | `ComboBox::from_label("L").show_ui(ui, |ui| {...})` | `&mut T` |
| ColorEdit | `ui.color_edit_button_srgba(&mut c)` | `&mut Color32` |

### 布局类

| Widget | 代码 |
|---|---|
| Horizontal | `ui.horizontal(|ui| {...})` |
| Vertical | `ui.vertical(|ui| {...})` |
| Grid | `Grid::new("id").show(ui, |ui| {...})` |
| ScrollArea | `ScrollArea::vertical().show(ui, |ui| {...})` |

### 面板类

| Panel | 代码 |
|---|---|
| CentralPanel | `CentralPanel::default().show_inside(ui, |ui| {...})` |
| 左侧栏 | `Panel::left("id").show_inside(ui, |ui| {...})` |
| 顶部栏 | `Panel::top("id").show_inside(ui, |ui| {...})` |

### 窗口/浮层

| Widget | 代码 |
|---|---|
| Window | `Window::new("title").show(ui, |ui| {...})` |
| Area | `Area::new("id").show(ui, |ui| {...})` |
| ContextMenu | `widget.context_menu(|ui| {...})` |

---

## 附录 B · Style / Visuals 字段参考

### Style

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `spacing.item_spacing` | `Vec2` | `(6, 3)` | 控件间距 |
| `spacing.button_padding` | `Vec2` | `(6, 2)` | 按钮内边距 |
| `spacing.indent` | `f32` | `14.0` | 缩进宽度 |
| `spacing.icon_width` | `f32` | `14.0` | 图标列宽 |
| `interaction.tooltip_delay` | `f32` | `0.5` | tooltip 延迟（秒） |
| `interaction.double_click_delay` | `f32` | `0.4` | 双击延迟（秒） |
| `animation_time` | `f32` | `0.1` | 默认动画时长（秒） |

### Visuals

| 字段 | 说明 |
|---|---|
| `dark_mode` | 是否暗色模式 |
| `override_text_color` | `Option<Color32>` — 覆盖所有文本颜色 |
| `widgets.noninteractive` | 不可交互控件的颜色组 |
| `widgets.inactive` | 非活动控件的颜色组 |
| `widgets.hovered` | 悬停控件的颜色组 |
| `widgets.active` | 按下控件的颜色组 |
| `widgets.open` | 展开状态控件的颜色组 |
| `selection.bg_fill` | 文本选中背景色 |
| `selection.stroke` | 文本选中边框 |
| `window_rounding` | `Rounding` — 窗口圆角（默认 `(8, 8)`） |
| `widgets.noninteractive.rounding` | `Rounding` — 控件圆角（默认 `(4, 4)`） |

### 常用颜色构造

```rust
Color32::RED / GREEN / BLUE / WHITE / BLACK / GRAY
Color32::TRANSPARENT
Color32::from_rgb(r, g, b)
Color32::from_rgba_unmultiplied(r, g, b, a)
```

---

## 附录 C · 常见错误与解决方案

### 编译错误

| 错误 | 原因 | 解决 |
|---|---|---|
| `does not live long enough` | 闭包捕获了临时引用 | 用 `move` 闭包或 `.clone()` |
| `trait bound eframe::App not satisfied` | 忘记实现 App trait | 添加 `impl eframe::App for MyApp {...}` |
| `trait bound Default not satisfied` | 忘记 impl Default | 添加 `impl Default` 或 `#[derive(Default)]` |
| `cannot borrow as mutable` | 同一作用域多个 `&mut` | 使用 `RefCell` 或重构代码 |
| `cannot find type Context` | 未 use | `use eframe::egui::Context;` |
| `method not found in Context` | API 调用方式错误 | 检查是否用了 `ctx.input(|i| ...)` 包裹 |

### 运行时问题

| 问题 | 症状 | 解决 |
|---|---|---|
| ID 冲突 | 两个 Widget 状态互串 | `ui.push_id()` 或 `.id_salt()` |
| 中文方框 | 中文字符显示为 □ | 配置中文字体（见 06-styling.md） |
| 窗口关闭后无法再打开 | open bool 未正确重置 | 确保 `open` 标志正确管理 |
| 按钮不响应 | 被其他区域遮挡 | 检查 `Area`/`Window` 的 order 和 z 序 |
| 布局坍塌 | Widget 尺寸为 0 | 设置 `min_size` 或 `desired_width` |
| 状态意外重置 | Slider 值跳回初始值 | 用 `MyApp` 字段而非局部变量 |

---

## 附录 D · 延伸阅读与社区资源

### 官方资源

| 资源 | 链接 |
|---|---|
| egui GitHub | https://github.com/emilk/egui |
| egui docs.rs | https://docs.rs/egui |
| eframe docs.rs | https://docs.rs/eframe |
| egui 官方示例 | `cargo run -p egui_demo_app` |
| egui CHANGELOG | https://github.com/emilk/egui/blob/main/CHANGELOG.md |
| egui_plot | https://github.com/emilk/egui_plot |

### 社区

| 资源 | 链接 |
|---|---|
| egui Discord | https://discord.gg/egui |
| Rust 官方论坛 | https://users.rust-lang.org |
| r/rust (Reddit) | https://reddit.com/r/rust |

### 推荐开源项目

| 项目 | 说明 |
|---|---|
| egui_demo_app | 官方演示应用——必读 |
| Rerun | 可视化工具，egui 重度使用 |
| egui_node_graph | 节点编辑器 |
| egui-snarl | 节点编辑器 |
| hello_egui | egui 示例合集 |

### 相关技术对比

| 框架 | 语言 | 模式 | 适合 |
|---|---|---|---|
| egui | Rust | Immediate Mode | 工具/编辑器 |
| iced | Rust | Retained (Elm-like) | 消费级应用 |
| slint | Rust/C++/JS | Declarative DSL | 嵌入式/跨平台 |
| tauri | Rust + Web | Web 技术栈 | 需要 HTML/CSS 的场景 |
| druid | Rust | Retained | （已不再活跃维护） |
| Dear ImGui | C++ | Immediate Mode | C++ 游戏开发工具 |

---

*本教程基于 egui 0.34.3，编写于 2026 年 6 月。来源：https://deapskf.github.io/AI-egui-tutorial/*
