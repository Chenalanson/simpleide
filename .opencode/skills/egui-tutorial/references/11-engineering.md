# 工程实践


---

第十一部分：工程实践

﻿# 43 · 测试 egui 应用

---

## 43.1 egui\_kittest — 官方测试框架

```rust
[dev-dependencies]
egui_kittest = "0.34"
```

---

## 43.2 单元测试：纯逻辑

```rust
// ✅ 简单：测试 logic() 中的数据逻辑
#[test]
fn test_increment() {
    let mut app = MyApp::default();
    app.increment();
    assert_eq!(app.count, 1);
}
```

---

## 43.3 egui\_kittest headless 测试

```rust
use egui_kittest::Harness;
use egui::accesskit::Toggled;

#[test]
fn test_button_click() {
    let mut checked = false;
    let app = |ctx: &egui::Context| {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.checkbox(&mut checked, "Check me!");
        });
    };

    let mut harness = Harness::builder()
        .with_size(egui::vec2(200.0, 100.0))
        .build(app);

    // 通过 AccessKit 标签查找控件
    let checkbox = harness.get_by_label("Check me!");
    assert_eq!(checkbox.toggled(), Some(Toggled::False));

    // 模拟点击
    checkbox.click();
    harness.run(); // 执行一帧

    let checkbox = harness.get_by_label("Check me!");
    assert_eq!(checkbox.toggled(), Some(Toggled::True));
}
```

---

## 43.4 快照测试

```rust
egui_kittest = { version = "0.34", features = ["wgpu", "snapshot"] }
```

```rust
#[test]
fn test_snapshot() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 300.0))
        .build(my_app);

    harness.run();
    harness.wgpu_snapshot("my_test_snapshot");
    // 生成 PNG 快照到 snapshot_tests/ 目录
}
```

```rust
UPDATE_SNAPSHOTS=true cargo test  # 更新参考快照
```

---

## 本章小结

* 纯逻辑：常规 `#[test]`（logic/ui 分离的好处！）
* UI 测试：`egui_kittest::Harness` + `get_by_label()` + `.click()` + `harness.run()`
* 快照测试：`harness.wgpu_snapshot("name")`

**下一章**：打包与分发。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
// 43 · 测试 egui 应用
// 本示例展示如何编写可测试的 egui 应用（通过 logic/ui 分离）
// egui_kittest 的测试代码通常放在 #[cfg(test)] 模块中

