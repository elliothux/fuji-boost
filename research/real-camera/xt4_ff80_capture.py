"""One-session X-T4 evidence capture over fffw's ff80 (jig) transport.

Reuses references/fffw/ff80 (fftlib/ffjlib) unchanged. Every operation is
appended to ops.jsonl as it happens, so an interrupted session keeps its data.

Interactive part: device info, config-data dump, RTC, live GPIO/ADC sampling
for guided control mapping. app80 requests travel through fffw's config-RAM
mailbox (bytes 0x80..0xA0); nothing is saved to NVRAM by this script.
Unattended part: sets config byte 0xF7 (USB debug) and runs DEBUG_PHASES from
the safest to the riskiest read, then restores 0xF7. Nothing writes flash.
"""
import argparse, hashlib, json, os, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'references/fffw/ff80'))

GPIO_PORTS = range(16)
ADC_CHANNELS = range(12)
ADC_NOISE = 24

# (group, kind, instruction). kind: press = hold then release; state = leave as is;
# dial = user types the position label, empty label ends the group.
STEPS = [
    ('按键', 'press', '半按快门并保持'),
    ('按键', 'press', '全按快门并保持（jig 模式下一般不会拍照）'),
    ('按键', 'press', '按住 MENU/OK'),
    ('按键', 'press', '按住 DISP/BACK'),
    ('按键', 'press', '按住 方向键 上'),
    ('按键', 'press', '按住 方向键 下'),
    ('按键', 'press', '按住 方向键 左'),
    ('按键', 'press', '按住 方向键 右'),
    ('按键', 'press', '按住 Q 按钮'),
    ('按键', 'press', '按住 AF-ON'),
    ('按键', 'press', '按住 AE-L'),
    ('按键', 'press', '按住 播放按钮'),
    ('按键', 'press', '按住 顶部 Fn 按钮（没有就输入 s 跳过）'),
    ('按键', 'press', '按住 VIEW MODE（取景器旁）'),
    ('按键', 'press', '按下并保持 前拨盘（往里按）'),
    ('按键', 'press', '按下并保持 后拨盘（往里按）'),
    ('按键', 'press', '按下并保持 对焦摇杆（中间按下）'),
    ('按键', 'press', '对焦摇杆 推向 上 并保持'),
    ('按键', 'press', '对焦摇杆 推向 下 并保持'),
    ('按键', 'press', '对焦摇杆 推向 左 并保持'),
    ('按键', 'press', '对焦摇杆 推向 右 并保持'),
    ('按键', 'press', '对焦摇杆 推向 右上 并保持'),
    ('按键', 'press', '对焦摇杆 推向 左下 并保持'),
    ('按键', 'press', '按住 快门速度盘中间的锁定按钮'),
    ('按键', 'press', '按住 镜头释放按钮（不要转镜头）'),
    ('按键', 'press', '用手指遮住取景器眼感应器'),
    ('按键', 'press', '用手指按住屏幕中央（触摸）'),
    ('拨盘', 'dial', '快门速度盘：从 A 开始逐格转到 B/T，每格输入盘上标识后回车'),
    ('拨盘', 'dial', 'ISO 拨盘：提起后逐格转动，每格输入标识（A、L、160…12800、H）'),
    ('拨盘', 'dial', '曝光补偿拨盘：逐格转动，每格输入标识（-3…+3、C）'),
    ('拨盘', 'dial', 'ISO 下方驱动环：逐格转动，每格输入标识（BKT、CH、CL、S、ADV…）'),
    ('拨盘', 'dial', '快门盘下方 静止/视频 拨杆：每个位置输入标识'),
    ('拨盘', 'dial', '机身前方 AF 模式开关 S/C/M：每个位置输入标识'),
    ('拨盘', 'state', '前拨盘 顺时针 转 1 格'),
    ('拨盘', 'state', '前拨盘 顺时针 再转 1 格'),
    ('拨盘', 'state', '前拨盘 顺时针 再转 1 格'),
    ('拨盘', 'state', '前拨盘 逆时针 转 1 格'),
    ('拨盘', 'state', '前拨盘 逆时针 再转 1 格'),
    ('拨盘', 'state', '后拨盘 顺时针 转 1 格'),
    ('拨盘', 'state', '后拨盘 顺时针 再转 1 格'),
    ('拨盘', 'state', '后拨盘 顺时针 再转 1 格'),
    ('拨盘', 'state', '后拨盘 逆时针 转 1 格'),
    ('拨盘', 'state', '后拨盘 逆时针 再转 1 格'),
    ('机身', 'state', '翻转屏：从机身拉开到侧面（屏幕仍朝后）'),
    ('机身', 'state', '翻转屏：转到朝前（自拍方向）'),
    ('机身', 'state', '翻转屏：反扣合上（屏幕朝里）'),
    ('机身', 'state', '翻转屏：恢复正常位置'),
    ('机身', 'state', '打开存储卡仓门（不要拔卡）'),
    ('机身', 'state', '拔出 卡槽 1 的卡'),
    ('机身', 'state', '拔出 卡槽 2 的卡（没有就 s）'),
    ('机身', 'state', '把卡写保护开关拨到 LOCK，插入卡槽 1'),
    ('机身', 'state', '拔出卡，写保护拨回解锁，插回卡槽 1'),
    ('机身', 'state', '插回卡槽 2 的卡（没有就 s）'),
    ('机身', 'state', '关上存储卡仓门'),
    ('接口', 'state', '插上 HDMI 线（没有就 s）'),
    ('接口', 'state', '拔下 HDMI 线（没有就 s）'),
    ('接口', 'state', '插上 3.5mm 麦克风/耳机（没有就 s）'),
    ('接口', 'state', '拔下 3.5mm 设备（没有就 s）'),
    ('接口', 'state', '热靴装上闪光灯（没有就 s）'),
    ('接口', 'state', '取下闪光灯（没有就 s）'),
    ('镜头', 'state', '镜头光圈环：拨到 A（没有光圈环就 s）'),
    ('镜头', 'state', '镜头光圈环：拨离 A 到某个光圈值（没有就 s）'),
    ('镜头', 'state', '卸下镜头（盖好机身盖）'),
    ('镜头', 'state', '装回镜头'),
    ('姿态', 'state', '相机竖拿（手柄朝上）'),
    ('姿态', 'state', '相机竖拿（手柄朝下）'),
    ('姿态', 'state', '相机恢复水平'),
]

