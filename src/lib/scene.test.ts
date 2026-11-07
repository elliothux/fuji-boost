import { expect, test } from 'bun:test'
import { capSize, fitSize, rgbaToRgb } from './scene'

test('packs RGB in top-row-first order and drops alpha', () => {
  expect(rgbaToRgb({ width: 2, height: 2, data: new Uint8ClampedArray([1, 2, 3, 0, 4, 5, 6, 255, 7, 8, 9, 128, 10, 11, 12, 0]) }))
    .toEqual(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]))
  expect(() => rgbaToRgb({ width: 1, height: 1, data: new Uint8ClampedArray(3) })).toThrow()
})

test('caps images by longest side and webcam frames by width without upscaling', () => {
  expect(capSize(2000, 1000, 1024)).toEqual({ width: 1024, height: 512 })
  expect(capSize(1000, 2000, 1024)).toEqual({ width: 512, height: 1024 })
  expect(capSize(100, 200, 1024)).toEqual({ width: 100, height: 200 })
  expect(capSize(1280, 1920, 640, 'width')).toEqual({ width: 640, height: 960 })
  expect(capSize(10000, 1, 1024)).toEqual({ width: 1024, height: 1 })
  expect(() => capSize(0, 600, 640)).toThrow()
})

test('fits a frame inside a max box without upscaling', () => {
  expect(fitSize(1920, 1080, 450, 300)).toEqual({ width: 450, height: 253 })
  expect(fitSize(100, 80, 450, 300)).toEqual({ width: 100, height: 80 })
  expect(fitSize(800, 800, 450, 300)).toEqual({ width: 300, height: 300 })
  expect(() => fitSize(0, 1, 450, 300)).toThrow()
})
