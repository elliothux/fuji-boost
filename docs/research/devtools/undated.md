# DevTools、Wasm与Web交互 · 日期未注明

[研究索引](../README.md) · 领域：`devtools`

原文中的“当前”“下一步”均指记录当时；最新结论见同领域最新日期文件。

本组原文没有明确日期，保留为日期未注明资料。

## 当前进展：原厂首次设置界面与虚拟导航

独立raw768冷启动已完成 **1024020606instructions/3297445439ticks**，无failure/error/guestHalt；snapshot SHA256 **`bc822ae7a3c6b4360be83f8981c5be393b9d463cabd6b997c4e4b279e42965d0`**。0001 total1080，首256receipt与先前停止点完全相同；新第257次完整ACK在旧failure tick1234674842真实完成。导出output1/seq1/900×600 RGBA，SHA256 **`98e541541ed5a4b5806a6c6973fe1d8f4cdc5c6786e82154adcac86d4619f355`**；宽高+RGBA hash等于LCDC，PNG全部CRC/解码逐byte一致。实际画面是 **START／言語/LANG.2/5，日本語被选中，中文简可见**。这是语言设置向导，不是正常拍摄或完整硬件仿真。图像、独立运行/像素审查。

必须保留容量界限：main trace4096条/drop432，末保留tick3022831003早于终态3297445439，不能以晚期缺行作穷尽否定。device ledger65536满，仅912个完整0001 frame可逐项wire核验，其余168仅有源码合格的aggregate count；未冒充1080帧均完整留存。首次hostcap试跑的1277trace/32770device记录均为新run精确前缀。

新工作台场景 **“首次设置实验（合成输入）”** 已接入独立raw768模式，并通过原SDK只释放六个已核逻辑键。该GUI场景在首批指令前将bank7导航bits8..12置高为1f00、9.5保持高，区别于只改read3的旧research run；原strict/preloaded/calibration三个场景保留。新六键守卫464B，SHA256 `b1d696cf60d92e8f89c88c7e3e3346e0f51051d854c18f202cdcf1b64db5830e`，绑定原scanner/map/accessor。Root仅转发现有`set_gpio_input`，不注入guest按键事件或改UI状态；新增pressed电平元数据与假设明确记录host干预。源映射为UP7.11/DOWN7.10/LEFT7.9/RIGHT7.8/OK7.12/BACK9.5，均low active。它们是固件逻辑输入，catalog实物连接仍unclaimed。六键原字节。

当前实现已改为**等待原固件采样反馈**：release→press→finally release，各阶段最多128×250k指令，要求RAM采样中的stable/current均达到目标，最后另运行1M；不可用、异常或超预算明确停止，不能填零推断。660B组合guard SHA256 `1afccdd94e2379633b100d71b438a86153a892a866ef5079b6678b9df0b3ef9a` 同时绑定六键映射与消抖语义；替代前面仅464B映射资格的运行接线。上述浏览器／首轮重放的Root source SHA256 `75b63b1462f84865c00746a4c43c7de5edb4fdb4ea9066d009b202f6d3dd5c8d`、Wasm `e7262ed872bfda4b53c63aa17be5da47fc346d49779aa589463ddcd8834c5298`。build/check、11项Bun协议/pulse测试、输入位隔离及34项独立静态检查通过。按键路径使用原task61定时扫描与threshold2消抖，不要求虚构GPIO边沿IRQ；stable/current只是采样反馈，不冒充UI已经处理完毕。当前实现审查、定时扫描源资格。

**实际浏览器导航已出现正结果。** 新setup冷会话先释放六键，在807020606instructions/2001209633ticks暂停于frame1日语语言页。三次UP并分别继续执行原固件，实际依次显示韩语、繁体中文、简体中文；第三次渲染frame4。按OK后继续运行，至1104770606instructions/3521447019ticks输出frame5，实际为**简体中文区域设置页**（伦敦/卡萨布兰卡、区域设定GMT±0:00、夏令时关、设定完成）。点击pulse结束后的1M往往不足以完成重绘，需要继续执行；不能将即时旧画面误判为按键失败。截图是浏览器实测证据，尚未声称与Node/native同预算像素一致；独立Node重放正在进行。正常拍摄UI和完整初始化仍未达到。语言选择、区域设置。

