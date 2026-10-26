# 布局系统


---

第三部分：布局系统

# 11 · egui 布局模型

本章讲解 egui 布局的核心概念——`Ui` 的本质、布局方向、间距系统、`Sense` 以及 immediate mode 下布局的工作原理。

---

## 11.1 `Ui` 的本质

在 Retained Mode GUI 中，"布局"意味着你创建一棵控件树，每个控件知道自己的父控件和子控件，形成一个可以递归计算尺寸的树状结构。

在 egui 中，**没有控件树**。`Ui` 只是一个"矩形区域 + 一个布局方向"的组合：

```rust
┌──────────────────────────────┐
│  Ui: max_rect                │
│  ├─ label("姓名:")           │ ← cursor 0→40
│  ├─ button("确认")           │ ← cursor 40→120
│  └─ remaining space          │ ← 剩余可用空间
└──────────────────────────────┘
```

每添加一个 Widget，egui 在当前 `Ui` 的矩形中分配一块空间，cursor（游标）向下推进，剩余可用空间缩小。下一个 Widget 紧跟其后。

### 关键属性

|  |  |
| --- | --- |
| 属性 | 含义 |

|  |  |
| --- | --- |
| `ui.max_rect()` | `Ui` 的总可用矩形 |
| `ui.available_size()` | 当前剩余可用空间 |
| `ui.min_rect()` | 已被使用的最小矩形 |
| `ui.cursor()` | 当前光标位置 |

---

## 11.2 布局方向

egui 默认**自上而下（top\_down）**布局。通过 `ui.horizontal()` 和 `ui.vertical()` 改变流向。

```rust
// 默认垂直排列
ui.label("第一行");
ui.label("第二行");
ui.label("第三行");
// 输出：
// 第一行
// 第二行
// 第三行
```

```rust
// 水平排列
ui.horizontal(|ui| {
    ui.label("左");
    ui.label("中");
    ui.label("右");
});
// 输出：左 中 右
```

嵌套：

```rust
ui.vertical(|ui| {
    ui.label("姓名：");
    ui.horizontal(|ui| {
        ui.label("姓：");
        ui.text_edit_singleline(&mut self.first_name);
        ui.label("名：");
        ui.text_edit_singleline(&mut self.last_name);
    });
    ui.label("地址：");
    ui.text_edit_singleline(&mut self.address);
});
```

---

## 11.3 间距系统 `spacing`

```rust
// 读取间距
let spacing = ui.spacing();

// 修改间距
ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);   // 控件间距
ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0); // 按钮内边距
ui.spacing_mut().indent = 20.0;                           // 缩进量
```

### Spacing 字段速查

|  |  |  |
| --- | --- | --- |
| 字段 | 默认值 | 说明 |

|  |  |  |
| --- | --- | --- |
| `item_spacing` | `(6, 3)` | 控件之间的水平/垂直间距 |
| `button_padding` | `(6, 2)` | 按钮内部 padding |
| `indent` | `14.0` | 缩进宽度 |
| `icon_width` | `14.0` | 图标列宽 |
| `icon_spacing` | `4.0` | 图标间距 |
| `slider_width` | 计算值 | 滑块默认宽度 |

---

## 11.4 布局对齐

```rust
// 水平居中
ui.vertical_centered(|ui| {
    ui.heading("居中的标题");
    ui.label("这段文字也居中");
});

// 水平居中 + 垂直填充
ui.vertical_centered_justified(|ui| {
    ui.heading("居中 + 撑满");
});
```

### Layout 枚举

```rust
use egui::Layout;

// 自上而下（默认）
Layout::top_down(egui::Align::Min);

// 自左向右
Layout::left_to_right(egui::Align::Center);

// 自右向左（适合阿拉伯语等 RTL 语言）
Layout::right_to_left(egui::Align::Min);
```

---

## 11.5 `Sense` —— 宣告交互意图

每个 Widget 都有 `Sense`——告诉 egui 这个 Widget 需要响应什么类型的交互：

