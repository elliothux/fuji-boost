export function capSize(width: number, height: number, cap: number, axis: 'longest' | 'width' = 'longest') {
  if (![width, height, cap].every(n => Number.isFinite(n) && n > 0)) throw new Error('Invalid scene dimensions')
  const scale = Math.min(1, cap / (axis === 'width' ? width : Math.max(width, height)))
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) }
}

export function fitSize(width: number, height: number, maxWidth: number, maxHeight: number) {
  if (![width, height, maxWidth, maxHeight].every(n => Number.isFinite(n) && n > 0)) throw new Error('Invalid scene dimensions')
  const scale = Math.min(1, maxWidth / width, maxHeight / height)
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) }
}

export function fillRgb(width: number, height: number, pixel: (x: number, y: number) => [number, number, number]): Uint8Array<ArrayBuffer> {
  if (![width, height].every(n => Number.isSafeInteger(n) && n > 0)) throw new Error('Invalid scene dimensions')
  const rgb = new Uint8Array(width * height * 3)
  let i = 0
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const [r, g, b] = pixel(x, y)
      rgb[i++] = r; rgb[i++] = g; rgb[i++] = b
    }
  }
  return rgb
}

export function grayRgb(width: number, height: number) {
  return fillRgb(width, height, () => [128, 128, 128])
}

export function analyticRgb(width: number, height: number) {
  const rw = Math.max(1, width - 1), rh = Math.max(1, height - 1)
  return fillRgb(width, height, (x, y) => [Math.round(x * 255 / rw), Math.round(y * 255 / rh), 128])
}

export function rgbaToRgb({ data, width, height }: Pick<ImageData, 'data' | 'width' | 'height'>): Uint8Array<ArrayBuffer> {
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0 || data.length !== width * height * 4) throw new Error('Invalid RGBA scene')
  const rgb = new Uint8Array(width * height * 3)
  for (let src = 0, dst = 0; src < data.length; src += 4, dst += 3) {
    rgb[dst] = data[src]; rgb[dst + 1] = data[src + 1]; rgb[dst + 2] = data[src + 2]
  }
  return rgb
}