# Follow-up for 2026-11-01 ambiguities; dials are named by position, not front/rear.
RECHECK_STEPS = [
    ('复核', 'press', '轻轻半按快门（对焦那一档，不要按到底）并保持'),
    ('复核', 'press', '按住【屏幕那一面、拇指转】的拨盘，往里按'),
    ('复核', 'press', '按住【手柄正面、食指转】的拨盘，往里按'),
    ('复核', 'state', '【拇指】拨盘 顺时针 转 1 格'),
    ('复核', 'state', '【拇指】拨盘 顺时针 再转 1 格'),
    ('复核', 'state', '【食指】拨盘 顺时针 转 1 格'),
    ('复核', 'state', '【食指】拨盘 顺时针 再转 1 格'),
    ('复核', 'state', '插上 HDMI 线（没有就 s）'),
    ('复核', 'state', '拔下 HDMI 线（没有就 s）'),
    ('复核', 'state', '卸下镜头，装上机身盖'),
    ('复核', 'state', '取下机身盖，装回镜头'),
]

# Tier C: status/capability registers, never data/FIFO/IRQ-acknowledge registers.
MMIO = [('gic-typer', 0xffff1004), ('gic-iidr', 0xffff1008)]
MMIO += [(f'gic-isenabler{i}', 0xffff1100 + 4 * i) for i in range(16)]
for base in (0xfffc4000, 0xfffc6200, 0xfffc8000, 0xfffca200):
    MMIO += [(f'sdhci-{base:x}+{o:x}', base + o) for o in (0x24, 0x28, 0x2c, 0x40, 0x44, 0x48, 0xfc)]
