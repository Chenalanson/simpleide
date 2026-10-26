# 样式与主题


---

第六部分：样式与主题

﻿# 21 · Style 与 Visuals 体系

**Style** 控制间距和交互参数，**Visuals** 控制颜色和形状。本章覆盖从全局到局部的样式定制。

---

## 21.1 `Style` 结构体

```rust
// 全局读取
let style = ctx.style();

// 修改全局样式（在 logic() 中设置一次）
ctx.global_style_mut(|style| {
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.animation_time = 0.2;  // 动画持续时间（秒）
});
```

### Style 关键字段

|  |  |  |
| --- | --- | --- |
| 字段 | 类型 | 说明 |

|  |  |  |
| --- | --- | --- |
| `spacing` | `Spacing` | 控件间距、按钮内边距、缩进 |
| `interaction` | `Interaction` | 双击间隔、tooltip 延迟 |
| `visuals` | `Visuals` | 颜色、圆角、阴影等 |
| `animation_time` | `f32` | 默认动画时长 |

---

## 21.2 `Visuals` — 控制颜色主题

```rust
// 在 ui() 中局部修改
ui.style_mut().visuals.override_text_color = Some(egui::Color32::RED);

// 全局切换暗/亮模式
ctx.global_style_mut(|style| {
    style.visuals = egui::Visuals::dark();  // 暗色主题
    // style.visuals = egui::Visuals::light();  // 亮色主题
});
```

### Visuals 关键颜色角色

|  |  |
| --- | --- |
| 字段 | 说明 |

|  |  |
| --- | --- |
| `override_text_color` | 覆盖所有文本颜色 |
| `widgets.hovered.bg_fill` | 悬停时控件背景色 |
| `widgets.active.bg_fill` | 按下时控件背景色 |
| `widgets.open.bg_fill` | 展开状态背景色 |
| `selection.bg_fill` | 文本选中背景色 |
| `window_rounding` | 窗口圆角半径 |
| `widgets.noninteractive.rounding` | 控件圆角半径 |

---

## 21.3 全局 vs 局部样式

```rust
// 全局设置：在 logic() 中
fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    ctx.global_style_mut(|style| {
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(4.0);
    });
}

// 局部覆盖：在 ui() 中
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // 只影响当前 ui 子树
    ui.style_mut().visuals.widgets.noninteractive.rounding = egui::Rounding::same(10.0);
    ui.button("圆角更大");
}

// 子区域临时修改
ui.style_mut().visuals.override_text_color = Some(egui::Color32::RED);
ui.label("红色文字");
ui.style_mut().visuals.override_text_color = None; // 恢复
ui.label("正常颜色");
```

---

## 21.4 样式探测器示例

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.heading("样式参数调节");

        ui.add(egui::Slider::new(&mut self.rounding, 0.0..=20.0).text("圆角"));
        ui.add(egui::Slider::new(&mut self.spacing, 0.0..=20.0).text("间距"));
        ui.add(egui::Slider::new(&mut self.anim_time, 0.0..=1.0).text("动画时长"));

        // 应用修改
        let mut style = (*ctx.style()).clone();
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(self.rounding);
        style.spacing.item_spacing = egui::vec2(self.spacing, self.spacing);
        style.animation_time = self.anim_time;
        ctx.set_style(style);

        // 预览区
        ui.separator();
        ui.label("预览：");
        ui.button("测试按钮");
        ui.checkbox(&mut self.dummy, "复选框");
    });
}
```

---

## 本章小结

* `ctx.global_style_mut(|s| {...})` — 全局修改
* `ui.style_mut()` — 局部修改
* `Visuals::dark()` / `Visuals::light()` — 预设主题
* `override_text_color` — 快速改变文本颜色

**下一章**：自定义字体——解决中文显示问题。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("样式体系", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { spacing: f32, dummy: bool }

impl Default for MyApp { fn default() -> Self { Self { spacing: 6.0, dummy: false } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("样式参数调节");

        ui.add(egui::Slider::new(&mut self.spacing, 0.0..=20.0).text("控件间距"));
        ui.separator();

        // 应用局部间距
        ui.style_mut().spacing.item_spacing = egui::vec2(self.spacing, self.spacing);

        ui.label("预览区:");
        let _ = ui.button("测试按钮");
        ui.checkbox(&mut self.dummy, "复选框");

        ui.separator();
        // 覆盖文字颜色
        ui.style_mut().visuals.override_text_color = Some(egui::Color32::RED);
        ui.label("红色文字");
        ui.style_mut().visuals.override_text_color = None;
        ui.label("正常颜色");
    }
}
```

