# Fujifilm X-T4 自定义固件：公开资料研究

研究日期：2026-10-23（Asia/Shanghai）。范围为公开源码、项目作者文档、官方资料和作者/使用者讨论；本子任务没有连接相机、执行第三方代码或刷写。主任务的本地固件分析结果另存，不应把本文的社区报告表述为本轮实机验证。

## 结论与证据矩阵

**X-T4 是可开展静态逆向研究的目标，但目前查到的公开证据不足以把它当作已有成熟自定义固件的平台。关键难点已不只是解包，而是版本对应的代码定位、跨核/硬件边界、补丁完整性，以及可验证的恢复路径。**

| 问题 | X-T4 证据 | 判断 |
|---|---|---|
| 已知平台 | tiredboffin 作者表：X6 容器、FF4230、X-Processor 4、四核 Cortex-A7、ThreadX SMP | 有较明确的逆向基础；属于作者逆向记录 |
| 现成 FujiHack 移植 | `model/xt4_150.h` 仅一行 `MODEL_NAME`，没有地址/stubs | 文件存在不代表已支持 |
| 修改固件后实机运行 | FujiHack 官网只列 X-A2 1.30、XF1 1.01 | 在本轮检索边界内未找到 X-T4 的同等级证明 |
| 解压研究 | fffw 有独立 `ffcompress`，FujiMod 有实际 method-4 decoder | 可复用并交叉验证 |
| 重封装 | ffcompress 作者报告 1259 段、484 固件、70+ 型号 bit-exact；不保证未知数据和硬件行为 | roundtrip 是必要检查，不能证明修改固件安全 |
| X-T4 完整恢复 | 未找到 X-T4 的公开、完整、已复现恢复流程 | 刷写前的重大缺口 |
| 不刷固件定制 | 官方 SDK 明列 X-T4；libfuji 支持 USB/Wi-Fi/BLE | 遥控、自动化有现实路线；具体能力需逐项验证 |

## 1. 用户给定的 yi-mirrorless-firmware-tools

来源：

