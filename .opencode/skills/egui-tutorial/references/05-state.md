# 状态管理


---

第五部分：状态管理

﻿# 18 · egui 的状态管理模型

这是 egui 最核心的概念之一。理解它，你就理解了 egui 的一半。

---

## 18.1 状态为什么不丢失？

回到第 2 章的计数器：

```rust
struct MyApp { count: i32 }

fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    if ui.button("+1").clicked() {
        self.count += 1;
    }
    ui.label(format!("{}", self.count));
}
```

`count` 的值在帧之间保留——因为 `MyApp` 由 eframe 持有在内存中。这很容易理解。
但 **Slider 的值**是如何跨帧保留的？

```rust
let mut value = 0.5;  // 局部变量！每帧重新初始化！
ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
```

如果 `value` 每帧都重置为 0.5，为什么 Slider 仍然"记住"了上次拖动的位置？
**答案**：`Slider` 在 `Memory` 中存储了它的值——按 `Id` 索引。

---

## 18.2 `Id` 系统

egui 中每个 Widget 都有一个 `Id`，用于在 `Memory` 中存取状态。

### Id 的生成

```rust
// 1. 从字符串生成
let id = egui::Id::new("my_widget");

// 2. 从 hashable 值生成
let id = egui::Id::from(42);

// 3. 层级 Id
let id = egui::Id::new("parent").with("child");
// 等价于: "parent.child" 的 hash
```

### Id 如何工作

```rust
ui.button("OK")
  └─ 内部计算 Id = hash("OK") + ui.next_auto_id()
      └─ 在 Memory 中: Memory.data[Id] → Any
          └─ Widget 跨帧读取/写入状态
```

### `ui.push_id()` —— 区分同名 Widget

```rust
// ❌ 错误：两个同名按钮共享同一个 Id，状态冲突
for i in 0..10 {
    ui.button("删除");  // 10 个按钮都叫 "删除" — Id 相同！
}

// ✅ 正确：用 push_id 为每个分配唯一 Id
for i in 0..10 {
    ui.push_id(i, |ui| {
        ui.button("删除");  // 每个按钮的 Id = hash("删除") + hash(i)
    });
}
```

---

## 18.3 `Memory` 的数据存储

`Memory` 是 `Context` 内部的一个持久化 K/V 存储：

```rust
// 写入自定义数据到 Memory
ctx.memory_mut(|mem| {
    mem.data.insert_temp::<MyState>(
        egui::Id::new("my_state"),
        my_state,
    );
});

// 读取
ctx.memory(|mem| {
    let state: Option<&MyState> =
        mem.data.get_temp::<MyState>(egui::Id::new("my_state"));
});
```

### 类型擦除

`Memory.data` 使用 `Any` 类型擦除——存的是 `Box<dyn Any>`，读取时需要指定具体类型。如果类型不匹配，返回 `None`（不会 panic）。

---

## 18.4 ID 冲突——头号陷阱

最常见的 egui 问题：两个 Widget 拥有相同的 `Id`，导致状态互相覆盖。

```rust
// ❌ 典型错误
ui.add(egui::TextEdit::singleline(&mut self.first_name));
ui.add(egui::TextEdit::singleline(&mut self.last_name));
// 危险！如果 first_name 和 last_name 是空字符串，
// TextEdit 自动生成的 Id 可能相同！
```

**解决方案**：

```rust
// ✅ 方式一：push_id
ui.push_id("first_name", |ui| {
    ui.add(egui::TextEdit::singleline(&mut self.first_name));
});
ui.push_id("last_name", |ui| {
    ui.add(egui::TextEdit::singleline(&mut self.last_name));
});

// ✅ 方式二：显式 id_source
ui.add(
    egui::TextEdit::singleline(&mut self.first_name)
        .id_salt("first_name")
);

// ✅ 方式三：检查实际 Id
// 名称字符串本身就是 Id 的一部分（但是否充分取决于上下文）
```

### ID 冲突的症状

* 两个输入框显示相同的内容
* Checkbox 的状态在另一个 Checkbox 上反映
* Slider 拖动时其他 Slider 跟着动
* 窗口位置/大小"漂移"到其他窗口

---

## 18.5 总结：三种状态存储

|  |  |  |
| --- | --- | --- |
| 存储位置 | 生命周期 | 适合 |

|  |  |  |
| --- | --- | --- |
| `MyApp` 字段 | 应用生命周期 | 应用数据（count、items、settings） |
| `Memory`（通过 `Id`） | 应用生命周期 | Widget 内部状态（slider 值、折叠状态） |
| 局部变量 | 单帧 | 临时计算 |

