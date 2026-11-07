# X-T4 startup hardware documentation search

Date: 2026-10-28, Asia/Shanghai. Read-only investigation; no camera access, Grok, Docker, builds, application/core edits, or upstream changes. Only this directory was written. This is a bounded search result, not a proof that matching documentation does not exist.

## Finding ledger

| ID | Question | Result and evidence | Confidence / remaining gap |
|---|---|---|---|
| H01 | Physical identity of `0xfff4a400` | No exact matching vendor register description or real-device trace found. Existing loader audit proves writes 0/1 around startup/pin configuration, not the electrical effect. | Unresolved. Need exact FF4230/XP4 register manual or measured register/pin/clock relationship. |
| H02 | Physical meaning of `0xfff48700` | No exact matching vendor description found. Existing source-bound audit proves a channel-dependent two-bit selector written during SPI configuration. | Unresolved. Clock/routing remains a hypothesis; parent frequencies, field-write semantics, reset state and effect on peripherals are unknown. |
| H03 | Four-byte SPI peer identity | The inspected PCB component database lists `R5F100EDA` under system control MCU for X-T4, and links original PCB images. Renesas identifies its family as RL78/G13, with CSI/SPI and RTC peripherals. | A concrete MCU candidate only. No schematic, continuity measurement, pin routing or firmware-level association to XP4 SPI1/GPIO12.5 was found. |
| H04 | Could peer simply be MAX77812? | Original ADI datasheet has 32-bit single SPI transactions, but its framing/address/data/check-select behavior differs from the observed loader protocol. | Excluded as a direct matching protocol based on this datasheet; it can still be present elsewhere on the board or behind an MCU. |
| H05 | Could MAX77961 be this SPI peer? | ADI describes I2C/resistor-programmed charger control, not the observed SPI protocol. | No match as the direct peer. No assertion about actual board routing. |
| H06 | Register17=`0x7f00` semantics | No matching register map, measured transaction/result sequence or controller application source found. | Unresolved. Do not declare a mask, reset, clear, acknowledgement or ordinary storage merely to pass startup. |
| H07 | Does existing FujiMod catalog identify the chip? | Pinned part is simply `xt3-power-registers`, a `power-controller` interface; X-T4 body has `power_control:null` and references that part. | No hardware chip identification. The generic catalog name and simulation model are not an independent hardware source. |
| H08 | RL78 firmware recognizer inputs | Official part capacity, memory map, vector layout and option byte positions are available; details below. | Useful for a bounded candidate scan. They neither establish that DAT contains this MCU's image nor prove a candidate image belongs to the power peer. |

## Existing artifact authority retained

This search reuses startup frontier and SPI frontier. Their byte identity, instruction observations and REA Evidence remain the authority for loader behavior. This documentation search does not generate new REA Evidence or re-run firmware.

- Source DAT SHA-256: `853d273b5603d3a2b93493f0ebd6a762b0b4b6bdc70317777775ae5f8ed93bc5`.
- Pinned FujiMod checkout: `1de936e9b1ca738b04ad5132c727288ddd5eb2ae` (verified locally).
- Exact local paths checked: `catalog/bodies/xt4/body.json`, `catalog/parts/xt3-power-registers/part.json`, `catalog/soc/xp4/soc.json`, `crates/emulator-xp4-io/src/checksum_power_serial.rs`, `crates/emulator-parts-io/src/power_controller.rs`; and bounded text searches across `references/fffw`.
- The latter generic power-controller file explicitly says it is an emulated register file, not an identified external PMIC. The checksum peer code is evidence of a modeled protocol, not evidence of the manufactured peer's register side effects.

## Board component and photograph locator

