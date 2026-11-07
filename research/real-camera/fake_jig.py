"""Offline stand-in for ffjlib.jig, used only by xt4_ff80_capture.py --dry-run."""
import time


class FakeJig:
    def __init__(self):
        self.cfg = bytearray(0x1000)
        self.request = bytearray(33)

    def info_tags(self):
        return {0x04: 'Product Name', 0x08: 'FW Version?'}

    def get_device_info(self, tag):
        return {0x04: 'X-T4', 0x08: '2.12'}[tag]

    def get_config_data_size(self, mb=1):
        return len(self.cfg), 0x100

    def read_config_data(self, page, ofs, size, mb=1):
        return self.cfg[ofs:ofs + size]

    def app80_request(self, cmd, params=bytearray()):
        self.cfg[0x80] = cmd
        self.cfg[0x81:0x81 + len(params)] = params
        self.request = bytearray([cmd]) + bytearray(params)

    def app80_response(self):
        cmd, arg = self.request[0], self.request[1]
        value = 0x1234 + arg
        if cmd == 0x01 and arg == 7 and int(time.time()) % 4 < 2:
            value ^= 1 << 12  # a "button" that toggles every two seconds
        return bytes([cmd, arg]) + value.to_bytes(2, 'big') + bytes(29)

    def read_data_diag(self, ident):
        if ident > 2:
            raise RuntimeError('not found')
        return bytes([ident]) * 16

    def get_config_usb_debug(self):
        return self.cfg[0xf7]

    def set_config_usb_debug(self, v=1):
        old, self.cfg[0xf7] = self.cfg[0xf7], v
        return old

    def debug_read_log(self, size=0x100):
        return b''

    def debug_read_ram(self, address, size=None):
        return bytes(size or 16)