MMIO += [('sd-bridge-fffc41e0', 0xfffc41e0), ('sd-bridge-fffc41e4', 0xfffc41e4)]
MMIO += [(f'sd-wrap-fffc6000+{o:x}', 0xfffc6000 + o) for o in (0x78, 0x80, 0xec)]
MMIO += [(f'gates+{o:x}', 0xfff48000 + o) for o in list(range(0x300, 0x338, 4)) + [0x600, 0x604, 0x608, 0x700]]
MMIO += [(f'i2c-fff3e000+{o:x}', 0xfff3e000 + o) for o in (0x0, 0x4, 0x8, 0x14, 0x18)]
MMIO += [(f'ff70f000+{o:x}', 0xff70f000 + o) for o in range(0, 0x40, 4)] + [('ff70a040', 0xff70a040)]
MMIO += [(f'gpio-fff41000+{o:x}', 0xfff41000 + o) for o in range(0, 0x400, 4)]


class Log:
    def __init__(self, out):
        self.out = out
        out.mkdir(parents=True, exist_ok=False)
        self.ops = (out / 'ops.jsonl').open('a', buffering=1)
        self.steps = (out / 'steps.jsonl').open('a', buffering=1)

    def op(self, **record):
        record['t'] = time.time()
        self.ops.write(json.dumps(record, ensure_ascii=False) + '\n')

    def step(self, **record):
        record['t'] = time.time()
        self.steps.write(json.dumps(record, ensure_ascii=False) + '\n')

    def blob(self, name, data):
        (self.out / name).write_bytes(data)
        self.op(kind='file', name=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())


class Camera:
    def __init__(self, jig, log):
        self.jig, self.log, self.delay = jig, log, 0.05

    def app80(self, cmd, params, delay=None):
        # X-T4 replies echo the command and first argument; re-read until they match.
        self.jig.app80_request(cmd, bytearray(params))
        time.sleep(self.delay if delay is None else delay)
        for attempt in range(20):
            r = bytes(self.jig.app80_response())
            if r[0] == cmd and r[1] == params[0]:
                break
            time.sleep(0.01)
        else:
            raise RuntimeError(f'app80 {cmd:#x} {list(params)} reply never matched: {r.hex()}')
        self.log.op(kind='app80', cmd=cmd, params=list(params), reply=r.hex(), attempt=attempt)
        return r

    def gpio(self, port, delay=None):
        r = self.app80(0x01, [port], delay)
        return int.from_bytes(r[2:4], 'big')

    def adc(self, channel):
        r = self.app80(0x02, [channel], max(self.delay, 0.1))
        return int.from_bytes(r[2:4], 'big')

    def sweep(self):
        return {'gpio': [self.gpio(p) for p in GPIO_PORTS], 'adc': [self.adc(c) for c in ADC_CHANNELS]}


def diff(before, after):
    out = []
    for p, (a, b) in enumerate(zip(before['gpio'], after['gpio'])):
        for bit in range(16):
            if (a ^ b) >> bit & 1:
                out.append(f'GPIO{p}.{bit} {a >> bit & 1}->{b >> bit & 1}')
    for c, (a, b) in enumerate(zip(before['adc'], after['adc'])):
        if abs(a - b) > ADC_NOISE:
            out.append(f'ADC{c} {a}->{b}')
    return out


def ask(prompt, auto):
    if auto:
        print(prompt + ' [auto]')
        return ''
    return input(prompt).strip()


def record(cam, seconds):
    samples, end = [], time.time() + seconds
    while time.time() < end or len(samples) < 2:
        samples.append(cam.sweep())
    return samples


