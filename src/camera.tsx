import { useEffect, useRef, useState, type CSSProperties, type PointerEvent, type ReactNode } from 'react'
import catalog from '../references/fujimod/catalog/bodies/xt4/controls.json'
import measured from '../profiles/measured/xt4-controls.json'
import { FirmwareDebug } from './firmware-debug'
import { FirmwareEngine } from './lib/engine'
import { MANIFESTS, numeric, presetsFrom, STOCK, type Preset } from './lib/firmware-presets'
import { firmwareError, TOUCH_PANEL, type CommandDialId, type Execution, type GuestFrame, type TouchContact, type VirtualButton } from './lib/protocol'
import { analyticRgb, capSize, fitSize, grayRgb, rgbaToRgb } from './lib/scene'
import './camera.css'

type ControlId = keyof typeof catalog.controls
type DialId = keyof typeof measured.dials
type SwitchId = keyof typeof measured.switches
type Source = 'gray' | 'analytic' | 'image' | 'webcam'
const navigation: Partial<Record<ControlId, VirtualButton>> = {
  selector_up_button: 'up', selector_down_button: 'down', selector_left_button: 'left',
  selector_right_button: 'right', menu_ok_button: 'ok', disp_back_button: 'back',
}
const at = (x: number, y: number, width: number, height = width): CSSProperties => ({
  left: `${x / 10}%`, top: `${y / 7}%`, width: `${width / 10}%`, height: `${height / 7}%`,
})
const message = (error: unknown) => error instanceof Error ? error.message : String(error)
type ImagePreset = Extract<Preset, { kind: 'image' }>
const TICKS_PER_MS = 54_000
// Image sessions advance in short slices of guest time, paced to the wall clock.
const IMAGE_SLICE_TICKS = 54_000_000 / 60
const IMAGE_BUDGET = 50_000_000
// The guest samples keys at its own frame rate; even a quick tap stays down this long.
const MIN_HOLD_MS = 80

function BodyKey({ id, label, children, x, y, width = 40, height = width, disabled, hold, pulse, holdAll, title, className }: {
  id: ControlId | `focus_stick.${string}`; label?: string; children: ReactNode; x: number; y: number; width?: number; height?: number;
  className?: string; disabled: boolean; hold: (id: string, pressed: boolean) => void; pulse: (id: ControlId) => void; holdAll: boolean; title?: string;
}) {
  const held = useRef(false)
  const control = catalog.controls[id.split('.')[0] as ControlId]
  const virtual = holdAll ? undefined : navigation[id as ControlId]
  const release = () => { if (held.current) { held.current = false; hold(id, false) } }
  return <button className={className ?? "body-key"} data-control={id} style={className ? undefined : at(x, y, width, height)}
    aria-label={label ?? control.face ?? control.name}
    title={title ?? control.name} disabled={disabled}
    onPointerDown={event => {
      event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId)
      if (virtual) pulse(id as ControlId)
      else { held.current = true; hold(id, true) }
    }} onPointerUp={release} onPointerCancel={release} onLostPointerCapture={release}
    onKeyDown={event => {
      if (![' ', 'Enter'].includes(event.key) || event.repeat) return
      event.preventDefault()
      if (virtual) pulse(id as ControlId)
      else { held.current = true; hold(id, true) }
    }} onKeyUp={release} onBlur={release}
    onClick={event => { if (event.detail === 0 && !held.current) {
        if (virtual) pulse(id as ControlId)
        else { hold(id, true); window.setTimeout(() => hold(id, false), 180) }
      } }}>
    {children}
  </button>
}

