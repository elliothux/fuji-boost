export interface Profile {
  id: string
  model: string
  version: string
  processor: string
  architecture: string
  size: number
  sha256: string
}

export interface Segment {
  index: number
  offset: number
  packedSize: number
  size: number
  sha256: string
  base: number | null
  entry: number | null
  initializedSize: number | null
}

export interface Inspection {
  profile: Profile | null
  family: string | null
  sha256: string
  size: number
  containerType: number
  version: string
  payloadOffset: number
  segments: Segment[]
  decodedSize: number
}

export interface Execution {
  guestHalt: string | null
  quiescent?: boolean
  outputs: { output: number; sequence: number; width: number; height: number }[]
  cpu: {
    abort_failure?: Record<string, unknown>
    instructions: number
    virtual_ticks: number
    failure: string | null
  }
  error: string | null
}

export type ExecutionMode = 'strict' | 'preloaded' | 'calibration' | 'setup' | 'reprogram'

export function isExecutionMode(value: unknown): value is ExecutionMode {
  return value === 'strict' || value === 'preloaded' || value === 'calibration' || value === 'setup' || value === 'reprogram'
}
export const VIRTUAL_BUTTON_INPUT_IDS = { up: 0x9f, down: 0xa0, left: 0xa1, right: 0xa2, ok: 0x0f, back: 0x11 } as const
export type VirtualButton = keyof typeof VIRTUAL_BUTTON_INPUT_IDS
export function isVirtualButton(value: unknown): value is VirtualButton {
  return typeof value === 'string' && Object.hasOwn(VIRTUAL_BUTTON_INPUT_IDS, value)
}
export interface GuestFrame { output: number; sequence: number; width: number; height: number; buffer: ArrayBuffer }

export type Scene =
  | { source: 'gray' | 'analytic' }
  | { source: 'rgb'; width: number; height: number; buffer: ArrayBuffer; address?: number; generation?: number }

export type CommandDialId = 'front_command_dial' | 'rear_command_dial'

export type TouchContact = { x: number; y: number }
export const TOUCH_PANEL = { width: 900, height: 600 } as const

export type Command =
  | { type: 'load'; buffer: ArrayBuffer }
  | { type: 'checkpoint'; buffer: ArrayBuffer }
  | ({ type: 'scene' } & Scene)
  | { type: 'read'; segment: number; offset: number; length: number }
  | { type: 'execute'; budget: number; mode: ExecutionMode }
  | { type: 'button'; button: VirtualButton }
  | { type: 'command-dial'; dial: CommandDialId; direction: number }
  | { type: 'gpio'; port: number; pin: number; high: boolean }
  | { type: 'adc'; channel: number; value: number }
  // A raw RAM image on the bare board (crates/xp4/src/ram_image.rs).
  | { type: 'load-image'; buffer: ArrayBuffer; loadAddress: number; entry: number }
  | { type: 'execute'; budget: number; mode: 'image'; ticks: number }
  | { type: 'button-state'; button: VirtualButton; pressed: boolean }
  // Fingers on the rear touch panel (at most two), in its 900x600 pixels.
  | { type: 'touch'; contacts: TouchContact[] }
  | { type: 'read-memory'; address: number; length: number }
  | { type: 'write-memory'; address: number; buffer: ArrayBuffer }
  | { type: 'save-checkpoint' }

export type Request = Command & { id: number }
export type Response =
  | { id: number; type: 'ok' }
  | { id: number; type: 'loaded'; report: Inspection }
  | { id: number; type: 'bytes'; bytes: Uint8Array }
  | { id: number; type: 'executed'; state: Execution; frame: GuestFrame | null; identity?: string }
  | { id: number; type: 'error'; message: string }

export const MAX_FIRMWARE_BYTES = 128 * 1024 * 1024

export function frameError(frame: Pick<GuestFrame, 'width' | 'height' | 'buffer'>): string | undefined {
  const { width, height, buffer } = frame
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0
    || width * height > 4096 * 4096) return 'Guest frame dimensions exceed the viewer limit.'
  if (buffer.byteLength !== width * height * 4) return 'Guest frame RGBA byte count does not match its dimensions.'
}

export function firmwareError(file: Pick<File, 'size' | 'name'>): string | undefined {
  if (!/\.dat$/i.test(file.name)) return '请选择富士官方 .DAT 固件文件。'
  if (file.size < 524 || file.size > MAX_FIRMWARE_BYTES) return '固件大小需在 524 bytes 到 128 MiB 之间。'
}

export function hex(value: number | null): string {
  return value === null ? '未定位' : `0x${value.toString(16).padStart(8, '0')}`
}

export function size(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / 1024 / 1024).toFixed(2)} MiB`
}