def tier_a(cam, log, args):
    jig = cam.jig
    info = {}
    for tag, name in jig.info_tags().items():
        if tag < 0x100:
            try:
                info[name] = jig.get_device_info(tag)
            except Exception as e:
                info[name] = f'error: {e}'
    log.op(kind='info', info=info)
    print('相机信息：', json.dumps(info, ensure_ascii=False))
    if 'X-T4' not in str(info.get('Product Name', '')) and not args.dry_run:
        sys.exit('不是 X-T4，停止。')

    if not args.recheck:
        size, chunk = jig.get_config_data_size()
        log.op(kind='cfgdata-size', size=size, chunk=chunk)
        print(f'读取配置数据 {size} bytes（只读备份）…')
        data = bytearray()
        while len(data) < size:
            data += jig.read_config_data(0, len(data), min(chunk, size - len(data)))
            if len(data) % (512 * 1024) == 0:
                print(f'    {len(data) * 100 // size}%', end='\r', flush=True)
        log.blob('cfgdata-start.bin', bytes(data))
        print('配置字节 0xF7 =', hex(data[0xf7]) if len(data) > 0xf7 else '?')

    try:
        rtc = bytes(cam.app80(0x06, [1, o])[3] for o in range(16))
        log.op(kind='rtc', bytes=rtc.hex())
    except Exception as e:
        log.op(kind='rtc-error', error=repr(e))
        print('RTC 读取跳过：', e)

    print('校准采样间隔…')
    reference = [cam.gpio(p, 0.05) for p in GPIO_PORTS] + [cam.gpio(p, 0.05) for p in GPIO_PORTS]
    # A stale reply is only detectable when neighbouring ports read differently.
    stable = reference[:16] == reference[16:] and len(set(reference[:16])) >= 4
    for delay in (0.02, 0.01):
        if stable and [cam.gpio(p, delay) for p in GPIO_PORTS] == reference[:16]:
            cam.delay = delay
    log.op(kind='calibration', stable=stable, delay=cam.delay, reference=reference)
    print(f'采样间隔 {cam.delay * 1000:.0f} ms')

    ask('\n【基线】把相机平放，不要碰任何按键，按回车开始（约 10 秒）', args.auto)
    baseline = record(cam, 10 * args.scale)
    log.step(group='基线', kind='baseline', samples=baseline)
    base = baseline[-1]
    noisy = sorted({d for s in baseline for d in diff(baseline[0], s)})
    if noisy:
        print('静止时也在变化（会在后续结果里标出）：', ', '.join(noisy))
    print(f'基线 GPIO: {[hex(v) for v in base["gpio"]]}\n基线 ADC: {base["adc"]}')

    steps = RECHECK_STEPS if args.recheck else STEPS
    state, n = base, len(steps)
    print('\n每一步：照提示操作，按回车开始记录。输入 s 跳过这一步，g 跳过这一组剩余步骤，q 结束采集。')
    skip_group = None
    for i, (group, kind, text) in enumerate(steps, 1):
        if group == skip_group:
            continue
        if kind == 'dial':
            print(f'\n[{i}/{n}] {group}：{text}\n    转到一格后，输入该格标识并回车；直接回车（空）结束这一项。')
            labels = iter(['A', '']) if args.auto else None
            while True:
                label = next(labels) if labels else ask('    位置标识> ', False)
                if label in ('', 's'):
                    break
                if label in ('g', 'q'):
                    skip_group = group
                    if label == 'q':
                        return finish_a(cam, log, base)
                    break
                samples = record(cam, 2 * args.scale)
                found = diff(state, samples[-1])
                log.step(group=group, kind=kind, text=text, label=label, samples=samples, changes=found)
                print('    变化：', ', '.join(found) or '无')
                state = samples[-1]
            continue
        reply = ask(f'\n[{i}/{n}] {group}：{text}\n    准备好后按回车（保持动作直到提示松开）> ', args.auto)
        if reply == 's':
            log.step(group=group, kind=kind, text=text, skipped=True)
            continue
        if reply in ('g', 'q'):
            skip_group = group
            if reply == 'q':
                return finish_a(cam, log, base)
            continue
        print('    记录中…保持住')
        samples = record(cam, 4 * args.scale)
        found = diff(state, samples[-1])
        print('    变化：', ', '.join(found) or '无（可能不走 GPIO/ADC）')
        entry = dict(group=group, kind=kind, text=text, samples=samples, changes=found)
        if kind == 'press':
            ask('    可以松开了，松开后按回车', args.auto)
            entry['release'] = cam.sweep()
            entry['releaseChanges'] = diff(state, entry['release'])
        else:
            state = samples[-1]
        log.step(**entry)
    return finish_a(cam, log, base)


