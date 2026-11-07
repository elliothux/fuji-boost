# X-T4 真机一次性采集清单

目标：一次坐下（约 45–60 分钟）收集硬件模拟需要的真机证据。所有输出写入 `.temp/real-camera/<时间戳>/`（含序列号与内存转储，只留本机，不进 Git），每步立即落盘，中途中断也不丢已采数据。

## 事先准备

- 电池充满；装好镜头（如有光圈环/OIS 开关更好）；机身盖备用。
- 两张 SD 卡：**卡 A 会被相机格式化，里面的数据会被清空**；卡 B 任意。
- USB-C 数据线连到这台 Mac。可选：HDMI 线、3.5mm 麦克风/耳机、闪光灯。
- 记下相机当前显示的电量百分比。

## 第 1 段：普通模式（相机正常开机）

1. 相机 `连接设置 → PC 连接模式` 选 `USB RAW 转换/备份恢复`，连上 USB，开机。
2. 记录 USB 描述符与 PTP 身份（只读）：

   ```bash
   .cache/capture-target/release/fuji-capture list
   ```

   把输出里的序列号填入下一条：

   ```bash
   .cache/capture-target/release/fuji-capture ptp-info --serial <序列号> --out .temp/real-camera/ptp-info.json
   ```

3. 在相机里用卡槽 1 **格式化卡 A**，不拍照。关机，取出卡 A 插入 Mac 读卡器，`diskutil list` 找到它（如 `disk4`），然后：

   ```bash
   research/real-camera/sd_image.sh disk4 .temp/real-camera/sd-card-a
   ```

   完成后 `diskutil eject disk4`，卡 A 插回卡槽 1，卡 B 插入卡槽 2。

## 第 2 段：jig 模式采集

1. 相机关机，保持 USB 连接。**按住快门 + 方向键上**，同时打开电源。
2. 运行：

   ```bash
   .cache/ff80-venv/bin/python research/real-camera/xt4_ff80_capture.py
   ```

   - **交互部分（约 30 分钟，需要你在场）**：终端逐步提示怎么按；回车开始每一步，`s` 跳过一步，`g` 跳过这一组，`q` 结束交互部分。
   - **无人值守部分（最长约 90 分钟，可以离开）**：脚本按风险从低到高自动读取，见下表。全部完成约占 3 GB 磁盘。
3. 回来后看终端最后一行。显示“完成”就关机；之后正常开机，确认画面和设置正常。

## 如果中途卡住

终端长时间不动或报 USB timeout：取出电池再装回，重新按住快门+上键开机进入 jig，然后从卡住阶段的**下一阶段**继续（终端会显示阶段名）：

```bash
.cache/ff80-venv/bin/python research/real-camera/xt4_ff80_capture.py --skip-a --start-phase <下一阶段名>
```

脚本会在结束时把 0xF7 恢复为第一次采集前记录的原值（保存在 `research/real-camera/f7-original.json`）。

## 变砖风险

所有步骤都不写 Flash、固件、校准数据或 EEPROM，也不调用把配置保存到 NVRAM 的命令。最坏情况是相机卡住，取出电池即可恢复。

| 顺序 | 内容 | 写入 | 可能的后果 |
| --- | --- | --- | --- |
| 交互 | 设备信息、配置数据整份备份、RTC、16 个 GPIO 端口与 12 路 ADC、约 70 步按键/拨盘/接口/卡槽/镜头映射、diag 数据集 0–15 | 仅 fffw 的 app80 邮箱（配置 RAM 0x80–0xA0） | 无已知风险 |
| 1 image | 主映像 RAM，并与 DAT 解压结果比对 | 开始前把配置 RAM 字节 0xF7（USB debug）置 1，结束恢复 | 固件代码所在内存，最安全 |
| 2 syslog | 调试日志 | 同上 | 低 |
| 3 dram-low | 512 MiB 以内其余 DRAM | 同上 | 低 |
| 4 dram-fw-high | 约 994 MiB 处固件写过的区域，并判断 512 MiB 镜像 | 同上 | 低 |
| 5 mmio | SD/门控/I²C/GIC/GPIO/FF70F000 等 349 个状态寄存器与成像 SRAM | 同上 | 可能卡住，需拔电池 |
| 6 dram-zero | 最低 16 MiB（可能未映射） | 同上 | 可能卡住 |
| 7 dram-512m-1g | 512 MiB–1 GiB | 同上 | 若无此内存可能卡住 |
| 8 dram-1g-2g | 1–2 GiB | 同上 | 若无此内存可能卡住 |

若时间预算不够，大区段会改为每 16 MiB 抽样 1 MiB。只想做交互部分时加 `--interactive-only`。

## 补充：标定寄存器 FF50–FF53（菜单进“设置”卡住的问题）

模拟器在菜单从 TC 标签切到扳手（设置）时卡在标定模式 5：固件 `0x11c33a8` 写四个单元（`FF5x5000…`），再轮询 `FF5x5384` 直到整字为 0。这一段只读真机这些寄存器，约 10 分钟。

1. 按住快门 + 上键开机进入 jig 模式，接好 USB。
2. 运行：

   ```bash
   .cache/ff80-venv/bin/python research/real-camera/xt4_calib_capture.py
   ```

3. 脚本先试读一个寄存器 `FF505384`。若终端 30 秒没反应或相机黑屏：取出电池再装回即可（`0xF7` 断电清零，不写 NVRAM），把情况告诉 Claude。
4. 然后读四个单元共 284 个寄存器的静态快照。
5. 脚本会问 jig 模式下按 MENU 菜单是否出现：
   - 出现（y）：按提示在相机上把菜单标签栏移到 TC，再按下键进设置，来回 3 次，回到电脑按回车。脚本以约每毫秒一次的频率记录 `FF5x5384`/`FF5x5000` 的每次变化，结束后再读一次快照。
   - 不出现（n）：只保留静态快照。
6. 结束时脚本恢复 `0xF7`。数据在 `.temp/real-camera/calib-<时间戳>/`。之后正常开机确认相机一切正常。

全程只读，不写闪存、NVRAM 或任何寄存器。