> **核心原则**：你的数据存在 `MyApp` 中。Widget 的内部状态由 egui 通过 Id 自动管理。**永远不需要手动保存 Slider 的值**——egui 已经在 Memory 中为你做好了。

---

## 本章小结

* `MyApp` 字段跨帧保留——因为结构体由 eframe 持有
* `Id` 系统为 Widget 在 `Memory` 中存取内部状态
* `ui.push_id()` 解决同名 Widget 的 Id 冲突
* ID 冲突是 egui 的 #1 陷阱——症状包括状态串扰、窗口漂移

**下一章**：帧逻辑分离——`logic()` 与 `ui()` 的最佳实践。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("状态管理", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { first_name: String, last_name: String }

impl Default for MyApp { fn default() -> Self { Self { first_name: String::new(), last_name: String::new() } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("ID 冲突演示");

        // ❌ 危险：两个空字符串的 TextEdit 可能有相同 Id
        ui.label("错误: 两个同名输入框（如均为空字符串）可能导致状态冲突");
        ui.push_id("first", |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.first_name).hint_text("名"));
        });
        ui.push_id("last", |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.last_name).hint_text("姓"));
        });

        ui.separator();
        ui.label(format!("全名: {} {}", self.first_name, self.last_name));

        // 动态列表（更典型的 Id 冲突场景）
        ui.separator();
        ui.heading("动态列表（push_id 演示）");
        for i in 0..3 {
            ui.push_id(i, |ui| {
                ui.label(format!("项目 #{}", i));
            });
        }
    }
}
```

---

第五部分：状态管理

﻿# 19 · 帧逻辑分离

egui 0.34 将旧的 `update(ctx, frame)` 拆分为 `logic(ctx, frame)` + `ui(ctx)`。本章深入讲解原理和最佳实践。

---

## 19.1 为什么拆分？

**0.33 及之前**：

```rust
fn update(&mut self, ctx: &Context, frame: &mut Frame) {
    // 逻辑和 UI 混在一起
    self.process_input(ctx);
    self.fetch_data();
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.button("...");
    });
    self.save(ctx);
}
```

问题：

* 数据处理和 UI 渲染交织，难以阅读
* 测试数据逻辑需要模拟 UI 上下文
* 副作用容易泄漏到 UI 代码中

**0.34**：

```rust
fn logic(&mut self, ctx: &Context, frame: &mut Frame) {
    // 纯逻辑
}
fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
    // 纯 UI 描述
}
```

---

## 19.2 数据流模式

```rust
logic() 处理输入/数据
  → 修改 MyApp 字段
    → ui() 读取字段并渲染
      → ui() 返回交互结果（Response）
        → 下一帧 logic() 消费交互结果
```

### 示例

```rust
struct MyApp {
    // 数据字段
    items: Vec<String>,
    filter: String,
    // 交互标志（ui 设置，logic 消费）
    save_requested: bool,
    delete_index: Option<usize>,
}

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 消费交互标志
        if self.save_requested {
            self.save_to_disk();
            self.save_requested = false;
        }
        if let Some(idx) = self.delete_index.take() {
            self.items.remove(idx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    self.save_requested = true; // 设置标志
                }
            });
            // ... 显示 items ...
            for (i, item) in self.items.iter().enumerate() {
                if ui.button("✕").clicked() {
                    self.delete_index = Some(i);
                }
            }
        });
    }
}
```

---

## 19.3 职责划分

|  |  |
| --- | --- |
| `logic()` | `ui()` |

|  |  |  |
| --- | --- | --- |
| `&mut self` | ✅ | ✅ |
| `&Context` | ✅ | ✅ |
| `&mut Frame` | ✅ | ❌ |
| 做什么 | 数据 CRUD、I/O、快捷键检测 | UI 描述、设置交互标志 |
| 不做什么 | 调用 `ui.button()` 等 Widget | 网络请求、文件写入 |

---

## 本章小结

* 0.34 拆分 = 可读性 + 可测试性
* 数据流：logic 改状态 → ui 读状态 + 设标志 → logic 消标志
* 交互标志模式：`save_requested` / `delete_index` 等

**下一章**：serde 持久化应用状态。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("帧逻辑分离", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { count: i32, save_requested: bool, saved_count: i32 }

impl Default for MyApp { fn default() -> Self { Self { count: 0, save_requested: false, saved_count: 0 } } }

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理快捷键
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::S) {
                self.save_requested = true;
            }
        });

        // 消费 ui() 设置的交互标志
        if self.save_requested {
            self.saved_count = self.count;
            self.save_requested = false;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("logic / ui 分离演示");

        if ui.button("+1").clicked() { self.count += 1; }
        if ui.button("-1").clicked() { self.count -= 1; }

        ui.label(format!("当前计数: {}", self.count));
        ui.label(format!("上次保存: {}", self.saved_count));

        if ui.button("保存").clicked() {
            self.save_requested = true; // 设置标志 → 在 logic 中消费
        }

        ui.separator();
        ui.label("提示: Ctrl+S 也能触发保存（在 logic 中检测）");
    }
}
```

