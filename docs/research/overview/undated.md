# 总览、边界与复现 · 日期未注明

[研究索引](../README.md) · 领域：`overview`

原文中的“当前”“下一步”均指记录当时；最新结论见同领域最新日期文件。

本组原文没有明确日期，保留为日期未注明资料。

## 研究范围与边界

- 用户尚无指定功能；覆盖离线解包/重封装、机身代码执行、菜单/按键、成像/视频、自动化、PTP/USB 外部控制与恢复路径。
- 原始目标：`FWUP0024.DAT`，51,278,280 字节；SHA-256 `853d273b5603d3a2b93493f0ebd6a762b0b4b6bdc70317777775ae5f8ed93bc5`。
- 原始固件只读；不刷写相机、不测试硬件接口、不执行提取出的代码。不修改用户级工具配置。
- 当前目录不是 Git 仓库。全部中间文件保存在本目录的 `research/` 内；本文汇总进度和结论。

## 问题清单

| 问题 | 状态 | 所需证据 |
|---|---|---|
| 文件身份、版本、容器与分区 | 部分回答 | 大小/哈希精确匹配 X-T4 2.12；type-6；4 段已解码，完整分区/overlay 尚未恢复 |
| 解包/解压是否可行 | 对 4 个 method-4 段已实测 | 两实现交叉解码一致；不是所有固件组件的完整提取 |
| 修改后能否无损重封装 | 部分回答 | 未改动内容可 bit-exact 往返；修改后大小、布局与完整性更新未验证 |
| 更新签名/启动验证及相机接受修改包 | 未解决 | X-T4 特定验证逻辑或机身演示 |
| 不同功能的现实实现路线 | 本轮已评估 | 下方功能矩阵；逐项实施能力仍需后续验证 |
| 恢复、回滚、调试入口 | 未解决 | X-T4 特定证据 |
| X 上作者/社区讨论 | 补充轮 Grok 查询完成 | 26 个 web_search 记录；14 个 status URL 可追到返回来源；页面直读受阻，作者声明未实机复现；见顶端补充结果 |

## 工具与当前进度

- 已启用用户指定的 `grok-executor`、`reverse-engineer-anything` 工作流；研究和工具脚本采用 Ponytail 的最小完整改动原则。
- REA 6.7.0 doctor：Codex 注册 aligned，skill 42 aligned；当前会话无 REA MCP 工具，因此通过 CLI 调用。已有 Hopper；未配置 Ghidra。诊断原文：`research/rea-doctor.json`。
- REA 固件区域检查依赖 caller-supplied Binwalk、Linux 和 prlimit；macOS 请求结果见 `research/rea-firmware-regions.json`。用户随后允许 OrbStack/Docker，已在 Linux arm64 容器成功运行。不会将普通 shell、Rust 或 Go 输出称为 REA Evidence。
- Grok Build 1.0.67 两次未能启动 ACP。诊断指向 wrapper 在临时 home 内创建的 `bundled` 符号链接，与 Grok 当前只读沙箱要求真实目录冲突。任务 brief：`research/grok-x-brief.md`。没有 Grok 研究结果，尚未获得原生 X 搜索结果；wrapper 本身还禁用了 web search 和 MCP，不能把失败调用描述为完成 X 搜索。
- 公开资料子任务已完成，完整来源笔记见 [research/public-sources.md](../../../research/public-sources.md)。已核实 FujiHack `model/xt4_150.h` 只有型号名，没有移植所需地址与 stubs。

## 下一阶段的合理顺序

1. **保留当前 2.12 的证据基线。** 先恢复段表、overlay、原始非压缩区、IBIS/其他处理器组件的边界，再导入反汇编器。只把已证实的地址映射写入分析数据库。
2. **优先研究完整性与恢复。** 精确定位 2.12 更新器的检查范围、校验算法、型号/版本约束、各组件更新步骤与启动验证。type-6 parser 没返回 type-8 DER 签名，并不能证明不存在其他验签。本轮也没有验证旧 patcher 的 checksum 增量算法适用于这里。
3. **选一个可观察的小目标。** 最合适的是调试输出/一个显示元素/一个按键分支；先证明目标函数和状态机，再评估是否能 RAM 中临时测试，避免先做永久更新。服务接口中的 RAM 与 EEPROM 操作必须区分；校准数据不是普通用户设置。
4. **并行做无需改固件的实用原型。** 遥控、拍摄序列、recipe/备份查看器可以先证明价值，也能帮助决定是否值得承担机内补丁的工作量。
5. **有 X-T4 对应的恢复证据后才设计实机补丁实验。** SD 卡正常更新界面可用、版本号可改、机型头能识别，都不是刷坏后仍可恢复的证明。本轮不生成“可刷写自定义固件”。

## 文件索引与复现

- `research/public-sources.md`：公开来源、作者声明、功能矩阵。
- `research/fujimod/`、`research/fffw/`：固定提交的上游源码 checkout。
- `research/probe/`：实际读取与解码目标的最小 Rust driver；只接受本轮已知大小/hash 的输入，拒绝覆盖已有输出目录。
- `research/decoder-tests/`：直接运行原始 decoder 源码测试的独立 manifest。
- `research/extracted/`：XOR 中间文件、四段 packed/bin、Go 交叉解码、repacked；`summary.json` 是完整元数据。
- `research/no-op-reconstructed.DAT`：与原始固件完全相同的重建证据，未添加功能。
- `research/Dockerfile.rea`、`docker-build.log`、`rea-image-id.txt`：Linux REA 环境记录。Docker 镜像本身由本机 Docker 管理。
- `research/rea-linux-*.json`：REA 原始 Evidence；`rea-firmware-regions.json` 是此前 macOS 请求的明确失败记录。
- `research/*tests.log`、`roundtrip-validation.json`、`container-roundtrip.json`、`tool-provenance.json`：验证与版本信息。

复现解码（输出目录必须不存在）：

```sh
cargo run --release --manifest-path research/probe/Cargo.toml -- FWUP0024.DAT research/extracted-new
cargo test --manifest-path research/decoder-tests/Cargo.toml --lib
```

复现单段交叉解码/重压测试（在 `research/fffw/ffcompress` 中先 `go build -o ../../ffcompress .`）：

```sh
research/ffcompress u research/extracted/stream-00-00260274.packed research/stream-00-check.bin
research/ffcompress t research/extracted/stream-00-00260274.packed research/stream-00-check.repacked
cmp research/stream-00-check.bin research/extracted/stream-00-00260274.bin
cmp research/stream-00-check.repacked research/extracted/stream-00-00260274.packed
```

所有本轮研究记录、源代码快照、提取与结果文件均在当前目录内；系统管理的包缓存与 Docker 镜像是运行工具的缓存/环境。原始 DAT 始终保持不变。

## 初步来源

- https://github.com/protyposis/yi-mirrorless-firmware-tools
- https://github.com/fujihack
- https://github.com/morluto/rea/blob/main/docs/firmware-analysis.md

## 证据分级

将实际本地结果、作者演示、源码推断和未解决问题分开记录。其他型号、相同 RTOS/处理器代际或静态字符串均不能证明 X-T4 运行行为。高熵不单独证明加密，校验和不等于数字签名；可重封装不等于相机可接受。

本轮最终核验：当前源码的check（含XP4 Clippy）、生产构建、原生/Wasm默认严格执行一致性均通过；4项实际DAT场景回归与14项外设测试通过。原生高级诊断尚未作为浏览器模式导出/验收，完整开机与UI目标保持未完成。


