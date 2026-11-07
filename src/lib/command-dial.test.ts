import { expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import measured from '../../profiles/measured/xt4-controls.json'

test('profile levels come from the actual front samples and encode one bit per detent', () => {
  const rows = readFileSync(new URL('../../research/real-camera/session-20261101/jig/steps.jsonl', import.meta.url), 'utf8')
    .trim().split('\n').map(line => JSON.parse(line)).filter(row => row.text?.startsWith('前拨盘 '))
  const states = measured.encoders.front_command_dial.clockwise
  expect(states).toEqual([rows[0].samples[0].adc.slice(1, 3), ...rows.slice(0, 3).map(row => row.samples[1].adc.slice(1, 3))])
  for (let i = 0; i < 4; i++) expect(states[i].filter((value, bit) => (value > 2048) !== (states[(i + 1) % 4][bit] > 2048))).toHaveLength(1)
  const rear = measured.encoders.rear_command_dial
  expect(rear.channels).toEqual([8, 9])
  expect(rear.holdInstructions).toBe(measured.encoders.front_command_dial.holdInstructions)
  expect(rear.clockwise.map(pair => pair.map(value => value > 2048))).toEqual(states.map(pair => pair.map(value => value > 2048)))
})

