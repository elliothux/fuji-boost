import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { access, mkdir, readFile } from 'node:fs/promises'
import { once } from 'node:events'
import { createServer } from 'node:net'
import { chromium, type Browser, type Page } from 'playwright-core'
import fixture from '../crates/xp4/tests/fixtures/xt4-rec.json' with { type: 'json' }
const { frontDialSha256, menuDownSha256, menuSha256, rearDialSha256, recSha256, shutterSha256, wizard } = fixture

const root = new URL('../', import.meta.url)
const output = new URL('../.cache/xt4-tests/browser/', import.meta.url)
const canvasSelector = 'canvas[aria-label="相机固件实际输出的显示帧"]'

await main()
async function main() {
  let browser: Browser | undefined
  let server: ReturnType<typeof spawn> | undefined
  let serverOutput = ''
  let nativeServer: ReturnType<typeof spawn> | undefined
  try {
    const port = await freePort()
    let url = `http://127.0.0.1:${port}`
    {
      const nativePort = await freePort()
      nativeServer = spawn('sh', ['scripts/native-camera.sh', String(nativePort)], { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] })
      await new Promise<void>((resolve, reject) => {
        let log = ''
        const timer = setTimeout(() => reject(new Error(`Native server did not start: ${log}`)), 300_000)
        const ready = (data: Buffer) => {
          log += data.toString()
          if (log.includes(`listening on ws://127.0.0.1:${nativePort}`)) { clearTimeout(timer); resolve() }
        }
        nativeServer!.stdout!.on('data', ready); nativeServer!.stderr!.on('data', ready)
        nativeServer!.once('error', error => { clearTimeout(timer); reject(error) })
        nativeServer!.once('exit', code => { clearTimeout(timer); reject(new Error(`Native server exited ${code}: ${log}`)) })
      })
      url += `/?port=${nativePort}`
    }
    const executablePath = chromium.executablePath()
    await access(executablePath).catch(() => { throw new Error(`Cached Chromium not found: ${executablePath}. No browser was downloaded.`) })
    server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] })
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`Vite did not start: ${serverOutput}`)), 30_000)
      const ready = (data: Buffer) => {
        serverOutput += data.toString()
        if (serverOutput.includes(`http://127.0.0.1:${port}`)) { clearTimeout(timer); resolve() }
      }
      server!.stdout!.on('data', ready); server!.stderr!.on('data', ready)
      server!.once('error', error => { clearTimeout(timer); reject(error) })
      server!.once('exit', code => { clearTimeout(timer); reject(new Error(`Vite exited ${code}: ${serverOutput}`)) })
    })
    browser = await chromium.launch({ executablePath, headless: true,
      args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] })
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } })
    page.setDefaultTimeout(120_000)
    const errors: string[] = []
    page.on('pageerror', error => errors.push(error.message))
    const results: Record<string, { passed: boolean; error?: string }> = {}
    let menuKeyLatencyMs: number | undefined
    const frameHashes: Record<string, string> = {}
    await mkdir(output, { recursive: true })
    async function check(name: string, action: () => Promise<void>) {
      try { await action(); results[name] = { passed: true } }
      catch (error) {
        results[name] = { passed: false, error: error instanceof Error ? error.message : String(error) }
      }
      await page.screenshot({ path: new URL(`${name}.png`, output).pathname, fullPage: true })
      console.error(`${name}: ${JSON.stringify(results[name])}`)
    }
    async function open() {
      await page.goto(url)
      // REC changes within 250 ms as AE starts; sample faster than that initial frame.
      await waitForFrame(page, recSha256, 180_000, 16)
      frameHashes.rec = recSha256
      await page.waitForFunction(() => !document.querySelector<HTMLButtonElement>('[data-control="selector_down_button"]')?.disabled)
      assert.deepEqual(await page.getByRole('alert').allTextContents(), [])
    }
    await open()
    await page.screenshot({ path: new URL('camera-rec.png', output).pathname, fullPage: true })
    const gray = await capture(page)
    assert.equal(gray.width, 900); assert.equal(gray.height, 600)
    assert(Math.max(...gray.centre) - Math.min(...gray.centre) <= 3 && gray.centre[0] > 30 && gray.centre[0] < 250)
    let analytic = gray
    await check('camera-analytic', async () => {
      await page.getByRole('group', { name: '画面源', exact: true }).getByRole('button', { name: '测试图', exact: true }).click()
      await page.waitForFunction(() => Array.from(document.querySelectorAll('button')).some(button => button.textContent === '测试图' && button.getAttribute('aria-pressed') === 'true' && !button.disabled))
      const instructions = (await capture(page)).instructions
      let previous = gray
      while (true) {
        const next = await capture(page)
        if (next.saturated > 10_000 && previous.saturated > 10_000 && next.hash !== previous.hash) { analytic = next; break }
        // The scene reaches the LCD a few sensor frames after the switch; 60M instructions is about seven frames at REC.
        assert(next.instructions - instructions <= 60_000_000, 'Two changing saturated analytic frames missing within 60M instructions')
        previous = next
        await page.waitForTimeout(16)
      }
    })
    await check('camera-webcam-image', async () => {
      const group = page.getByRole('group', { name: '画面源', exact: true })
      await group.getByRole('button', { name: '摄像头', exact: true }).click()
      await page.waitForFunction(() => document.querySelector('video')?.readyState === 4)
      const media = await page.locator('video').evaluateHandle(video => (video as HTMLVideoElement).srcObject as MediaStream)
      const instructions = (await capture(page)).instructions
      while (true) {
        const webcam = await capture(page)
        if (difference(webcam.region, analytic.region) > 50 && difference(webcam.region, gray.region) > 8) break
        assert(webcam.instructions - instructions <= 20_000_000, 'Webcam frame did not replace analytic bars within 20 batches')
        await page.waitForTimeout(16)
      }
      const png = await page.evaluate(() => {
        const canvas = document.createElement('canvas'); canvas.width = 128; canvas.height = 96
        const context = canvas.getContext('2d')!; context.fillStyle = '#dc28c8'; context.fillRect(0, 0, 128, 96)
        return canvas.toDataURL('image/png').split(',')[1]
      })
      await page.getByLabel('选择画面图片', { exact: true }).setInputFiles({ name: 'scene.png', mimeType: 'image/png', buffer: Buffer.from(png, 'base64') })
      await page.waitForFunction(() => document.querySelector('video')?.srcObject === null)
      assert(await media.evaluate(stream => stream.getTracks().every(track => track.readyState === 'ended')), 'Camera page did not stop webcam tracks')
      await media.dispose()
      await page.waitForFunction(selector => {
        const canvas = document.querySelector<HTMLCanvasElement>(selector)!
        const d = canvas.getContext('2d')!.getImageData(450, 300, 1, 1).data
        return d[0] > 180 && d[1] < 80 && d[2] > 160
      }, canvasSelector)
      await advance(page, (await capture(page)).instructions + 4_000_000)
      await page.setViewportSize({ width: 600, height: 900 })
      await page.screenshot({ path: new URL('camera-compact.png', output).pathname, fullPage: true })
      await page.setViewportSize({ width: 1440, height: 1000 })
    })
    // Each sequence starts from REC; navigation checks the exact first and second menu rows.
    await open()
    await check('camera-shutter', async () => {
      const shutter = page.getByRole('slider', { name: 'Shutter speed dial', exact: true })
      await shutter.focus(); await shutter.press('ArrowRight')
      assert.equal(await shutter.getAttribute('aria-valuetext'), '8000')
      await waitForFrame(page, shutterSha256, 600_000)
      frameHashes.shutter = shutterSha256
    })
    await open()
    await check('camera-ok', async () => {
      const started = performance.now()
      await page.getByRole('button', { name: 'MENU/OK', exact: true }).click()
      await waitForFrame(page, menuSha256, 600_000, 16)
      frameHashes.menu = menuSha256
      menuKeyLatencyMs = Math.round(performance.now() - started)
      console.error(`MENU/OK to menu frame: ${menuKeyLatencyMs}ms`)
    })
    await check('camera-down', async () => {
      assert.equal((await capture(page)).hash, menuSha256, 'Down must start at the first menu row')
      await page.getByRole('button', { name: '下', exact: true }).click()
      await waitForFrame(page, menuDownSha256, 600_000)
      frameHashes.menuDown = menuDownSha256
    })
    for (const name of ['front', 'rear']) {
      if (name === 'rear') {
        await open()
        await page.getByRole('button', { name: 'MENU/OK', exact: true }).click()
        await waitForFrame(page, menuSha256, 600_000, 16)
        await page.getByRole('button', { name: '下', exact: true }).click()
        await waitForFrame(page, menuDownSha256, 600_000)
      }
      await check(`camera-${name}-dial`, async () => {
        const before = await capture(page)
        assert.equal(before.hash, menuDownSha256, `${name} dial must start at the static second menu row`)
        const dial = page.locator(`[data-control="${name}_command_dial"]`)
        assert(await dial.isEnabled())
        if (name === 'rear') {
          const box = await dial.boundingBox()
          assert(box)
          await dial.click({ position: { x: box.width * .75, y: box.height / 2 } })
        } else { await dial.focus(); await dial.press('ArrowRight') }
        const deadline = Date.now() + 600_000
        while ((await capture(page)).hash === before.hash) {
          assert.deepEqual(await page.getByRole('alert').allTextContents(), [])
          assert(Date.now() < deadline, `${name} detent did not change the LCD within 600s`)
          await page.waitForTimeout(250)
        }
        const after = await capture(page)
        assert.notEqual(after.hash, before.hash, `One ${name} detent must change the LCD`)
        assert.equal(after.hash, name === 'front' ? frontDialSha256 : rearDialSha256, `${name} detent must match the fixture frame`)
        frameHashes[`${name}Dial`] = after.hash
      })
    }
    await check('camera-file-prompt', async () => {
      await page.route('**/local/FWUP0024.DAT', route => route.fulfill({ status: 404 }))
      await page.goto(url)
      await page.getByLabel('加载本地固件', { exact: true }).waitFor({ state: 'attached' })
      await page.getByLabel('加载本地固件', { exact: true }).setInputFiles(new URL('FWUP0024.DAT', root).pathname)
      await waitForFrame(page, recSha256, 180_000, 16)
      frameHashes.rec = recSha256
      await page.unroute('**/local/FWUP0024.DAT')
      const denied = await page.request.get(new URL('/local/controls.json', url).href)
      assert.equal(denied.status(), 404, 'Local middleware must not expose any other file')
    })
    if (process.argv.includes('--modded')) await check('camera-modded', async () => {
      let checkpointRequests = 0
      await page.route('**/local/xt4-rec.checkpoint', route => { checkpointRequests++; return route.continue() })
      await page.getByLabel('加载本地固件', { exact: true }).setInputFiles(new URL('../.cache/modded-firmware/FWUP0024-MENU.DAT', import.meta.url).pathname)
      await page.getByRole('status').filter({ hasText: '自定义固件冷启动' }).waitFor()
      const keys = { up: '上', down: '下', ok: 'MENU/OK' }
      for (const step of wizard) {
        await waitForFrame(page, step.sha256, 180_000, 16)
        await page.getByRole('button', { name: keys[step.button as keyof typeof keys], exact: true }).click()
      }
      await waitForFrame(page, recSha256, 180_000, 16)
      await page.screenshot({ path: new URL('camera-modded-rec.png', output).pathname, fullPage: true })
      await page.getByRole('button', { name: 'MENU/OK', exact: true }).click()
      const result = JSON.parse(await readFile(new URL('../.cache/modded-firmware/result.json', import.meta.url), 'utf8')) as { rec: { atRec: { menuSha256: string } } }
      await waitForFrame(page, result.rec.atRec.menuSha256, 180_000, 16)
      assert.equal(checkpointRequests, 0, 'Picking a modified DAT must not fetch a stock checkpoint')
      frameHashes.moddedMenu = result.rec.atRec.menuSha256
      await page.unroute('**/local/xt4-rec.checkpoint')
      await page.getByLabel('加载本地固件', { exact: true }).setInputFiles(new URL('FWUP0024.DAT', root).pathname)
      await waitForFrame(page, recSha256, 180_000, 16)
      assert(!(await page.getByRole('status').textContent())?.includes('自定义固件'))
    })
    assert.deepEqual(errors, [])
    console.log(JSON.stringify({ passed: Object.values(results).every(value => value.passed), results, menuKeyLatencyMs, frameHashes, backend: 'native', screenshots: output.pathname }))
    if (Object.values(results).some(value => !value.passed)) process.exitCode = 1
  } catch (error) {
    if (browser) {
      const page = browser.contexts()[0]?.pages()[0]
      if (page) { await mkdir(output, { recursive: true }); await page.screenshot({ path: new URL('failed.png', output).pathname, fullPage: true }).catch(() => {}) }
    }
    console.log(JSON.stringify({ passed: false, error: error instanceof Error ? error.message : String(error) }))
    process.exitCode = 1
  } finally {
    try { await browser?.close() }
    finally {
      for (const child of [server, nativeServer]) {
        if (!child || child.exitCode !== null || child.signalCode !== null) continue
        const exited = once(child, 'exit')
        child.kill('SIGTERM')
        const timer = setTimeout(() => child.kill('SIGKILL'), 5000)
        await exited
        clearTimeout(timer)
      }
    }
  }
}