use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("可测试的应用", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { count: i32 }

impl Default for MyApp { fn default() -> Self { Self { count: 0 } } }

impl MyApp {
    /// 纯逻辑方法——可被单元测试调用
    fn increment(&mut self) { self.count += 1; }
    fn decrement(&mut self) { self.count -= 1; }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("测试演示");

        if ui.button("+1").clicked() { self.increment(); }
        if ui.button("-1").clicked() { self.decrement(); }
        ui.label(format!("计数: {}", self.count));

        ui.separator();
        ui.label("运行 cargo test 查看测试结果");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_increment() {
        let mut app = MyApp::default();
        app.increment();
        assert_eq!(app.count, 1);
    }

    #[test]
    fn test_decrement() {
        let mut app = MyApp::default();
        app.decrement();
        assert_eq!(app.count, -1);
    }
}
```

---

第十一部分：工程实践

# 44 · 打包与分发

---

## 44.1 Release 构建

```rust
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

```rust
cargo build --release

# 输出: target/release/my_app.exe (~5MB)
```

---

## 44.2 Windows 分发

* 直接分发 `.exe`（用户需安装 VC++ Runtime）
* `cargo-wix` — 生成 MSI 安装包
* 嵌入图标：`.rc` 文件 + `windres` 或使用 `eframe::NativeOptions::icon_data`

---

## 44.3 macOS 分发

* `.app` bundle 结构
* `cargo-bundle` 自动生成：

```rust
  cargo install cargo-bundle
  cargo bundle --release
```

* 代码签名和公证（notarization）— 发布到 Mac App Store 需要

---

## 44.4 Linux 分发

* AppImage：`linuxdeploy` 工具打包
* `.deb` / `.rpm`：`cargo-deb` / `cargo-rpm`
* Flatpak 概念简述

---

## 44.5 WebAssembly 部署

```rust
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release

# 使用 trunk 构建和打包

cargo install trunk
trunk build --release

# 输出: dist/ 目录（HTML + JS + WASM）
```

### eframe 的 web 入口

```rust
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn start(canvas_id: &str) -> Result<(), eframe::wasm_bindgen::JsValue> {
        let web_options = eframe::WebOptions::default();
        eframe::WebRunner::new()
            .start(canvas_id, web_options, Box::new(|_cc| Ok(Box::new(MyApp::default()))))?;
        Ok(())
    }
}
```

### Web 平台限制

* 无原生多窗口（Viewport 回退为 Embedded 模式）
* 无文件系统（无法 `std::fs::read`）→ 用 wasm 存储 API
* AccessKit 实验性
* 复制粘贴需要 `--cfg=web_sys_unstable_apis`

---

## 44.6 跨平台 CI/CD

```rust
# .github/workflows/release.yml

* uses: actions-rs/cargo@v1

with:
    command: build
    args: --release

* uses: actions/upload-artifact@v4

with:
    name: my-app-windows
    path: target/release/my_app.exe
```

---

## 44.7 减小文件体积

* `strip = true` — 移除符号
* `panic = "abort"` — 移除 unwinding 代码
* `upx` 压缩（可选）
* 移除不必要的依赖 features

---

## 本章小结

* Windows: `.exe` + MSI
* macOS: `.app` bundle + `cargo-bundle`
* Linux: AppImage / deb / rpm
* Web: `wasm32-unknown-unknown` + `trunk build`
* CI: GitHub Actions 自动构建发布

**下一章**：bevy\_egui 快速概览。

---

> 📁 本章无代码示例（配置 + 脚本）

---

第十一部分：工程实践

# 45 · bevy\_egui 快速概览

---

## 45.1 Bevy 是什么

[Bevy](https://bevyengine.org) 是一个 Rust 编写的游戏引擎，使用 ECS（Entity Component System）架构。核心概念：

* **Entity** — 游戏对象（空 ID）
* **Component** — 数据（位置、速度、健康值）
* **System** — 逻辑（移动系统、渲染系统）
* **Plugin** — 功能模块

---

## 45.2 bevy\_egui 集成

```rust
bevy = "0.15"
bevy_egui = "0.34"
```

```rust
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, egui};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)  // 添加 egui 插件
        .add_systems(Update, ui_system)
        .run();
}

fn ui_system(mut egui_ctx: Query<&mut EguiContext>) {
    // 在 Bevy System 中使用 egui
    egui::Window::new("设置").show(egui_ctx.ctx_mut(), |ui| {
        ui.label("游戏设置");
    });
}
```

---

## 45.3 与独立 eframe 的区别

|  |  |  |
| --- | --- | --- |
| 维度 | eframe | bevy\_egui |

|  |  |  |
| --- | --- | --- |
| 渲染上下文 | egui 独占窗口 | egui 叠加在 Bevy 3D 场景上 |
| 应用模型 | `MyApp` → `logic()` + `ui()` | Bevy ECS System |
| 适用场景 | 桌面工具、编辑器 | 游戏内 UI（背包、对话框） |
| 学习曲线 | 低 | 高（需理解 ECS） |

---

## 45.4 何时选择 bevy\_egui

```rust
需要 3D 渲染/物理/音频？
  ├─ 是 → bevy_egui（在 Bevy 游戏中使用 egui 做 UI）
  └─ 否 → eframe（做桌面工具/编辑器）
```

---

## 本章小结

* bevy\_egui 将 egui 集成到 Bevy ECS
* 适用：游戏内 UI
* 不适用：独立桌面工具（eframe 更简单）

**恭喜！你已经完成了 egui 全部 45 章的学习！** 🎉

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
// 45 · bevy_egui 快速概览
// bevy_egui 需要配合 Bevy 游戏引擎使用（见教程正文）
// 此处展示核心集成方式

use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("bevy_egui概览", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("bevy_egui 快速概览");

        ui.label("bevy_egui 将 egui 集成到 Bevy ECS 游戏引擎:");
        ui.label("1. App::new().add_plugins(EguiPlugin) — 添加 egui 插件");
        ui.label("2. 在 Bevy System 中使用 egui::Context");
        ui.label("3. egui 渲染层叠加在 Bevy 3D 场景之上");
        ui.label("4. 适用于: 游戏内 UI、背包、对话框、编辑器");

        ui.separator();
        ui.label("// Bevy 中的代码示例:");
        ui.label("fn ui_system(mut egui_ctx: Query<&mut EguiContext>) {");
        ui.label("    egui::Window::new(\"设置\").show(ctx, |ui| {");
        ui.label("        ui.button(\"游戏设置\");");
        ui.label("    });");
        ui.label("}");

        ui.separator();
        ui.label("何时选择 bevy_egui:");
        ui.label("  ✓ 需要 3D/物理/音频 → bevy_egui");
        ui.label("  ✓ 纯桌面工具/编辑器 → eframe");
    }
}
```