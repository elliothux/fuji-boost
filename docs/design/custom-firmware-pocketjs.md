# 自定义固件 × PocketJS：模拟器先行的移植方案

日期：2026-11-07。状态：M0–M5 已在模拟器内实现并达标，M6（真机准备）未开始。实测与勘误记录见 [platform/2026-11-07](../research/platform/2026-11-07.md)。下文已按实现修订；与最初方案不同之处以"实现："标出。

使用方法：

```sh
bun run setup              # 额外固定拉取 references/pocketjs 与 references/quickjs-rs
bun run firmware:build     # 打包 TSX 应用、编译映像，写 firmware/dist/fuji-boost.json 清单（约 11 s）
bun run test:xt4:custom    # 8 个模拟器测试，证据写入 .cache/custom-firmware/
bun run camera:native 5299 # 另开终端：bun run dev，打开 /?port=5299，在右侧"固件"里切换预设
bun run firmware:app       # 监听 firmware/apps/*，页面勾选"自动热重载"后改 TSX 约 4 s 见画面
```

也可以直接用 `/?port=5299&firmware=fuji-boost/pocket`（或 `fuji-boost/hello`）打开指定预设。

模拟器与固件的边界（2026-11-07 解耦后）：模拟器只提供硬件，即原厂 DAT 会话或"RAM 映像 + 入口"会话，外加 GPIO/ADC、LCD 帧、内存读写和 checkpoint。固件的调试约定写在 `firmware/protocol.json`（日志环、状态字、应用槽），由固件构建复制进清单。网页按清单解释这些约定。两者唯一的耦合点是网页上可切换的固件预设。

```text
firmware/protocol.json ──build.rs──▶ 固件常量（firmware/src/protocol.rs）
        └──firmware/build.ts──▶ firmware/dist/fuji-boost.json（映像、加载地址/入口、协议、应用索引）
                                      └──▶ 网页预设与调试面板；crates/xp4/tests/custom.rs 的测试夹具
```

## 0. 结论

**可行，建议做。** 理由：

1. 现有 X-T4 模拟器已经把一个裸机固件所需的全部外设建成了确定性的模型：四核 Cortex-A7、2 GiB RAM、LCDC（BGRA8888 平面、Q12 缩放、两路相位 IRQ）、GIC-400、8 路间隔定时器、16 组 GPIO、12 路 ADC，以及对空闲核心（WFI）的时间跳过。自定义固件只要把自己当成"另一份 RAM 映像"装进去，不需要原厂 DAT，也不需要任何新的硬件模型。
2. PocketJS 的结构正好是为"没有 OS 的小设备"设计的：`engine/core` 是 `no_std + alloc` 的 Rust；`engine/ui-cabi` 产出 C staticlib，自带 `bare-platform`、`software-only`、`host-allocator` 三个特性；`engine/quickjs-c/pocket_runtime.c` 是可直接编译的 QuickJS 胶水；`hosts/gba`（无 OS 的 ARM 裸机）、`hosts/esp-idf`（caller-composed 组件、软件 RGB565 damage 渲染）和 `hosts/psp`（单 arena 同时服务 QuickJS/Rust/newlib）给出了全部需要的先例。
3. 性能数量级匹配。模拟器实测原生聚合吞吐约 90–130 MIPS（见 §2.3），单核裸机固件把其余核心放进 WFI 后基本独占这个吞吐，相当于一颗 100 MHz 级的 ARM。PocketJS 公开的 ESP32-S3（240 MHz Xtensa）与 PSP（333 MHz MIPS）数据说明它在这个档次能跑菜单/仪表盘类 UI。**这是数量级判断，不是实测承诺**，§6 给出了必须测出的数字。
   - 实现：§6 全部达标。仪表盘交互帧 p95 为 678K ticks，占 30 Hz 预算的 37.7%；静止帧 79K。启动到首帧 88M 指令，宿主 0.55 s。宿主不限速 206–227 fps，网页限速为 30 fps、实时 100%。

必须一起说清楚的边界：

- 本方案只承诺**模拟器内**跑通。真机需要 DDR 初始化、电源 MCU 握手、面板初始化、SD 启动入口等，这些在 X-T4 上均未验证（§8）。方案通过 HAL 分层保证将来两条真机路线（SD 双启动裸机 / 寄生在原厂固件内的 payload）都能复用 UI 与应用层。
- "性能基本可用"在此定义为 §6 的验收数字，不是 60 fps 全屏动画。
- 模拟器的 guest 时钟按 profile 为 1 指令/tick、54 MHz tick，比真机 A7 慢一个数量级以上；模拟器里达标的预算在真机上只会更宽松，反向不成立（真机有自己的未知）。

## 1. 目标与范围

### 做什么

- 一份独立于原厂 DAT 的 ARM32 裸机固件映像 `fuji-boost.bin`，在现有 XP4 模拟器中启动，驱动 LCD 输出 900×600 画面，读取按键/拨轮，运行 PocketJS 运行时与 TSX 应用。
- 模拟器侧新增"自定义固件"会话入口、WebSocket 命令、网页模式和自动化验收测试。
- 以可测量的指令预算和帧率作为验收，并形成热重载应用 bundle 的开发闭环。

### 不做什么（本阶段）

- 不刷写、不连接实体相机，不构造 `ffboot.bin`/`ffinit.bin`（§8 只列前提）。
- 不移植原厂拍摄/成像/无线功能；不实现 Camera bridge 的真实后端，只预留扩展点。
- 不引入 MicroTS AOT 路线（首版单一执行方式：QuickJS）。
- 固件只使用 §3.5 列出的已建模寄存器子集。实现：触摸走同一份 XP4 F_I2C + 0x55 面板模型（`patches/fujimod-devtools.patch`），原厂会话默认不装，自定义映像会话安装。

## 2. 现状依据

### 2.1 模拟器能力（已读源码确认）

