"""Read-only capture of the X-T4 calibration banks FF50xxxx..FF53xxxx over ff80 (jig) USB debug.

Purpose: the emulator hangs when MENU moves from the TC tab to Setup, where firmware 0x11c33a8 re-enters
calibration mode 5 on units 0..3 and polls FF5x5384 until the whole word is zero. This script records
what the real hardware holds there, and — only if the normal UI runs in jig mode — how FF5x5384 and
FF5x5000 change while you make that menu move.

Reuses xt4_ff80_capture.py (transport, log, 0xF7 handling). Reads only; nothing writes flash, NVRAM or
any register. Config byte 0xF7 (USB debug) is set for the session and restored in `finally`.
"""
import argparse, json, sys, threading, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from xt4_ff80_capture import CAPTURES, F7_ORIGINAL, ROOT, Log, ask, connect, read_ram

UNITS = [0xff500000 + 0x10000 * unit for unit in range(4)]
# Offsets firmware 0x11c33a8 writes in mode 5 (see .cache/setup-hang/mode5-required-capture.json), plus the status word.
OFFSETS = [*range(0x5000, 0x505c, 4), *range(0x5200, 0x5260, 4), 0x5384, *range(0x5400, 0x545c, 4)]
POLLED = [(f'u{unit}+{offset:x}', base + offset) for offset in (0x5384, 0x5000) for unit, base in enumerate(UNITS)]


def word(cam, address):
    return int.from_bytes(read_ram(cam, address, 4), 'little')


def snapshot(cam, log, name):
    values = {}
    for unit, base in enumerate(UNITS):
        for offset in OFFSETS:
            # Logged before the read: on 2026-11-06 the first snapshot read hung the bus without a record.
            log.op(kind='read-try', address=base + offset)
            values[f'u{unit}+{offset:x}'] = f'{word(cam, base + offset):08x}'
    log.op(kind='snapshot', name=name, values=values)
    (log.out / f'{name}.json').write_text(json.dumps(values, indent=1) + '\n')
    print(f'  {name}: {len(values)} 个寄存器；状态字 5384 =', [values[f'u{u}+5384'] for u in range(4)])
    return values


def poll(cam, log, seconds):
    stop = threading.Event()
    threading.Thread(target=lambda: (sys.stdin.readline(), stop.set()), daemon=True).start()
    last, reads, start = {}, 0, time.time()
    while not stop.is_set() and time.time() - start < seconds:
        for name, address in POLLED:
            value = word(cam, address)
            reads += 1
            if last.get(name) != value:
                log.op(kind='poll-change', name=name, value=f'{value:08x}', ms=round((time.time() - start) * 1000, 2))
                print(f'  {time.time() - start:8.3f}s {name} = {value:08x}')
                last[name] = value
    elapsed = time.time() - start
    log.op(kind='poll-done', reads=reads, seconds=elapsed, readsPerSecond=reads / max(elapsed, 1e-6))
    print(f'  轮询结束：{reads} 次读取，平均 {1000 * elapsed / max(reads, 1):.2f} ms/次')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', default=str(CAPTURES / time.strftime('calib-%Y%m%d-%H%M%S')))
    parser.add_argument('--poll-seconds', type=float, default=180)
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--auto', action='store_true', help='no prompts (dry-run self test)')
    args = parser.parse_args()
    log = Log(Path(args.out))
    log.op(kind='start', argv=sys.argv)
    jig, _handles = connect(args)
    cam = type('Cam', (), {'jig': jig})()
    old = jig.get_config_usb_debug()
    if not F7_ORIGINAL.exists():
        F7_ORIGINAL.write_text(json.dumps({'value': old, 'session': str(log.out)}) + '\n')
    original = json.loads(F7_ORIGINAL.read_text())['value']
    log.op(kind='f7-before', value=old, original=original)
    try:
        jig.set_config_usb_debug(1)
        print('1/4 试读一个标定寄存器 FF505384（若这里卡住超过 30 秒：拔电池，告诉 Claude）')
        log.op(kind='probe-try', address=0xff505384)
        value = word(cam, 0xff505384)
        log.op(kind='probe', address=0xff505384, value=f'{value:08x}')
        print(f'  FF505384 = {value:08x}')
        # The LCD stays black in jig mode; the read returning at all is the health check.
        ask('  读取已返回（jig 模式黑屏属正常）。按回车继续：', args.auto)

        print('2/4 读取四个标定单元的静态快照')
        snapshot(cam, log, 'before')

        print('3/4 判断 jig 模式下菜单能否操作')
        ui = ask('  按一下 MENU/OK：相机屏幕出现菜单了吗？（jig 模式通常黑屏，输入 n）(y/n) ', args.auto).lower() == 'y' or args.auto
        log.op(kind='ui-in-jig', menu=ui)
        if ui:
            print('  开始轮询。请在相机上操作：菜单左侧标签栏 → 一路按下键到 TC → 再按一次下键进入扳手（设置），')
            print('  停 3 秒，再按上键回 TC、再按下键进设置，重复 3 次。完成后在这里按回车。')
            poll(cam, log, 1 if args.auto else args.poll_seconds)
            print('4/4 操作后的静态快照')
            snapshot(cam, log, 'after')
        else:
            print('  菜单不可用：只保留静态快照。动态时序需要另行讨论（代码注入方案，需你单独同意）。')
    finally:
        try:
            jig.set_config_usb_debug(original)
            log.op(kind='f7-restored', value=jig.get_config_usb_debug())
            print('0xF7 已恢复为', jig.get_config_usb_debug())
        except Exception as e:
            log.op(kind='f7-restore-failed', error=repr(e))
            print(f'0xF7 未能恢复（{e}）。拔电池即可清零（它不会写入 NVRAM）。')
    print('完成，数据在', log.out)


if __name__ == '__main__':
    main()