```rust
// Sense::hover() — 仅检测悬停（便宜）
// Sense::click() — 检测点击（需额外计算）
// Sense::drag()  — 检测拖拽（需持续追踪）

// 默认大多数 Widget 使用 Sense::click()
// Label 默认使用 Sense::hover()（不可点击）
```

### 显式设置 Sense

```rust
// 让一个不可交互的区域变得可点击
let response = ui.allocate_rect(
    rect,
    egui::Sense::click(), // 显式声明需要点击
);
if response.clicked() {
    // ...
}
```

### Sense 的性能影响

|  |  |  |
| --- | --- | --- |
| Sense | CPU 开销 | 何时使用 |

|  |  |  |
| --- | --- | --- |
| `Sense::hover()` | 低 | 不需要点击和拖拽 |
| `Sense::click()` | 中 | 需要点击响应 |
| `Sense::drag()` | 高 | 需要拖拽响应 |

> 给不需要交互的 Widget 使用 `Sense::hover()` 可以节省不必要的计算——这是 egui 性能优化的重要技巧（详见第 36 章）。

---

## 11.6 布局为何"不能嵌套"？

新手常见困惑：在 immediate mode 中，你**不能**像 retained mode 那样"先创建父容器，再往里面加子控件"。

```rust
// ❌ Retained Mode 思维（在 egui 中无法实现）：
// container = new VBox();
// container.add(new Button("确定"));
// container.add(new Button("取消"));
// 这不是 egui 的工作方式

// ✅ egui 的工作方式——即时描述：
ui.vertical(|ui| {
    if ui.button("确定").clicked() { / ... / }
    if ui.button("取消").clicked() { / ... / }
});
```

关键理解：**没有控件树 = 没有布局递归**。`ui.vertical()` 不是一个"容器对象"——它只是一个在当前 Ui 中创建了一个**新的子 Ui 并改变了布局方向**的临时作用域。作用域结束后，子 Ui 销毁，布局恢复。

---

## 本章小结

* `Ui` = 矩形区域 + 布局方向 + cursor
* 默认自上而下，`ui.horizontal()` 改为自左向右
* `ui.spacing()` 控制间距
* `Sense` 宣告交互意图，影响性能和响应
* Immediate mode 布局不是"嵌套控件树"而是"逐帧分配空间"

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("布局模型", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("布局模型演示");

        ui.separator();
        ui.label(format!("可用空间: {:?}", ui.available_size()));

        ui.spacing_mut().item_spacing = egui::vec2(10.0, 5.0);

        ui.horizontal(|ui| {
            ui.label("水平排列:");
            ui.label("A");
            ui.label("B");
            ui.label("C");
        });

        ui.vertical_centered(|ui| {
            ui.heading("居中的标题");
            ui.label("这段文字也居中显示");
        });

        ui.separator();
        ui.label("Sense 演示:");
        let (resp, _painter) = ui.allocate_painter(egui::vec2(200.0, 50.0), egui::Sense::click());
        if resp.clicked() { ui.label("矩形区域被点击!"); }
        if resp.hovered() { ui.label("鼠标悬停中..."); }
    }
}
```

---

第三部分：布局系统

﻿# 12 · 水平与垂直排列

本章深入 `ui.horizontal()`、`ui.vertical()` 的变体和 `allocate_ui` 手动空间划分，以及响应式布局基础。

---

## 12.1 水平排列与自动换行

```rust
// 水平排列，超出宽度自动换行
ui.horizontal_wrapped(|ui| {
    for tag in &self.tags {
        ui.add(egui::Button::new(tag).small());
    }
});
```

### 水平对齐变体

```rust
// 水平排列，顶部对齐
ui.horizontal_top(|ui| { / ... / });

// 水平排列，居中对齐
ui.horizontal_centered(|ui| { / ... / });
```

"居中对齐"的含义是：如果水平行中有不同高度的控件，它们在**垂直方向**上居中对齐。

---

## 12.2 垂直排列变体

```rust
// 垂直排列，水平居中
ui.vertical_centered(|ui| {
    ui.heading("居中标题");
    ui.label("这段也居中");
});

