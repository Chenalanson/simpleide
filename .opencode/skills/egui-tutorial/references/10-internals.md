# 深入原理


---

第十部分：深入原理

# 38 · Immediate Mode 设计哲学

---

## 38.1 历史渊源

Immediate Mode GUI 的现代复兴始于 **Dear ImGui**（2014 年，C++ 库，作者 Omar Cornut）。它的诞生背景是游戏开发工具——需要快速迭代、嵌入到已有渲染循环中、不引入状态同步复杂度。

egui 是 Dear ImGui 在 Rust 生态中的精神继承者——借鉴了核心思想，但以纯 Rust 重写并获得更强的类型安全。

---

## 38.2 核心思想

> **UI = f(state)**
>
> 界面是应用状态的函数，每帧从状态计算得到 UI 描述。

```rust
state → ui() → UI 描述 → layout → tessellation → 渲染
  ↑                                                    │
  └────────── 用户交互 ─────────────────────────────────┘
```

对比 Retained Mode：

```rust
state → 数据绑定 → 控件树更新 → 增量布局 → 增量渲染
         ↑ 复杂！     ↑ 持久对象  ↑ 增量算法
```

Immediate Mode 的简单性来自于**不需要增量更新**——每帧从零构建。

---

## 38.3 CPU 换开发体验

每帧重建 UI 描述的代价有多大？

|  |  |
| --- | --- |
| 操作 | 大致耗时 |

|  |  |
| --- | --- |
| 一个 button 的 ui() 调用 | ~50 ns |
| 一个 label 的 ui() 调用 | ~30 ns |
| 1000 个 Widget 的一帧 | ~50 µs |
| 布局计算 + tessellation | ~500 µs ~ 2 ms |

对于 99% 的应用（几百个 Widget + 复杂绘制），每帧总耗时在毫秒级别——完全不影响 60 FPS。

---

## 38.4 适用与不适用

|  |  |
| --- | --- |
| ✅ 最优 | ❌ 不太适合 |

|  |  |
| --- | --- |
| 桌面工具/编辑器 | 手机/平板（电池敏感：持续重绘耗电） |
| 数据仪表盘 | 游戏内 UI（已有自己的渲染管线） |
| 开发调试面板 | 复杂动画/视频/PDF（需特殊优化） |
| 快速原型 | 需要原生外观的应用 |

---

## 本章小结

* Immediate Mode = UI 是状态的函数，每帧重建
* 灵感来源：Dear ImGui（C++）
* 核心取舍：CPU 换开发体验
* 对于工具类应用，这是最优选择

**下一章**：egui 架构剖析。

---

> 📁 本章无代码示例（纯概念）

---

第十部分：深入原理

# 39 · egui 架构剖析

---

## 39.1 crate 数据流

```rust
用户代码 (MyApp)
    │ logic()
    │ ui()  ──── 调用 Widget API
    ▼
egui crate
    │ Context::run() ── 调用 ui()
    │ Ui ── 分配空间、管理布局
    │ Painter ── 收集 Shapes
    ▼
epaint crate
    │ Tessellator ── Shapes → 三角形网格
    │ Fonts ── 文本排版、字形图集管理
    │ TextureManager ── GPU 纹理
    ▼
egui-wgpu / egui_glow
    │ 提交 GPU 命令
    ▼
GPU 渲染
```

---

## 39.2 `Context::run()` 执行流程

```rust
// 简化后的 Context::run() 内部流程
impl Context {
    fn run(&mut self, new_input: RawInput, ui_closure: impl FnOnce(&Context)) -> FullOutput {
        // 1. begin_frame: 处理输入、更新 Memory、准备字体
        self.begin_frame(new_input);

        // 2. 调用用户的 ui() 闭包 → 收集所有 Widget 的 Shape
        ui_closure(self);

        // 3. end_frame: 执行布局、Tessellation、生成 GPU 命令
        self.end_frame()
    }
}
```