- [仓库 README](https://github.com/protyposis/yi-mirrorless-firmware-tools/blob/master/README.md)
- [issue #3：未修改数据重压缩结果不同](https://github.com/protyposis/yi-mirrorless-firmware-tools/issues/3)
- [issue #10：直接替换不同固件片段后无法启动的用户报告](https://github.com/protyposis/yi-mirrorless-firmware-tools/issues/10)
- [firmware.js](https://github.com/protyposis/yi-mirrorless-firmware-tools/blob/master/firmware.js)
- [lzss.js](https://github.com/protyposis/yi-mirrorless-firmware-tools/blob/master/lzss.js)

仓库主要针对 YI M1，README 的 Fujifilm 案例是 **X-A10**，不是 X-T4。其 `LENGTH=` 文本段头、ND1/IPL/PTBL/A9262 属于另一套 ODM 格式。作者给出的部分 YI M1 系统线索包括 EV9X、ARM926 等。仓库于 2024-05-12 归档。

README 明确写重封装刷写未测试、压缩未完全正确，建议不要刷写重封装文件。issue #3 解释 lookup 选择导致输入→解压→重压后的字节不一致，问题没有在本轮资料中被证明解决；issue #10 是一位用户将 Fujifilm 片段替换进 M1 后无法开机的报告，不能用作 X-T4 的行为证据，但说明不能跨机型替换片段。

其有参考价值的部分是分段、checksum、压缩算法研究方法；**不应直接把工具用于 X-T4，也不应从 X-A10 可解包推断 X-T4 可刷写**。

## 2. FujiHack 的实际支持边界

来源：

- [FujiHack 官网](https://fujihack.org/)
- [仓库 README](https://github.com/fujihack/fujihack/blob/master/README.md)
- [Supported Models 的范围说明](https://wiki.fujihack.org/supported/)
- [X-T4 型号文件 xt4_150.h](https://github.com/fujihack/fujihack/blob/master/model/xt4_150.h)
- [Web patcher README](https://github.com/fujihack/patcher/blob/master/README.md)
- [实际 patcher.js](https://github.com/fujihack/patcher/blob/master/patcher.js)
- [Firmware header 文档](https://wiki.fujihack.org/firmware/)

官网截至检索日列出的 hacked models 为：

- X-A2，固件 1.30：bin loader、USB code runner。
- XF1，固件 1.01：USB code runner。
- 部分镜头验证过固件降级。

官网仍写没有可供公众直接使用的现成 hacks。Wiki 的 supported models 特别声明：这是采用 Fujifilm 相关 RTOS 的型号列表，**不是 Fujihack 已经能运行的机型名单**；其中列 X-T4 不能理解成支持 X-T4 自定义固件。

`model/xt4_150.h` 实际只有：

```c
#define MODEL_NAME "Fujifilm X-T4"
```

没有补丁地址、运行时 stubs、屏幕参数或已测能力宏。现成 patcher 的数据库依赖型号代码及每版本的地址；不能因存在型号名就直接选择补丁。

FujiHack 所展示的按钮映射、记录时长、图形绘制、任务创建、SQLite、DOOM 等，是研究能力与旧机型成果。对于 X-T4 需要重新定位版本对应函数、验证调用约定和硬件状态，不能直接搬地址。

旧格式 header 文档说 payload 是按位取反，加和 checksum；patcher.js 实际也做取反、注入、checksum 调整。它不是现代压缩段完整重建器，也不提供 X-T4 实机成功证明。

相关功能讨论原始来源：

- [issue #19：Gyroflow/陀螺仪日志请求](https://github.com/fujihack/fujihack/issues/19)：是功能问题，不是已实现记录器。
- [issue #18：画面 de-squeeze 请求](https://github.com/fujihack/fujihack/issues/18)：是功能问题，不是 X-T4 的已有实现。
- [issue #6：Future Plans](https://github.com/fujihack/fujihack/issues/6)：其中多个计划仍未完成，不能当作产品功能清单。
- [DOOM 作者说明](https://fujihack.org/doom.html)：2023-04-01，实机目标为 X-A2。
- [AUTO_ACT.SCR](https://wiki.fujihack.org/autoactscr/)：文档说需 EEPROM 特定标志才能激活，并非将脚本放 SD 就能在所有相机运行。本轮没有为 X-T4 验证该前提，也没有写 EEPROM。

## 3. fffw：更相关的资料，但发布状态需要逐个检查

来源：

- [fffw README](https://github.com/tiredboffin/fffw/blob/main/README.md)
- [ffun README](https://github.com/tiredboffin/fffw/blob/main/ffun/README.md)
- [ffundat Python 占位入口](https://github.com/tiredboffin/fffw/blob/main/ffun/py/ffundat/ffundat.py)
- [ffundat Go wrapper](https://github.com/tiredboffin/fffw/blob/main/ffun/internal/cli/ffundat/ffundat.go)
- [ffcompress README](https://github.com/tiredboffin/fffw/blob/main/ffcompress/README.md)
- [fflz README 与局限](https://github.com/tiredboffin/fffw/blob/main/ffcompress/fflz/README.md)
- [X-T4 2.12 枚举头文件](https://github.com/tiredboffin/fffw/tree/main/ffun/ports/xt4/0212)
- [作者研究入口](https://tiredboffin.github.io/)

顶层 README 描述 ffun 能拆 DAT、解压、重封装、转换 ELF，覆盖 EXR 至 X-Processor 5。但 `ffun/README.md` 同时说明 Python→Go 迁移尚未完成，很多模块还不能构建；本轮读取的 `ffun/py/ffundat/ffundat.py` 只有 `# TBP` 和 usage 占位。因此不应简单给用户“下载 fffw 后运行 dat2elf 即可”的承诺。

独立 `ffcompress` 目录存在真实 Go packer/unpacker 和测试，README 指定 Go 1.23+、无依赖，支持 fflz 与较旧 LENGTH= 的 LZSS。作者声明 fflz 对 1259 个 unique segments、484 个固件、70+ 机型做过 bit-exact 解压重压测试；这只是作者公布的样本结果。

明确局限：

- 硬件解压逻辑未公开，软件实现来自样本推断。
- 16K 页面边界处 token 切分需要保持规则。
- 未知 stream ending 默认拒绝；强制 guess 选项有告警。
- 恰好落页边界且裁掉末尾零的极少数样本置信度低。
- 原始段 bit-exact roundtrip 不意味着修改后的段、总容器、启动和成像路径正确。

X-T4 2.12 头文件实际含 task IDs、UI/menu/opcode、PTP、film simulation 等枚举，可作为搜索种子，不能把枚举名字当可直接调用的运行时地址：

- [task.h](https://github.com/tiredboffin/fffw/blob/main/ffun/ports/xt4/0212/task.h)
- [mode_fsim.h](https://github.com/tiredboffin/fffw/blob/main/ffun/ports/xt4/0212/mode_fsim.h)
- [libptp.h](https://github.com/tiredboffin/fffw/blob/main/ffun/ports/xt4/0212/libptp.h)
- [v61_syscalls.h](https://github.com/tiredboffin/fffw/blob/main/ffun/ports/xt4/0212/v61_syscalls.h)

`ff80` 是服务/调试协议工具，能涉及 RAM 和校准/调整数据。本文没有执行；将“只读离线固件研究”升级到服务模式操作前需另定范围，不要因存在工具就写校准或持久化状态。

## 4. FujiMod：新的现成解码代码，可交叉验证

来源：

- [FujiMod 仓库](https://github.com/justjake/fujimod)
- [firmware crate README](https://github.com/justjake/fujimod/blob/main/crates/firmware/README.md)
- [实际 Rust decoder lib.rs](https://github.com/justjake/fujimod/blob/main/crates/firmware/src/lib.rs)
- [decoder tests.rs](https://github.com/justjake/fujimod/blob/main/crates/firmware/src/tests.rs)
- [Python firmware API stub](https://github.com/justjake/fujimod/blob/main/bindings/python/python/fujimod/firmware.pyi)
- [Python binding 构建/运行说明](https://github.com/justjake/fujimod/blob/main/bindings/python/README.md)

实际实现包括：

1. `scan(source)` 先 XOR `0xFF`，搜索压缩头中 `page_size=0x4000, method=4`。
2. 压缩头是五个 little-endian u32：`unpacked_size, packed_size, page_count, 0x4000, 4`。
3. token `<0x80` 为 literal 长度；否则两字节 backref，`distance=((c&0x7f)<<4)|(b>>4)`，`length=b&15`，支持重叠复制，验证边界与输出大小。
4. `inner_header` 识别 32/64 位：第 0 word 为 0，第 7 word 为 `0xAAAAFFFF`；第 1/2/3 word 为 base、initialized size、entry。
5. `overlays` 解析有边界约束的 9-word section 与 6-word overlay 表，输入必须提供偏移、hash、flash/DAT bias，不能靠猜测映射。共享地址的 overlay bank 是独立单元。
6. `outer_header` 支持 type 4/6/8，payload offset 分别 `0x25c/0x27c/0x2bc`；type8 查找 ECDSA P-256 DER trailer。**语法解析不验证签名，也不验证更新 checksum。**

其 [X-T4 2.12 catalog](https://github.com/justjake/fujimod/blob/main/catalog/firmware/xt4/2.12/firmware.json) 记录：

```text
size = 51278280
sha256 = 853d273b5603d3a2b93493f0ebd6a762b0b4b6bdc70317777775ae5f8ed93bc5
boot_rom = null
installation = null
generated_facts = null
```

这是很好的原始文件身份交叉参考，并不证明可安装的 X-T4 补丁。其 [xt4-212 run.json](https://github.com/justjake/fujimod/blob/main/configurations/emulator/xt4-212/run.json) 和 [body.json](https://github.com/justjake/fujimod/blob/main/catalog/bodies/xt4/body.json) 使用 discovery 配置、部分共享 XT3 部件模型与声明输入。不能将该配置文件的存在写成完整 X-T4 硬件/启动/成像仿真已通过。

此外根 README 引用的 `crates/mod-install/README.md` 在检索时返回 404，说明文档/公开树存在不完全同步，采用工具前应依实际代码检查入口。

## 5. 架构、容器与签名是不同问题

来源：

- [tiredboffin 的 CPU and OS History Table](https://github.com/tiredboffin/fffw/wiki/CPU-and-OS-History-Table)（页面显示 2025-12-26 编辑）
- [Fujifilm Camera History — RE perspective](https://github.com/tiredboffin/fffw/wiki/Fujifilm-Camera-History-%E2%80%90--RE-perspective)（页面显示 2026-07-11 编辑）
- [作者对研究/恢复边界的说明](https://tiredboffin.github.io/)

作者表格中 `XT4` 行是：X6、FF4230、X-Processor 4 Cortex-A7 4 Cores、ThreadX SMP；`XA10` 为 LEN、EV9XES、Main ARM946ES/Sub ARM926。这直接解释了为什么 yi 工具与 X-T4 不可简单互换。

作者历史页说明：X-Processor 4 是四核 Cortex-A7；有 IBIS 的机型还带负责防抖/OIS 协作的独立模块代码，包含 Thumb-2。fffw 自述 ELF 路线只包括 ARM 主体，不覆盖所有 IBIS、视频 codec、DSP、AI 代码。因此单个解压 ARM 映像不等于完整系统所有功能。

同一作者对较新 X-Processor 5 安全更新的 ECDSA 描述不能反向套到 X-T4。X-T4 的外层格式、可识别 trailer、更新程序如何校验和启动链如何校验，应依据其确切版本文件和代码分别判断；本文不声称“有加和 checksum 就没有其他校验”。

## 6. 2026 年社区进展：需要保留证据分级

### X-T3 胶片模拟移植

- [作者帖子：X-T3 running Classic Negative + Eterna Bleach Bypass](https://www.reddit.com/r/FujifilmX/comments/1wda43y/xt3_running_classic_negative_eterna_bleach_bypass/)（2026-09-11）
- [原版 r/fujifilm 帖及评论，主帖现已删除](https://www.reddit.com/r/fujifilm/comments/1wda2x6/removed/)（2026-09-11 起）
- [另一位使用者的 Classic Neg on X-T3 帖子](https://www.reddit.com/r/fujifilm/comments/1wqyyln/classic_neg_on_xt3/)（2026-09-26）

作者自述比较 X-T3 5.11 与 X-T4 2.12，保留 X-T3 基础移植 Classic Negative/ETERNA Bleach Bypass，已刷入自己的 X-T3。其说明将 outer inversion、压缩段、处理映射、参数、曲线等分开研究，并承认仿真没有覆盖完整 sensor→JPEG 链路。

后续作者/测试者评论报告：Q 菜单/RAW conversion、图标显示、FX Blue 持久化等问题；有人指出最初 CN 只有色彩部分，缺少 tone curve，落回 PROVIA 曲线；不同时间的链接版本及是否修复存在混淆。原作者另自述刷实验新增功能版本时自己的相机变砖，停止一段时间。

这些是**作者与使用者的第一人称报告**，本轮没有独立复现、验证下载文件或验证色彩准确性。它们提示跨相近处理器移植并非空想，但**不证明 X-T4 可以刷入这些 X-T3 文件，更不证明任意新代模拟可直接移植**。本轮没有下载散发的修改固件。

### FujiMod / justice-jake

- [I'm building custom firmware. What's on your wishlist?](https://www.reddit.com/r/fujifilm/comments/1x611yb/im_building_custom_firmware_whats_on_your_wishlist/)（2026-10-14，作者 u/justice-jake）
- [作者公开代码](https://github.com/justjake/fujimod)

作者主要目标是 EXR Processor II/X70，声称已有 MF Snap Focus、高光加权测光、额外模拟、SD 卡文本 recipes、film simulation editor、HDMI，以及若干仿真截图；不是 X-T4 成果发布。评论提及 SD boot recovery，但同时承认尚无通用生成恢复文件的办法。不能把其他 EXR 机型的恢复说明当成 X-T4 的恢复流程。

作者 GitHub 个人资料链接 X 用户 `@jitl`，可供 X 研究交叉检索；本子任务没有检索 X，该部分由主任务 Grok 独立进行。

## 7. 定制功能分层评估

| 功能方向 | 公开证据与现实路线 | 对 X-T4 的判断 |
|---|---|---|
| 手机/电脑遥控、自动拍摄工作流、文件传输 | 官方 SDK 明列 X-T4；libfuji 有 USB/Wi-Fi/BLE 模式实现 | 优先做外部应用，具体属性/模式需逐项验证 |
| AE/ISO/WB/DR/胶片模拟包围 | 原生已支持 | 基础场景无需修改固件 |
| 延时/对焦堆栈 | 原生 interval timer、曝光平滑、AUTO/MANUAL FOCUS BKT | 优先用现成功能；多步骤组合可研究外部控制 |
| 更多 recipe 管理/导入导出 | XApp 有备份/恢复；PTP/外部应用可研究；fffw 有未完整发布的 backup parser 说明 | 外部管理有现实入口；机内无限 slot 尚需 UI/持久化/索引修改 |
| 按键逻辑、快捷键、菜单行为 | FujiHack 有旧机型先例；X-T4 有枚举资料 | 适合首次代码定位，但尚缺 X-T4 实机补丁证明 |
| 辅助线、XPan/4:3 框线、LCD/EVF 显示 | 旧机型图形 API 与新 X70 作者成果可参考 | 可研究；显示 overlay 和更改最终 JPEG 输出应分开验证 |
| 新模拟、自定义色调曲线 | 最近 X-T3 作者/测试者报告，X-T4 2.12 有枚举资料 | 值得研究；需色彩参数、曲线、菜单、recipe、RAW conversion、视频路径全覆盖 |
| 录像时长/码率 | 有旧机型限制修改经验，X-T4 原生最高400Mbps | 不能只把数值改大；需编码器、存储、温度、分段与模式约束验证 |
| RAW video、新 codec、更高分辨率/帧率 | 本轮未找到 X-T4 可用实现 | 受 sensor readout、ISP、专用硬件和带宽约束，不能当隐藏开关 |
| 新代 AI AF/重大 IBIS 增强 | 跨处理器/专用模块/校准边界 | 极高不确定性，不适合作为近期承诺 |
| 陀螺仪日志/Gyroflow | FujiHack 有功能请求，缺 X-T4 已实现证据 | 先证明数据源、采样率、时间同步和记录开销 |
| clean HDMI / 10bit 输出 | 原生已支持 HDMI 4:2:2 10bit | 无需固件开发；可研究外部监视/记录工作流 |

## 8. 官方能力与外部控制来源

- [X-T4 官方固件页](https://www.fujifilm-x.com/global/support/download/firmware/cameras/x-t4/)：检索日仍列 2.12、2024-07-09，文件名 FWUP0024.DAT。2.00 加入 XApp 及备份/恢复等相关功能；文件名在不同版本之间不唯一，需 hash 鉴别。
- [官方 Camera Control SDK](https://www.fujifilm-x.com/en-us/camera-control-sdk/)：明列 X-T4，USB/TCP/IP，macOS、Linux、Android（USB）等；自动图像传输与基本遥控。页面说明不提供 RAF 数据转换信息，并明确个人用该 SDK 连接/控制相机会使有限产品保修失效。这是实际页面条款，不应把 SDK 描述为完全无风险。
- [libfuji README](https://github.com/petabyt/libfuji/blob/master/README.md)：实现 XApp/Camera Connect、BLE、PC Autosave、USB/无线 tether、Raw Studio 转换模式。库的一般支持清单不是每项功能在 X-T4 的独立测试证明。
- [X-T4 SHOOTING SETTING 手册](https://app.fujifilm-dsc.com/en/manual/x-t4/menu_shooting/shooting_setting/index.html)：AUTO/MANUAL FOCUS BKT、interval timer、曝光平滑。
- [X-T4 Bracketing 手册](https://fujifilm-dsc.com/en/manual/x-t4/taking_photo/bracketing/index.html)：AE、ISO、Film Simulation、WB、DR、Focus BKT。
- [X-T4 Movie Setting 手册](https://fujifilm-dsc.com/en/manual/x-t4/menu_shooting/movie_setting/)：clean HDMI 4:2:2 10bit、输出选择与 REC control。
- [X-T4 Specifications 手册](https://fujifilm-dsc.com/en-int/manual/x-t4/technical_notes/spec/index.html)：internal H.265 4:2:0 10bit、HDMI 4:2:2 10bit、最高400Mbps等已有规格。

## 检索边界与保留线索

本轮读取上述项目 README、关键源文件、型号文件、完整公开 tree、FujiHack/fffw 的公开 issues，以及官方固件/SDK/手册。未在这些边界内发现 X-T4 完整补丁安装/运行/恢复的公开复现记录；这不是对所有 Discord、未公开仓库或未来开发的否定。

`fuji-x4 user overlays/fftools DAT2BIN` 这些精确名称经公开关键词与 GitHub code search 尚未定位到可核实的相机项目；如有完整 URL，应再单独核实，不能仅凭名称当作存在且适配的工具。

最值得继续验证的离线路线是：用 FujiMod 的 scan/decompress 和独立 ffcompress 交叉检查本机 X-T4 2.12 四段解码及 roundtrip；再依据实际段表区分主 ARM 映射、overlay 与其他处理器数据。不要把 bit-exact 原始 roundtrip、能反汇编或能仿真局部函数等同于修改后可安全刷写。

未执行硬件操作或验证社区修改固件；最大剩余风险是 X-T4 缺乏已复现的补丁运行和可靠恢复证据。