---

第六部分：样式与主题

# 22 · 自定义字体

本章解决 egui 开发中的 #1 痛点——中文字体配置。

---

## 22.1 egui 字体系统架构

```rust
FontDefinitions
  └─ families: BTreeMap<FontFamily, Vec<FontData>>
      ├─ FontFamily::Proportional  → [字体1, 字体2, ...]  ← 回退机制
      └─ FontFamily::Monospace     → [字体1, ...]
```

当前字体找不到某字形时，**自动尝试列表中的下一个字体**——这就是 **字体回退（fallback）** 机制。

---

## 22.2 嵌入字体（推荐）

将 `.ttf` 文件放在项目 `assets/` 目录下，用 `include_bytes!` 嵌入到二进制中：

```rust
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // 为 Proportional 字体族添加中文字体
    // 放置在最前面：egui 先尝试这个字体
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        FontData::from_owned(
            // include_bytes! 在编译时将文件嵌入二进制
            std::fs::read("assets/NotoSansSC-Regular.ttf")
                .unwrap_or_default()
        ),
    );

    // 将中文字体放在 proportional 族的第一位（最高优先级）
    fonts.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());

    // 同理添加等宽中文字体
    // fonts.families
    //     .entry(FontFamily::Monospace)
    //     .or_default()
    //     .insert(0, "noto_sans_mono".to_owned());

    ctx.set_fonts(fonts);
}
```

### 在启动时配置

```rust
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "应用",
        native_options,
        Box::new(|cc| {
            // cc.egui_ctx 就是 Context
            configure_fonts(&cc.egui_ctx);
            Ok(Box::new(MyApp::default()))
        }),
    )
}
```

---

## 22.3 中文推荐字体

|  |  |  |
| --- | --- | --- |
| 字体 | 文件 | 特点 |

|  |  |  |
| --- | --- | --- |
| 思源黑体 (Noto Sans SC) | `NotoSansSC-Regular.ttf` | Google 开源，覆盖面广 |
| 思源宋体 (Noto Serif SC) | `NotoSerifSC-Regular.ttf` | 衬线风格 |
| 文泉驿微米黑 | `wqy-microhei.ttc` | Linux 常见 |
| 微软雅黑 | `msyh.ttc` | Windows 自带（注意许可） |

> 推荐思源黑体：免费、开源、字符覆盖完整。

---

## 22.4 字体回退机制

```rust
fonts.families
    .entry(FontFamily::Proportional)
    .or_default()
    .insert(0, "my_chinese_font".to_owned());  // 优先级最高
// 默认的 egui 西文字体自动作为回退
```

顺序：先查第 1 个字体 → 缺字形则查第 2 个 → 第 3 个 → ... → 全部无则显示方框。

---

## 22.5 `FontId` 与文本大小

```rust
use eframe::egui::FontId;

// Proportional 字体，14px
ui.add(egui::Label::new(
    egui::RichText::new("文本")
        .font(FontId::proportional(14.0))
));

// Monospace 字体，12px
ui.add(egui::Label::new(
    egui::RichText::new("code")
        .font(FontId::monospace(12.0))
));
```

### 默认字号映射

|  |  |
| --- | --- |
| 用途 | 字号 |