---

## 39.3 Ui 的内部机制

`Ui` 包含两个关键部分：

* **placer**（布局器）— 决定 Widget 放置位置
* **region**（可用区域）— 当前 Ui 的矩形范围

每添加一个 Widget，placer 向前推进 cursor，region 缩小，`available_size` 更新。

---

## 39.4 Tessellator 网格化

```rust
Shape (圆形/矩形/文本/路径...)
    → Tessellator::tessellate()
        → Vec<ClippedPrimitive>
            → ClipRect + Mesh(vertices, indices)
                → GPU 可使用的三角形数据
```

文本特殊处理：已预排版为 `Galley`（在 Fonts 中缓存），tessellator 直接从字形图集中查找纹理。

---

## 本章小结

* egui → epaint → 渲染后端 = 三层架构
* `Context::run()` = begin\_frame → ui() → end\_frame
* `Ui` = placer + region
* `Tessellator` = Shapes → Mesh → GPU 三角形

**下一章**：手写极简 Immediate Mode GUI。

---

> 📁 本章无代码示例（源码分析）

---

第十部分：深入原理

# 40 · 手写极简 Immediate Mode GUI

本章实现一个 ~300 行的玩具 IM GUI——只依赖 `winit` + `wgpu`，不依赖 `egui`。目标是理解 IM GUI 的**本质**。

---

## 40.1 目标

实现一个极简 IM GUI 库，包含：

* 自上而下布局
* 一个按钮（矩形 + 文字）
* 一个滑块
* 颜色绘制

---

## 40.2 核心架构

```rust
// 布局器
struct Ui {
    cursor: f32,           // 当前 Y 位置
    spacing: f32,
}

impl Ui {
    fn allocate(&mut self, height: f32) -> Rect { / ... / }
}

// 按钮（返回是否被点击）
fn button(ui: &mut Ui, input: &Input, label: &str) -> (Rect, bool) {
    let rect = ui.allocate(30.0);
    let hovered = rect.contains(input.mouse_pos);
    let clicked = hovered && input.mouse_clicked;
    // 加入绘制命令
    add_draw_command(DrawCommand::Rect { rect, color: if hovered { ... } else { ... } });
    add_draw_command(DrawCommand::Text { pos: rect.left_top(), text: label });
    (rect, clicked)
}
```

---

## 40.3 事件循环

```rust
// winit 事件循环
event_loop.run(move |event, _, control_flow| {
    match event {
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::RedrawRequested => {
                // 1. 构建 UI（每帧重新构建）
                let mut ui = Ui::new();
                let mut draw_commands = Vec::new();

                if button(&mut ui, &input, "点击我", &mut draw_commands) {
                    count += 1;
                }

                // 2. 绘制到 wgpu
                render(&draw_commands, &surface, &device, &queue);
            }
            _ => {}
        },
        _ => {}
    }
});
```

---

## 40.4 回顾：玩具版 vs 真正的 egui

|  |  |  |
| --- | --- | --- |
| 特性 | 玩具版 | 真正的 egui |

|  |  |  |
| --- | --- | --- |
| 布局 | 固定间距 | 灵活布局系统（horizontal/vertical/grid） |
| 绘制 | 简单矩形 | Shape 枚举（14+ 变体） |
| 文本 | ASCII 字形 | 完整字体系统 + 纹理图集 + Fallback |
| 状态 | 无 | Id → Memory 持久化 |
| 样式 | 硬编码 | Visuals + Style 完整体系 |
| 输入 | 简单鼠标 | 键盘/鼠标/触控/IME |
| 渲染 | 单 pass | 多 pass + 裁剪 + PaintCallback |

---

## 本章小结

* ~300 行代码可以写出 IM GUI 的基本骨架
* 核心循环：构建 UI → 绘制 → 下一帧重建
* egui 的价值在于那 14 个特性——让玩具变成生产级工具

