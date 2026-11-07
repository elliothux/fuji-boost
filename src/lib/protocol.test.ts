import { describe, expect, test } from 'bun:test'
import { firmwareError, frameError, hex, isExecutionMode, isVirtualButton, MAX_FIRMWARE_BYTES } from './protocol'

describe('firmware input boundary', () => {
  test('rejects oversized, empty, and non-DAT files before transfer', () => {
    expect(firmwareError({ name: 'FWUP0024.DAT', size: 51278280 })).toBeUndefined()
    expect(firmwareError({ name: 'image.bin', size: 51278280 })).toBeDefined()
    expect(firmwareError({ name: 'empty.DAT', size: 0 })).toBeDefined()
    expect(firmwareError({ name: 'large.DAT', size: MAX_FIRMWARE_BYTES + 1 })).toBeDefined()
  })
  test('keeps an unknown mapping separate from address zero', () => {
    expect(hex(null)).toBe('未定位')
    expect(hex(0)).toBe('0x00000000')
    expect(hex(0x12910c4)).toBe('0x012910c4')
  })
})

test('accepts complete RGBA frames and rejects unsafe sizes or truncated payloads', () => {
  expect(frameError({ width: 1, height: 1, buffer: new ArrayBuffer(4) })).toBeUndefined()
  expect(frameError({ width: 1, height: 1, buffer: new ArrayBuffer(3) })).toBeDefined()
  for (const [width, height] of [[0, 1], [-1, 1], [1.5, 1], [NaN, 1], [8192, 8192]]) {
    expect(frameError({ width, height, buffer: new ArrayBuffer(4) })).toBeDefined()
  }
})

test('allows only the five declared execution scenarios at the transport boundary', () => {
  for (const mode of ['strict', 'preloaded', 'calibration', 'setup', 'reprogram']) expect(isExecutionMode(mode)).toBe(true)
  for (const mode of ['discovery', 'calibration_reprogram', 'reprogram-extra', '__proto__', '', undefined, 1, { mode: 'reprogram' }]) expect(isExecutionMode(mode)).toBe(false)
})

test('accepts only the six declared virtual buttons', () => {
  for (const button of ['up', 'down', 'left', 'right', 'ok', 'back']) expect(isVirtualButton(button)).toBe(true)
  for (const button of ['__proto__', 'constructor', 'shutter', '', undefined, 1, { button: 'ok' }]) expect(isVirtualButton(button)).toBe(false)
})
