# 固件格式、资源与版本 · 日期未注明

[研究索引](../README.md) · 领域：`firmware`

原文中的“当前”“下一步”均指记录当时；最新结论见同领域最新日期文件。

本组原文没有明确日期，保留为日期未注明资料。

## 身份与格式

- 大小：51,278,280 字节。
- 原件 SHA-256：`853d273b5603d3a2b93493f0ebd6a762b0b4b6bdc70317777775ae5f8ed93bc5`。
- 与 [FujiMod 的 X-T4 2.12 catalog](https://github.com/justjake/fujimod/blob/0bc556ec632663b54d15a2af0cf01e471db356ec/catalog/firmware/xt4/2.12/firmware.json) 完全一致。这里确认的是版本身份，不宣称文件是本轮从富士官方下载，也不把它称为最新固件。
- outer type = 6；版本原始字段 `0x2` / `0x12`，按富士十六进制显示规则对应 2.12。解码器 outer payload offset 为 `0x27c`，其中包含其他内部结构，不能当作全部代码的起点。
- 已通过逐字节 XOR `0xff` 得到中间文件，并从中识别 4 个 method-4 压缩流。XOR 是可逆混淆；本结果不证明所有组件没有其他保护。
- 反混淆后 `0x7b250` 有完整 ThreadX SMP/Cortex-A7/GNU 版本标识；与作者记录的 X-Processor 4、Cortex-A7、ThreadX SMP 一致。四核、FF4230 来自作者逆向表，本轮没有读硬件寄存器证明。

## 压缩段结果

| 段 | DAT 偏移 | 压缩字节 | 解压字节 | 可识别 inner header |
|---|---:|---:|---:|---|
| 00 | `0x00260274` | 3,449,330 | 7,420,408 | 32-bit；base `0x01021000`；entry `0x012910c4` |
| 01 | `0x005c0274` | 5,254,523 | 9,437,184 | 32-bit；base `0x01e0f000`；entry `0x01e1de60` |
| 02 | `0x00ae0274` | 4,632,713 | 10,240,000 | 未识别；不能自行假设加载地址或用途 |
| 03 | `0x01a00274` | 3,309,617 | 6,446,348 | 未识别；不能自行假设加载地址或用途 |

第 00 段 inner header 的 initialized size 为 `0x714000`，大于实际解压字节 1,544；本轮保留实际结果，没有擅自补零或合成为完整内存镜像。第 02 段出现菜单/显示相关文本，但字符串不足以证明其整体角色。第 03 段没有本轮所选 ASCII 模式，不代表没有数据或必然加密。

完整段哈希、literal/match 统计见 firmware-probe.json，实际 packed/bin 文件在 `research/extracted/`。

## 交叉验证与无改动重建

1. 审阅并使用 `fujimod-firmware` 的 Rust 解码器，外置最小 driver 不修改上游仓库。
2. 构建 `fffw/ffcompress` 的 Go 实现，用同样四个压缩段解码；四段输出与 Rust 结果逐字节一致。
3. 用 Go `t` 重压四段，四次 `TESTOK`，并额外做完整字节比较，均相同。
4. 将重压结果放回反混淆文件的原偏移、其他字节原样保留，再取反，输出 `research/no-op-reconstructed.DAT`。与原件逐字节相同、SHA-256 相同。它只是无改动重建证据，不是修改固件，也不是通用 DAT 构建器的验证。

证据：roundtrip-validation.json、container-roundtrip.json。

已运行 Go 三个 package 的测试，均通过。上游 Rust 整个 workspace 的测试入口被 manifest 继承错误阻断（`default-features` 覆盖冲突）；没有改动上游修它。改用独立 test manifest 直接指向同一份未修改 decoder 源码，**原有 7 项测试全部通过**，含畸形 token、头部、边界和 overlay 检查。分别记录在 `research/ffcompress-tests.log`、`research/fujimod-firmware-tests.log`、`research/fujimod-metadata.stderr`、`research/decoder-source-tests.log`。

## REA 的实际使用与边界

Docker 镜像 `fuji-research-rea:6.7.0-binwalk3.1` 使用 REA 6.7.0、Binwalk 3.1.0；Dockerfile、构建日志和镜像 ID 都在 `research/`。构建第一次缺 fontconfig，补齐镜像内依赖后成功。运行时网络关闭、根文件系统只读、输入单文件只读挂载、临时文件使用 tmpfs；没有透传相机/USB。镜像仍保留于本地 Docker，便于复用。

| REA 目标 | Evidence ID | 返回与解释 |
|---|---|---|
| 原始 DAT | `ev_74208344118b4cba0f1bbaf5da9023cbb53709816654730960615cb47ace94af` | 9 个通用 signature candidates：5 zlib、4 gpg_signed；不足以认定真实分区或签名 |
| XOR 中间文件 | `ev_c3ff4beea6f9d8afd2e8be47835577826880d64d4d60ea4c1772ca5198d0f08c` | 283 个 candidates；其中 `0x7b250` 的 ThreadX 标识有原文支持；其余仍需格式和上下文验证 |

原文：rea-linux-original.json、rea-linux-inverted.json。Binwalk 不理解这份富士 method-4 容器；实际四段解压来自上面两套格式实现，不能归功于 REA 自动解包。大量 `gpg_signed` 匹配出现在压缩数据中，本轮没有验证 GPG 包结构、签名、信任链，绝不能据此断言富士更新包使用 GPG 签名。AES/SHA 表的匹配同样不能建立启动验证链。

未运行 Unblob：当前没有已证明适用于此富士私有格式的额外收益；未把扫描候选机械提取成“分区”。未用 Hopper/Ghidra 做原生函数调用图或伪代码恢复。两段已知地址与第三、四段未知用途、overlay 和协处理器组件，应先校准完整内存布局再做这一步。

## 已找到的代码定位线索

这些是静态字符串，说明有可检索入口，不是已验证的控制流程：

| 文件 | 文件内偏移 | 文本线索 |
|---|---|---|
| stream-00 | `0x3e7767` | `ui_film_sim_val`：胶片模拟设置 |
| stream-00 | `0x3e7ba3` | `ui_movie_bitrate_val`：视频码率设置 |
| stream-00 | `0x453244` | `UI_MENUCOM_ITEM_ISO`：菜单项 |
| stream-00 | `0x403964` | `LIB_PTP_E_SUCCESS`：PTP 子系统 |
| stream-01 | `0x6c7f44` | `JIG_BACKUP`：服务/备份相关 |

见 focused-strings.json；string-evidence.json 是每文件前 120 个匹配的有限样本，不是完整字符串索引。偏移属于各解压文件，不能直接当成 DAT 偏移或运行地址。