**下一章**：egui + winit + wgpu 手动集成。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
// 40 · 手写极简 IM GUI（概念演示）
// 完整实现需 winit + wgpu（约300行），此处展示核心逻辑骨架

use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("极简IMGUI概念", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp { count: i32 }

impl Default for MyApp { fn default() -> Self { Self { count: 0 } } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("Minimal IMGUI Concept");

        ui.label("core loop: | Ui state + user code → drawing commands → GPU → repeat");
        ui.label(format!("Think: count = {}", self.count));

        // 模拟手写版的核心循环
        // 1. fn button(ui, label) → rect + clicked
        // 2. fn label(ui, text) → rect
        // 3. fn paint(draw_commands, renderer)
        // 4. event_loop.run(move |event| { ... })

        if ui.button("+1").clicked() { self.count += 1; }
    }
}
```

---

第十部分：深入原理

﻿# 41 · egui + winit + wgpu 手动集成

本章绕过 eframe，手动搭建 egui 的底层集成——以深度理解 egui 与窗口系统和 GPU 的关系。

---

## 41.1 为什么绕过 eframe？

* 完全控制事件循环
* 集成到游戏引擎（如自定义 wgpu 管线）
* 深度定制渲染流程
* 学习目的：理解每一层在做什么

---

## 41.2 依赖

```rust
[dependencies]
egui = "0.34"
egui-winit = "0.34"
egui-wgpu = "0.34"
winit = "0.30"
wgpu = "24.0"
```

---

## 41.3 核心集成流程

```rust
// 1. 创建 winit 窗口
let event_loop = winit::event_loop::EventLoop::new()?;
let window = winit::window::Window::new(&event_loop)?;

// 2. 创建 wgpu 实例和设备
let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
let surface = instance.create_surface(&window)?;
let adapter = instance.request_adapter(...).await?;
let (device, queue) = adapter.request_device(...).await?;
let surface_config = / ... /;
let mut egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1);

// 3. 创建 egui-winit 事件桥接
let mut egui_winit_state = egui_winit::State::new(
    egui::Context::default(),
    egui::ViewportId::ROOT,
    &window,
    None,
    None,
);

// 4. 事件循环
event_loop.run(move |event, _, control_flow| {
    match event {
        Event::WindowEvent { event, .. } => {
            // 将 winit 事件输入 egui
            let egui_response = egui_winit_state.on_window_event(&window, &event);

            if egui_response.consumed {
                // egui 消费了此事件（如点击按钮）
                return;
            }

            match event {
                WindowEvent::RedrawRequested => {
                    // 获取 egui 输入
                    let raw_input = egui_winit_state.take_egui_input(&window);

                    // 运行 egui 帧
                    let full_output = egui_ctx.run(raw_input, |ctx| {
                        // ui() 代码
                        egui::CentralPanel::default().show_inside(ui, |ui| {
                            ui.label("手动集成 egui");
                        });
                    });

                    // 更新 winit 状态
                    egui_winit_state.handle_platform_output(
                        &window, full_output.platform_output
                    );

                    // 渲染 egui
                    let paint_jobs = egui_ctx.tessellate(
                        full_output.shapes,
                        full_output.pixels_per_point
                    );
                    // ... 更新 wgpu buffer ...
                    // ... render pass 中调用 egui_renderer.render() ...
                }
                _ => {}
            }
        }
        _ => {}
    }
});
```

---

## 本章小结

* 手动集成 = 可控但繁琐
* `egui-winit` — 事件桥接
* `egui-wgpu` — 渲染集成
* 一般开发直接用 eframe 即可

**下一章**：渲染管线详解。

---

> 📁 完整可运行代码见下方源码框，或进入 `example/` 目录执行 `cargo run`

📄 src/main.rs — 完整可运行代码

[查看源文件](example/src/main.rs)
 |
cd example && cargo run

```rust
// 41 · egui + winit + wgpu 手动集成
// 完整实现需 winit + wgpu + egui-winit + egui-wgpu（见教程正文）
// 此处展示核心集成骨架，用 eframe 模拟

