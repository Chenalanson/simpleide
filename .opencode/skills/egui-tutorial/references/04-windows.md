# 窗口与弹窗


---

第四部分：窗口与弹窗

﻿# 16 · 窗口（Window）

`Window` 是 egui 中的浮动窗口——可在屏幕上拖拽、缩放、折叠、关闭。

---

## 16.1 基本窗口

```rust
egui::Window::new("设置")
    .open(&mut self.show_settings) // 绑定 open/close 状态
    .show_inside(ui, |ui| {
        ui.label("这里是设置内容");
        if ui.button("关闭").clicked() {
            self.show_settings = false;
        }
    });
```

必须调用 `.show_inside(ui, |ui| {...})` 窗口才会显示。如果 `self.show_settings` 为 `false`，窗口不显示。

---

## 16.2 窗口外观控制

```rust
egui::Window::new("工具")
    .open(&mut self.show_tools)
    .resizable(true)            // 可拖拽缩放（默认 true）
    .collapsible(true)          // 可折叠（默认 false）
    .title_bar(true)            // 显示标题栏（默认 true）
    .constrain(true)            // 限制在父窗口内（默认 true）
    .default_size([400.0, 300.0]) // 默认尺寸
    .default_pos([100.0, 100.0])  // 默认位置
    .min_size([200.0, 150.0])     // 最小尺寸
    .fixed_size([500.0, 400.0])   // 固定尺寸（不可缩放）
    .auto_sized()                  // 自动适应内容大小
    .show_inside(ui, |ui| {
        // ...
    });
```

---

## 16.3 多窗口管理

```rust
egui::Window::new("工具 A")
    .id(egui::Id::new("tool_a")) // 显式 ID（避免冲突）
    .open(&mut self.show_tool_a)
    .show_inside(ui, |ui| {
        ui.label("工具 A 的内容");
    });

egui::Window::new("工具 B")
    .id(egui::Id::new("tool_b"))
    .open(&mut self.show_tool_b)
    .show_inside(ui, |ui| {
        ui.label("工具 B 的内容");
    });
```

---

## 16.4 窗口状态持久化

配合 eframe 的 `save`/`load`（第 20 章），窗口位置和大小可以自动持久化：

```rust
// 在 save() 中保存窗口状态
fn save(&self, _storage: &mut dyn eframe::Storage) {
    // eframe 自动处理大部分窗口状态
    // 参见第 20 章
}
```

---

## 本章小结

* `Window::new("标题").open(&mut bool).show_inside(ui, |ui| {...})` — 基本窗口
* `.resizable()`, `.collapsible()`, `.constrain()` — 外观控制
* `.default_pos()`, `.default_size()`, `.fixed_size()` — 尺寸/位置
* 多个 Window 用不同 ID 区分

**下一章**：右键菜单与弹窗。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("浮动窗口", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { show_settings: bool, show_about: bool }

impl Default for MyApp { fn default() -> Self { Self { show_settings: false, show_about: false } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            if ui.button("打开设置").clicked() { self.show_settings = true; }
        });

        // 设置窗口
        let mut show_about = self.show_about;
        egui::Window::new("设置")
            .open(&mut self.show_settings)
            .resizable(true)
            .default_size([300.0, 200.0])
            .show(ui, |ui| {
                ui.label("这里是设置面板");
                ui.checkbox(&mut show_about, "显示关于窗口");
            });
        self.show_about = show_about;

        // 关于窗口
        egui::Window::new("关于")
            .open(&mut self.show_about)
            .collapsible(true)
            .default_size([250.0, 150.0])
            .show(ui, |ui| {
                ui.label("EGUI 全面教程 - ch16");
                if ui.button("关闭").clicked() { /* handled by open flag */ }
            });

        if ui.button("显示关于").clicked() { self.show_about = true; }
    }
}
```

---

第四部分：窗口与弹窗

﻿# 17 · 右键菜单与弹窗

本章覆盖 `ContextMenu`（右键菜单）、`menu::bar`（菜单栏）、`popup`（弹出框）、`modal`（模态对话框）和 `Area`（浮动区域）。

---

## 17.1 `ContextMenu` 右键菜单

```rust
// 方式一：附加到 Widget
ui.label("右键点击这里")
    .context_menu(|ui| {
        if ui.button("复制").clicked() {
            ui.close(); // 关闭菜单
            // 执行复制...
        }
        if ui.button("粘贴").clicked() {
            ui.close();
            // 执行粘贴...
        }
        ui.separator();
        if ui.button("删除").clicked() {
            ui.close();
            // 执行删除...
        }
    });