function freePort() {
  return new Promise<number>((resolve, reject) => {
    const probe = createServer().once('error', reject).listen(0, '127.0.0.1', () => {
      const { port } = probe.address() as { port: number }
      probe.close(() => resolve(port))
    })
  })
}


async function waitForFrame(page: Page, expected: string, timeout: number, pollMs = 250) {
  const deadline = Date.now() + timeout
  // Interval polling treats an async predicate as truthy; await the digest explicitly.
  while (!await page.evaluate(async ({ selector, expected }) => {
    if (document.querySelector('[role="alert"]')) return true
    const canvas = document.querySelector<HTMLCanvasElement>(selector)
    if (!canvas || canvas.width !== 900 || canvas.height !== 600) return false
    const data = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height).data
    const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', data)), b => b.toString(16).padStart(2, '0')).join('')
    return hash === expected
  }, { selector: canvasSelector, expected })) {
    assert(Date.now() < deadline, `LCD frame ${expected} did not appear within ${timeout}ms`)
    await page.waitForTimeout(pollMs)
  }
  assert.deepEqual(await page.getByRole('alert').allTextContents(), [])
}

async function advance(page: Page, instructions: number) {
  await page.waitForFunction(instructions => {
    if (document.querySelector('[role="alert"]')) return true
    const status = document.querySelector('[role="status"]')?.textContent ?? ''
    return Number(status.match(/([\d,]+)\s*指令/)?.[1].replaceAll(',', '') ?? 0) >= instructions
  }, instructions, { timeout: 600_000 })
  assert.deepEqual(await page.getByRole('alert').allTextContents(), [])
}