function Dial({ id, label, x, y, width, ready, change }: {
  id: DialId; label: string; x: number; y: number; width: number; ready: boolean;
  change: (id: DialId, position: string) => void;
}) {
  const dial = measured.dials[id]
  const [index, setIndex] = useState(0)
  const [operated, setOperated] = useState(false)
  const position = useRef(index)
  const drag = useRef<{ x: number; moved: boolean } | null>(null)
  function step(amount: number) {
    const next = Math.max(0, Math.min(dial.positions.length - 1, position.current + amount))
    if (next === position.current && operated) return
    position.current = next; setIndex(next); setOperated(true); change(id, dial.positions[next].id)
  }
  return <div className={`body-dial ${id === 'drive_dial' ? 'drive-ring' : ''}`} style={at(x, y, width, id === 'drive_dial' ? 32 : width * .7)}>
    <span className="dial-caption">{label}</span>
    <div role="slider" tabIndex={ready ? 0 : -1} aria-label={catalog.controls[id].name}
      aria-disabled={!ready} aria-valuemin={0} aria-valuemax={dial.positions.length - 1}
      aria-valuenow={index} aria-valuetext={operated ? dial.positions[index].id.toUpperCase() : '—'}
      data-control={id} title="左右拖动或点击两侧逐档转动；方向键调节"
      onPointerDown={event => {
        if (!ready) return
        event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId)
        drag.current = { x: event.clientX, moved: false }
      }} onPointerMove={event => {
        const start = drag.current
        if (!start) return
        const steps = Math.trunc((event.clientX - start.x) / 16)
        if (steps) { start.moved = true; start.x = event.clientX; step(steps) }
      }} onPointerUp={event => {
        if (drag.current && !drag.current.moved) {
          const rect = event.currentTarget.getBoundingClientRect()
          step(event.clientX < rect.left + rect.width / 2 ? -1 : 1)
        }
        drag.current = null
      }} onPointerCancel={() => { drag.current = null }} onLostPointerCapture={() => { drag.current = null }}
      onKeyDown={event => {
        if (!ready || !['ArrowLeft', 'ArrowDown', 'ArrowRight', 'ArrowUp'].includes(event.key)) return
        event.preventDefault(); event.stopPropagation(); step(['ArrowLeft', 'ArrowDown'].includes(event.key) ? -1 : 1)
      }}>
      <span aria-hidden="true">‹</span><strong>{operated ? dial.positions[index].id.toUpperCase() : '—'}</strong><span aria-hidden="true">›</span>
    </div>
  </div>
}

function CommandDial({ id, ready, rotate }: { id: CommandDialId; ready: boolean; rotate: (id: CommandDialId, direction: number) => void }) {
  const rear = id === 'rear_command_dial'
  const label = rear ? '后' : '前'
  const button = useRef<HTMLButtonElement>(null)
  const drag = useRef<{ x: number; moved: boolean } | null>(null)
  useEffect(() => {
    const element = button.current!
    const wheel = (event: WheelEvent) => {
      if (!ready || !event.deltaY) return
      event.preventDefault(); rotate(id, event.deltaY > 0 ? 1 : -1)
    }
    element.addEventListener('wheel', wheel, { passive: false })
    return () => element.removeEventListener('wheel', wheel)
  }, [id, ready, rotate])
  return <span className="command-dial" style={rear ? at(833, 214, 45, 29) : at(880, 222, 58, 24)}>
    <button ref={button} disabled={!ready} aria-label={catalog.controls[id].name} data-control={id}
      title={`${label}拨盘：滚轮、左右拖动、点击两侧或方向键逐格转动`}
      onPointerDown={event => {
        event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId)
        drag.current = { x: event.clientX, moved: false }
      }} onPointerMove={event => {
        const start = drag.current
        if (!start) return
        const steps = Math.trunc((event.clientX - start.x) / 16)
        if (steps) {
          start.moved = true; start.x += steps * 16
          for (let step = 0; step < Math.abs(steps); step++) rotate(id, Math.sign(steps))
        }
      }} onPointerUp={event => {
        if (drag.current && !drag.current.moved) {
          const rect = event.currentTarget.getBoundingClientRect()
          rotate(id, event.clientX < rect.left + rect.width / 2 ? -1 : 1)
        }
        drag.current = null
      }} onPointerCancel={() => { drag.current = null }} onLostPointerCapture={() => { drag.current = null }}
      onKeyDown={event => {
        if (!['ArrowLeft', 'ArrowDown', 'ArrowRight', 'ArrowUp'].includes(event.key)) return
        event.preventDefault(); event.stopPropagation(); rotate(id, ['ArrowLeft', 'ArrowDown'].includes(event.key) ? -1 : 1)
      }} onClick={event => { if (event.detail === 0) rotate(id, 1) }}>
      ‹ || ›
    </button><small>{label}</small>
  </span>
}