def finish_a(cam, log, base):
    jig = cam.jig
    if cam.recheck:
        return
    for diag in range(16):
        try:
            log.blob(f'diag-{diag:02d}.bin', bytes(jig.read_data_diag(diag)))
        except Exception as e:
            log.op(kind='diag', id=diag, error=str(e))
    ask('\n【结束基线】把相机恢复到开始时的状态（卡、镜头、拨盘都放回），平放后按回车', cam.auto)
    end = record(cam, 6 * cam.scale)
    log.step(group='基线', kind='baseline-end', samples=end, changes=diff(base, end[-1]))
    size, chunk = jig.get_config_data_size()
    data = bytearray()
    while len(data) < size:
        data += jig.read_config_data(0, len(data), min(chunk, size - len(data)))
    log.blob('cfgdata-end.bin', bytes(data))
    start = (log.out / 'cfgdata-start.bin').read_bytes()
    changed = [i for i in range(min(len(start), len(data))) if start[i] != data[i]]
    log.op(kind='cfgdata-diff', offsets=changed)
    outside = [hex(i) for i in changed if not 0x80 <= i <= 0xa0]
    print('配置 RAM 变化：mailbox 0x80–0xA0 以外 =', outside or '无')


MIB = 1 << 20
# Phases run from the safest to the riskiest, so a hang only loses later phases.
# (name, description, regions). Regions are dumped in 16 MiB files.
# jig RAM reads move 256 bytes per request (about 20 KB/s), so most regions are
# mapped with 64 KiB samples; 'budget' dumps fully until --ram-minutes runs out.
DEBUG_PHASES = [
    ('image', '主映像（固件代码正在这里运行）', [(0x01000000, 0x00800000, 'full')]),
    ('syslog', '调试日志', []),
    ('alias', '固件写过的约 994 MiB 处，与 512 MiB 内对应位置比较', [
        (0x1e1b0000, 0x2000, 'full'), (0x3e1b0000, 0x2000, 'full'), (0x3e18f000, 0x23000, 'full')]),
    ('mmio', '状态/能力寄存器与成像 SRAM', []),
    ('dram-map', '512 MiB 以内每 16 MiB 抽样 64 KiB', [(0x01800000, 0x1e800000, 'sample')]),
    ('dram-data', '512 MiB 以内其余 DRAM（时间预算内完整读取）', [(0x01800000, 0x1e800000, 'budget')]),
    ('dram-zero', '最低 64 KiB（向量区，可能未映射）', [(0, 0x10000, 'full')]),
    ('dram-512m-1g', '512 MiB–1 GiB 抽样', [(0x20000000, 0x20000000, 'sample')]),
    ('mmio-rest', '其余寄存器（跳过 2026-11-01 读到卡死的 FFFC6000/FFFCA000 两组 SD 主机）', []),
    ('dram-1g-2g', '1–2 GiB 抽样', [(0x40000000, 0x40000000, 'sample')]),
]
# Reading FFFC6224 hung the bus in jig mode; those two hosts are left out of mmio-rest.
HUNG_MMIO = ('sdhci-fffc6200', 'sdhci-fffca200', 'sd-wrap-fffc6000')
F7_ORIGINAL = ROOT / 'research/real-camera/f7-original.json'
# Raw captures hold the serial number and RAM dumps: they stay local, outside research/.
CAPTURES = ROOT / '.temp/real-camera'