```

---

## 17.2 菜单栏 `menu::bar`

```rust
egui::MenuBar::new().ui(ui, |ui| {
    ui.menu_button("文件", |ui| {
        if ui.button("新建").clicked() { ui.close(); }
        if ui.button("打开").clicked() { ui.close(); }
        ui.separator();
        if ui.button("退出").clicked() { ui.close(); }
    });
    ui.menu_button("编辑", |ui| {
        if ui.button("撤销").clicked() { ui.close(); }
        if ui.button("重做").clicked() { ui.close(); }
    });
    ui.menu_button("帮助", |ui| {
        if ui.button("关于").clicked() { ui.close(); }
    });
});
```

---

## 17.3 Popup vs Modal

### Popup（弹出框）

点击外部自动关闭，不阻塞其他 UI：

```rust
let popup_id = egui::Id::new("my_popup");
if ui.button("显示提示").clicked() {
    ui.memory_mut(|mem| mem.open_popup(popup_id));
}

egui::popup::popup_below_widget(ui, popup_id, &response, |ui| {
    ui.label("这是一条提示信息");
});
```

### Modal（模态对话框）

必须处理才能继续，背景有遮罩：

```rust
if self.show_confirm {
    egui::Window::new("确认删除")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show_inside(ui, |ui| {
            ui.label("确定要删除吗？此操作不可撤销。");
            ui.horizontal(|ui| {
                if ui.button("确定").clicked() {
                    // 执行删除
                    self.show_confirm = false;
                }
                if ui.button("取消").clicked() {
                    self.show_confirm = false;
                }
            });
        });
}
```

egui 没有原生 modal 遮罩——但用 `.anchor(CENTER_CENTER)` + 禁用背景 UI 可以模拟。

---

## 17.4 `Area` 绝对定位浮动元素

```rust
egui::Area::new("my_area")
    .fixed_pos(egui::pos2(100.0, 100.0))  // 固定位置
    .order(egui::Order::Foreground)         // 绘制在最前面
    .show_inside(ui, |ui| {
        ui.label("浮动面板");
    });
```

### Area 的交互模式

`Area` 默认不参与布局流——它"悬浮"在其他 UI 之上。适合：

* 工具提示
* 浮动调色板
* 自定义弹出面板

---

## 本章小结

* `context_menu(|ui| {...})` — 附加到任何 Widget 的右键菜单
* `ui.menu_button("标签", |ui| {...})` — 菜单栏下拉
* `popup` — 非阻塞弹出，点击外部关闭
* `Window` + `.anchor(CENTER_CENTER)` — 模拟模态对话框
* `Area` — 绝对定位的浮动元素

**下一章**：状态管理模型——理解 egui 如何跨帧保留数据。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("菜单与弹窗", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { show_confirm: bool, text: String }

impl Default for MyApp { fn default() -> Self { Self { show_confirm: false, text: String::new() } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 菜单栏（新版 API）
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("文件", |ui| {
                if ui.button("新建").clicked() { ui.close(); self.text.clear(); }
                if ui.button("退出").clicked() { ui.close(); std::process::exit(0); }
            });
            ui.menu_button("帮助", |ui| {
                if ui.button("关于").clicked() { ui.close(); }
            });
        });

        // 右键菜单
        ui.separator();
        ui.label("右键点击下方文本：");
        let resp = ui.label("右键点击这里");
        resp.context_menu(|ui| {
            if ui.button("复制").clicked() { ui.close(); }
            if ui.button("粘贴").clicked() { ui.close(); }
            ui.separator();
            if ui.button("删除").clicked() { ui.close(); self.text.clear(); }
        });

        // 输入区
        ui.add(egui::TextEdit::singleline(&mut self.text).hint_text("输入文本...").desired_width(200.0));

        // 确认对话框
        if ui.button("删除所有内容").clicked() { self.show_confirm = true; }

        if self.show_confirm {
            egui::Window::new("确认")
                .collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui, |ui| {
                    ui.label("确定要删除吗？");
                    ui.horizontal(|ui| {
                        if ui.button("确定").clicked() { self.text.clear(); self.show_confirm = false; }
                        if ui.button("取消").clicked() { self.show_confirm = false; }
                    });
                });
        }
    }
}
```