| 项 | 事实 | 来源 |
| --- | --- | --- |
| 处理器 | 4×Cortex-A7，`cbar=0xffff0000`（GIC-400），L1 32K/32K，L2 1 MiB | `profiles/boards/xt4-212.generated.json` |
| RAM | `ram_size=0x80000000`（2 GiB，物理 0 起）；真机 jig 读 RAM 证实 ≥2 GiB 可寻址，第 0 页未映射 | 同上；`docs/research/hardware-access/2026-11-01.md` |
| 装载方式 | 会话把一份 RAM 映像 `board.write(0, &ram)` 写入，再为四核设置 `PC=plan.entry`；没有 Boot ROM | `crates/xp4/src/lib.rs` `prepare_inputs` 末尾 |
| LCD | LCDC1 `0xfffa9000`，`ticks_per_pixel=2`，IRQ 117/118，支持 `bgra8888-straight` 图片平面与 `yycbcr422` 视频平面，Q12 最近邻采样可放大 | `references/fujimod/crates/emulator-xp4-display/src/lcdc.rs`、`emulator-graphics/src/raster.rs` |
| 定时器 | 8 路递减计数器，控制字 `+0`，通道 n 的计数 `+0x10+8n`、重载 `+0x14+8n`，IRQ 57–64。实现：board profile 的 `timer_base` 是 `0xfff3c300`（0x50 字节窗），`0xfff38000` 不可用 | `emulator-xp4-io/src/interval_timers.rs`、`profiles/boards/xt4-212.generated.json` |
| GPIO | `0xfff41000`，端口 p 的寄存器组在 `+0x40*p`，`+0x14`（寄存器 5）读外部输入电平，写寄存器 0–4 为高 16 位掩码写 | `emulator-ip/src/socionext/gpio.rs` |
| 按键 | 虚拟键 up/down/left/right/ok/back = GPIO 7.11/7.10/7.9/7.8/7.12/9.5，按下为低电平 | `crates/xp4/src/lib.rs` `VIRTUAL_BUTTONS`、`set_virtual_button` |
| 拨轮 | 前拨轮 ADC1/2 编码器、后拨轮 ADC8/9；其他控件映射见实测 | `profiles/measured/xt4-controls.json` |
| 空闲 | 核心执行 WFI 进入 `waiting`，板级 `idle()` 直接把时间推进到下一个设备/定时器截止，不消耗宿主时间 | `emulator-xp4-board/src/lib.rs` |
| 严格性 | `discovery=false` 时任何未建模的 MMIO 访问都会令 CPU 停止并报告失败 | 同上 |
| 调试读内存 | `read_physical_memory(address,len≤64 KiB)`，宿主可直接读 RAM | `crates/xp4/src/lib.rs` |

### 2.2 PocketJS（固定 `e27bff3e96df3c7b3c8263a34f2362b964df04fb`）

| 组件 | 事实 | 对本方案的意义 |
| --- | --- | --- |
| `engine/core` | `no_std + alloc`，taffy 0.11 flexbox，f32 数学，无 libm | 直接交叉编译到 ARMv7-A |
| `engine/ui-cabi` | `staticlib`，特性 `bare-platform`/`software-only`/`host-allocator`；要求 nightly + `-Z build-std`；已有 `hosts/nokia-e7/targets/armv6-symbian-eabi.json` 自定义 target 先例 | 用同一套方法新增 `armv7a-fuji-eabihf.json` |
| `engine/quickjs-c/pocket_runtime.c` | 只依赖 `quickjs.h`、`pocket_ui_cabi.h`、`pocket_spec.h`、`<string.h>`；管理 runtime/context、安装 `ui.*`、`pocket_runtime_boot`/`pocket_runtime_tick(buttons…)`；有 `POCKET_RUNTIME_EXTENSION` 钩子 | 相机 host 几乎不用写 JS 绑定；Camera bridge 走扩展钩子 |
| QuickJS 实现 | ESP-IDF 组件用 quickjs-ng 0.14.0；PSP 用 `pocket-nexus/quickjs-rs` 预编译 | 实现：`pocket_runtime.c` 针对的是 Bellard QuickJS。用 `quickjs-rs` `ba5bdd0` 的 `libquickjs-sys/embed/quickjs` 源码编译，与 PocketJS 的 Symbian 工具链为同一版本 |
| 帧契约 | `frame(buttons, …)` 一帧一事务，时间即帧计数，无 wall clock | 无需 RTC；按键必须跨帧保持才能被边沿检测 |
| 渲染 | `hosts/esp-idf/components/pocketjs_render_rgb565`：prepare / render_strip / commit / abort 的脏矩形条带渲染；`core/src/damage.rs` | 避免全屏重绘，是模拟器内达标的关键 |
| 内存 | `hosts/psp`：单一 arena 同时服务 Rust 全局分配器、QuickJS 和 newlib malloc | 照搬 |
| 无 OS 先例 | `hosts/gba`：`start.s` 初始化 RAM/栈后进入 Rust，自己提供 allocator 和 `critical-section` | 启动代码结构照搬 |
| 目标注册 | `contracts/spec/platforms.ts` 的 `TargetProfile`：`physicalViewport`、`logicalViewports`、`rasterDensity`、`capabilities`；`pocket check --target` 做准入 | 实现：不打补丁。仿照 `iphone2g-dev`，在 `firmware/app/profile.ts` 里于仓库外定义 `fuji-xt4` 契约注册表，用 `validateAndResolveBuildPlan` 解析 manifest 后交给 `tools/build.ts` |

### 2.3 模拟器性能基线（宿主 Apple Silicon，原生）

| 来源 | 数据 | 换算 |
| --- | --- | --- |
| `.cache/xt4-tests/endurance.json` | 31,386M 指令 / 120 guest 秒 / 319.4 wall 秒 | 98 MIPS；0.38 guest-s/wall-s |
| `.cache/opt15/acceptance-results/endurance.json` | 29,794M 指令 / 120 guest 秒 / 225.1 wall 秒 | 132 MIPS；0.53 guest-s/wall-s |
| `.cache/opt13/REPORT.md` 串行 REC | 100M 指令 1.119 s | 89 MIPS |