继续真实按键测试：区域页Down→Down选中“设定完成”，OK后于1221520606instructions/4096390188ticks显示frame8日期/时间页（默认2020-01-01 00:00）；再次OK并继续运行，frame9显示原厂手机应用二维码介绍页（1272770606instructions/4314614835ticks的截图）。没有扫描二维码或访问外部应用；此页仅是固件实际绘制。原OK“结束”后继续启动，在 **1296413221instructions/4384315270ticks** 自动停止，CPUfailure=`ARMv7-A device made no deadline progress`，当前outputs为空。终态六键host电平均released，所有采样stable/current为0；已保存UI全文/寄存器/采样与截图，但尚无同一browserVM的完整device snapshot，Node独立重放正在采集。原厂PC12807d4/LR1280844属于RAM锁释放及恢复中断状态的调用链，不能把当前PC直接当作造成deadline错误的MMIO或故障器件。上述设置改变属于本地虚拟会话；没有相机命令、机身设置或持久化写入。日期页、应用介绍页。

已完成的首次设置源码核验含474项断言/433条LLVM原指令匹配：静态screen request421..426是可选请求，不自行推出页面顺序；group32候选action6中的1124e64是原厂return0空函数，不能冒充保存成功。实际页面顺序来自上面的浏览器截图。另有明确host源码边界：Devices::due固定循环64次，第64次后不再检查是否已清完，过期queued event也消耗次数；该消息不能单独证明器件完全不进展。尚在用独立完整device snapshot定位，未改cap或丢弃事件来放行。 调度器源码审查的49项断言通过：SLVS追赶过去的frame/end时刻可能产生之后只消耗调度预算的旧回调；SPI/Requests/Flash-controller-own-due/DMA-completion也有依赖匹配event退休的边界，正常arm路径确实安排event。均为静态候选。现有snapshots()不包含全局event queue及末64次选择，不能仅凭各设备状态宣称已经区分过期队列与缺失event；若完整快照仍不够，将补最小被动调度记录。

## 同一导航的独立重放与首个错误保留修复

独立Node首轮重放实际运行原`pressVirtualButton`，在首批run前释放六键，记录1035次run/78次输入setter；frame1在776020606instructions输出，其RGBA与既有日语页的`98e541…`逐字节相同。随后实际导出seq2韩语、seq3繁体、seq4简体、seq5区域、seq8日期、seq9手机应用介绍，共9帧；每帧同刻保存原始state/diagnostics/full JSON与RGBA/PNG。输入时刻不同于浏览器，不能比较为同预算等价。独立重放目录。

首轮最终 **907686881instructions/2204337261ticks** 再现同一generic deadline错误，`cpu.next_device_tick==now`。当前唯一显式到期模型字段是DMA通道5：submitted=true、completed=0、submitted_at2204337244、next2204337261，源`fff34008`、目标`177bcc0`、总量4bytes、固定源width1。该地址匹配SDK的SPI2 FIFO地址布局，但尚未提供该peer或SPI-DMA支持；不是可当作RAM读的地址。完整snapshot并不含全局event queue，不能仅凭到期字段证明64次都是它或队列为空。

**小型真实ARM原生回归已证实一处错误覆盖。** 同一DMA5的有效RAM固定源对照正常复制`[1,1,1,1]`；源换为fff34008后目的保持5A，初始拒绝应为`Outside declared ARM32 physical memory`，但旧`Cluster::due`后续调用把它覆盖成generic deadline错误。修复只将错误赋值改为`failure.get_or_insert(error)`，仍停止全部core并保留原64预算、原MMIO/输入/设备模型/guest。修复前正确回归失败、修复后通过；最初fixture误把固定源结果写成递增值的日志另存`initial-fixture.log`，不作回归证据。首错实验。

