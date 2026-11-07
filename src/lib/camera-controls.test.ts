import { expect, test } from 'bun:test'
import assert from 'node:assert/strict'
import measured from '../../profiles/measured/xt4-controls.json'
import source from '../../research/real-camera/session-20261101/controls.json'
import catalog from '../../references/fujimod/catalog/bodies/xt4/controls.json'

test('camera dials retain every measured detent and only their own ADC channel', () => {
  const dials = [
    ['shutter_speed_dial', '快门速度盘', 6], ['sensitivity_dial', 'ISO 拨盘', 5],
    ['exposure_compensation_dial', '曝光补偿拨盘', 7], ['drive_dial', 'ISO 下方驱动环', 3],
  ] as const
  for (const [id, name, channel] of dials) {
    expect(id in catalog.controls).toBe(true)
    const dial = measured.dials[id]
    expect(dial.channel).toBe(channel)
    assert.deepEqual(dial.positions.map(p => p.value), source.dialPositionsAdc[name].map(([, vector]) => vector[channel]))
    expect(dial.positions.every(p => Number.isInteger(p.value) && p.value >= 0 && p.value <= 4095)).toBe(true)
  }
})

test('body buttons preserve the measured active-low GPIO pins, including both full-shutter lines', () => {
  const buttons = {
    shutter_button: 'shutter_full', q_button: 'q', af_on_button: 'af_on', ae_lock_button: 'ae_l',
    playback_button: 'play', fn1_button: 'fn_top', view_mode_button: 'view_mode',
    'focus_stick.press': 'joystick_push', 'focus_stick.up': 'joystick_up', 'focus_stick.down': 'joystick_down',
    'focus_stick.left': 'joystick_left', 'focus_stick.right': 'joystick_right',
  } as const
  for (const [id, name] of Object.entries(buttons)) {
    const actual = measured.buttons[id as keyof typeof measured.buttons].map(([port, pin]) => `GPIO${port}.${pin}`)
    const expected = source.controls.gpio[name]
    expect(actual).toEqual(Array.isArray(expected) ? expected : [expected])
  }
  expect(measured.switches.still_movie_mode_dial.MOVIE).toEqual([[9, 6, false]])
  expect(measured.switches.focus_mode_selector.C).toEqual([[4, 7, true], [4, 8, true]])
  expect('front_command_dial' in measured.buttons).toBe(false)
  expect('rear_command_dial' in measured.buttons).toBe(false)
})