这些都是原厂四核固件（四核始终忙）的数字。单核裸机固件 + 三核 WFI + 主核在帧间 WFI 时，宿主只为真正的工作付费，所以 **每帧工作 ≤ 1.8M 指令时模拟器可以接近 30 fps wall**（1.8M / ~100 MIPS ≈ 18 ms，再加设备回调与 LCDC 合成开销）。guest 侧 1.8M 指令 = 54 MHz × 33 ms，正好也是 30 Hz 的 guest 帧预算。

## 3. 架构

### 3.1 分层

```text
TSX 应用（Solid / Vue Vapor / Octane）         ← 电脑上 pocket build --target fuji-xt4
  bundle.js + styles.bin + assets.pak（可热重载）
──────────────────────────────────────────────
pocket_runtime.c（QuickJS 胶水、ui.* 安装、帧推进）   ← PocketJS 原样编译
quickjs-ng（C）                                      ← 固定版本
pocketjs-ui-cabi（Rust staticlib：core + 软件渲染）   ← PocketJS 原样编译，自定义 target
──────────────────────────────────────────────
fuji-boost host（我们写）
  frame loop · 输入采样与映射 · damage→strip 渲染 · 翻页 · 诊断邮箱 · Camera bridge 扩展点
──────────────────────────────────────────────
HAL（我们写，trait/函数表，两种实现）
  display: surface(addr,pitch,fmt) / present(addr) / wait_vsync
  input:   sample() -> buttons, axis deltas
  time:    ticks() 54 MHz
  log:     write(bytes)
  alloc:   arena base/size
  ┌──────────────────────┐   ┌─────────────────────────────┐
  │ bare-xp4（本阶段）    │   │ 将来：stock-hosted payload / │
  │ LCDC·GIC·timer·GPIO· │   │ SD 双启动裸机（§8）           │
  │ ADC 直接寄存器驱动    │   └─────────────────────────────┘
  └──────────────────────┘
──────────────────────────────────────────────
boot（start.S + link.ld）：核心分发、栈、VBAR、VFP/NEON、.bss、GIC、进入 main
```

应用层与 host 层之间只通过 PocketJS 既有契约（`frame()`、`ui.*`、扩展钩子）交互；host 与硬件之间只通过 HAL。将来换真机路线时，只重写 HAL 与 boot。

### 3.2 内存布局（物理地址，MMU 关闭）

| 区域 | 地址 | 大小 | 说明 |
| --- | --- | --- | --- |
| 禁区 | `0x0000_0000` | 4 KiB | 真机第 0 页未映射，固件不得触碰 |
| 映像 | `0x0010_0000` | ≤ 8 MiB | `.text/.rodata/.data/.bss`，入口 = 映像基址，VBAR 指向此处 |
| 栈 | `0x00F0_0000` 向下 | 核 0 256 KiB，其余各 16 KiB | QuickJS 递归较深，核 0 不要吝啬 |
| 日志环 | `0x0F00_0000` | 64 KiB | 魔数 + 写指针 + 字节环；宿主用 `read_physical_memory` 拉取 |
| 诊断邮箱 | `0x0F01_0000` | 4 KiB | 32 位字数组：启动阶段、帧号、上一帧各阶段 tick 数、峰值堆、JS 异常计数等（参照 `hosts/gba` 的 diagnostic words） |
| 应用槽 | `0x0F10_0000` | 8 MiB | 实现：64 字节头，依次为魔数 `FBAP`、版本 1、头长、js 长、pak 偏移（16 字节对齐）、pak 长、js‖pak 的 FNV-1a、代号；后接 bundle.js 与 pak（样式已编进 bundle）。宿主写入后置邮箱字 14（reload request），固件在下一帧边界校验并复制到堆上（js 补 NUL）后重启 runtime；校验失败则保留当前应用 |
| Arena | `0x1000_0000` | 64 MiB | 实现：`firmware/libc/src/malloc.c` 的单一堆。ui-cabi 的全局分配器与 QuickJS 都经 C `malloc` 进入；邮箱公布在用、峰值和块数 |
| 帧缓冲 | `0x1400_0000` | 450×300×4 | 实现：单一 present 缓冲。ui-cabi 渲染到自己的 450×300 RGBA 面，帧循环在 IRQ 118（捕获完成）之后只把 damage 行复制到 present 缓冲，下一次捕获之前完成，因此不需要翻页 |

全部位于 2 GiB 之内且低于原厂使用的 `0x2000_0000` 之下，真机 RAM 也覆盖。地址都是设计选择，可在 `link.ld` 与 `hal/bare-xp4/memory.rs` 一处修改。

### 3.3 启动序列（`boot/start.S`）

1. 四核同时从入口开始（模拟器为每核写 PC）。读 `MPIDR` 低 2 位，非 0 核进入 `WFI` 死循环（将来需要时再唤醒）。
2. 核 0：设置各模式栈、`VBAR`、`CPACR`（cp10/cp11 全访问）与 `FPEXC.EN`，清 `.bss`，必要时复制 `.data`。
3. 初始化 GIC-400：Distributor 使能；CPU interface `PMR=0xff`、使能；打开 LCDC 相位 IRQ 118（进入消隐 = 一帧扫描结束，可安全翻页）和定时器通道 0 的 IRQ 57。
   - 实现：只开 IRQ 118，并在 `ICFGR` 配成边沿触发；按电平配置时，脉冲落下 pending 就会消失。`PMR=0xf0`。CPSR.I 始终屏蔽，WFI 因 GIC 有待处理中断而醒来，由主循环轮询 IAR 确认。定时器不用 IRQ。SCTLR 清 A 位，不因非对齐访问产生异常。
4. 进入 Rust `main`：填日志环魔数、写邮箱 `stage=1`，初始化 HAL（§3.5），进入 host 帧循环。
5. MMU/缓存在本阶段关闭（模拟器速度与之无关）；M6 再加恒等映射段表与 cache 开启，为真机做准备。

### 3.4 帧循环（host）

