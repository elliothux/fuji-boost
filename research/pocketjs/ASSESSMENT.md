# PocketJS → Fujifilm X-T4 可行性

日期：2026-10-23（Asia/Shanghai）。仅源码审阅，未编译、移植或实机运行。

源码固定在 `e27bff3e96df3c7b3c8263a34f2362b964df04fb`。`source/` 是 no-checkout partial clone，选取的源码在 `files/`，版本记录在 `provenance.json`。GitHub connector 本轮连接失败，GitHub tree API 403，改用只读 Git/source blob 获取；未安装依赖或改动工具环境。

## 判断

PocketJS **适合作为自定义机内 UI 的候选移植对象**。它提供可复用的无浏览器 UI 核心、C 接口和便携 QuickJS 运行时。ThreadX 与此设计并不冲突，但项目没有可确认的现成 X-T4/ThreadX host；本轮 tree 内无 ThreadX 命名路径。不能把“具备可移植结构”称为“已支持 X-T4”。

相机的主控制/UI 可理解为 ThreadX 上协作的任务，很多代码与 RTOS、驱动一起静态链接进固件；任务不等于可独立安装、卸载的桌面/手机应用。ISP、编码与协处理器部分也不能全部视作 ThreadX 普通任务。

## 实际读到的关键源码

| 源码 | 实际机制 | 对移植的意义 |
|---|---|---|
| `engine/core/Cargo.toml`、`src/lib.rs` | `no_std` + `alloc`；布局依赖 taffy 0.11 的 alloc/flexbox 功能；核心不依赖 QuickJS | 不要求移植 Linux/Rust std，但仍需要内存分配器 |
| `engine/ui-cabi/Cargo.toml`、`src/lib.rs` | `staticlib`；C ABI；`bare-platform`、`software-only`、`host-allocator`；宿主提供 alloc/realloc/free 和 abort | 可由机内 C/汇编扩展调用；需要匹配 ARM ABI、内存对齐和错误行为 |
| `engine/quickjs-c/pocket_runtime.c`、`.h`、README | 管理 QuickJS runtime/context、安装 UI operations、加载 bundle、逐帧推进 guest/jobs/core | 可以复用这层，目标 host 无需从零实现全部 UI HostOps；仍需编译匹配的 QuickJS 及其依赖 |
| `engine/ui-cabi/src/lib.rs` 顶部契约 | 同一 UI 线程同步调用，单个 Ui 实例；software capture 返回 top-left ARGB32；GLES 需已有当前 context | 对 ThreadX 用单一拥有者任务和队列，不能在多核/中断中随意重入；相机实际显示格式未确定 |
| `hosts/esp-idf/components/pocketjs_ui_qjs/README.md` | binding 只执行 turn，明确不渲染、不 present、不创建 task | 执行 JS 与输出屏幕可分开适配 |
| `hosts/esp-idf/components/pocketjs_runner/README.md` 与 `runner.h` | 可选 owning task、输入采样/after_turn callbacks、停止/加入逻辑 | 架构可参考；其 ESP-IDF 类型和 API 不能直接编译成 ThreadX host |
| `hosts/esp-idf/components/pocketjs_render_rgb565/README.md` | 软件 RGB565 renderer；宿主管理缓冲与同步，prepare/render/commit/abort | 初版可以软件渲染；仍需适配真实相机 display/overlay 格式和所有权 |

参考链接：

