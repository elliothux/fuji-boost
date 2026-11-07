# fuji-boost

Inspired by [FujiMod](https://github.com/justjake/fujimod).

用于 Fujifilm 固件研究的本地 DevTools：Rust / WebAssembly 执行核心，React、TanStack Router、Tailwind、Vite 和 shadcn/ui 工作台，使用 Bun 管理依赖。

**当前状态：研究原型，正常可交互的 X-T4 相机仿真尚未完成。** 已有测试仅证明声明的合成输入能推进到设置向导和带“卡错误”的拍摄UI；实时场景／摄像头输入、持续菜单与拍摄操作、可用的虚拟SD拍摄流程仍未接通或验证。此前“DevTools已完成”的交付结论应按此更正。FujiMod已发布的EXR Processor II运行时与X-T4的差距见[研究对照](docs/research/devtools/2026-10-31.md)。

## 运行

需要 Bun、Rust/rustup、Node.js 24+、Python 3、CMake、Ninja 和 Git。

```sh
bun install --frozen-lockfile
bun run setup
bun run dev
```

打开 http://127.0.0.1:5173，选择官方 `.DAT` 固件。文件在浏览器 Worker 内处理，并保存到此浏览器的 IndexedDB 本地库。

选择执行场景后，可单批执行或点击“连续运行”。“暂停”在当前批次结束后生效；未知设备错误、固件停止或无后续事件时会自动停止。重置会话可重新加载固件并选择场景，也可恢复加载失败后的会话。

首次设置和扩展初始化场景在会话暂停后可使用虚拟按键，即使当前没有显示帧；每次输入仍等待固件确认按下与释放采样。

“扩展初始化（实验）”使用 `linked_dma_input` 路径及明确声明的2.12合成输入，包含有限协议应答、重编程、FA握手、后续控制请求、合成就绪事件和链式DMA，并支持六个虚拟按键。链式DMA限定供应192240个4字节全零word，以每word至少8个虚拟tick和描述符链结束时的完成IRQ作为实验条件。两个卡槽各插一张FujiMod SD模型的16 GiB空FAT32卡（写保护按Milbeaut反向极性）。独立重放已核对九个原厂向导画面，并在1,100,270,606条指令时取得实际900×600拍摄UI，包含H.264、4K、剩余录像时间、ISO及AF信息，不再显示卡错误或卡保护。取景区域默认是合成灰色输入，可换成测试图、图片或摄像头；照片／视频文件写入、物理数据格式、搬运和时序未验证，见[显示研究与实际画面](docs/research/display/2026-10-31.md)。

网页在首次创建CPU前启用TCG JIT；“CPU执行”中的“执行加速器（宿主统计）”单独显示编译、执行与失败统计。`bun run test:browser:boot` 在无头Chromium页面中从DAT起步，通过九个设置向导画面、九次按键和1050个执行批次，最终Canvas与独立Node测试的拍摄UI像素完全相同，随后验证灰场、测试图与虚拟摄像头画面源。传感器和完整拍摄功能仍未验证。

宿主画面源同时供应 MOVDC 和 AE 统计：RGB 每通道先以 sRGB EOTF（`s=c/255`；`s<=0.04045` 时 `L=s/12.92`，否则 `L=((s+0.055)/1.055)^2.4`）转为 `clamp(L/L(128)*2048,0,4095)`，再按像素覆盖面积平均到16×16统计网格并四舍五入。保留 profile 的 `samples_per_cell`、`black_level`、`ae_cell_area` 和 `signal_shift`。测试图的统计固定使用第0帧，显示仍有动画；灰色选项恢复原始 profile 场景。画面源及统计场景属于宿主输入，不写入存档；恢复存档保留当前输入，旧存档中的场景字段被忽略。60×1M 指令的黑白对照已确认统计 SRAM 不同，但传感器编程未变化，AE 反应仍未通过验收。

`setup` 仅获取固定版本的 FujiMod / Unicorn 源码并应用既有补丁；不安装浏览器执行工具链。当前架构与运行方式以末尾 Native camera 为准，本文其他浏览器工作台相关说明均为历史研究记录，不再适用于当前界面。

React/CSS 支持热更新；工作台代码更新会释放旧执行会话并回到待加载状态，固件可从本地库重新加载。修改 Rust 核心、机型索引或嵌入的 `profiles/fixtures/*.bin` 后会重新编译对应 Wasm，成功后统一刷新页面。README、RESEARCH与研究目录不触发页面重载。`bun run build` 生成 `dist/`，`bun run preview` 预览。Wasm CPU 模块需要 COOP/COEP 响应头，开发和预览服务均已配置。

## 当前能力

- 官方 DAT 的身份校验、映像解压、分段哈希和原始字节检查。
- X-T4 2.12 的四核 Cortex-A7 执行、寄存器和停止原因显示；默认严格模式，可显式选择带假设的预加载诊断或理想校准实验。
- 真实入口里程碑、合成输入与启动假设展示；guest scanout接入Canvas，已导出900×600原厂设置向导和带“卡错误”的拍摄UI。
- 从固定 catalog 生成 XP4 外设配置，接入 GPIO、ADC、定时器、SPI、电源校验协议、显示和成像模型；配置来源及未验证假设单独记录。
- 从 FujiMod 的 RAM 预加载冷启动 loader 开始执行，界面列出所用的四处补丁；这不是 Boot ROM 复位启动。

当前研究目标为 X-T4 **2.12**；2026-10-29核对的[官方最新版](https://www.fujifilm-x.com/global/support/download/firmware/cameras/x-t4/)仍为该版本，新下载DAT与根目录文件逐字节一致。升级后普通PTP只读查询也返回X-T4/2.12，未取得live binary hash；旧版资料已清理。开发以本地仿真为主，模拟器写入发生在电脑的虚拟内存中。真实低风险分析操作须在执行前评估变砖风险，不能把任意RAM/debug写入默认当作低风险。

浏览器固件库升级时会删除不在当前profile中的旧X-T4记录及文件Blob，保留当前2.12固件；清理结果已验证可跨刷新保留。

**X-T4 原厂拍摄UI已在合成输入场景中显示，完整拍摄功能仍未验证。** 理想校准场景保留更新模式错误界面；扩展初始化经语言、区域、日期及手机应用介绍的九个原厂画面后可显示拍摄UI。默认严格模式在7,346条指令后停止于未建模的 `0xfff4a400`；原生/Wasm四核状态一致。原有被动预加载基线仍会等待 `0xff614670`，零poll干预。

独立“理想校准实验（合成输入）”已执行原厂冷初始化、四个unit的mode5、profile1保存/移除和首次audio reset。当前原生/Wasm在80,020,606条指令的所有保存字段一致，网页64寄存器与四核计数也一致。累计实验明确声明虚拟校准、RTC及独立电源输入；这些是实验条件，实机行为未测量。

累计实验另声明一个4字节FFFAE080命令传输模型，复用既有Requests并选择8个guest tick完成；独立F0状态及物理模块功能仍未建模。原厂命令等待已退出，累计实验提供独立64/65及class3批次13个完整u16合成读值，物理值未测量。word2分别显式应答checksum有效的完整0001与0101，read2保持独立0；没有word2专属电源/复位效果。0101保留首256个应答tick并累计总数，避免诊断记录容量中断传输。新冷启动已执行1,024,020,606条指令，无模型错误，记录output1/900×600；达到预算不代表完整开机。音频初始4145对数据逐项匹配原表，原厂软件状态到2；物理音频仍未验证，I²C先前返回-255。

当前还将15个有来源与哈希的DAT范围映射到虚拟NOR，共36,307,968bytes。压缩槽保留原始编码，raw槽保留原始填充；未供应区仍为erasedFF。这已解除缺失object8造成的扫描循环，以及resources1头为FF造成的余数检查自环。早期1024M重放已导出2160000bytes实际RGBA，识别为日文固件更新／存储卡错误画面；无观察器重放除host trace/probe外所有保存字段完全一致。后续扩展初始化的拍摄UI结果与各阶段限制见 [研究记录](docs/research/README.md)。**没有真实机身刷写功能；完整相机硬件与文件拍摄尚未验证。**

另有显式`gpio9_high`研究入口：仅将虚拟GPIO9.5初始电平设高。原厂property9变为0并避开UPDATE，但1024M运行转入SETUP_REC后FORCE_POWOFF，未产生显示帧或正常REC入口；默认网页场景因此保持原对照。被动记录已将此次关机闭环到原A00B电池耗尽软件事件，实际链已追到软件缓存1790fb8及其getter返回4，不能据此判断真实机身低电。源码guard、输入隔离和原生/Wasm验证见本机归档`.temp/research-archive/xt4-board/settings-origin/power2-bit8-echo/gpio9-high/`。这不代表真实引脚意义或电平已经确认。 另有`power3_raw768`研究入口，只将外部read3固定值512改为768，并为该模式提供首256条0001应答与总计数。1024M已输出原厂语言设置向导，不能称为实机健康电池输入。网页“首次设置实验（合成输入）”还会释放六个逻辑导航键，暂停后可模拟一次按下/释放，等待原固件采样确认。实际已通过语言、区域、日期和手机应用介绍页；按键完成后可能还需“连续运行”才能完成重绘。介绍页“结束”后止于尚未支持的SPI2接收DMA。首错被后续deadline覆盖的问题已修复，两轮完整重放除错误文本外全部保存状态一致。

SPI2 FIFO/DMA基础设施现已通过组合板级ARM测试：接收先启动时等待实际FIFO数据，发送后逐byte收取不同值；原有四个网页场景保持关闭；新增扩展初始化仅提供显式声明的16byte应答序列。只读`read_virtual_memory(core,address,length)`供研究脚本导出虚拟内存，长度限制1–65536bytes并拒绝MMIO；这不连接真实机身。现另有只读终态内存捕获helper，按64KiB导出core0虚拟视图与源资格小字段，无法读的块保持unknown；它不是物理RAM或可恢复VM快照。这些早期SPI2对照已读到REC3模式字段，但当时尚无正常拍摄画面。当前首个biscom请求的虚拟RAM发送头为000820d0、状态3。独立RAM导出确认两处历史字段H均为0；新增仅供研究脚本使用的`spi2_first_header`入口，严格测试该4byte请求与合成全零回复，记录原厂回调/消费者/后续请求，拒绝第五byte。实际重放已确认首帧完成、原消费者返回0并进入state4，再构造第二头08108022；第二帧被严格拒绝。它不代表完整peer、正常拍摄或物理时序。原有四个网页场景保持原配置。

## 验证

把自行取得的 X-T4 2.12 官方固件放在根目录 `FWUP0024.DAT`（不随仓库分发），然后运行：

```sh
bun run check
bun run test
bun run test:xt4
bun run test:xt4:cpu
bun run test:xt4:checkpoint
bun run test:xt4:long
bun run test:xt4:calibration
bun run test:xt4:scenarios
bun run test:xt4:rec
```

默认 `test` 覆盖原生固件检查、XP4 单元测试及前端协议；`test:xt4:long` 核对 80M 指令的 CPU、RTC 和未知访问记录，`test:xt4:calibration` 保留原有校准断言。它们不代表完整开机通过；历史跨后端证据保存在 [研究记录](docs/research/README.md)。

`test:xt4:rec` 直接运行原生 Rust 长测试，使用官方 2.12 DAT、同一按键 ACK 实现和 `crates/xp4/tests/fixtures/xt4-rec.json`，逐帧核对九个向导及 900×600 REC 的完整 RGBA 哈希。`:rec:measured` 使用实测输入；`:checkpoint` 生成 `.cache/xt4-rec.checkpoint`，`:checkpoint:load` 验证恢复与后续 20 批一致性；均有原有预算限制，不连接机身。

另有用于研究的诊断命令，结果写入 `.temp/xt4-board/`：

| 命令 | 逐步增加的实验条件 | 当前结果 |
| --- | --- | --- |
| `bun run research:xt4` | discovery 记录未知 MMIO | 电源 ready 轮询 |
| `bun run research:xt4:ready` | `reg0=0x8000` | 191,598 条指令，停止于写 `reg17=0x7f00` |
| `bun run research:xt4:commands` | 状态16固定为0，记录并应答观察到的初始化命令 | 225,174 条指令，停止于读 `reg31` |
| `bun run research:xt4:retained` | 寄存器31–34为会话内存储，初始全零 | 3,000,000条预算，停留在 Flash 完成状态轮询 |
| `bun run research:xt4:flash` | 显式 erased NOR，使用 catalog 的几何与身份声明 | JEDEC 命令完成，等待未实现的校准握手 |
| `bun run research:xt4:training` | 两阶段数字握手及固件内可验证的128字节校准图案 | 完成两组 tap 扫描，选择 tap62，继续等待启动屏障 |
| `bun run research:xt4:smp` | 100k指令的被动RAM观察 | 证实次核到达标记被core0后续初始化覆盖 |
| `bun run research:xt4:ordered` | 主核完成原始屏障初始化后，再调度次核 | 四槽自然归零，原厂主映像入口 `0x012910c4` 已执行；尚无相机UI |

`bun run research:xt4:inputs` 比较 catalog／measured 两组原生 800M 输入轨迹；旧的冻结快照读取探针已移除，80M 被动观察对照由 `test:xt4:long` 覆盖。证据写入 `.cache/xt4-tests/`。

诊断记录包含假设、影子访问、干预和输入覆盖，始终保持 `bootVerified: false`。网页需要用户显式选择对应的诊断或合成输入场景。命令模型只应答 `0x7f00` / `0x3700`；低两位电源转换请求会记录并停止，其他未支持值会拒绝。预加载场景还声明仅一个NOR厂家身份字节2c、status18=0的inactive countdown（只应答0100初始化，拒绝arm/expiry）、外部GPIO bank0.bit0为低，以及初始status16=0010。17初始化命令之后的16值保持未知，若再次读取就停止；不猜测清零或保持规则。虚拟RTC采用源码寄存器角色和明确的功能时序（54M guest ticks/秒、1-tick提交strobe）；不代表实机测量。最新场景禁止自动poll改位，保留未知访问记录。保留字场景不提供 RTC、提交或断电保持语义。校准场景声明8-tick完成延迟、状态清除规则和所有tap均能读回同一图案的理想通道；不模拟模拟前端，也不提供完整物理 Flash 内容。启动次序场景只调度原始指令，不修改屏障RAM；它不证明真实BootROM放核顺序。原生 `discover` 示例可额外接收1–512个百万指令批次，默认3批；长运行仍不能单凭预算耗尽判为开机成功。

理想校准实验独立于预加载基线。它从用户 DAT 的预加载 RAM 中提取686项初始配置、7项准备配置和17项 profile，校验来源哈希；四个额外11位结果字段选择768，完成时延选择8个guest tick，unit0显式预初始化。这些是合成输入，不能作为真实DDR校准结果。另从DAT提取每unit71项mode5原始表，共2272 bytes；完整写完才调度8-tick事件将5384整个word清零。现在还从DAT提取44-byte保存/移除脚本，完整写完后调度8-tick事件设置7090 bit0；FF70C080复用8-tick posted-request模型。外部控制RMW仍为被动shadow，没有物理路由或电源效果。仅支持一次冷初始化、mode5及移除；restore、其他模式、复位和重入仍不支持。 audio任务的独立FFF4F01C fixture只接受一次write1，按8-tick虚拟约定返回整word0；其他输入、初始读取及完成后重入拒绝。其物理完成条件和时序未测量。单元测试覆盖失败结果、延迟/重复命令、独立事件与检查点恢复。累计场景另选择只读power register14整16位为0，校验原始180-byte消费者后才注入；所有位均为合成选择，没有由无效cache或失败SPI残留推断实机值，也不强行清除旧snapshot控制的GPIO分支。

复现并核验这个实验：

```sh
bun run test:xt4:calibration
python3 .temp/research-archive/xt4-board/verify_patches.py  # 仅本机归档
```

完整状态对照包括原厂函数返回、设备事件、寄存器及零poll干预；80M测试确认profile API、保存/移除与首次audio reset等局部执行结果，不代表完整启动。`ideal_calibration`为当前累计场景；严格模式与`preloaded`基线不隐式启用这些条件。`class3_inputs_baseline`保留0001-only、拒绝0101的对照；`power2_echo_baseline`保留未提供class3输入、停在read47的对照；`power64_inputs_baseline`保留无word2应答的旧拒绝边界。各`*_baseline`入口保留先前边界，包括未提供新电源读输入、禁用RTC锁存/原值应答、未挂resources1压缩槽，以及`packed_resources_baseline`中未挂其余9个catalog槽。对照运行与输入清单保留在本机归档`.temp/research-archive/xt4-board/settings-origin/`（不入库）。67写、主动countdown及物理时序仍未验证。

延长Wasm研究预算（每批1M，最多4096批）：

```sh
bun run test:xt4:long
bun run test:xt4:calibration
```

当前长测试及校准轨迹写入 `.cache/xt4-tests/`；已有历史快照在本机归档 `.temp/research-archive/xt4-board/`，不用新结果覆盖。物理输入、时序与完整启动的限制仍见 [研究记录](docs/research/README.md)。

独立研究入口`spi2_second_header`继续测试第二头08108022与合成回复0800a0da，最多8bytes后停止，记录原厂消费者/状态6/下一请求。原生与Wasm均已实到state6并提交全零第三头，在第9byte停止；完整保存态/诊断与9帧交叉核验一致。它不代表协议owner命令完成、全RAM等价或正常拍摄。原有四个网页场景仍保留原配置。

研究脚本可在fresh Wasm module创建任何会话前调用一次`configure_tcg_jit(true)`，复用上游TCI编译tier；API默认关闭，网页已在首次创建CPU前显式启用。每次run后自动flush，`tcg_jit_statistics_json()`单独返回host统计（未启用时为null）。strict/preloaded9M/四核80M，以及完整首次设置导航到SPI2第二头边界的开关对照均已逐byte一致；这不代表正常开机或普遍提速。

原生研究复放器读取本项目已保存的导航账本，复用同一组预算和按键；输出路径须为当前项目内尚不存在的目录（父目录需已存在）：

```sh
cargo run --release --manifest-path crates/xp4/Cargo.toml --example replay_spi2 -- --self-check
cargo run --release --manifest-path crates/xp4/Cargo.toml --example replay_spi2 -- .temp/xt4-native-replay-new
```

`--owner-ack` 选择独立四头实验，例如在上述 `--` 后传入 `--owner-ack .temp/xt4-owner-replay-new`。它预声明最多16bytes，第17byte或不符请求会被拒绝，并观察原厂消息投递、业务线程和出队记录；不连接实体相机。原生/Wasm已实到成功投递、owner出队和结果函数入口，公共保存点及9帧交叉核验通过；Wasm随后停于尚未建模的mode2校准写入。正常取景与拍摄仍未通过，证据见 [研究记录](docs/research/README.md)。

`calibration_mode2`是进一步的独立研究入口：在同一16byte SPI2场景上启用原mode2表的一次完整写入与显式合成完成事件，旧场景默认关闭。源码、原生测试及完整原固件重放已验证unit0达到Mode2Done，随后停在Removed unit1的重初始化首写；正常REC仍未通过，不连接真实相机。

原生复放器默认运行2.12第二头场景；两种模式均复用固定导航账本。exit0表示结果已保存，不表示相机开机成功。

## 代码与机型

`crates/core` 提供固件检查，`crates/xp4` 连接 FujiMod XP4 / Unicorn 执行核心，`src/workers` 承担浏览器执行，`src/workbench.tsx` 展示调试结果。

`profiles/index.json` 注册固件的精确哈希、机型和版本。目前仅 X-T4 2.12 接入 CPU 执行；X70 条目用于身份识别。新增机型或版本还需接入对应的启动流程和硬件模型。

`scripts/board-profile.ts` 生成 `profiles/boards/*.generated.json`；修改生成器会触发 Wasm 重编译，请不要手改生成文件。当前电源 SPI 的四字节帧选择由实际 loader 反汇编与运行证据修正，不能套用三字节 peer 的 RTC 寄存器语义。

FujiMod 源码固定在 `1de936e9b1ca738b04ad5132c727288ddd5eb2ae`，保留为外部引用；当前源码变更统一保存在 `patches/fujimod-devtools.patch`，以避免层叠补丁的重叠上下文破坏重复 setup。原有八份补丁保留作研究来源。聚合补丁已从固定上游及 Unicorn gitlink 重建，干净应用后33份文件与当前源码逐字节相同。项目自身的 MIT 许可不覆盖第三方源码；来源说明见 `vendor/fujimod-firmware/NOTICE.md`，生成的 CPU runtime 随附 Unicorn 的许可文本。

## 自定义固件（PocketJS）

`firmware/` 是不依赖原厂 DAT 的 ARM32 裸机映像，在同一 XP4 模拟器里运行 PocketJS 运行时（QuickJS + ui-cabi）与 `firmware/apps/*` 的 TSX 应用，450×300 逻辑面由 LCDC 放大到 900×600。需要 Homebrew `llvm@19` 和 rustup nightly（`firmware/rust-toolchain.toml`）；`bun run setup` 会拉取固定版本的 PocketJS 与 QuickJS 源码。

```sh
bun run firmware:build     # firmware/dist/：映像、fuji-boost.json 清单、apps/
bun run test:xt4:custom    # 首帧黄金图、按键、热重载、120 s 耐久与 checkpoint、确定性
bun run firmware:app       # 监听应用源码并重打包；配合网页自动热重载
bun run firmware:profile   # PC 采样热点
```

在相机页右侧"固件"里切换原厂与自定义映像，或直接打开 `/?port=<xt4-camera 端口>&firmware=fuji-boost/pocket`。自定义映像下可直接点 LCD（后触屏，最多两指），并用「画面源」把灰场 / 测试图 / 图片 / 摄像头送到应用里的取景窗。模拟器只提供硬件（映像加载、执行、输入、LCD、内存读写、checkpoint）；日志、状态字与应用槽的约定由固件清单里的 `firmware/protocol.json` 描述。设计与验收数字见 [docs/design/custom-firmware-pocketjs.md](docs/design/custom-firmware-pocketjs.md)，只在模拟器内验证，未上机。

## Native camera

Run `bun run camera:native` (loopback port 5199; append a port to override).
In another terminal run `bun run dev`, then open `http://localhost:5173/` (or `/?port=port` for a custom server port).
The camera always uses the native WebSocket server. Generate the REC checkpoint with `bun run test:xt4:checkpoint` before first use.