// 垂直排列，居中 + 撑满高度
ui.vertical_centered_justified(|ui| {
    // 内容在垂直方向均匀分布
});
```

---

## 12.3 `with_layout` 高级布局

```rust
use eframe::egui::{self, Layout, Align};

// 自定义布局：自右向左、底部对齐
ui.with_layout(
    Layout::right_to_left(Align::BOTTOM),
    |ui| {
        ui.label("从右边开始");
        ui.label("底部对齐");
    },
);
```

---

## 12.4 `allocate_ui` — 手动分配子区域

当你需要精确控制某个矩形内的布局时——比如将一个 UI 放在指定的坐标和大小内：

```rust
// 在屏幕左上角 200x100 的区域内创建子 UI
let rect = egui::Rect::from_min_size(
    egui::Pos2::new(10.0, 10.0),  // 左上角
    egui::vec2(200.0, 100.0),     // 尺寸
);
ui.scope_builder(rect, |ui| {
    ui.label("固定位置的 UI");
});
```

配合百分比布局：

```rust
let available = ui.available_size();
let left_rect = egui::Rect::from_min_size(
    ui.cursor().min,
    egui::vec2(available.x * 0.3, available.y),
);
ui.scope_builder(left_rect, |ui| {
    ui.label("左侧 30%");
});

let right_rect = egui::Rect::from_min_size(
    egui::pos2(ui.cursor().min.x + available.x * 0.3 + 4.0, ui.cursor().min.y),
    egui::vec2(available.x * 0.7 - 4.0, available.y),
);
ui.scope_builder(right_rect, |ui| {
    ui.label("右侧 70%");
});
```

---

## 12.5 响应式尺寸与控件禁用

### 响应式尺寸

egui 的响应式能力有限——没有 CSS 的 `@media query`。但你可以用 `ui.available_width()` 手动调整：

```rust
let width = ui.available_width();
if width > 600.0 {
    // 宽屏：水平排列
    ui.horizontal(|ui| { / ... / });
} else {
    // 窄屏：垂直堆叠
    ui.vertical(|ui| { / ... / });
}
```

### `ui.set_enabled()` / `ui.add_enabled()`

控制整个 UI 区域的启用/禁用状态：

```rust
// 方式一：整个区域
ui.set_enabled(false);
ui.button("被禁用的按钮");
ui.checkbox(&mut self.value, "被禁用的复选框");
ui.set_enabled(true);  // 恢复

// 方式二：单个 Widget（更精确）
ui.add_enabled(self.is_admin, egui::Button::new("删除"));
ui.add_enabled(self.is_admin, egui::Checkbox::new(&mut self.dangerous, "危险模式"));
```

---

## 本章小结

* `ui.horizontal_wrapped` — 水平自动换行
* `ui.vertical_centered` / `vertical_centered_justified` — 垂直居中变体
* `ui.with_layout(...)` — 自定义布局方向和对齐
* `ui.scope_builder(...)` — 手动划分子区域
* `ui.add_enabled(cond, widget)` — 条件性启用/禁用控件

**下一章**：Grid 与列布局——制作表格和表单。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("水平与垂直排列", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { tags: Vec<String> }

impl Default for MyApp {
    fn default() -> Self { Self { tags: vec!["Rust".into(),"EGUI".into(),"GUI".into(),"Tutorial".into(),"中文".into(),"Hello".into(),"World".into()] } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("水平排列与自动换行");
        ui.horizontal_wrapped(|ui| {
            for tag in &self.tags {
                ui.add(egui::Button::new(tag).small());
            }
        });

        ui.separator();
        ui.heading("启用/禁用区域");
        ui.add_enabled(false, egui::Button::new("禁用按钮"));
        ui.add_enabled(true, egui::Button::new("正常按钮"));

        ui.separator();
        ui.heading("手动分配区域");
        let w = ui.available_width();
        let left = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(w * 0.4, 60.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
            ui.label("左侧 40%");
        });
        let right = egui::Rect::from_min_size(egui::pos2(ui.cursor().min.x + w * 0.4 + 4.0, ui.cursor().min.y - 60.0), egui::vec2(w * 0.6 - 4.0, 60.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
            ui.label("右侧 60%");
        });
    }
}
```