export function Camera() {
  const engine = useRef<FirmwareEngine | null>(null)
  const lcd = useRef<HTMLCanvasElement>(null)
  const video = useRef<HTMLVideoElement>(null)
  const stream = useRef<MediaStream | null>(null)
  const source = useRef<Source>('gray')
  const imageInput = useRef<HTMLInputElement>(null)
  const captureCanvas = useRef<HTMLCanvasElement | null>(null)
  const firmware = useRef<File | null>(null)
  const checkpoint = useRef<File | null>(null)
  const label = useRef('')
  const started = useRef(false)
  const running = useRef(false)
  const inputs = useRef(Promise.resolve())
  const unthrottled = useRef(false)
  const pressedAt = useRef<Record<string, number>>({})
  const finger = useRef<{ pointer: number; at: TouchContact } | null>(null)
  const moveQueued = useRef(false)
  const sceneLayout = useRef<{ address: number; width: number; height: number } | null>(null)
  const sceneGeneration = useRef(0)
  const [presets, setPresets] = useState<Preset[]>([STOCK])
  const [manifestsRead, setManifestsRead] = useState(false)
  const [presetId, setPresetId] = useState(() => new URLSearchParams(location.search).get('firmware') ?? STOCK.id)
  const preset = presets.find(candidate => candidate.id === presetId)
  const imageMode = preset?.kind === 'image'
  const hasScene = preset?.kind === 'image' && !!preset.manifest.protocol?.scene
  const [missing, setMissing] = useState<string[]>([])
  const [status, setStatus] = useState('正在加载固件与存档…')
  const [error, setError] = useState('')
  const [ready, setReady] = useState(false)
  const [fast, setFast] = useState(false)
  const [selectedSource, setSelectedSource] = useState<Source>('gray')
  const [sceneBusy, setSceneBusy] = useState(false)
  const [positions, setPositions] = useState<Partial<Record<SwitchId, string>>>({})
  // Guest seconds per wall second, smoothed; virtual ticks run at the XP4 54 MHz guest clock.
  const pace = useRef<{ ticks: number; at: number; rate?: number }>(undefined)

  function stopWebcam() {
    stream.current?.getTracks().forEach(track => track.stop()); stream.current = null
    if (video.current) video.current.srcObject = null
  }
  function render(result: { state: Execution; frame: GuestFrame | null }) {
    const { state, frame } = result
    if (frame && lcd.current) {
      lcd.current.width = frame.width; lcd.current.height = frame.height
      lcd.current.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(frame.buffer), frame.width, frame.height), 0, 0)
    }
    const failure = state.cpu.failure ?? state.error ?? state.guestHalt
    if (failure) throw new Error(failure)
    if (state.quiescent) throw new Error('客机已静止：所有核心都在等待，且没有待处理的设备事件。')
    const now = performance.now(), last = pace.current
    const sample = last && now > last.at ? (state.cpu.virtual_ticks - last.ticks) / TICKS_PER_MS / (now - last.at) : undefined
    const rate = sample === undefined ? last?.rate : last?.rate === undefined ? sample : last.rate * 0.9 + sample * 0.1
    pace.current = { ticks: state.cpu.virtual_ticks, at: now, rate }
    setStatus(`${label.current ? `${label.current} · ` : ''}运行中${rate === undefined ? '' : ` · 实时速度 ${(rate * 100).toFixed(rate < 0.1 ? 1 : 0)}%`} · ${state.cpu.instructions.toLocaleString()} 指令`)
  }
  async function postRgb(current: FirmwareEngine, width: number, height: number, rgb: Uint8Array<ArrayBuffer>) {
    const layout = sceneLayout.current
    if (layout) {
      sceneGeneration.current += 1
      await current.setScene({
        source: 'rgb', width, height, buffer: rgb.buffer,
        address: layout.address, generation: sceneGeneration.current,
      })
    } else {
      await current.setScene({ source: 'rgb', width, height, buffer: rgb.buffer })
    }
  }
  async function sendImage(current: FirmwareEngine, image: CanvasImageSource, width: number, height: number, webcam = false) {
    const layout = sceneLayout.current
    const dimensions = layout
      ? fitSize(width, height, layout.width, layout.height)
      : capSize(width, height, webcam ? 640 : 1024, webcam ? 'width' : 'longest')
    const canvas = captureCanvas.current ??= document.createElement('canvas')
    canvas.width = dimensions.width; canvas.height = dimensions.height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('无法读取画面。')
    context.drawImage(image, 0, 0, canvas.width, canvas.height)
    const rgb = rgbaToRgb(context.getImageData(0, 0, canvas.width, canvas.height))
    await postRgb(current, dimensions.width, dimensions.height, rgb)
  }
  function fail(caught: unknown, current: FirmwareEngine) {
    if (engine.current !== current) return
    running.current = false; stopWebcam()
    setError(message(caught)); setStatus('已停止'); setReady(false)
  }
  async function start(current: FirmwareEngine) {
    if (started.current || !firmware.current) return
    started.current = true; setError(''); setMissing([]); pace.current = undefined
    try {
      const invalid = firmwareError(firmware.current)
      if (invalid) throw new Error(invalid)
      setStatus('正在加载固件…')
      const inspection = await current.load(await firmware.current.arrayBuffer())
      if (inspection.profile?.id !== 'xt4-212' && inspection.family !== 'xt4-212') throw new Error('请选择 X-T4 2.12 固件。')
      if (engine.current !== current) return
      const modded = inspection.profile?.id !== 'xt4-212'
      label.current = modded ? '改装 DAT 冷启动' : ''
      if (!modded && !checkpoint.current) {
        started.current = false; setMissing(['checkpoint']); setStatus('请选择 REC 存档'); return
      }
      setStatus(modded ? '改装 DAT 冷启动，请完成首次设置…' : '正在恢复拍摄画面…')
      const result = modded ? await current.execute(1, 'reprogram')
        : await current.loadCheckpoint(await checkpoint.current!.arrayBuffer())
      if (engine.current !== current) return
      render(result); running.current = true; setReady(true)
      const tick = async () => {
        if (engine.current !== current || !running.current) return
        try {
          if (source.current === 'webcam' && video.current && video.current.readyState >= 2) {
            await sendImage(current, video.current, video.current.videoWidth, video.current.videoHeight, true)
          }
          const next = await current.execute(1_000_000, 'reprogram')
          if (engine.current !== current) return
          render(next)
          // setTimeout keeps the camera running in background tabs, where animation frames pause.
          window.setTimeout(() => { void tick() }, 0)
        } catch (caught) { fail(caught, current) }
      }
      window.setTimeout(() => { void tick() }, 0)
    } catch (caught) {
      if (engine.current !== current) return
      started.current = false; setMissing(['firmware', 'checkpoint']); fail(caught, current)
    }
  }
  async function startImage(current: FirmwareEngine, image: ImagePreset, signal: AbortSignal) {
    started.current = true; label.current = image.label
    try {
      setStatus(`正在加载 ${image.label}…`)
      const response = await fetch(image.base + image.image.file, { signal, cache: 'no-store' })
      if (!response.ok) throw new Error(`${image.image.file}: HTTP ${response.status}，请先运行 bun run firmware:build`)
      const bytes = await response.arrayBuffer()
      if (engine.current !== current) return
      const loaded = await current.loadImage(bytes, numeric(image.image.loadAddress), numeric(image.image.entry))
      if (engine.current !== current) return
      render(loaded); running.current = true; setReady(true)
      const layout = sceneLayout.current
      if (layout) await postRgb(current, layout.width, layout.height, grayRgb(layout.width, layout.height))
      const clock = { wall: performance.now(), ticks: loaded.state.cpu.virtual_ticks }
      while (engine.current === current && running.current) {
        if (source.current === 'webcam' && video.current && video.current.readyState >= 2) {
          await sendImage(current, video.current, video.current.videoWidth, video.current.videoHeight, true)
        }
        const result = await current.run(IMAGE_SLICE_TICKS, IMAGE_BUDGET)
        if (engine.current !== current) return
        render(result)
        // Hold guest time to the wall clock; never bank lag, and re-anchor after a checkpoint jump.
        const now = performance.now()
        let ahead = (result.state.cpu.virtual_ticks - clock.ticks) / TICKS_PER_MS - (now - clock.wall)
        if (unthrottled.current || Math.abs(ahead) > 250) { clock.wall = now; clock.ticks = result.state.cpu.virtual_ticks; ahead = 0 }
        // setTimeout keeps the camera running in background tabs, where animation frames pause.
        await new Promise(resolve => window.setTimeout(resolve, Math.max(0, ahead)))
      }
    } catch (caught) {
      if (engine.current !== current) return
      started.current = false; fail(caught, current)
    }
  }
  useEffect(() => {
    const abort = new AbortController()
    void Promise.all(MANIFESTS.map(async url => {
      const response = await fetch(url, { signal: abort.signal, cache: 'no-store' })
      if (response.status === 404) return []
      if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`)
      return presetsFrom(url, await response.json())
    })).then(lists => setPresets([STOCK, ...lists.flat()]))
      .catch(caught => { if (!abort.signal.aborted) setError(message(caught)) })
      .finally(() => { if (!abort.signal.aborted) setManifestsRead(true) })
    return () => abort.abort()
  }, [])
  useEffect(() => {
    if (!preset) setStatus(manifestsRead ? `找不到固件预设 ${presetId}，请在右侧选择` : '正在读取固件清单…')
  }, [preset, presetId, manifestsRead])
  useEffect(() => {
    if (!preset) return
    const current = new FirmwareEngine(); engine.current = current; started.current = false; inputs.current = Promise.resolve(); finger.current = null
    const scene = preset.kind === 'image' ? preset.manifest.protocol?.scene : undefined
    sceneLayout.current = scene ? { address: numeric(scene.address), width: scene.maxWidth, height: scene.maxHeight } : null
    sceneGeneration.current = 0
    setReady(false); setError(''); setMissing([]); setPositions({}); source.current = 'gray'; setSelectedSource('gray'); pace.current = undefined
    const abort = new AbortController()
    const cleanup = () => { running.current = false; const active = engine.current; engine.current = null; abort.abort(); stopWebcam(); active?.dispose() }
    if (preset.kind === 'image') {
      void startImage(current, preset, abort.signal)
      return cleanup
    }
    label.current = ''
    void Promise.all(['FWUP0024.DAT', 'xt4-rec.checkpoint'].map(async (name, index) => {
      const response = await fetch(`/local/${name}`, { signal: abort.signal })
      if (response.status === 404) return index === 0 ? 'firmware' : 'checkpoint'
      if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`)
      const file = new File([await response.blob()], name)
      if (engine.current !== current) return
      if (index === 0) firmware.current ??= file
      else checkpoint.current = file
    })).then(absent => {
      if (engine.current !== current || started.current) return
      const names = absent.filter(name => name !== undefined)
      setMissing(names)
      if (names.length) setStatus('选择本地文件以开始拍摄')
      else void start(current)
    }).catch(caught => fail(caught, current))
    return cleanup
  }, [preset])

  function choosePreset(id: string) {
    if (id === presetId) return
    const url = new URL(location.href)
    if (id === STOCK.id) url.searchParams.delete('firmware')
    else url.searchParams.set('firmware', id)
    history.replaceState(null, '', url)
    setPresetId(id)
  }

  async function pick(file: File, kind: string) {
    if (kind === 'firmware') {
      const invalid = firmwareError(file)
      if (invalid) { setError(invalid); return }
      firmware.current = file
      if (started.current && engine.current) {
        running.current = false; stopWebcam(); engine.current.dispose()
        engine.current = new FirmwareEngine(); started.current = false; inputs.current = Promise.resolve()
        setReady(false); source.current = 'gray'; setSelectedSource('gray'); setPositions({})
      }
    } else checkpoint.current = file
    setMissing(previous => previous.filter(name => name !== kind))
    if (engine.current) await start(engine.current)
  }
  function act(action: (current: FirmwareEngine) => Promise<unknown>) {
    const current = engine.current
    if (!current || !ready) return
    inputs.current = inputs.current.then(async () => {
      if (engine.current === current) await action(current)
    }).catch(caught => fail(caught, current))
  }
  function pulse(id: ControlId) {
    const button = navigation[id]
    if (button) act(async current => {
      const result = await current.pressButton(button)
      if (engine.current === current) render(result)
    })
  }
  function hold(id: string, pressed: boolean) {
    const pins = measured.buttons[id as keyof typeof measured.buttons]
    if (imageMode) {
      const button = navigation[id as ControlId]
      if (!button && !pins) return
      const apply = (current: FirmwareEngine) => button ? current.setButton(button, pressed)
        : Promise.all(pins.map(([port, pin]) => current.setGpioInput(port, pin, !pressed)))
      if (pressed) { pressedAt.current[id] = performance.now(); act(apply); return }
      const wait = (pressedAt.current[id] ?? 0) + MIN_HOLD_MS - performance.now()
      window.setTimeout(() => act(apply), Math.max(0, wait))
      return
    }
    if (pins) act(async current => {
      // Let a short click reach the guest before releasing its active-low lines.
      try {
        if (!pressed) {
          const result = await current.execute(1_000_000, 'reprogram')
          if (engine.current === current) render(result)
        }
      } finally {
        for (const [port, pin] of pins) await current.setGpioInput(port, pin, !pressed)
      }
    })
  }
  function panelPoint(event: PointerEvent<HTMLCanvasElement>): TouchContact {
    const box = event.currentTarget.getBoundingClientRect()
    const scale = (offset: number, size: number, pixels: number) =>
      Math.min(pixels - 1, Math.max(0, Math.floor(offset / size * pixels)))
    return {
      x: scale(event.clientX - box.left, box.width, TOUCH_PANEL.width),
      y: scale(event.clientY - box.top, box.height, TOUCH_PANEL.height),
    }
  }
  // One finger. Down and up are always sent, in order; moves coalesce to
  // the latest position so a drag cannot flood the input queue.
  function touchDown(event: PointerEvent<HTMLCanvasElement>) {
    if (!imageMode || !ready || finger.current || event.button !== 0) return
    event.currentTarget.setPointerCapture(event.pointerId)
    const at = panelPoint(event)
    finger.current = { pointer: event.pointerId, at }
    act(current => current.setTouch([at]))
  }
  function touchMove(event: PointerEvent<HTMLCanvasElement>) {
    if (finger.current?.pointer !== event.pointerId) return
    finger.current.at = panelPoint(event)
    if (moveQueued.current) return
    moveQueued.current = true
    act(async current => {
      moveQueued.current = false
      if (finger.current) await current.setTouch([finger.current.at])
    })
  }
  function touchUp(event: PointerEvent<HTMLCanvasElement>) {
    if (finger.current?.pointer !== event.pointerId) return
    finger.current = null
    act(current => current.setTouch([]))
  }
  function dialChange(id: DialId, position: string) {
    const dial = measured.dials[id]
    const sample = dial.positions.find(p => p.id === position)!
    act(current => current.setAdcInput(dial.channel, sample.value))
  }
  function rotateDial(id: CommandDialId, direction: number) {
    act(async current => {
      const result = await current.rotateCommandDial(id, direction)
      if (engine.current === current) render(result)
    })
  }
  function switchChange(id: SwitchId, position: string) {
    setPositions(previous => ({ ...previous, [id]: position }))
    const values = measured.switches[id] as Record<string, (number | boolean)[][]>
    act(async current => { for (const [port, pin, high] of values[position]) await current.setGpioInput(Number(port), Number(pin), Boolean(high)) })
  }
  useEffect(() => {
    const keys: Record<string, ControlId> = { ArrowUp: 'selector_up_button', ArrowDown: 'selector_down_button', ArrowLeft: 'selector_left_button', ArrowRight: 'selector_right_button', Enter: 'menu_ok_button', Backspace: 'disp_back_button' }
    const key = (event: KeyboardEvent) => {
      if (event.target instanceof Element && event.target.closest('button, input, [role="slider"]')) return
      const id = keys[event.key]
      if (!id || !ready) return
      event.preventDefault()
      if (event.repeat) return
      if (imageMode) hold(id, event.type === 'keydown')
      else if (event.type === 'keydown') pulse(id)
    }
    window.addEventListener('keydown', key); window.addEventListener('keyup', key)
    return () => { window.removeEventListener('keydown', key); window.removeEventListener('keyup', key) }
  }, [ready, imageMode])

  async function saveCheckpoint(current: FirmwareEngine) {
    const bytes = await current.saveCheckpoint()
    const link = document.createElement('a')
    link.href = URL.createObjectURL(new Blob([bytes as BlobPart]))
    link.download = `${presetId.replaceAll('/', '-')}.checkpoint`; link.click()
    URL.revokeObjectURL(link.href)
  }
  function restoreCheckpoint(file: File) {
    act(async current => {
      const result = await current.loadCheckpoint(await file.arrayBuffer())
      if (engine.current === current) render(result)
    })
  }

  async function changeSource(next: Source, file?: File) {
    if (next === 'image' && !file) { imageInput.current?.click(); return }
    const current = engine.current
    if (!current || !ready || sceneBusy) return
    setSceneBusy(true); setError('')
    try {
      if (next === 'image' && file) {
        const image = await createImageBitmap(file)
        source.current = 'gray'
        try { await sendImage(current, image, image.width, image.height) } finally { image.close() }
        stopWebcam()
      } else {
        source.current = 'gray'; stopWebcam()
        if (next === 'webcam') {
          const media = await navigator.mediaDevices.getUserMedia({ video: true })
          if (engine.current !== current) { media.getTracks().forEach(track => track.stop()); return }
          stream.current = media
          if (!video.current) throw new Error('摄像头预览不可用。')
          video.current.srcObject = media; await video.current.play()
        } else {
          const layout = sceneLayout.current
          if (layout) {
            const rgb = next === 'gray' ? grayRgb(layout.width, layout.height) : analyticRgb(layout.width, layout.height)
            await postRgb(current, layout.width, layout.height, rgb)
          } else await current.setScene({ source: next as 'gray' | 'analytic' })
        }
      }
      if (engine.current === current) { source.current = next; setSelectedSource(next) }
    } catch (caught) {
      if (engine.current !== current) return
      stopWebcam(); source.current = 'gray'; setSelectedSource('gray')
      const layout = sceneLayout.current
      if (layout) await postRgb(current, layout.width, layout.height, grayRgb(layout.width, layout.height)).catch(() => {})
      else await current.setScene({ source: 'gray' }).catch(() => {})
      setError(message(caught))
    } finally { if (engine.current === current) setSceneBusy(false) }
  }
  const key = (id: ControlId, children: ReactNode, x: number, y: number, width = 40, height = width, label?: string, title?: string) =>
    <BodyKey key={id} {...{ id, children, x, y, width, height, label, title, hold, pulse }} holdAll={imageMode} disabled={!ready} />
  const selector = (id: SwitchId, x: number, y: number, width: number) => <div className="body-switch" style={at(x, y, width, 28)} aria-label={catalog.controls[id].name}>
    {catalog.controls[id].positions.map(p => <button key={p.id} disabled={!ready} aria-pressed={positions[id] === p.id} onClick={() => switchChange(id, p.id)}>{p.label}</button>)}
  </div>
  const imageActions = <>
    <label><input type="checkbox" checked={fast} onChange={event => { unthrottled.current = event.currentTarget.checked; setFast(event.currentTarget.checked) }} />不限速</label>
    <button disabled={!ready} onClick={() => act(saveCheckpoint)}>保存存档</button>
    <label className={ready ? 'debug-button' : 'debug-button disabled'}>恢复存档<input type="file" accept=".checkpoint" hidden disabled={!ready}
      onChange={event => { const file = event.currentTarget.files?.[0]; event.currentTarget.value = ''; if (file) restoreCheckpoint(file) }} /></label>
  </>
  return <main className="camera-page">
    <header className="camera-heading"><span>FUJIFILM</span><h1>X-T4</h1><p>拿起相机，开始拍摄。</p></header>
    <div className="camera-stage">
      <div className="camera-body" key={presetId} aria-label="X-T4 相机机身">
        <svg className="body-drawing" viewBox="0 0 1000 700" aria-hidden="true">
          <defs>
            <linearGradient id="body-metal" x2="0" y2="1"><stop stopColor="#44484a"/><stop offset=".3" stopColor="#202224"/><stop offset="1" stopColor="#121314"/></linearGradient>
            <pattern id="leather" width="5" height="5" patternUnits="userSpaceOnUse"><path d="m0 0 2 1M3 3l2 1" stroke="#505152" strokeWidth=".55"/></pattern>
          </defs>
          <path d="M50 186Q43 163 68 151L90 127H333L365 64H542L580 127H889Q951 133 955 194L966 596Q967 647 919 655H82Q38 653 38 607Z" fill="url(#body-metal)" stroke="#090a0b" strokeWidth="5"/>
          <path d="M48 266H950L960 595Q962 644 916 646H84Q47 644 47 604Z" fill="#232526"/>
          <path d="M48 266H950L960 595Q962 644 916 646H84Q47 644 47 604Z" fill="url(#leather)"/>
          <path d="M841 229Q930 204 944 272L951 570Q944 603 908 610H856Z" fill="#181a1b" stroke="#111" strokeWidth="3"/>
          <path d="M333 131L363 64H543L578 131" fill="#292c2e" stroke="#56585a" strokeWidth="2"/>
          <rect x="360" y="112" width="191" height="112" rx="30" fill="#101112" stroke="#080909" strokeWidth="9"/>
          <rect x="391" y="136" width="128" height="62" rx="15" fill="#222932" stroke="#4b4d50" strokeWidth="2"/>
          <rect x="410" y="146" width="90" height="40" rx="10" fill="#101922"/>
          <rect x="104" y="265" width="588" height="379" rx="13" fill="#0c0d0e" stroke="#494c4f" strokeWidth="3"/>
          <path d="M112 283h-10v324h10M108 635h578" fill="none" stroke="#55585a" strokeWidth="2"/>
          <rect x="127" y="275" width="540" height="360" rx="2" fill="#060707"/>
          <text x="455" y="99" textAnchor="middle" fill="#cacbcc" fontSize="17" fontWeight="700" letterSpacing="2">FUJIFILM</text>
          <text x="77" y="244" fill="#c9cbcc" fontSize="20" fontWeight="600">X-T4</text>
          <text x="397" y="652" textAnchor="middle" fill="#7d8082" fontSize="11" letterSpacing="3">FUJIFILM</text>
          <circle cx="875" cy="634" r="4" fill="#3d4940"/>
          <path d="M37 204H20v48h20M959 204h20v48h-20" fill="#373a3c" stroke="#080909" strokeWidth="5"/>
        </svg>
        <canvas ref={lcd} width="900" height="600" className={imageMode ? 'camera-lcd touchable' : 'camera-lcd'}
          aria-label={imageMode ? '相机固件实际输出的显示帧（触摸屏）' : '相机固件实际输出的显示帧'}
          onPointerDown={touchDown} onPointerMove={touchMove} onPointerUp={touchUp}
          onPointerCancel={touchUp} onLostPointerCapture={touchUp} />
        <Dial id="sensitivity_dial" label="ISO" x={112} y={104} width={146} ready={ready} change={dialChange}/>
        <Dial id="drive_dial" label="DRIVE" x={115} y={210} width={143} ready={ready} change={dialChange}/>
        <Dial id="shutter_speed_dial" label="快门速度" x={588} y={104} width={146} ready={ready} change={dialChange}/>
        <Dial id="exposure_compensation_dial" label="± EV" x={768} y={105} width={108} ready={ready} change={dialChange}/>
        {selector('still_movie_mode_dial', 586, 212, 152)}
        {selector('focus_mode_selector', 62, 563, 65)}
        {key('shutter_button', '●', 890, 123, 48, 34, '快门（全按）', '仅全按：GPIO0.0 + GPIO0.1；半按未在 GPIO/ADC 上测得')}
        {key('fn1_button', 'Fn', 894, 180, 31)}
        {key('view_mode_button', <>VIEW<br/>MODE</>, 553, 163, 32, 42, 'VIEW MODE')}
        {key('playback_button', '▶', 282, 230, 33, 28, 'PLAY')}
        {key('ae_lock_button', 'AE-L', 702, 264, 45, 31, 'AE-L')}
        {key('af_on_button', 'AF-ON', 767, 264, 54, 31, 'AF-ON')}
        {key('q_button', 'Q', 894, 314, 34)}
        <div className="focus-stick" style={at(728, 330, 64)} aria-label={catalog.controls.focus_stick.name}>
          {(['up', 'down', 'left', 'right', 'press'] as const).map(direction => <BodyKey key={direction}
            id={`focus_stick.${direction}`} className={`stick-${direction}`} label={`焦点摇杆 ${direction}`}
            x={0} y={0} width={0} disabled={!ready} hold={hold} pulse={pulse} holdAll={imageMode}>
            {direction === 'press' ? '●' : { up: '▲', down: '▼', left: '◀', right: '▶' }[direction]}
          </BodyKey>)}
        </div>
        <div className="selector-ring" style={at(716, 425, 128)} />
        {key('selector_up_button', '▲', 761, 430, 38, 30, '上')}
        {key('selector_down_button', '▼', 761, 516, 38, 30, '下')}
        {key('selector_left_button', '◀', 721, 472, 30, 38, '左')}
        {key('selector_right_button', '▶', 809, 472, 30, 38, '右')}
        {key('menu_ok_button', <>MENU<br/>OK</>, 754, 463, 52, 48, 'MENU/OK')}
        {key('disp_back_button', <>DISP<br/>BACK</>, 763, 578, 43, 37, 'DISP/BACK')}
        <CommandDial id="rear_command_dial" ready={ready && !imageMode} rotate={rotateDial}/>
        <CommandDial id="front_command_dial" ready={ready && !imageMode} rotate={rotateDial}/>
      </div>
      <aside className="scene-picker">
        <span>固件</span><div role="group" aria-label="固件" className="firmware-picker">
          {presets.map(candidate => <button key={candidate.id} aria-pressed={candidate.id === presetId} onClick={() => choosePreset(candidate.id)}>{candidate.label}</button>)}
        </div>
        {manifestsRead && presets.length === 1 && <p>bun run firmware:build 后可切换自定义固件</p>}
        <span>画面源</span><div role="group" aria-label="画面源" title={imageMode && !hasScene ? '这份自定义固件没有声明 scene 缓冲' : undefined}>
          {([['gray', '灰场'], ['analytic', '测试图'], ['image', '图片'], ['webcam', '摄像头']] as const).map(([id, label]) => <button key={id} disabled={!ready || sceneBusy || (imageMode && !hasScene)} aria-pressed={selectedSource === id} onClick={() => void changeSource(id)}>{label}</button>)}
        </div><p>拖动拨盘 · 按住机身按键<br/>方向键 / Enter / Backspace</p>
      </aside>
    </div>
    <div className="camera-status" role="status">{status}</div>
    {!imageMode && <div className="camera-files">选择本地文件{['firmware', ...missing.filter(kind => kind !== 'firmware')].map(kind => <label key={kind}>{kind === 'firmware' ? 'DAT 固件' : 'checkpoint 存档'}<input type="file" accept={kind === 'firmware' ? '.DAT,.dat' : '.checkpoint'} aria-label={kind === 'firmware' ? '加载本地固件' : '读取存档文件'} onChange={event => { const file = event.currentTarget.files?.[0]; if (file) void pick(file, kind); event.currentTarget.value = '' }} /></label>)}</div>}
    {error && <p className="camera-error" role="alert">{error}</p>}
    {preset?.kind === 'image' && <FirmwareDebug session={ready ? engine.current : null} preset={preset} actions={imageActions} onError={setError} />}
    <input ref={imageInput} type="file" accept="image/*" aria-label="选择画面图片" hidden onChange={event => { const file = event.currentTarget.files?.[0]; if (file) void changeSource('image', file); event.currentTarget.value = '' }}/>
    <video ref={video} muted playsInline hidden />
    <footer className="camera-footer"><span>X-T4 · {preset?.label ?? presetId}</span></footer>
  </main>
}