```text
loop:
  wait_vsync()                       # WFI 直到 IRQ 118；模拟器跳过空闲时间
  input = sample()                   # GPIO 7/9 低有效按键 → PocketJS 按键位；ADC 编码器 → axisDeltas
  t0 = ticks()
  pocket_runtime_tick(&input)        # 一次 JS frame + 一次 core tick
  t1 = ticks()
  strips = ui_cabi damage → render_strip 到后缓冲（BGRA8888）
  t2 = ticks()
  present(back)                      # 写 LCDC +0x44/+0x4c（二者必须相等）；交换前后缓冲
  mailbox[frame, t1-t0, t2-t1, heap_peak] = …
```

实现（`firmware/src/frame.rs`）：

```text
anchor: gic::discard(118); wait(118); origin = now - FRAME_TICKS/16   # 丢掉启动期锁存的陈旧边沿
loop:
  若相位已过 3/4 帧（迟到）→ 记 missed，等下一次捕获，continue
  present(damage 行 → PRESENT_BASE)                 # 上一帧的结果，紧跟捕获之后
  reload request? → 校验应用槽、重启 runtime、重新 anchor，continue
  buttons = gpio::buttons()
  pocket_runtime_frame_contacts → render → 合并 damage   # 邮箱记 JS/渲染/总 tick、heap、action
  超过 1 帧 → overruns++ 并写日志
  wait(118)                                          # WFI；模拟器跳过空闲时间
```

按键映射首版：up/down/left/right → 方向；MENU/OK（7.12）→ CIRCLE（确认）；DISP/BACK（9.5）→ CROSS（返回）。后续按 `xt4-controls.json` 加 Q、Fn、AEL/AFL、快门半按，拨轮走 `axisDeltas`。模拟器侧按下必须跨至少一帧（§5.2）。

### 3.5 HAL `bare-xp4` 只使用以下寄存器

| 设备 | 寄存器 | 用法 |
| --- | --- | --- |
| LCDC1 `0xfffa9000` | `+0x10..+0x24` 时序；`+0x44`/`+0x4c` 平面地址（高位 `+0x40`/`+0x48`=0）；`+0x50 = 0x10000 \| pitch_px`；`+0x54 = sw \| sh<<16`；`+0x58 = 0`（x,y）；`+0x5c = 900 \| 600<<16`；`+0x60 = dx \| dy<<16`（Q12，`0x1000`=1:1，`0x800`=放大 2×）；`+0x64`/`+0x68 = 0`；`+0x8 = 0x11000`（图片层 + 输出控制）；最后 `+0x4 = 1` | 时序示例（30 Hz）：宽 900 + 消隐 100 = 总 1000，`+0x10=1000`、`+0x14=900\|(100<<16)`、`+0x18=0`；高 600 + 消隐 300 = 总 900，`+0x1c=900`、`+0x20=600\|(300<<16)`、`+0x24=0`。1000×2 ticks×900 行 = 1.8M ticks = 30 Hz。像素时钟按模型固定 27 MHz，900×600 做不到 60 Hz，极小消隐时上限约 49 Hz。原厂实际时序值可从 `.cache/xt4-rec.checkpoint` 里 `xp4-lcdc1` 的 `regs` 读出作对照 |
| 定时器 `0xfff3c300` | 实现：通道 7，`+0x48` 计数、`+0x4c` 重载、`+0` 控制（`bit(16+7)` 选中 + `bit7` 使能），自由运行 | 读计数差作为 54 MHz 时钟（扩展到 64 位）；IRQ 不开，不唤醒 WFI |
| GIC-400 `CBAR+0x1000`（Distributor）/`CBAR+0x2000`（CPU interface），`CBAR=0xffff0000` | Distributor `CTLR/ISENABLER/IPRIORITYR/ITARGETSR`，CPU `CTLR/PMR/IAR/EOIR` | 标准 GICv2；板级只解码 `CBAR+0x1000..0x3000` |
| GPIO `0xfff41000` | 端口 7 `+0x1c0+0x14`，端口 9 `+0x240+0x14` 只读输入 | 不写任何 GPIO 输出 |
| ADC `0xfff40000` | `+0` 控制（bit0 使能、bit1、`bit16..27` 通道掩码）、`+4` 分频、`+8` 启动、`+0xc` 状态、`+0x10+4n` 通道 n 采样值（12 位） | M1 先不用；拨轮在 M4 接入时按 `references/fujimod/crates/emulator-xp4-io/src/adc.rs` 的合法值范围编程 |
| F_I2C unit2 `0xfff3f000` | BSR `+0`、BCR `+4`、CCR `+8`、DAR `+0x10`、CSR `+0x14`、FSR `+0x18`；面板 7 位地址 0x55；INT 为 GIC153（边沿，只读 ISPENDR/ICPENDR，不开使能） | 实现：轮询读 reg 0x10（计数/状态）和 0x11（两条 7B 记录），坐标 900×600，每帧最多两指。详见 [input/2026-11-07](../research/input/2026-11-07.md) |
| 宿主 scene 缓冲（`protocol.json` `scene`，默认 `0x0F020000`） | 16B 头 magic/generation/width/height + RGB888；应用把 `#010008` 涂在取景窗，HAL 每帧把该色换成缩放后的宿主画面 | 不是 XP4 imaging sensor。原厂取景仍走 MOVDC 画面源。 |

任何其他地址访问在严格模式下会让 CPU 停止，这正好把"固件越界"变成测试失败。

### 3.6 显示分辨率与密度的取舍

| 方案 | 像素成本（相对） | 观感 | 决定 |
| --- | --- | --- | --- |
| A 逻辑 450×300，`rasterDensity 1`，LCDC `dx=dy=0x800` 放大 2× | 1× | 像素块状、字体锯齿 | **M1–M3 默认**，先把功能与预算测出来 |
| B 逻辑 450×300，`rasterDensity 2`，LCDC 1:1 | 4× | 清晰 | M4 依据 A 的实测数据决定是否切换 |
| C 逻辑 900×600，`rasterDensity 1`，1:1 | 4× | 清晰但 UI 元素偏小 | 备选 |

PocketJS 的 "一份 bundle 多种密度" 正是为此设计的：切换只改 target profile 和 host 常量，应用不改。