use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native("手动集成概念", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

struct MyApp;

impl Default for MyApp { fn default() -> Self { Self } }

impl eframe::App for MyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("egui + winit + wgpu 手动集成");

        ui.label("核心流程:");
        ui.label("1. winit::EventLoop → Window");
        ui.label("2. wgpu::Instance → Adapter → Device + Queue → Surface");
        ui.label("3. egui_winit::State — 事件桥接");
        ui.label("4. egui_wgpu::Renderer — 渲染后端");
        ui.label("5. event_loop.run(move |event| { ... })");

        ui.separator();
        ui.label("// 伪代码:");
        ui.label("let mut egui_winit = egui_winit::State::new(...);");
        ui.label("let mut egui_renderer = egui_wgpu::Renderer::new(...);");
        ui.label("// event_loop.run(|event| match event {");
        ui.label("//   RedrawRequested => {");
        ui.label("//     let input = egui_winit.take_egui_input(&window);");
        ui.label("//     let output = egui_ctx.run(input, |ctx| { ui_code(ctx); });");
        ui.label("//     egui_renderer.update_buffers(...);");
        ui.label("//     egui_renderer.render(&mut render_pass, ...);");
        ui.label("//   }");
        ui.label("// })");

        ui.separator();
        ui.label("完整可运行示例见教程正文 ch41");
    }
}
```

---

第十部分：深入原理

# 42 · 渲染管线详解

---

## 42.1 Tessellation 过程

```rust
Shape (Circle/Rect/Text/Path/Bezier/Mesh)
  ├─ 贝塞尔 → 线段近似
  ├─ 文本 → 字形图集中查找 → 四边形
  └─ 圆角矩形 → 三角形扇
      ↓
  ClippedPrimitive (ClipRect + Mesh)
      ↓
  GPU Draw Call (wgpu/glow)
```

* 圆角矩形 → 分成 9 个 patch（4 角 + 4 边 + 中心）
* 文本 → 每个字形 = 一个四边形 + 从 Font Atlas 采样的纹理坐标

---

## 42.2 字体图集 (Font Atlas)

```rust
Font Atlas (纹理)
┌─────────────────────┐
│ ABCDEFGHIJK         │
│ LMNOPQRSTUV         │  ← 每个字形预先渲染到纹理
│ WXYZabcdefg         │
│ hijk...你好世       │  ← 按需追加（用到才加载）
│ 界...□□□...         │  ← 未加载的字形 = □
└─────────────────────┘
```

* 初始大小：约 1024×1024
* 按需加载字形：用过的字形才上传到纹理
* 图集满时：调整大小或逐出未使用的字形（LRU）
* 这就是为什么首次显示中文字时会有短暂延迟——字形从字体文件加载到纹理

---

## 42.3 wgpu vs glow 对比

|  |  |  |
| --- | --- | --- |
| 特性 | wgpu | glow |

|  |  |  |
| --- | --- | --- |
| 底层 API | Vulkan/Metal/DX12/WebGPU | OpenGL 3.3+ / WebGL 2.0 |
| 跨平台 | ✅ 极好 | ✅ 好 |
| Web 支持 | WebGPU (Chrome) + WebGL fallback | WebGL 2.0 |
| 安全性 | Rust wrapper，零 unsafe | GL 绑定，有 unsafe |
| 性能 | 现代 API 开销更低 | GL 驱动开销 |

**0.34 起 wgpu 是默认值**——大多数情况不需要改变。

---

## 本章小结

* Tessellation = Shape → Mesh → GPU
* Font Atlas = 按需加载的字形纹理缓存
* wgpu 是现代默认渲染后端

**下一章**：测试 egui 应用。

---

> 📁 本章无代码示例（原理章）