---

第三部分：布局系统

# 13 · Grid 与列布局

`Grid` 是 egui 中制作表单和数据表格的主要方式。

---

## 13.1 `Grid` 基础

```rust
egui::Grid::new("my_grid")
    .show(ui, |ui| {
        // 第 1 行
        ui.label("姓名:");
        ui.text_edit_singleline(&mut self.name);
        ui.end_row();

        // 第 2 行
        ui.label("年龄:");
        ui.add(egui::DragValue::new(&mut self.age));
        ui.end_row();

        // 第 3 行
        ui.label("邮箱:");
        ui.text_edit_singleline(&mut self.email);
        ui.end_row();
    });
```

每行末尾调用 `ui.end_row()` 或直接用 `Grid::new(...).show(ui, |ui| {...})`。Grid 自动对齐各列。

---

## 13.2 Grid 高级控制

```rust
egui::Grid::new("advanced_grid")
    .num_columns(2)             // 列数
    .min_col_width(80.0)        // 最小列宽
    .max_col_width(200.0)       // 最大列宽
    .striped(true)              // 条纹行（交替背景色）
    .spacing(egui::vec2(10.0, 6.0)) // 间距
    .show(ui, |ui| {
        // 列级对齐
        // ui.col().align = egui::Align::Center;

        for (label, value) in &self.data {
            ui.label(*label);
            ui.label(*value);
            ui.end_row();
        }
    });
```

---

## 13.3 动态 Grid

```rust
egui::Grid::new("dynamic_grid")
    .striped(true)
    .show(ui, |ui| {
        // 表头
        ui.strong("项目");
        ui.strong("数量");
        ui.strong("价格");
        ui.end_row();

        // 数据行（从 Vec 生成）
        for item in &self.items {
            ui.label(&item.name);
            ui.label(format!("{}", item.quantity));
            ui.label(format!("¥{:.2}", item.price));
            ui.end_row();
        }

        // 合计行
        ui.separator();
        ui.separator();
        ui.separator();
        ui.end_row();
        ui.strong("合计:");
        ui.label("");
        ui.strong(format!("¥{:.2}", self.total()));
        ui.end_row();
    });
```

---

## 本章小结

* `Grid::new("id").show(ui, |ui| {...})` — 创建网格
* `ui.end_row()` — 结束当前行
* `.striped(true)` — 条纹行
* `.num_columns()`, `.min_col_width()`, `.max_col_width()` — 列控制
* Grid 不适合 >100 行——此时用 `egui_extras::Table`（第 27 章）