### 3.7 日志与诊断

- 固件 `log::write` 只往 RAM 环写，不碰 UART（模拟器的 PL011 只计数不保存字节）。宿主每批执行后用 `read_physical_memory` 增量读出。
- 邮箱提供每帧阶段耗时（guest ticks），宿主提供每批 `cpu.instructions` 与 LCDC `sequence`，两者合起来即"每帧指令数/每阶段 tick 数"的客观度量。
- 严重错误（JS 未捕获异常、分配失败、断言）写邮箱错误字并进入 `WFI` 死循环，画面保留最后一帧；宿主测试据此判失败而不是超时。

## 4. 工具链

| 用途 | 选择 | 备注 |
| --- | --- | --- |
| Rust（core、ui-cabi、boot/hal/host） | nightly（PocketJS ui-cabi 固定 `nightly-2026-07-02`；本机已有 `nightly-2026-10-10`，以能编过 PocketJS 为准）+ `-Z build-std=core,alloc,compiler_builtins -Z build-std-features=compiler-builtins-mem` + 自定义 `targets/armv7a-fuji-eabihf.json` | 由 `rustc -Z unstable-options --print target-spec-json --target armv7a-none-eabihf` 派生，`cpu=cortex-a7`，`features=+vfp4,+neon`，`panic-strategy=abort`，`relocation-model=static` |
| C（quickjs-ng、pocket_runtime.c、newlib 桩） | `brew install --cask gcc-arm-embedded`（arm-none-eabi-gcc + newlib-nano）| `-mcpu=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=hard -O2 -ffunction-sections -fdata-sections`；Rust 与 C 必须同为 hard-float ABI |
| libc 桩 | `_sbrk`（指向 arena）、`_write`（日志环）、`_read/_close/_fstat/_isatty/_lseek/_exit/_kill/_getpid`、`_gettimeofday`（返回帧计数换算或 0） | QuickJS 的 `Date` 在 PocketJS 里不作为时间源 |
| 链接 | `arm-none-eabi-ld` + `link.ld` → ELF（留符号）→ `objcopy -O binary` → `fuji-boost.bin` | ELF 用于 PC 采样对照 |
| 应用 | Bun + `references/pocketjs` 的 CLI：`pocket build --target fuji-xt4` | 需要 `patches/pocketjs-fuji-target.patch` 注册 target（§7 M3） |
| 本机现状 | 有 clang 21、llvm@19、rustup（stable/nightly）、Bun、CMake、Ninja；**没有** `arm-none-eabi-gcc` | M1 第一步安装 |

实现：没有安装 gcc-arm-embedded/newlib，最终工具链如下。

- Rust 用 `firmware/rust-toolchain.toml` 固定的 `nightly-2026-10-10`，内置 target `armv7a-none-eabihf` 加 `-C target-cpu=cortex-a7`，`build-std=core,alloc,compiler_builtins` 加 `compiler-builtins-mem`。不需要自定义 target JSON。
- C 由 `firmware/build.rs` 通过 `cc` crate 调用 Homebrew `llvm@19` 的 clang 与 llvm-ar（可用 `FUJI_LLVM_BIN` 覆盖），参数为 `--target=armv7a-none-eabihf -mcpu=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=hard -marm -mno-unaligned-access -O2 -nostdlibinc`。
- libc 是 `firmware/libc/`：malloc、字符串（包括按字比较的 `memcmp`/`bcmp`，用来覆盖 compiler_builtins 的逐字节弱符号）、`snprintf`、math、time 桩。
- 链接用 rust-lld 加 `link.ld`，`llvm-objcopy -O binary` 产出 `firmware/dist/fuji-boost.bin`（1.9 MB）和 `firmware/dist/fuji-boost-hello.bin`（12 KB，HAL 自检图案）。加载地址与入口由 `llvm-readelf` 从 ELF 读出，写进清单 `firmware/dist/fuji-boost.json`。
- 应用由 `firmware/build.ts` 打包：解析 `fuji-xt4` 计划，用 `references/pocketjs/tools/build.ts` 输出到 `firmware/dist/apps/<app>/`，写 `index.json`（含 sha256），并通过 `FUJI_APP_JS`/`FUJI_APP_PAK` 把计数器内嵌进映像。`--watch` 只重打包应用，不重链固件。

## 5. 模拟器侧改动

### 5.1 `crates/xp4/src/custom.rs`（新）

```rust
pub struct CustomImage<'a> { pub bytes: &'a [u8], pub load_address: u32, pub entry: u32 }
impl Xp4Session {
    /// 自定义 RAM 映像：沿用 xt4-212 board profile 的全部设备模型，
    /// 不做任何原厂源码/哈希守卫，四核 PC 均置为 entry。
    pub fn custom_image(image: CustomImage<'_>) -> Result<Self, String>;
    pub fn write_physical_memory(&self, address: u32, bytes: &[u8]) -> Result<(), String>; // 应用槽热重载
}
```

- 复用 `BOARD_PROFILE` 反序列化出的 `Definition`，`Session::new(definition, None)`，`board.write(load_address, bytes)`，四核 `PC=entry`。
- `checkpoint_identity` 用映像 SHA-256，使现有 checkpoint 机制可用于"跳过启动直接到首帧"。
- `set_gpio`/`set_adc`/`frame_rgba`/`read_physical_memory`/`execute_batch` 原样可用；`set_virtual_button` 的 raw768 守卫不适用，自定义模式直接走 `set_gpio`。

实现：没有扩展 `Xp4Session`，而是新增与固件无关的 `crates/xp4/src/ram_image.rs`：`ImageSession::new(&Image {bytes, load_address, entry})`。它用同一 board profile 建板，要求映像落在 RAM 内、高于 page 0、入口为映像内的 ARM 地址；所有实测按键线初始为松开（高电平）。身份为 `image/<sha256>/<load>/<entry>/<board>`。它不认识日志、状态字或应用槽，只提供以下接口：