|  |  |
| --- | --- |
| `Body` | 14.0 |
| `Button` | 14.0 |
| `Heading` | 20.0 |
| `Small` | 10.0 |
| `Monospace` | 12.0 |

---

## 本章小结

* `FontDefinitions` → `ctx.set_fonts()` 配置字体
* 中文字体放到 `Proportional` 族第一位实现回退
* 推荐使用思源黑体（Noto Sans SC）
* `FontId::proportional(size)` / `monospace(size)` 选择字号

**下一章**：主题定制——明暗切换与自定义配色。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("自定义字体", eframe::NativeOptions::default(),
        Box::new(|cc| {
            // 配置字体（第22章示例 - 基础骨架）
            let fonts = egui::FontDefinitions::default();
            // 实际应用中加载中文字体:
            // fonts.font_data.insert("my_font".into(), egui::FontData::from_owned(
            //     std::fs::read("path/to/font.ttf").unwrap_or_default()
            // ));
            // fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "my_font".into());
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(MyApp::default()))
        }),
    )
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("字体配置演示");
        ui.label("This text uses the proportional font.");
        ui.label(egui::RichText::new("等宽字体文字").font(egui::FontId::monospace(14.0)));
        ui.label(egui::RichText::new("大字").size(24.0));
        ui.label(egui::RichText::new("小字").size(10.0));
        ui.label(
            egui::RichText::new("中文测试 - Font Test")
                .size(16.0)
                .strong()
        );
    }
}
```

---

第六部分：样式与主题

﻿# 23 · 主题定制

---

## 23.1 明/暗主题切换

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    ui.horizontal(|ui| {
        if ui.button("☀ 亮色").clicked() {
            ui.ctx().global_style_mut(|s| s.visuals = egui::Visuals::light());
        }
        if ui.button("🌙 暗色").clicked() {
            ui.ctx().global_style_mut(|s| s.visuals = egui::Visuals::dark());
        }
    });
```

---

## 23.2 跟随系统主题

```rust
let native_options = eframe::NativeOptions {
    follow_system_theme: true,  // 自动跟随系统明/暗
    ..Default::default()
};
```

---

## 23.3 自定义配色

```rust
fn custom_blue_theme() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(30, 60, 120);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(20, 80, 160);
    visuals.selection.bg_fill = egui::Color32::from_rgb(40, 80, 180);
    visuals.window_rounding = egui::Rounding::same(6.0);
    visuals.widgets.noninteractive.rounding = egui::Rounding::same(4.0);
    visuals
}

// 应用自定义主题
ctx.global_style_mut(|s| s.visuals = custom_blue_theme());
```

---

## 23.4 Widget 级别微调

```rust
// 单个按钮改色
if ui.add(
    egui::Button::new("危险操作")
        .fill(egui::Color32::from_rgb(200, 40, 40))
).clicked() { / ... / }
```

---

## 本章小结

* `ctx.global_style_mut(|s| s.visuals = Visuals::dark())` — 切换主题
* `follow_system_theme: true` — 跟随系统
* 自定义 `Visuals` — 完全控制配色、圆角

**下一章**：Painter 与 Shape API——自定义绘制。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("主题定制", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { dark_mode: bool }

impl Default for MyApp { fn default() -> Self { Self { dark_mode: true } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("主题切换");

        ui.horizontal(|ui| {
            if ui.button("🌙 暗色").clicked() {
                self.dark_mode = true;
                ui.ctx().global_style_mut(|s| s.visuals = egui::Visuals::dark());
            }
            if ui.button("☀ 亮色").clicked() {
                self.dark_mode = false;
                ui.ctx().global_style_mut(|s| s.visuals = egui::Visuals::light());
            }
        });

        ui.label(format!("当前: {}", if self.dark_mode { "暗色" } else { "亮色" }));
        ui.separator();

        let _ = ui.button("控件预览");
        ui.checkbox(&mut self.dark_mode, "选项");
        ui.add(egui::Slider::new(&mut 50i32, 0..=100).text("滑动条"));
    }
}
```