---

第五部分：状态管理

# 20 · serde 持久化应用状态

eframe 内置了对应用状态持久化的支持——通过 `App::save()` 和 `App::load()`。

---

## 20.1 serde 极小入门

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Config {
    theme: String,
    font_size: f32,
}

// 序列化
let json = serde_json::to_string(&config).unwrap();

// 反序列化
let config: Config = serde_json::from_str(&json).unwrap();
```

在 `Cargo.toml` 中：

```rust
[dependencies]
eframe = "0.34"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

## 20.2 `save()` / `load()` 签名

```rust
impl eframe::App for MyApp {
    fn save(&self, _storage: &mut dyn eframe::Storage) {
        // ⚠️ 注意：&self 是不可变引用！
        // 可以读取字段，但不能修改
        let json = serde_json::to_string(&self.config).unwrap();
        _storage.set_string("config", json);
    }

    fn load(&self, _storage: &dyn eframe::Storage) {
        // ⚠️ 同样 &self 不可变！
        // 无法直接覆盖 MyApp 字段！
    }
}
```

---

## 20.3 `load()` 限制的解决方案

因为 `load(&self)` 不可变，无法直接修改 `MyApp` 字段。三种方案：

### 方案 ③（推荐）：在 `MyApp::new` 中通过 `CreationContext` 读取

```rust
impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 从 cc.storage 读取持久化数据（此时可以自由修改）
        if let Some(storage) = cc.storage {
            if let Some(json) = storage.get_string("config") {
                if let Ok(config) = serde_json::from_str(&json) {
                    return Self { config, ..Default::default() };
                }
            }
        }
        Self::default()
    }
}
```

`CreationContext` 在 `main` 中的闭包里可用。

---

## 20.4 自动持久化

eframe 自动约每 30 秒调用一次 `save()`。你也可以手动触发：

```rust
ctx.request_save(); // 请求在下一帧保存
```

---

## 20.5 持久化窗口布局

窗口位置/大小会自动持久化（eframe 内置）。你也可以手动控制：

```rust
// 在 save 中手动保存窗口状态
fn save(&self, storage: &mut dyn eframe::Storage) {
    // 窗口状态已自动保存
}
```

---

## 20.6 版本兼容性

```rust
#[derive(Serialize, Deserialize)]
struct Config {
    theme: String,
    font_size: f32,
    #[serde(default)]  // 新字段必须有默认值
    last_file: String,
}
```

---

## 本章小结

* `save(&self, &mut dyn Storage)` — 每 ~30s 自动调用
* `load(&self, &dyn Storage)` — 不可变引用是限制
* **推荐**在 `MyApp::new(cc)` 中读取初始配置
* `#[serde(default)]` 保证版本兼容
* `ctx.request_save()` 主动触发保存

**下一章**：Style 与 Visuals——控制 egui 的一切外观。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("持久化", eframe::NativeOptions::default(),
        Box::new(|cc| {
            let mut app = MyApp::default();
            if let Some(storage) = cc.storage {
                if let Some(json) = storage.get_string("app_state") {
                    if let Ok(state) = serde_json::from_str::<AppState>(&json) {
                        app.count = state.count;
                    }
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AppState { count: i32 }

struct MyApp { count: i32 }

impl Default for MyApp { fn default() -> Self { Self { count: 0 } } }

impl eframe::App for MyApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = AppState { count: self.count };
        if let Ok(json) = serde_json::to_string(&state) {
            storage.set_string("app_state", json);
        }
    }

    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("持久化演示");
        ui.label("关闭窗口后重新打开，计数会被保留");

        if ui.button("+1").clicked() { self.count += 1; }
        ui.label(format!("计数: {}", self.count));

        ui.separator();
        ui.label("提示：eframe 约每 30 秒自动调用 save()");
    }
}
```