- 执行：`execute_batch(budget)` 与 `run_for(ticks, limit)`。后者按 guest 时间推进：忙碌核心的单批指令不超过剩余 tick，所有核心等待时跳到下一个设备或定时器事件，跳过目标后立即返回，因此最多越过一个空闲间隔。停点只取决于客机状态，checkpoint 恢复后行为一致。
- 输入：`set_button`（导航键）、`set_gpio`、`set_adc`。
- 内存：`read_physical_memory`、`write_physical_memory`。
- 快照：`checkpoint`/`restore_checkpoint`，接入现有 `checkpoint_file`（约 4.1 MB）。

最初的 `CustomSession` 把邮箱、日志、应用槽写死在模拟器里，已删除。这些约定现在只在 `firmware/protocol.json` 定义一次：`firmware/build.rs` 生成固件常量，清单把它交给网页与测试。

### 5.2 `xt4-camera` 服务端

新增命令：`LoadImage {load_address, entry}`（原始字节随包）、`Execute {mode:"custom"}`、`ButtonState {button, pressed}`（替代原厂模式的"按下-确认-释放"扫描，因为 PocketJS 要求按键跨帧保持）、`ReadMemory {address, length}`（日志环/邮箱）、`WriteMemory {address}`（应用槽热重载）。帧下发路径 `executed()` 不变。

实现（kebab-case `type`）：

- `load-image {loadAddress, entry}`：原始字节随包，两个地址都必填（来自清单），回包带 `identity`。
- `execute {mode:"image", budget ≤ 50M, ticks 1–54M}`：调用 `run_for`，回包与原厂执行相同。
- `button-state {button, pressed}`：导航键按住/松开。`gpio`、`adc` 在映像会话下作用于映像会话。
- `read-memory {address, length ≤ 16 MiB}`、`write-memory {address}`。
- `save-checkpoint`：保存当前会话（原厂或映像）。映像会话下的 `checkpoint` 会恢复到映像会话。

原厂的 `load` 会清掉映像会话。映像会话下，原厂执行模式、`button`、`command-dial` 会报错。`scene` 只接受带 `address` 与 `generation` 的 `rgb`（写进固件声明的 scene 缓冲），灰场/测试图由网页合成 RGB。

### 5.3 网页

`src/camera.tsx` 增加"自定义固件"模式：选择 `.bin`（或从 `firmware/dist/` 拉取）、连续运行、按键按下/抬起按指针事件发送、日志面板、FPS（LCDC `sequence` 差 / wall 秒）与每帧指令数、应用热重载按钮。

实现：没有单独页面，统一相机页 `src/camera.tsx` 右侧增加"固件"预设切换。预设包括"原厂 2.12"和 `MANIFESTS`（`src/lib/firmware-presets.ts`）中每个清单的每个映像，URL 参数为 `?firmware=<清单名>/<映像>`。vite 中间件从 `firmware/dist/` 提供 `/local/firmware/*`。映像预设的行为：

- 运行：按清单的 `loadAddress`/`entry` 调用 `load-image`。每次请求推进 1/60 s guest 时间，按墙钟限速，落后超过 250 ms 时重新对齐；可勾选"不限速"。
- 输入：机身按键都按"按住"处理（导航键走 `button-state`，其余按实测 GPIO 线），按住至少 80 ms；键盘方向键、Enter、Backspace 同样按住。拨盘走 ADC，模式拨盘走 GPIO。指令拨盘仍禁用。画面源在清单声明了 `protocol.scene` 时可用：网页把 RGB 写进该缓冲，HAL 把应用里的 `#010008` 换成缩放后的宿主画面。
- 调试面板（`src/firmware-debug.tsx`）只读清单里的 `protocol`：状态字全表（每 500 ms）、日志环增量（每 400 ms）、应用槽。映像是清单 `apps.image` 时显示应用按钮，每 1 s 轮询 `apps/index.json`，sha 变化时自动写入应用槽并置重载字。
- 存档：`save-checkpoint` 下载，`checkpoint` 恢复。

协议解释器的单元测试是 `src/lib/firmware-presets.test.ts`，直接读 `firmware/protocol.json`。

### 5.4 测试

`crates/xp4/tests/custom.rs`（`#[ignore]`，需要已构建的固件）：

- `boot_first_frame`：从 `.bin` 启动，运行到首个 LCDC 帧，与仓库内黄金 PNG 逐字节比较（PocketJS 帧契约保证确定性）；邮箱 `stage` 达到 `running`。
- `buttons`：`ButtonState` 按下 → 执行 ≥ 2 帧 → 抬起；断言文本区域像素变化，邮箱计数 +1。
- `budget`：脚本化 300 帧输入，断言 §6 的每帧指令数与堆峰值，结果写 `.cache/custom-firmware/`。
- `endurance`：120 guest 秒无 `cpu.failure`（即没有越界 MMIO）。

对应 `package.json` 脚本：`firmware:build`、`test:xt4:custom`。

实现（`bun run test:xt4:custom` 先构建，再串行跑 8 个测试，约 35 s）。测试文件里自带协议解释（状态字、日志环、工作量环、应用槽），布局读自 `firmware/dist/fuji-boost.json`，模拟器库只提供 `ImageSession`：

- `hello_first_frame`、`hello_buttons`、`hello_endurance_and_checkpoint`：HAL 自检映像，逐字节对照宿主生成的参考图。
- `pocket_first_frame`：内嵌计数器首帧，与 `crates/xp4/tests/fixtures/custom-first-frame.json` 的 RGBA SHA-256 一致。PNG 一并入库；`FUJI_UPDATE_GOLDEN=1` 重写。
- `pocket_buttons`：OK 计数 1→2，RIGHT+OK 复位，邮箱 action 序号 3。
- `pocket_scene`：把红色 RGB 写入清单 `scene` 地址，取景窗变成红、+1 按钮仍在。
- `pocket_hot_reload`：经应用槽依次注入仪表盘与计数器，按键行为与 action 正确，堆记录在案。
- `pocket_endurance_and_checkpoint`：仪表盘 120 guest 秒脚本输入，统计静止/交互分位数、堆、宿主 fps。每次选中最多超预算一帧，不丢捕获。checkpoint 恢复后同输入同画面。
- `pocket_deterministic`：两次独立运行的启动指令数与帧哈希一致。