**下一章**：滚动区域。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
﻿use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("Grid与列布局", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct Item { name: String, quantity: u32, price: f64 }

struct MyApp { items: Vec<Item> }

impl Default for MyApp {
    fn default() -> Self {
        Self { items: vec![
            Item { name: "苹果".into(), quantity: 3, price: 5.5 },
            Item { name: "香蕉".into(), quantity: 5, price: 3.0 },
            Item { name: "橙子".into(), quantity: 2, price: 7.0 },
        ]}
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("Grid 表格");

        egui::Grid::new("my_grid").striped(true).min_col_width(60.0).show(ui, |ui| {
            ui.strong("名称"); ui.strong("数量"); ui.strong("单价"); ui.strong("小计");
            ui.end_row();
            let mut total = 0.0;
            for item in &self.items {
                ui.label(&item.name);
                ui.label(format!("{}", item.quantity));
                ui.label(format!("¥{:.2}", item.price));
                let sub = item.quantity as f64 * item.price;
                ui.label(format!("¥{:.2}", sub));
                ui.end_row();
                total += sub;
            }
            ui.separator(); ui.separator(); ui.separator(); ui.separator();
            ui.end_row();
            ui.strong("合计:"); ui.label(""); ui.label("");
            ui.strong(format!("¥{:.2}", total));
            ui.end_row();
        });
    }
}
```

---

第三部分：布局系统

﻿# 14 · 滚动区域

当内容超出可见区域时，`ScrollArea` 提供滚动支持。

---

## 14.1 垂直滚动

```rust
egui::ScrollArea::vertical().show(ui, |ui| {
    for i in 0..100 {
        ui.label(format!("第 {} 行", i));
    }
});
```

设置最大高度：

```rust
egui::ScrollArea::vertical()
    .max_height(200.0)
    .show(ui, |ui| {
        // 超长内容...
    });
```

---

## 14.2 水平滚动

```rust
egui::ScrollArea::horizontal().show(ui, |ui| {
    ui.horizontal(|ui| {
        for i in 0..50 {
            ui.button(format!("按钮 {}", i));
        }
    });
});
```

---

## 14.3 自动滚到底部（聊天窗口）

```rust
// 记录滚动状态，实现"自动滚动到底部"
let mut scroll_to_bottom = self.auto_scroll;

egui::ScrollArea::vertical()
    .id_salt("chat_scroll")
    .stick_to_bottom(scroll_to_bottom) // 自动跟随底部
    .show(ui, |ui| {
        for msg in &self.messages {
            ui.label(msg);
        }
    });

fn logic(&mut self, ...) {
    // 如果用户手动上滚，关闭自动滚动
    // ...
}
```

---

## 14.4 大数据量列表的性能策略

⚠️ **重要**：egui 不内置虚拟滚动。如果列表有 10 万行，每帧都渲染它们会导致严重卡顿。
三种策略：- **手动分页**：只渲染当前页的数据
- **窗口化裁剪**：只渲染可见区域附近的 N 行
- **`ui.skip_ahead()`**：跳过不可见 Widget 的布局计算（第 36 章）
对于大多数桌面工具，数据量在几百条以内，不需要这些优化。

---

## 本章小结

* `ScrollArea::vertical()` / `horizontal()` — 滚动区域
* `.max_height()` / `.max_width()` — 限制滚动区域尺寸
* `.stick_to_bottom(true)` — 自动滚底
* 大数据集需要手动优化——egui 无虚拟滚动

**下一章**：面板系统——构建完整的应用布局框架。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
﻿use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("滚动区域", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { messages: Vec<String>, auto_scroll: bool }

impl Default for MyApp {
    fn default() -> Self { Self { messages: (0..5).map(|i| format!("消息 #{}", i)).collect(), auto_scroll: true } }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("聊天窗口（自动滚动）");
        ui.checkbox(&mut self.auto_scroll, "自动滚到底部");

        if ui.button("添加消息").clicked() {
            self.messages.push(format!("消息 #{}", self.messages.len()));
        }

        egui::ScrollArea::vertical()
            .id_salt("chat")
            .stick_to_bottom(self.auto_scroll)
            .max_height(200.0)
            .show(ui, |ui| {
                for msg in &self.messages { ui.label(msg); }
            });

        ui.separator();
        ui.label(format!("共 {} 条消息", self.messages.len()));
    }
}
```

---

第三部分：布局系统

# 15 · 面板系统

面板（Panel）是 egui 中构建完整应用布局的核心组件。

> egui 0.34 统一了面板 API：`Panel::top()` / `Panel::left()` 等替代了旧的 `TopBottomPanel` / `SidePanel`，`.show_inside()` 替代了 `.show()`。

---

## 15.1 `CentralPanel` 中央面板

```rust
egui::CentralPanel::default().show_inside(ui, |ui| {
    ui.heading("主内容区");
    // 所有主要内容...
});
```

占据整个可用空间（减去侧边栏和上下栏）。

---

## 15.2 侧边栏 `Panel::left()` / `Panel::right()`

```rust
// 左侧栏
egui::Panel::left("left_panel").resizable(true)    // 可拖拽调整宽度
    .default_size(200.0)                            // 默认宽度
    .min_width(100.0)                               // 最小宽度
    .show_inside(ui, |ui| {
        ui.heading("导航");
        ui.separator();
        for page in &["首页", "设置", "关于"] {
            if ui.selectable_label(self.current_page == page, page).clicked() {
                self.current_page = page.to_string();
            }
        }
    });

// 右侧栏
egui::Panel::right("right_panel")
    .default_size(250.0)
    .show_inside(ui, |ui| {
        ui.heading("属性面板");
    });

// 主内容区
egui::CentralPanel::default().show_inside(ui, |ui| {
    ui.heading(format!("当前页面: {}", self.current_page));
});
```

> 旧版 API（仍可用但已废弃）：`SidePanel::left(...)` → `Panel::left(...)`、`SidePanel::right(...)` → `Panel::right(...)`

---

## 15.3 上下栏 `Panel::top()` / `Panel::bottom()`

```rust
// 顶部工具栏
egui::Panel::top("toolbar")
    .resizable(true)
    .min_height(30.0)
    .show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("我的应用");
            ui.separator();
            if ui.button("新建").clicked() { / ... / }
            if ui.button("保存").clicked() { / ... / }
        });
    });

// 底部状态栏
egui::Panel::bottom("status_bar")
    .show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("就绪");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("行: 1, 列: 1");
            });
        });
    });
```

> 旧版 API（已废弃）：`TopBottomPanel::top(...)` → `Panel::top(...)`、`TopBottomPanel::bottom(...)` → `Panel::bottom(...)`

---

## 15.4 完整布局骨架

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    // 顶部工具栏
    egui::Panel::top("toolbar")
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("编辑器");
                if ui.button("打开").clicked() { / ... / }
                if ui.button("保存").clicked() { / ... / }
            });
        });

    // 左侧文件树
    egui::Panel::left("file_tree").resizable(true)
        .default_size(180.0)
        .show_inside(ui, |ui| {
            ui.heading("文件");
            for file in &self.files {
                ui.selectable_label(false, file);
            }
        });

    // 右侧属性面板
    egui::Panel::right("properties")
        .default_size(200.0)
        .show_inside(ui, |ui| {
            ui.heading("属性");
        });

    // 底部状态栏
    egui::Panel::bottom("status")
        .show_inside(ui, |ui| {
            ui.label("第 1 行, 第 1 列 | UTF-8 | 就绪");
        });

    // 中央编辑区
    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.content)
                    .desired_width(f32::INFINITY)
            );
        });
    });
}
```

---

## 本章小结

* `CentralPanel` — 占据剩余空间的主内容区
* `Panel::left("id")` / `Panel::right("id")` — 可拖拽调整宽度的侧边栏
* `Panel::top("id")` / `Panel::bottom("id")` — 工具栏/状态栏
* `Panel` → `.show_inside(ui, |ui| {...})` — 新统一 API
* 旧 API（`SidePanel` / `TopBottomPanel`）已废弃但仍可用

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("面板系统", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { current_page: String, content: String, files: Vec<String> }

impl Default for MyApp {
    fn default() -> Self {
        Self { current_page: "首页".into(), content: String::new(),
            files: vec!["main.rs".into(), "app.rs".into(), "utils.rs".into()] }
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 顶部工具栏
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("编辑器");
                if ui.button("新建").clicked() { self.content.clear(); }
                if ui.button("保存").clicked() { /* save */ }
            });
        });

        // 左侧文件树
        egui::Panel::left("file_tree").resizable(true).default_size(150.0).show_inside(ui, |ui| {
            ui.heading("文件");
            for file in &self.files {
                let _ = ui.selectable_label(self.current_page == *file, file);
            }
        });

        // 底部状态栏
        egui::Panel::bottom("status").show_inside(ui, |ui| {
            ui.label(format!("{} | UTF-8 | 就绪", self.current_page));
        });

        // 中央编辑区
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(format!("当前页面: {}", self.current_page));
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.content).desired_width(f32::INFINITY).desired_rows(15));
            });
        });
    }
}
```