def read_ram(cam, address, size):
    data = bytes(cam.jig.debug_read_ram(address, size))
    if len(data) != size:
        raise RuntimeError(f'short RAM read at {address:#x}: {len(data)}/{size}')
    return data


def dump(cam, log, address, size, mode, deadline):
    for chunk in range(address, address + size, 16 * MIB):
        length = min(16 * MIB, address + size - chunk)
        full = mode == 'full' or (mode == 'budget' and time.time() + length / cam.rate < deadline)
        data = bytearray()
        for offset in range(0, length if full else min(64 * 1024, length), 64 * 1024):
            data += read_ram(cam, chunk + offset, min(64 * 1024, length - offset))
        name = f'ram-{chunk:08x}' + ('' if full else '-sample64K') + '.bin'
        log.blob(name, bytes(data))
        print(f'    {chunk:#010x} {len(data) // 1024} KiB' + ('' if full else '（抽样）'))


def phase_extra(cam, log, name):
    jig = cam.jig
    if name == 'image':
        dumped = log.out / 'ram-01000000.bin'
        stream = ROOT / '.temp/research-archive/extracted/stream-00-00260274.bin'
        if dumped.exists() and stream.exists():
            image = dumped.read_bytes()[0x21000:]
            reference = stream.read_bytes()
            same = reference == image[:len(reference)]
            log.op(kind='image-compare', matchesStream00=same)
            print('    RAM 主映像与 DAT 解压结果一致：', same)
    elif name == 'syslog':
        log_data, empty = bytearray(), 0
        while len(log_data) < 16 * MIB and empty < 3:
            r = bytes(jig.debug_read_log(0x100))
            empty = empty + 1 if not r else 0
            log_data += r
        log.blob('syslog.bin', bytes(log_data))
        print(f'    调试日志 {len(log_data)} bytes')
    elif name == 'alias':
        low, high = log.out / 'ram-1e1b0000.bin', log.out / 'ram-3e1b0000.bin'
        if low.exists() and high.exists():
            alias = low.read_bytes() == high.read_bytes()
            log.op(kind='dram-alias-1e1b-3e1b', identical=alias)
            print('    0x1E1B0000 与 0x3E1B0000 相同（可能是 512 MiB 镜像）：', alias)
    elif name in ('mmio', 'mmio-rest'):
        done = {r['name'] for r in cam.done_mmio} if name == 'mmio-rest' else set()
        for reg, address in MMIO:
            if name == 'mmio-rest' and (reg in done or reg.startswith(HUNG_MMIO)):
                continue
            log.op(kind='mmio-try', name=reg, address=address)
            log.op(kind='mmio', name=reg, address=address, value=read_ram(cam, address, 4).hex())
        log.op(kind='mmio-try', name='input-buffer-sram', address=0xfff10000)
        log.blob('sram-fff10000.bin', read_ram(cam, 0xfff10000, 0xe000))
        print('    寄存器与 SRAM 56 KiB 已读取')