另有诊断用的 `pocket_profile`（`bun run firmware:profile`），它对 PC 采样，用 `llvm-nm` 符号化后按组件汇总到 `.cache/custom-firmware/profile.txt`。

## 6. 性能预算与验收

数字是**目标**，由 §5.4 的测试测出并记录；达不到就回到 M4 调优，而不是修改验收。

| 指标 | 目标 | 测量方法 | 实测（2026-11-07） |
| --- | --- | --- | --- |
| 启动到首个 PocketJS 帧 | ≤ 1.0G 指令，宿主 ≤ 10 s | `cpu.instructions` 与 LCDC 首个 `sequence` | 88.1M 指令，宿主 0.55 s |
| 静止帧（无输入、无动画） | ≤ 0.3M 指令/帧 | 邮箱阶段 tick + 批指令差 | 78.9K |
| 交互帧（焦点移动、文本更新、列表滚动）p95 | ≤ 2.0M 指令/帧 | 同上，300 帧脚本 | 仪表盘 3600 帧：p95 678K，p99 969K |
| 宿主帧率（原生服务端，Apple Silicon） | 静止 ≥ 30 fps，交互 ≥ 20 fps | 网页 FPS 或测试计时 | 不限速 206–227 fps；网页 30 fps |
| 内存 | QuickJS 堆上限 8 MiB、Rust 侧 ≤ 4 MiB、帧缓冲 ≤ 4.2 MiB、arena 峰值 ≤ 32 MiB | 邮箱峰值字 | 单一堆峰值 2.59 MB（热重载两次后 3.58 MB）；present 缓冲 0.54 MB |
| 稳定性 | 120 guest 秒脚本输入无 `cpu.failure`、无 JS 未捕获异常 | `endurance` | 通过 |
| 确定性 | 同一输入脚本两次运行帧哈希序列一致 | 测试重复两次比较 | 通过 |

超预算的情况：选中配方那一帧约 2.7M ticks，重复显示一帧，原因是 damage 并集接近全屏；切换应用后的首帧约 4.6M ticks。两者都计入超时帧，不丢捕获。

预算推导：30 Hz guest 帧 = 54 MHz / 30 = 1.8M ticks ≈ 1.8M 指令。PSP 上 PocketJS 示例 JS 每帧 2.2–3.6 ms（约 0.7–1.1M MIPS 指令量级）；450×300 逻辑面全屏软件重绘约 135k 像素，纯填充 1–2 指令/像素，带 alpha 的文字 10–20 指令/像素，所以**全屏重绘每帧不可接受，脏矩形条带渲染是前提**。

## 7. 里程碑

每个里程碑都以"测试通过 + 数据写入 `.cache/custom-firmware/` + 研究记录追加到 `docs/research/platform/YYYY-MM-DD.md`"为完成标准。

| 里程碑 | 内容 | 退出条件 |
| --- | --- | --- |
| M0 方案 | 本文；安装 `gcc-arm-embedded`；从 REC checkpoint 读出原厂 LCDC 时序值存入 `firmware/hal/bare-xp4/lcdc_timing.rs` 注释 | 文档合入；工具链 `--version` 通过 |
| M1 裸机 hello | `start.S`、`link.ld`、自定义 Rust target、GIC/LCDC/定时器/GPIO 驱动、日志环与邮箱；画渐变 + 边框，按键改变颜色；`custom_image` + `LoadImage` + `custom.rs` 测试 | 首帧黄金图一致；30 Hz guest 帧；宿主 ≥ 30 fps；`endurance` 通过 |
| M2 原生 core | 交叉编译 `pocketjs-ui-cabi`（`bare-platform,software-only,host-allocator`）；C/Rust host 用 C ABI 建树、载入 `styles.bin` 与字体 pak、绘出 `FUJI BOOST` 与几个矩形；接入 damage→strip 渲染 | 文本/矩形正确；静止帧 ≤ 0.3M 指令 |
| M3 QuickJS | quickjs-ng + newlib 桩 + `pocket_runtime.c`；`patches/pocketjs-fuji-target.patch` 注册 `fuji-xt4`（takeover、900×600 物理、450×300 逻辑、density 1、`input.buttons`、`text.glyphs.baked`）；最小 Solid 计数器应用；应用槽加载 | 计数器响应按键；启动 ≤ 1.0G 指令；交互帧 p95 ≤ 2.0M |
| M4 性能达标 | 以 `set_instruction_observer`/PC 采样做热点分析；GC 阈值、堆上限；脏矩形覆盖率；决定 §3.6 的 A/B/C；可选 bytecode 预编译（宿主侧用同版本 quickjs-ng 的 `JS_WriteObject`）；示例应用升级为"状态仪表盘 + 配方列表"（真实目标 UI 形态） | §6 全部指标达标并记录 |
| M5 开发闭环 | 网页自定义模式、日志面板、FPS、`WriteMemory` 热重载 bundle、`pocket build` → 槽注入一键脚本；checkpoint 跳过启动 | 修改 TSX 到看到画面 ≤ 5 s；无需重链固件 |
| M6 真机准备（可选） | MMU 恒等映射 + cache 开启；`ffboot.bin` 打包实验（仅生成文件，不刷写）；或 stock-hosted HAL 骨架；列出 §8 前提的逐项验证计划 | 模拟器行为不变；前提清单有 owner |

实现状态（2026-11-07）：

- **M0–M5 已完成**，退出条件均有 `.cache/custom-firmware/` 证据。
- 首帧黄金图是 SHA 加 PNG；M1 的"30 Hz guest 帧、宿主 ≥ 30 fps"由 hello 测试覆盖。
- M3 的 target 改为仓库外 profile，没有补丁。
- M4 测了 NaN boxing、C `-O3` 和 `memcmp` 三组 A/B，只保留 `memcmp` 覆盖；分辨率维持方案 A。没有做 bytecode 预编译，因为启动已远低于预算。
- M5 实测改 TSX 到见画面 3.64 s。
- **M6 未开始。**