async function capture(page: Page) {
  return page.evaluate(async selector => {
    const canvas = document.querySelector<HTMLCanvasElement>(selector)!
    const data = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height).data
    const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', data)), b => b.toString(16).padStart(2, '0')).join('')
    const status = document.querySelector('[role="status"]')?.textContent ?? ''
    const instructions = Number(status.match(/([\d,]+)\s*指令/)?.[1].replaceAll(',', '') ?? 0)
    const centre = Array.from(data.slice((300 * canvas.width + 450) * 4, (300 * canvas.width + 450) * 4 + 3))
    const region: number[] = []
    for (let y = 220; y < 380; y += 8) for (let x = 350; x < 550; x += 8) {
      const i = (y * canvas.width + x) * 4; region.push(data[i], data[i + 1], data[i + 2])
    }
    let saturated = 0
    for (let y = 47; y < 553; y++) for (let x = 0; x < canvas.width; x++) {
      const i = (y * canvas.width + x) * 4
      const max = Math.max(data[i], data[i + 1], data[i + 2]), min = Math.min(data[i], data[i + 1], data[i + 2])
      if (max > 180 && min < 80 && max - min > 120) saturated++
    }
    return { width: canvas.width, height: canvas.height, hash, instructions, centre, region, saturated }
  }, canvasSelector)
}

function difference(a: number[], b: number[]) {
  assert.equal(a.length, b.length)
  return a.reduce((sum, value, i) => sum + Math.abs(value - b[i]), 0) / a.length
}
