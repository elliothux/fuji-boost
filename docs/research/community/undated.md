# 社区、X与参考项目 · 日期未注明

[研究索引](../README.md) · 领域：`community`

原文中的“当前”“下一步”均指记录当时；最新结论见同领域最新日期文件。

本组原文没有明确日期，保留为日期未注明资料。

## 补充 Tweet 调查结果（本机 Grok 已完成）

用户明确本次只需调用 Grok 做 Tweet research。已停止 Docker 路线，撤回本轮对 grok-executor skill 的五个文件改动。直接调用本机 Grok 1.0.67，工具只开放 `web_search,web_fetch`，禁止 shell、写文件与 MCP；没有让 Grok修代码或连接相机。该进程已正常返回 `end_turn`。以下历史章节中的“没有 Grok 结果”仅描述首轮失败，现在由本节补全。

实际 session 为 `01a16794-2c35-72b3-9e09-58790ea28eea`。我检查了该 session 的搜索来源元数据：有 **26 个 web_search 查询记录、200 个去重来源 URL**（包含非 X 页面，不是 200 条相关 Tweet）。报告列出的 14 个独立 status URL 都在返回的来源池中，且各帖 ID 的 Snowflake 时间与报告日期一致。检验见 grok-tweet-validation.json。

**证据边界：** Grok 自述其搜索返回 X 正文，但可检查的后端工具类型仅为 `web_search`，没有单独的 `x_search` 调用记录。`web_fetch` 打开 X 时因解析到 `198.18.*` 而被 SSRF 拦截，因此本轮没有直接打开 X 页面二次核对全文；没有为此修改 DNS、Surge 或其他网络设置。以下内容按“Grok 的 X 搜索返回/作者声明”使用，不把它表述为我的逐帖页面验证或实机复现。

## 新增的相关作者贴文

下表日期使用 Asia/Shanghai。Grok 原稿保留来源 UTC 日期。