## 8. 风险与未知

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| TCG 浮点/NEON 慢且 QuickJS 大量使用 double | JS 帧超预算 | M3 先跑 JS 微基准，热点改用整数/f32；core 本身 f32；必要时在 PocketJS 侧减少每帧 JS 工作（它的设计目标就是"动画不走 JS"）。实现：guest 时间按指令计 tick，浮点指令不额外计费，宿主侧也没有出现浮点瓶颈；稳定帧 JS 约 0.42M ticks，热点是 `JS_CallInternal` |
| quickjs-ng 在 newlib-nano 裸机上的 libc 缺口（`fenv`、`clock_gettime`、`setjmp` 等） | 链接失败/运行时异常 | 先用 `-ffreestanding` 清单化依赖；参考 `tools/meizu-m8/quickjs-wince.patch`、`tools/blackberry-qnx/quickjs-qnx.patch` 的处理方式。实现：自写 mini libc，链接期缺口只有 `lrint`/`modf`/`atanh`，以及 ui-cabi 的 panic handler（需 `extern crate`） |
| 模拟器严格模式下任何越界 MMIO 直接停机 | 开发期频繁停机 | 视为优点；HAL 只允许 §3.5 清单；测试覆盖 |
| LCDC 模型约束：`+0x40==+0x48`、`+0x50` 高位必须恰为 `0x10000`、几何边界 | 翻页/缩放写错即失败 | HAL 封装一次写对；M1 覆盖 |
| 忙等待代替 WFI | 宿主时间被空转吃掉 | 帧循环与错误态一律 WFI；`endurance` 看 wall/guest 比 |
| PocketJS 上游变化快 | 补丁冲突 | 固定提交；只打一个聚合补丁（与 fujimod 同策略） |
| 真机前提全部未验证：X-T4 SD_BOOT（FujiMod 对 X-T4 标 Not yet）、DDR 初始化（ffinit）、电源 MCU 握手/看门狗（原厂有 FORCE_POWOFF 路径）、面板与背光初始化、EVF 切换、按键矩阵完整表 | 模拟器成功不等于能上机 | 本阶段明确不承诺；HAL 分层保证届时只重做底层；逐项放入 M6 清单 |
| 许可 | — | PocketJS/quickjs-ng MIT、newlib BSD 类、FujiMod 另有 NOTICE；固件产物随附第三方许可文本 |

## 9. 仓库布局

实现后的布局（下方保留最初建议，供对照）：

```text
firmware/                      # 独立 cargo workspace（rust-toolchain.toml、.cargo/config.toml）
  Cargo.toml  build.rs  link.ld  build.ts  profile.ts
  protocol.json                # 调试约定的唯一来源（日志环、状态字、应用槽）
  src/  start.S  main.rs  protocol.rs（生成常量）  layout.rs  mmio.rs  mailbox.rs  log.rs  libc.rs
        hal/{gic,lcdc,timer,gpio}.rs  frame.rs  hello.rs  pocket.rs
  libc/ include/*.h  src/{malloc,math,stdio,stdlib,string,time}.c
  app/profile.ts               # fuji-xt4 target（仓库外注册）
  apps/counter/  apps/dashboard/   # pocket.json + main.tsx + app.tsx
  dist/                        # 忽略：fuji-boost{,-hello}.bin、fuji-boost.json 清单、apps/
crates/xp4/src/ram_image.rs  crates/xp4/src/bin/xt4-camera.rs（image 命令）
crates/xp4/tests/custom.rs  crates/xp4/tests/fixtures/custom-first-frame.{json,png}
src/camera.tsx（固件预设）  src/firmware-debug.tsx  src/lib/firmware-presets.ts
references/pocketjs（e27bff3）  references/quickjs-rs（ba5bdd0）  # bun run setup 拉取，已忽略
```

最初建议：

```text
firmware/                                  # 自定义固件，独立 cargo workspace
  Cargo.toml
  targets/armv7a-fuji-eabihf.json
  boot/        start.S  link.ld  vectors.rs
  hal/         mod.rs（trait）  bare-xp4/{lcdc,gic,timer,gpio,adc,log,memory}.rs
  host/        frame_loop.rs  input_map.rs  present.rs  mailbox.rs  camera_ext.c（扩展点，M4+）
  c/           quickjs-ng/（固定版本，子模块或打包）  newlib_stubs.c  pocket_runtime.c（来自 pocketjs）
  app/         pocket.json  src/app.tsx（计数器 → 仪表盘）
  build.ts     # 一键：pocket build → 编 Rust/C → 链接 → objcopy → dist/fuji-boost.bin + build.json（哈希、工具链）
  dist/        # 忽略
patches/pocketjs-fuji-target.patch
crates/xp4/src/custom.rs
crates/xp4/tests/custom.rs
crates/xp4/tests/fixtures/custom-first-frame.png
docs/design/custom-firmware-pocketjs.md    # 本文
docs/research/platform/YYYY-MM-DD.md       # 各里程碑的实测记录
```

## 10. 后续应用开发模型（本方案要达成的"方便"）

- 应用用 TSX 写，`pocket build --target fuji-xt4` 产出 bundle/styles/pak；`bun run firmware:app` 注入应用槽并请求 guest 重启，不重链固件。
  - 实现：新应用放在 `firmware/apps/<name>/`（`pocket.json` + `main.tsx`），`firmware:build`/`firmware:app` 会自动发现。`firmware:app` 只负责监听与重打包，注入由网页完成：sha 变化时自动写入应用槽，或点击应用按钮。测试里用 `crates/xp4/tests/custom.rs` 的 `Guest::load_app`。
- 相机能力通过 `POCKET_RUNTIME_EXTENSION` 向 JS 暴露 `globalThis.camera`（读状态、发命令）。模拟器阶段其后端是假数据或读模拟器输入；真机阶段换成原厂服务调用或裸机驱动，应用代码不变。
- 多应用/启动器复用 PocketJS 已有的 `appTable/appLaunch` 多 app 机制，不另造。
