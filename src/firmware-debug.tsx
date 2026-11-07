import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { FirmwareEngine } from './lib/engine'
import { buildAppSlot, numeric, readLog, readMailbox, wordAddress, type AppIndex, type Mailbox, type Preset } from './lib/firmware-presets'

type ImagePreset = Extract<Preset, { kind: 'image' }>
const message = (error: unknown) => error instanceof Error ? error.message : String(error)

async function fetchBytes(path: string): Promise<Uint8Array> {
  const response = await fetch(path, { cache: 'no-store' })
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`)
  return new Uint8Array(await response.arrayBuffer())
}

/** Host side of an image's self-described debug protocol (status words, log ring, app slot). */
export function FirmwareDebug({ session, preset, actions, onError }: {
  session: FirmwareEngine | null; preset: ImagePreset; actions: ReactNode; onError: (message: string) => void
}) {
  const { protocol, apps } = preset.manifest
  const logView = useRef<HTMLPreElement>(null)
  const loaded = useRef<{ name: string; sha256: string } | null>(null)
  const generation = useRef(0)
  const autoReload = useRef(true)
  const [mailbox, setMailbox] = useState<Mailbox | null>(null)
  const [log, setLog] = useState('')
  const [index, setIndex] = useState<AppIndex | null>(null)
  const [app, setApp] = useState('')
  const [hot, setHot] = useState(true)
  const [note, setNote] = useState('')
  const slot = apps?.image === preset.key && protocol?.appSlot && protocol.mailbox ? protocol.appSlot : undefined
  const indexPath = apps ? `${preset.base}${apps.index}` : ''

  async function loadApp(current: FirmwareEngine, next: AppIndex, name: string) {
    if (!slot || !protocol?.mailbox) return
    const entry = next.apps[name], folder = indexPath.slice(0, indexPath.lastIndexOf('/') + 1)
    const started = performance.now()
    const [javaScript, pack] = await Promise.all([fetchBytes(folder + entry.javaScript), fetchBytes(folder + entry.pack)])
    const bytes = buildAppSlot(slot, javaScript, pack, ++generation.current)
    await current.writeMemory(numeric(slot.address), bytes.buffer as ArrayBuffer)
    await current.writeMemory(wordAddress(protocol.mailbox, slot.reloadWord), Uint32Array.of(1).buffer)
    loaded.current = { name, sha256: entry.sha256 }; setApp(name)
    setNote(`已写入 ${name}（第 ${generation.current} 代，${(bytes.length / 1024).toFixed(0)} KiB，${((performance.now() - started) / 1000).toFixed(2)} s）`)
  }

  useEffect(() => {
    if (!session) return
    let alive = true, logTotal = 0
    const read = (address: number, length: number) => session.readMemory(address, length)
    const poll = (period: number, task: () => Promise<void>) => {
      let busy = false
      return window.setInterval(() => {
        if (busy || !alive) return
        busy = true
        task().catch(caught => { if (alive) onError(message(caught)) }).finally(() => { busy = false })
      }, period)
    }
    const timers: number[] = []
    if (protocol?.mailbox) {
      const layout = protocol.mailbox
      timers.push(poll(500, async () => { const next = await readMailbox(layout, read); if (alive) setMailbox(next) }))
    }
    if (protocol?.log) {
      const layout = protocol.log
      timers.push(poll(400, async () => {
        const next = await readLog(layout, read, logTotal)
        if (alive && next.total !== logTotal) { logTotal = next.total; setLog(previous => (previous + next.text).slice(-20_000)) }
      }))
    }
    if (slot) timers.push(poll(1000, async () => {
      const response = await fetch(indexPath, { cache: 'no-store' })
      if (!response.ok) return
      const next = await response.json() as AppIndex
      if (!alive) return
      setIndex(next)
      const active = loaded.current
      if (autoReload.current && active && next.apps[active.name] && next.apps[active.name].sha256 !== active.sha256) await loadApp(session, next, active.name)
    }))
    return () => { alive = false; timers.forEach(clearInterval); setMailbox(null); setLog(''); setNote(''); setApp(''); loaded.current = null; generation.current = 0 }
  }, [session, preset.id])
  useEffect(() => { logView.current?.scrollTo(0, logView.current.scrollHeight) }, [log])

  const shown = mailbox ? Object.entries(mailbox.words).filter(([name]) => name !== 'magic' && name !== 'status') : []
  return <section className="firmware-debug" aria-label={`${preset.label} 调试`}>
    <div className="debug-column">
      <div className="debug-actions">{actions}</div>
      {protocol?.mailbox && <>
        <h2>状态字 <span className={mailbox?.stopped ? 'debug-stopped' : ''}>{mailbox ? mailbox.valid ? mailbox.status : '未初始化' : '—'}</span></h2>
        <dl className="debug-words">
          {shown.map(([name, value]) => <div key={name}><dt>{name}</dt><dd>{value.toLocaleString()}</dd></div>)}
          {mailbox && Object.entries(mailbox.strings).map(([name, value]) => <div key={name}><dt>{name}</dt><dd>{value || '—'}</dd></div>)}
        </dl>
      </>}
    </div>
    {protocol?.log && <pre ref={logView} className="debug-log" aria-label="固件日志">{log || '（日志为空）'}</pre>}
    {slot && <div className="debug-column debug-apps">
      <h2>应用 <small>bun run firmware:app</small></h2>
      <div>{Object.keys(index?.apps ?? {}).map(name => <button key={name} disabled={!session} aria-pressed={app === name}
        onClick={() => { if (session && index) loadApp(session, index, name).catch(caught => onError(message(caught))) }}>{name}</button>)}</div>
      <label><input type="checkbox" checked={hot} onChange={event => { autoReload.current = event.currentTarget.checked; setHot(event.currentTarget.checked) }} />bundle 变化时自动热重载</label>
      {note && <p>{note}</p>}
    </div>}
  </section>
}