| 作者 / 日期 | 原帖 | 搜索返回内容与对 X-T4 的意义 |
|---|---|---|
| @jitl / 2026-10-23 | [工具与刷写警告](https://x.com/jitl/status/2113300975054356495) | 指向 justjake/fujimod；称 mod 在证明不会刷砖前仍保留，强调先有双启动。未给 X-T4 可用补丁或恢复方法。 |
| @jitl / 2026-10-23 | [社区救回刷砖相机](https://x.com/jitl/status/2113319889209540935) | 声称又救回一台富士；没有明确型号/恢复步骤，不能用于证明 X-T4 可恢复。 |
| @jitl / 2026-10-13 | [X70 胶片模拟编辑](https://x.com/jitl/status/2110018071016394821) | 明确是 X70 的成像引擎/曲线/颜色参数研究；不是 X-T4。 |
| @jitl / 2026-10-13 | [SD / USB RAM 注入](https://x.com/jitl/status/2109676468766675008) | 称当前改造从 SD 或 USB debug 内存注入运行，避免直接刷写；X70 线程。支持优先研究临时执行的方向，不证明 X-T4 入口已打通。 |
| @jitl / 2026-10-04 | [EEPROM 设置与运行时 RAM](https://x.com/jitl/status/2106605775082623439) | 作者称改变 EEPROM 设置后可在相机运行时通过 USB 读写 RAM、测试补丁。没有 X-T4 型号、偏移或可复现步骤；不要推断 EEPROM 地址。 |
| @jitl / 2026-10-12 | [SD 卡 TOML 配方](https://x.com/jitl/status/2109653817621188716) | X70 配方文件研究方向，证明这类功能有跨机型先例，仍需 X-T4 独立实现。 |
| @jitl / 2026-10-05 | [snap focus](https://x.com/jitl/status/2107102661241602086) | X70 功能演示线索，不是 X-T4 支持声明。 |
| @danielcdev / 2023-01-16 | [FujiHack quick PTP patch](https://x.com/danielcdev/status/1614741135225667584) | 作者说 PTP 补丁让代码测试更容易；贴文本身没有 X-T4。应与官网 X-A2/XF1 支持范围对照。 |

补充相关链接：[作者说较少投入新一代机型](https://x.com/jitl/status/2109676144874135820)、[X100VI 虚拟机研究](https://x.com/jitl/status/2109766271374073984)、[LaurieWired 的 FujiHack/DOOM 概括](https://x.com/lauriewired/status/1916703295026196961)。虚拟机演示不等于相机执行；概括贴也不能证明 X-T4 存在可用 JavaScript 入口。

**对原结论的影响：** 新找到的作者贴文增强了配方编辑、成像参数、USB RAM 调试等方向的研究价值，但本次仍未取得 X-T4 特定的修改包接受、机上执行或可靠恢复证据。没有检索到的目标包括 X-T4/FWUP0024 破解实证、tiredboffin/fffw 的相关作者 X 贴和 xrock3rr 的相关 X 贴；这是有限搜索结果，不能说这些讨论不存在。

原始交付：Grok 报告原稿、结果 JSON、会话导出、搜索来源元数据、诊断日志。原稿的“原生 X”及若干绝对否定表述须以上述证据边界为准；所有细节均未经本轮实机复现。

## yi-mirrorless-firmware-tools

[项目 README](https://github.com/protyposis/yi-mirrorless-firmware-tools/blob/master/README.md) 的主要对象是 YI M1，相关富士案例是 X-A10。其 `LENGTH=` 文本头、ND1/IPL/PTBL、LZSS 等与本文件 type-6/method-4 不同；2024-05-12 已归档。作者明确说重封装刷写未测试、压缩还不完全正确。[issue #3](https://github.com/protyposis/yi-mirrorless-firmware-tools/issues/3) 还有未改内容重压不一致的问题。它可作研究方法参考，不能作为 X-T4 的现成刷机工具。

## FujiHack

[官网](https://fujihack.org/) 的 hacked models 是 X-A2 1.30 和 XF1 1.01，且仍写没有面向公众直接使用的成熟 hacks。[supported models](https://wiki.fujihack.org/supported/) 只是相关 RTOS 型号范围；[xt4_150.h](https://github.com/fujihack/fujihack/blob/master/model/xt4_150.h) 只有型号名。旧机型的按键重映射、延长录像、读陀螺仪、画 framebuffer、任务创建、DOOM 等提供技术先例，但不能直接套用到 X-T4。

## fffw / FujiMod

- [fffw](https://github.com/tiredboffin/fffw) 更贴近 X-T4；`ffun/ports/xt4/0212/` 有 task、UI、libptp、film simulation、v61 syscall 等大量命名资料。顶层列出的部分工具尚未发布，`ffundat.py` 是占位，不能把说明书当成已可执行的完整工具。
- `ffcompress` 是本轮实际构建、测试和使用的实现；原作者的大样本测试说明是背景，本轮结论以自己的四段实测为准。
- [FujiMod decoder](https://github.com/justjake/fujimod/tree/main/crates/firmware) 是本轮另一条实际解码路径。其 X-T4 catalog 中 `boot_rom`、`installation`、`generated_facts` 均为 null。虽然有 `xt4-212` discovery 配置，[emulator README](https://github.com/justjake/fujimod/blob/main/crates/emulator/README.md) 的明确 catalog boot 支持是 X70/x70-110；不能称为已完整仿真 X-T4。
- 本轮固定源码版本：fffw `ccee949884a6dad10095231441e4d608c10e59f0`；FujiMod `0bc556ec632663b54d15a2af0cf01e471db356ec`。详见 tool-provenance.json。源码 checkout 都保留在当前目录，未改其受版本控制文件。

## 社区与 X 搜索

- 用户授权 Grok 做只读 X 调查；任务范围、验收和禁止外部写入都写进 `research/grok-x-brief.md`。Grok Build 1.0.67 在创建 ACP 会话前退出，原因是 wrapper 的临时 `bundled` 符号链接与沙箱要求真实目录冲突；没有模型研究输出、没有文件修改、没有原生 X 检索结果。该 wrapper 还显式禁用了 web search/MCP，因此不擅自移除保护来完成搜索。
- 后备做了公开索引查询：`site:x.com fujihack firmware X-T4`、`site:x.com petabyt Fujifilm firmware hack`、`"fujihack" "twitter"`、`"tiredboffin" "twitter"`、`site:x.com/jitl "fujifilm"` 等；`https://x.com/jitl` 返回 403。结果噪声多，没有获得足以引用为技术证据的相关 Tweet。这个结果不证明 X 上没有相关讨论。
- [X-T3 移植 Classic Negative / Eterna Bleach Bypass 原帖](https://www.reddit.com/r/FujifilmX/comments/1wda43y/xt3_running_classic_negative_eterna_bleach_bypass/) 有作者/使用者自述，基于 X-T3 5.11 和 X-T4 2.12 的组件；后续反馈包括 bugs/曲线缺口以及实验刷砖。它是重要线索，**不是本轮独立复现，更不是 X-T4 修改包成功证据**。
- [2026 年 10 月的 custom firmware 讨论](https://www.reddit.com/r/fujifilm/comments/1x611yb/im_building_custom_firmware_whats_on_your_wishlist/) 指向 FujiMod；主要目标与演示是 EXR II/X70 范围。其 recipe、snap focus、测光与显示成果值得看，但 recovery 或 patch 不能直接迁移为 X-T4 已有能力。