The [Photo-parts X-T4 record](https://photo-parts.com.ua/parts/dbphp.php?part=Fujifilm+X-T4) lists `R5F100EDA` in the **System control processor, MCU** column, alongside XP4 `FF4230` and several power devices (`MAX77812B`, `MAX77961`, `MAX8972CEWI` and others). Its [component profile](https://photo-parts.com.ua/parts/part_profile.php?part=9219) repeats the MCU classification but has no application protocol notes; the profile's image link is a placeholder, not a chip photograph.

The model record's PCB-folder link resolves to this [original image index](https://photo-parts.com.ua/parts/imgbrowse.php?folder=IMG_STORE/Fujifilm%20X-T4%20PCB). It exposes:

- [Fujifilm XT4_PCB.jpg](https://photo-parts.com.ua/parts/IMG_STORE/Fujifilm%20X-T4%20PCB/Fujifilm%20XT4_PCB.jpg)
- [Fujifilm XT4_PCB_1.jpg](https://photo-parts.com.ua/parts/IMG_STORE/Fujifilm%20X-T4%20PCB/Fujifilm%20XT4_PCB_1.jpg)
- `Fujifilm XT4_SD_reader_pcb.jpg`, `Fujifilm XT4_SD_reader_pcb_1.jpg`, `Fujifilm XT4_back.jpg`, `Fujifilm XT4_top.jpg` in the same index.

**Boundary:** this pass verified the component table and actual image links, not readable package markings or a traced board net in the image pixels. The table is a repair site's hardware observation, not a Fujifilm schematic or manufacturer confirmation. No component coordinates, solder test points or net names are established here. The cached HTML preserves the table and links; it is not a board wiring map.

## R5F100EDA / RL78-G13: official facts for a candidate scan

Sources: [Renesas exact part page R5F100EDANA#W0](https://www.renesas.com/en/products/rl78-g13/part-details/r5f100edana-w0), [G13 datasheet R01DS0131EJ0380, 2024-10-31](https://www.renesas.com/document/dst/rl78g13-data-sheet?language=en), and downloaded [G13 hardware manual R01UH0146EJ0380](https://www.renesas.com/en/document/man/rl78g13-users-manual-hardware?language=en). The board table reports a shortened marking; full ordering suffix is unverified.

| Property | Official family/part fact |
|---|---|
| Core / ISA | RL78 16-bit MCU, not ARM/Thumb. |
| R5F100ED memory | 48 KiB code flash, 4 KiB data flash, 3 KiB RAM; 40-pin family member. The ED code does **not** denote 64 KiB flash. |
| Code flash | `0x00000..0x0bfff` for the xD family (hardware manual printed pp108,121). |
| RAM | `0xff300..0xffeff`; the top 32 bytes `0xffee0..0xffeff` are the general-purpose register area. Do not treat them as extra RAM outside the stated 3 KiB. |
| Data flash | `0xf1000..0xf1fff` for the R5F100 variant. |
| Vector table | `0x00000..0x0007f`, two-byte addresses, low byte at even address and high byte at odd address (manual p121). |
| Reset vector | First two code bytes; reset vector address `0x00000` also confirmed by [official interrupt migration note Table4.6](https://www.renesas.com/en/document/apn/rl78g13-78k0kx2-migration-guide-78k0-rl78-interrupt-functions-rev100), printed p10. |
| CALLT / options / debug ID | `0x00080..0x000bf`; `0x000c0..0x000c3`; `0x000c4..0x000cd` respectively (manual p108). |
| Boot swap | The manual specifies another vector table at `0x01000..0x0107f` when boot swap is used. This is conditional, not a required signature in all images. |
| Serial interfaces | 40-pin diagram (manual p37) has CSI00/11/20/21; datasheet explicitly calls CSI the interface generally called SPI. It also has RTC and GPIOs. |

These are **candidate-recognition constraints, not a unique firmware magic**. For a raw full image assumed to start at MCU address zero, a plausible little-endian reset destination inside the code extent, coherent used vectors and meaningful RL78 code/data improve a candidate; arbitrary 48 KiB blocks and low numeric words alone do not identify the image. Unused vectors, an updater wrapper, partial update, compression, offset loading or boot-cluster handling can defeat a naive vector scan. Lack of a matching raw layout cannot establish that the DAT contains no MCU update.

The MCU's `P12`, `P13`, etc. are Renesas pin names. They are **not** the XP4 GPIO bank/pin pair `12/5`. The Renesas pin manual describes MCU pad alternate functions but does not identify the X-T4 board trace, CS polarity, voltage, clock rate, XP4 GPIO12.5 destination or application register0/16/17 protocol. Application firmware for the external controller, a board net trace or a real transaction capture is still needed. No claim is made that this MCU is necessarily the peer or that its update is necessarily included in the camera DAT.

## Vendor protocol comparison: MAX77812 is a false friend

[ADI MAX77812 datasheet](https://www.analog.com/media/en/technical-documentation/data-sheets/max77812.pdf), printed pp40–42, describes SPI with low-active SCS, R/W then single/multiple selection, reserved bits, ten address bits, packet length and **eight-bit** data. Its single operation is 32 bits, but there is no matching 7-bit register / 16-bit payload / sum-to-255 frame. The [register map](https://www.analog.com/media/en/technical-documentation/data-sheets/max77812.pdf) p44 names `0x10/0x11` as GPI debounce bytes; numeric overlap with loader registers16/17 is not identity evidence.

The [MAX77960/MAX77961 vendor datasheet](https://www.analog.com/media/en/technical-documentation/data-sheets/MAX77960-MAX77961.pdf), p1, instead advertises I2C or resistor control. It does not explain the loader SPI link. Neither comparison identifies what the actual peer does with `91 7f 00 ef`; it only prevents importing an unrelated PMIC register map.

## Related SoC material: do not transplant registers

Primary [Linux M10V DTS](https://raw.githubusercontent.com/torvalds/linux/master/arch/arm/boot/dts/socionext/milbeaut-m10v.dtsi) identifies `socionext,sc2000a` and maps its clock control at `0x1d021000`; the [Socionext-authored clock driver](https://raw.githubusercontent.com/torvalds/linux/master/drivers/clk/clk-milbeaut.c) has SPI parent mux definitions. Neither establishes XP4 FF4230 identity, address aliases or the two questioned registers. A common Cortex-A7 CPU or Socionext IP naming is insufficient to map M10V clock fields to `0xfff48700` or a nearby pin-control gate to `0xfff4a400`.

The inspected [GFX lens-bus measurement article](https://26oclock.com/posts/fuji-g-firmware-decompilation/) contains a four-byte serial transaction discussion but concerns a **body-to-GF-lens link**, not the XP4 power-control peer. It supplies no exact register0/16/17 or MCU association and was not used as a power protocol definition.

## Exact missing evidence and next bounded question

1. FF4230/XP4 register documentation or a physically identified trace resolving `0xfff4a400` electrical side effects, reset state and legal values.
2. Matching per-channel SPI selector documentation establishing `0xfff48700` source/routing choices and field update behavior.
3. Board-level identity for SPI1 clock/MOSI/MISO and XP4 GPIO12.5: schematic/netlist or continuity/logic capture with identifiable endpoint.
4. A real startup trace containing register0/16 replies, the `0x11=0x7f00` request and the immediate status/interrupt/power consequences; or the identified peer's application firmware with its command dispatcher.
5. If a separately updated RL78 image is found in DAT, its container bounds/version/identity and an RL78-side decoder are needed before assigning these register semantics. No such image was identified by this documentation task.

At the current evidence boundary the faithful result is **unresolved hardware semantics**. Existing strict refusals should remain; a selected ready input is a declared scenario, and accepting unknown writes as no-ops or storage would be an intervention rather than a verified hardware model. Camera boot remains unverified.

## Files and checks

- `sources.json`: source URLs, inspected sections and evidence classification.
- `search-log.json`: bounded exact-address/protocol/part queries and outcomes.
- `photo-parts-xt4.html`: 36,210-byte original model record, table/link cache; SHA-256 `61b8b5b165e7e2b5192c79eee045023c6abf287d9aad1fd7034a3194aa579fc2`.
- `rl78g13-hardware.pdf`: vendor manual, 11,296,482 bytes, SHA-256 `f25502674005c6e9f06241ab8348c15698336eea1361d8d5b87dd6b65bb1a2f9`.
- `rl78g13-printed-page-*.txt` and `rl78g13-pdf-page-0144-vector.txt`: selected text extracted by bundled pypdf, not an OCR-derived register map. Printed page108 corresponds to PDF page131; printed p121 corresponds to PDF page144.
- `download-manifest.json` records downloads and a timeout fetching ADI PDF through the shell. The ADI datasheet itself was successfully opened and read through the web PDF tool; no repeated shell download was attempted.

Documentation files and JSON parsing were checked. No tests/builds or physical measurements were run. The most useful new fact is a specific RL78 MCU candidate plus an official-protocol exclusion of the tempting four-byte MAX77812 match; neither resolves peer register17.