新Root source **8d3acc1330dbd987d0c33ea6741b831bbd06c44b8e5660d8149eeef7d1c93c7b**、Wasm **3b32b102fb22392ad4b4963d25e0e79b79a6f0d5c547ff14ea2849f515a21161**、SDK board **1e1c3ad8461ee26a61c5bd6079e38994174f96ed417cf56086ad191b8ebd024c**。仅第三份passive-discovery patch增加首错保留（新SHA706e0ef55961ad2177ad2180b02ca4dfd421cbf606a4a3f7ba1c26d14ce82d6c），其余五份未为此更改；六patch clean/reverse/idempotency通过。build/check、默认测试（含3个原生bridge测试、62IO、11Bun）通过，pkg/public/dist字节一致。**新Wasm全导航重放现已实际复现正确首错**：run-02同为907686881instructions/2204337261ticks，failure=`Outside declared ARM32 physical memory`。Python精确整数比较仅去掉两份terminal的`cpu.failure`后，全部其余保存state（含完整diagnostics）相等；全部9帧与所有非terminal文件共120个artifact逐byte相等，78次输入／1035次预算一致；PNG CRC、RGBA、LCDC digest及全部checkpoint/schedule核验通过。新terminal完整SHA256 **6ee326ccfa88ce98694a04dd78a5d000641c69797fa8a333e5af3adee67d8d8d**。两轮完整核验。这确认修复只保留正确拒绝原因，没有改变设备模型、输入或固件执行。独立首错修复审查的60项断言通过，无must-fix；该改动不宣称重设计所有错误优先级，后续device servicing仍会发生。新浏览器setup1M实跑的64个寄存器与四核计数均等于旧released1M对照，只比较这些可见字段，未声称完整browserVM snapshot相等。开发服务器已恢复。两处网页文案也已修正，说明采样与重绘分离、无当前帧不等于从未出现过UI。 新原生／Wasm严格7346、preloaded1M/9M及各constructor边界对照也已通过；没有为此重复原生80M。

SPI2/DMA原字节调查进一步绑定descriptor15→controller2、TX descriptor5→DMA6/request5、RX descriptor4→DMA5/request4。原119e344先在119e490启动RX，119e4c4才启动TX；冻结状态的RX5 submitted/TX6未提交与此相符。当前ready_time却把非NOR源当作立即就绪RAM，缺的是外设请求与FIFO逐byte门控，而非固定RAM源byte循环复制。当前不存在SPI2模型，尚无SPI2任何交换bit的正证据；客户端11fba28使用TX177b480/RX177bcc0，count4至少对应三条固定调用点，无法仅凭长度选定。a5 5a 3c c3请求及C33CA55A接收检查是原厂候选／期望，不是已观测或注入的数据。物理peer身份仍未知；下一步最小被动观察点与4byteTX范围已在报告中列出。

官方XAcquire已定位到安装版本1.27.0.6/menu-bar agent，但Computer按应用名、已核路径、bundleID读取界面均超时，未进入备份/恢复向导，未生成新版备份，也未再次确认相机连接/版本；不宣称新的Flash/RAM或设置实机导出。没有发送非官方相机命令，官方app自身连接活动未观察。检查记录。

## 用固件建立开发模拟器

**可以逐步建立专用环境，优先目标为局部原生执行与 UI 开发；现阶段没有可直接加载 DAT、完整启动的虚拟 X-T4。** 本轮进一步审阅发现，已有 FujiMod 不仅有 X70：还包含 Unicorn Cortex-A7、XP4 多核/部分外设模型，以及 RAM read/write、单步预算运行、scanout 和未知 MMIO frontier 等研究接口，可作为底座。

X-T4 2.12 配置仍选择 `xt4-discovery`，为 loader-start、`execution=null`；boot ROM、installation、generated facts 均缺失。标准 boot CLI 明确拒绝 loader/XP4，手动 XP4 镜像入口则标注 `boot_capable=False`。部分显示/电源等模型参考 X-T3 或使用声明输入，不能当作 X-T4 硬件已完整还原。catalog 的 512 MiB DRAM 也是声明输入，不是本轮实测规格。

实施方向：复用 XP4 → 确认固件映射和最小函数执行 → strict 模式记录/补齐 MMIO → 跑最小 UI 子系统 → 启动到菜单 → 完成虚拟 Flash 更新循环。PocketJS 页面可先在桌面 host 开发。更新 DAT 不等于完整 Flash 镜像；模拟烧录成功也不证明实机接受修改包或具备恢复能力。

完整源码证据、缺口与验收步骤见 模拟器评估，解析与源码 hash 见 source-audit.json。复用了既有 REA 固件 Evidence，并完成只读 CLI doctor；本轮没有重复固件扫描、构建/执行模拟器或接触相机。

**所需用户输入与后续采集：** 当前起点为实机/镜头版本、可重复观察的参考机与 USB 连接条件；无需先升级/降级、拆机或开启 debug。完整功能仿真还需逐步获得 Boot ROM/loader/存储布局、设置与校准、必要控制器状态，以及 RAM/CPU/页表和外设交互证据；能否读取、如何读取仍待研究。通信抓包、按键/菜单录像和受控 RAW/JPEG 可作行为对照，但不能替代 MMIO/IRQ/DMA 证据或完整恢复 ISP。详细材料、用途、采集条件及原始记录要求已补入模拟器评估。上述清单是缺口与采集规划，不是已取得这些能力的声明。

