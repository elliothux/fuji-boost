import { decodeNative, encodeNative } from './native-wire'
import type { Command, CommandDialId, Execution, ExecutionMode, GuestFrame, Inspection, Request, Response, Scene, TouchContact, VirtualButton } from './protocol'

export class FirmwareEngine {
  private socket?: WebSocket
  private ready = Promise.resolve()
  private sequence = 0
  private pending = new Map<number, { resolve: (value: Response) => void; reject: (error: Error) => void }>()
  private disposed = false

  constructor() {
    const receive = (data: Response) => {
      const request = this.pending.get(data.id)
      if (!request) return
      this.pending.delete(data.id)
      if (data.type === 'error') request.reject(new Error(data.message))
      else request.resolve(data)
    }
    const port = new URLSearchParams(globalThis.location.search).get('port') || '5199'
    if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) throw new Error('Invalid native camera port.')
    const socket = this.socket = new WebSocket(`ws://127.0.0.1:${port}`)
    socket.binaryType = 'arraybuffer'
    this.ready = new Promise<void>((resolve, reject) => {
      socket.onopen = () => resolve()
      socket.onerror = () => { const error = new Error('Native camera could not connect. Run bun run camera:native.'); reject(error); this.close(error) }
      socket.onclose = () => { const error = new Error('Native camera connection closed.'); reject(error); this.close(error) }
    })
    void this.ready.catch(() => {})
    socket.onmessage = ({ data }: MessageEvent<ArrayBuffer>) => {
      try { receive(decodeNative<Response>(data)) }
      catch (error) { this.close(error instanceof Error ? error : new Error(String(error))) }
    }
  }

  private fail(error: Error) {
    for (const request of this.pending.values()) request.reject(error)
    this.pending.clear()
  }

  private request(command: Command): Promise<Response> {
    if (this.disposed) return Promise.reject(new Error('Firmware session was closed.'))
    const id = ++this.sequence
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      const request = { ...command, id } satisfies Request
      void this.ready.then(() => {
        if (this.disposed) throw new Error('Firmware session was closed.')
        this.socket!.send(encodeNative(request))
      }).catch(error => { this.pending.delete(id); reject(error) })
    })
  }

  async load(buffer: ArrayBuffer): Promise<Inspection> {
    const response = await this.request({ type: 'load', buffer })
    if (response.type !== 'loaded') throw new Error('Unexpected firmware response.')
    return response.report
  }

  async loadCheckpoint(buffer: ArrayBuffer): Promise<{ state: Execution; frame: GuestFrame | null }> {
    const response = await this.request({ type: 'checkpoint', buffer })
    if (response.type !== 'executed') throw new Error('Unexpected checkpoint response.')
    return { state: response.state, frame: response.frame }
  }

  async setScene(scene: Scene): Promise<void> {
    const response = await this.request({ type: 'scene', ...scene })
    if (response.type !== 'ok') throw new Error('Unexpected scene response.')
  }

  async read(segment: number, offset: number, length: number): Promise<Uint8Array> {
    const response = await this.request({ type: 'read', segment, offset, length })
    if (response.type !== 'bytes') throw new Error('Unexpected memory response.')
    return response.bytes
  }

  async execute(budget: number, mode: ExecutionMode = 'strict'): Promise<{ state: Execution; frame: GuestFrame | null }> {
    const response = await this.request({ type: 'execute', budget, mode })
    if (response.type !== 'executed') throw new Error('Unexpected CPU response.')
    return { state: response.state, frame: response.frame }
  }

  async pressButton(button: VirtualButton): Promise<{ state: Execution; frame: GuestFrame | null }> {
    const response = await this.request({ type: 'button', button })
    if (response.type !== 'executed') throw new Error('Unexpected virtual button response.')
    return { state: response.state, frame: response.frame }
  }

  async setGpioInput(port: number, pin: number, high: boolean): Promise<void> {
    const response = await this.request({ type: 'gpio', port, pin, high })
    if (response.type !== 'ok') throw new Error('Unexpected GPIO response.')
  }

  async rotateCommandDial(dial: CommandDialId, direction: number): Promise<{ state: Execution; frame: GuestFrame | null }> {
    const response = await this.request({ type: 'command-dial', dial, direction })
    if (response.type !== 'executed') throw new Error('Unexpected command dial response.')
    return { state: response.state, frame: response.frame }
  }

  async setAdcInput(channel: number, value: number): Promise<void> {
    const response = await this.request({ type: 'adc', channel, value })
    if (response.type !== 'ok') throw new Error('Unexpected ADC response.')
  }

  private async executed(command: Command): Promise<{ state: Execution; frame: GuestFrame | null }> {
    const response = await this.request(command)
    if (response.type !== 'executed') throw new Error('Unexpected execution response.')
    return { state: response.state, frame: response.frame }
  }

  /** Replace the session with a raw RAM image entered on every core at `entry`. */
  loadImage(buffer: ArrayBuffer, loadAddress: number, entry: number) {
    return this.executed({ type: 'load-image', buffer, loadAddress, entry })
  }

  /** Image session: advance `ticks` of 54 MHz guest time unless `budget` instructions pass first. */
  run(ticks: number, budget: number) {
    return this.executed({ type: 'execute', mode: 'image', budget, ticks })
  }

  /** Image session: hold or release a navigation key. */
  async setButton(button: VirtualButton, pressed: boolean): Promise<void> {
    const response = await this.request({ type: 'button-state', button, pressed })
    if (response.type !== 'ok') throw new Error('Unexpected button response.')
  }

  /** Image session: the fingers now on the rear touch panel; `[]` lifts them all. */
  async setTouch(contacts: TouchContact[]): Promise<void> {
    const response = await this.request({ type: 'touch', contacts })
    if (response.type !== 'ok') throw new Error('Unexpected touch response.')
  }

  async readMemory(address: number, length: number): Promise<Uint8Array> {
    const response = await this.request({ type: 'read-memory', address, length })
    if (response.type !== 'bytes') throw new Error('Unexpected memory response.')
    return response.bytes
  }

  async writeMemory(address: number, buffer: ArrayBuffer): Promise<void> {
    const response = await this.request({ type: 'write-memory', address, buffer })
    if (response.type !== 'ok') throw new Error('Unexpected memory response.')
  }

  async saveCheckpoint(): Promise<Uint8Array> {
    const response = await this.request({ type: 'save-checkpoint' })
    if (response.type !== 'bytes') throw new Error('Unexpected checkpoint response.')
    return response.bytes
  }

  private close(error: Error) {
    if (this.disposed) return
    this.disposed = true
    this.socket?.close()
    this.fail(error)
  }

  dispose() {
    this.close(new Error('Firmware session was closed.'))
  }
}