def debug_phases(cam, log, args):
    jig = cam.jig
    old = jig.get_config_usb_debug()
    if not F7_ORIGINAL.exists():
        F7_ORIGINAL.write_text(json.dumps({'value': old, 'session': str(log.out)}) + '\n')
    original = json.loads(F7_ORIGINAL.read_text())['value']
    log.op(kind='f7-before', value=old, original=original)
    cam.done_mmio = [json.loads(l) for f in sorted(CAPTURES.glob('session-*/*/ops.jsonl'))
                     for l in f.open() if '"kind": "mmio"' in l]
    names = [p[0] for p in DEBUG_PHASES]
    first = names.index(args.start_phase) if args.start_phase else 0
    deadline = time.time() + args.ram_minutes * 60
    print(f'\n【无人值守阶段】可以离开电脑。数据区最多读 {args.ram_minutes:.0f} 分钟，全程约 1 小时，风险高的放在最后。')
    try:
        jig.set_config_usb_debug(1)
        t = time.time()
        read_ram(cam, 0x01021000, 64 * 1024)
        cam.rate = 64 * 1024 / max(time.time() - t, 1e-6)
        log.op(kind='ram-rate', bytesPerSecond=cam.rate)
        print(f'RAM 读取速度 {cam.rate / 1024:.1f} KiB/s')
        for name, text, regions in DEBUG_PHASES[first:]:
            print(f'\n阶段 {name}：{text}（若在此卡住：拔电池，重进 jig 后加 --skip-a --start-phase 下一阶段）')
            log.op(kind='phase-start', phase=name)
            for address, size, mode in regions:
                dump(cam, log, address, size, mode, deadline)
            phase_extra(cam, log, name)
            log.op(kind='phase-done', phase=name)
    finally:
        try:
            jig.set_config_usb_debug(original)
            log.op(kind='f7-restored', value=jig.get_config_usb_debug())
            print('0xF7 已恢复为', jig.get_config_usb_debug())
        except Exception as e:
            log.op(kind='f7-restore-failed', error=repr(e))
            print(f'0xF7 未能恢复（{e}）。拔电池重进 jig 后运行 --skip-a --start-phase <下一阶段>，结束时会恢复为 {original}。')


def connect(args):
    if args.dry_run:
        from fake_jig import FakeJig
        return FakeJig(), None
    import usb1, fftlib, ffjlib
    context = usb1.USBContext()
    handle = context.openByVendorIDAndProductID(0x04cb, 0xff80, skip_on_error=True)
    if handle is None:
        sys.exit('没找到 04cb:ff80。确认相机已用“快门+上键”开机进入 jig 模式并接好 USB。')
    handle.claimInterface(0)
    fft = fftlib.ftl(handle)
    fft.open_session()
    return ffjlib.jig(fft), (context, handle, fft)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', default=str(CAPTURES / time.strftime('%Y%m%d-%H%M%S')))
    parser.add_argument('--interactive-only', action='store_true', help='skip the debug (0xF7) phases')
    parser.add_argument('--skip-a', action='store_true', help='skip the interactive part')
    parser.add_argument('--recheck', action='store_true', help='short follow-up step list, no config dump')
    parser.add_argument('--start-phase', choices=[p[0] for p in DEBUG_PHASES])
    parser.add_argument('--ram-minutes', type=float, default=45)
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--auto', action='store_true', help='no prompts (dry-run self test)')
    parser.add_argument('--scale', type=float, default=1.0)
    args = parser.parse_args()
    log = Log(Path(args.out))
    log.op(kind='start', argv=sys.argv)
    jig, handles = connect(args)
    cam = Camera(jig, log)
    cam.auto, cam.scale, cam.recheck = args.auto, args.scale, args.recheck
    print('输出目录：', log.out)
    try:
        if not args.skip_a:
            tier_a(cam, log, args)
        if not args.interactive_only:
            debug_phases(cam, log, args)
    except KeyboardInterrupt:
        log.op(kind='interrupted')
        print('\n已中断，已保存到', log.out)
    except Exception as e:
        log.op(kind='error', error=repr(e))
        print('\n出错：', e, '\n已保存到', log.out, '\n如果相机无响应：取出电池再装回。')
        raise
    finally:
        if handles:
            try:
                handles[2].close_session()
            except Exception as e:
                log.op(kind='close-error', error=repr(e))
    log.op(kind='done')
    print('\n完成。数据在', log.out)


if __name__ == '__main__':
    main()