- [项目 README](https://github.com/pocket-nexus/pocketjs/blob/e27bff3e96df3c7b3c8263a34f2362b964df04fb/README.md)
- [核心 manifest](https://github.com/pocket-nexus/pocketjs/blob/e27bff3e96df3c7b3c8263a34f2362b964df04fb/engine/core/Cargo.toml)
- [C ABI manifest](https://github.com/pocket-nexus/pocketjs/blob/e27bff3e96df3c7b3c8263a34f2362b964df04fb/engine/ui-cabi/Cargo.toml)
- [C ABI 实现](https://github.com/pocket-nexus/pocketjs/blob/e27bff3e96df3c7b3c8263a34f2362b964df04fb/engine/ui-cabi/src/lib.rs)
- [Portable QuickJS C runtime](https://github.com/pocket-nexus/pocketjs/blob/e27bff3e96df3c7b3c8263a34f2362b964df04fb/engine/quickjs-c/README.md)
- [Native contract](https://github.com/pocket-nexus/pocketjs/blob/e27bff3e96df3c7b3c8263a34f2362b964df04fb/site/content/docs/native-contract.md)

## 候选 X-T4 Host 需要提供什么

### 通用 ThreadX 适配与设备适配的边界

可以将未来适配分成三部分，但项目当前没有本轮已确认的通用 ThreadX host：

- **可复用运行时：** TS/TSX 应用、QuickJS glue、PocketJS core、布局/字体/draw list 与软件渲染算法。应用所需能力与目标提供的能力仍须匹配。
- **可复用 ThreadX glue：** 任务/队列、内存池或独立 arena、帧驱动、停止与清理；前提是 ThreadX API 可调用且配置满足需求。它不能自动获得显示、按键或相机服务。
- **设备/固件适配：** CPU target 与 ABI、可用内存区域、像素格式/stride/缓存与显示提交、输入扫描/事件、SD 资源读取、相机命令、加载入口。X-T4 是向已有闭源系统加入扩展，原厂服务的地址、结构与所有权还可能随固件版本变化。

因此“能移植到 ThreadX”指源代码层面的可复用架构，不等于一个二进制可复制到任意 ThreadX 设备。若设备厂商已提供统一显示/输入/文件/系统回调，设备差异可集中在少量回调内；X-T4 目前没有我们掌握的这套 SDK，仍需先逆向这些接口。建议先把 X-T4 2.12 端到端打通，稳定后再按实际重复部分整理通用层，不提前开发完整跨设备框架。

1. **加载与生命周期。** 先证明可在原厂固件内执行本地代码，初始化/关闭扩展、进入/退出自定义页面；PocketJS 不提供富士 loader 或代码执行入口。
2. **平台 ABI 与内存。** ARM32 target、浮点/调用约定与原厂一致；提供受控独立 heap、栈、分配失败和 panic 行为。`no_std` 不代表零分配；不能把相机标称总 RAM 当作可供扩展使用的连续空闲内存。
3. **调度。** 一个 ThreadX 任务或受原厂 UI 调度的单一拥有者推进帧；控制执行时间、JS 内存和中断长时间执行，防止抢占拍摄工作。JS 运行与原厂 UI 调用跨任务时采用消息队列，不重入单例。
4. **呈现。** 接收 PocketJS 软件渲染结果或 draw list，转换到已校准的相机 overlay 格式并在正确刷新时机提交；LCD/EVF 切换和 alpha/双缓冲未知。优先软件渲染，不以已有 OpenGL/EGL 枚举头文件推断 GPU 接口可调用。
5. **输入。** 将 Fn/Q/方向键/拨轮/触摸转换为 PocketJS 帧输入；页面关闭时交还原厂事件处理，快门/退出等重要行为需明确共存。
6. **资源。** 首版将 JS bundle、样式、字体和少量图片与扩展一起打包，减少对文件系统接口的依赖；稳定后再接入 SD 文件加载。
7. **Camera bridge。** 只绑定确认过的设置读取/命令，例如读取 ISO、应用一组已支持 recipe 参数，通过原厂服务执行。PocketJS 自带 UI 不会赋予新胶片算法或任意 ISP 表写入能力。

## 组件组合与最小验证

```text
电脑上 TS/TSX 构建的应用 bundle / 字体 / 样式
                       ↓
相机内 QuickJS + portable C runtime
                       ↓
PocketJS no_std core + C ABI + software renderer
                       ↓
我们实现的 X-T4 host（内存、帧、显示、输入、相机命令）
                       ↓
现有 ThreadX / 富士服务 / 原厂驱动
```

先做一个内存中运行的纯 UI：显示 `FUJI BOOST`，一个按键递增计数并能退出；随后用只读 Camera bridge 显示真实 ISO。两者成功后才加入 recipe 写入和手机通信。这样可以把代码入口、UI 移植和成像功能分别定位问题。

UI 核心可先不接 QuickJS，通过 C ABI 验证“布局→像素→相机显示”；这只证明 native core/display 路径，并不等于完整 PocketJS/TSX 应用已移植。第二步再接真实 QuickJS 与应用 bundle。

## 限制

- 未做 ARM 交叉编译和链接验证；QuickJS 的具体 libc/math/platform 依赖仍需按目标构建逐项核实。
- 源码 no_std/C ABI 是实际代码事实；在 X-T4 上可行是基于这些事实的工程判断。
- README 的 PSP 8 MB 和帧率是作者特定应用/设备测量，不能作为 X-T4 内存预算或性能承诺。
- MicroTS AOT 是另一条正在开发的路线；不能把它假设成完整 TS/JS 的无解释器等价替换，不建议首版同时引入两种执行方式。
- 未证明 X-T4 加载入口、显示接口、可用 heap、任务上下文和恢复能力；没有刷写。

## 性能要求与 X-T4 运行判断

**工程判断：普通菜单、状态 dashboard、配方选择器很可能不受 X-T4 处理器档次限制，值得做原型；尚无法保证与连拍/录像并行时的帧率、响应和稳定性。** 目前没有本机空闲 heap、可用连续块、UI 可分配 CPU 时间或显示吞吐的实测，不能给“最低 RAM”或“稳 60fps”的结论。

### PocketJS 自身的公开测量

固定版本 README 给出的 PSP 条件是单个 333MHz MIPS core、32MB 总 RAM，某个完整动画应用占 8MB。其示例数据包含 OpenStrike 每帧 JS 约 2.2ms、总 CPU 约 8.4ms，以及不同 UI demo 的 3.6–6.5ms 等结果。**这都是作者特定样本测量，不是 PocketJS 的最低硬件配置。** 作者注明每个应用只取 7 个样本，不同表行包含不同工具链运行，不能跨行拼出通用性能倍数。

[OpenStrike 作者文章](https://pocketjs.pocket.nexus/blog/shipping-openstrike/) 的明确显示目标是 480×272，利用 PSP 专用 GE 硬件绘图。它能跑 60fps，并不证明 X-T4 初版软件渲染、未知分辨率/像素格式下也有同样速度。文章说明 PSP 应用可见 RAM 是 24MB；32MB 是机器总 RAM，不能混淆。

X-T4 的四核 Cortex-A7 来自 [fffw 作者表](https://github.com/tiredboffin/fffw/wiki/CPU-and-OS-History-Table)，固件 ThreadX/Cortex-A7 标识已在本地验证。普通 JS/布局运算具备合理硬件基础，但不能由频率或核数直接推断速度倍数，也不能认为有一个核可供我们独占。相机 4K 编码/ISP 能力不等于 JS 或软件绘图性能。

### 内存与像素成本

以下仅举 **640×480** 表面例子，不宣称这是 X-T4 可用 overlay 的实际尺寸/格式：

| 格式 | 单缓冲 | 双缓冲 | 30fps 全画面输出写入量 |
|---|---:|---:|---:|
| RGB565 | 0.586MiB | 1.172MiB | 17.58MiB/s |
| ARGB32 | 1.172MiB | 2.344MiB | 35.16MiB/s |

这只计输出像素写入，未计纹理、字体 atlas、JS heap、Rust UI tree、布局缓存、栈、alpha 混合读操作、提交复制和 cache 同步。透明叠加层往往需要 alpha，RGB565 不能被当作任意透明 UI 的等价替换。计算原文在 `performance-budget.json`。

因此扩展预算应先通过原型测出：runtime 基线、字体/图片峰值、JS/Rust 两侧 heap 峰值、显示缓冲以及最坏 GC/帧耗时。相机总 RAM 或 RAW 缓冲大小不能当作可分配内存。

### 首版策略与验收

- 只做菜单/状态文字/配方列表，烘焙字体和资源，避免大图、全屏模糊和每帧重新布局。
- 先从打开自定义菜单时运行开始，退出后释放或停用；随后单独验证取景叠加、拍摄和录像并行。
- 以约 30fps 的交互目标起步，host、bundle 与 UI core 的 tick 声明须一致；减少像素变化区域，采用已有 dirty-region/strip 渲染支持。静态页面仍按运行时契约推进，尽量不重复呈现未变像素。
- 保留原厂拍摄优先级，给自定义 JS 设置内存/执行预算；监测长帧、GC 峰值、快门响应、丢帧、休眠唤醒和录制稳定性。
- 软件渲染先证明可用；之后才以实际热点决定是否值得适配原厂图形硬件。没有要求先移植完整 GPU 栈。

建议验收结果不是单个 fps 数字，而是“页面能操作、原厂拍摄不受影响、峰值内存可控、退出和错误恢复正常”。本轮只做来源核对和预算计算，未跑 X-T4 性能测试。
