import { mkdir } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import soc from '../references/fujimod/catalog/soc/xp4/soc.json'
import body from '../references/fujimod/catalog/bodies/xt4/body.json'
import inputs from '../references/fujimod/configurations/emulator/xt4-discovery/hardware-inputs.json'
import measured from '../profiles/measured/xt4.json'
import { FUJIMOD_COMMIT } from './setup.ts'

const root = resolve(import.meta.dir, '..')
const directory = join(root, 'profiles/boards')

async function writeChanged(name: string, value: unknown) {
  const file = Bun.file(join(directory, name))
  const text = JSON.stringify(value, null, 2) + '\n'
  if (!await file.exists() || await file.text() !== text) await Bun.write(file, text)
}

export async function generateBoardProfile() {
  const app = soc.hardware.application
  const maps = soc.hardware.profiles.xp4.register_maps
  const reset = inputs.reset
  const spi = reset.spi_inputs['1']
  const power = spi.power_registers
  const sensor = reset.sensor_input
  const imaging = maps.imaging
  const ibuf = imaging.input_buffer
  const definition = {
    ram_size: measured.dram_bytes, cores: soc.processors.length, discovery: false,
    cbar: app.configuration_base,
    cache_sizes: [app.caches.l1_data_bytes, app.caches.l1_instruction_bytes, app.caches.l2_bytes],
    timer_base: maps.interval_timer.base, timer_layout: maps.interval_timer.layout,
    timer_routes: maps.interval_timer.routes.map((route) => [route.channel, route.irq]),
    gpio: reset.gpio_input.levels,
    adc: [reset.adc_input.samples, maps.adc.sample_ticks],
    temperature: [reset.temperature_input.millicelsius, maps.temperature.sample_ticks],
    io_group_base: maps.io_groups.base,
    control_requests: maps.control_requests.registers.map((address) => [address, maps.control_requests.ack_ticks]),
    spi_inputs: [4, 5].map((index) => [maps.spi[String(index) as '4' | '5'].base, reset.spi_inputs[String(index) as '4' | '5'].level === 'high']),
    // The actual X-T4 loader uses four-byte, seven-bit-address checksum frames.
    checksum_power_spi: [maps.spi['1'].base, spi.level === 'high', power.chip_select.port, power.chip_select.pin,
      power.chip_select.active_high, power.inputs, [], null, []],
    decompressor: [maps.decompressor.base, maps.decompressor.irq_route],
    dmp: [maps.dmp.base, maps.dmp.irq_route],
    lcdc: maps.lcdc.map((output) => [output.base, output.pixel_clock_ticks, ...output.irq_routes,
      output.picture_layout ?? null, output.video_layout ?? null]),
    sd_clocks: maps.sd_host_clocks.words.map((address) => [address, maps.sd_host_clocks.settle_ticks]),
    movdc: maps.movdc.map((output) => [output.base, output.irq, output.reset_ticks,
      reset.processed_frame_input.frame_period_ticks, reset.processed_frame_input.luma]),
    imaging: [
      [imaging.command_base, imaging.receiver_base, imaging.normal_irqs, imaging.error_irqs, imaging.timing_irqs],
      [sensor.frame_period_ticks, sensor.frame_active_ticks, imaging.command_ticks, imaging.reset_ticks],
      [sensor.read_gate.register, sensor.read_gate.mask],
      [ibuf.base, ibuf.irq, ibuf.reset_ticks, ibuf.sram_base, ibuf.bank_size, JSON.stringify(sensor.statistics_scene)],
    ],
  }
  if (soc.schema !== 1 || body.schema !== 1 || inputs.schema !== 1 || measured.schema !== 1 || definition.cores !== 4 || body.soc_id !== soc.id) {
    throw new Error('The pinned X-T4 / XP4 input schema changed')
  }
  await mkdir(directory, { recursive: true })
  await writeChanged('xt4-212.generated.json', definition)
  const flash = reset.serial_flash
  await writeChanged('xt4-erased-flash.generated.json', {
    ...definition,
    storage_jedec: flash.jedec_id,
    storage_geometry: [flash.geometry.erase_bytes, flash.geometry.page_bytes,
      flash.geometry.erase_ticks, flash.geometry.page_ticks],
    storage_irq: maps.serial_flash.irq_route,
    storage_dma_routes: maps.dma.irq_routes,
  })
  const sources = ['catalog/soc/xp4/soc.json', 'catalog/bodies/xt4/body.json', 'configurations/emulator/xt4-discovery/hardware-inputs.json']
  await writeChanged('provenance.generated.json', {
    upstream_commit: FUJIMOD_COMMIT,
    sources: await Promise.all(sources.map(async (path) => ({ path, sha256: new Bun.CryptoHasher('sha256').update(await Bun.file(join(root, 'references/fujimod', path)).arrayBuffer()).digest('hex') }))),
    measured: { path: 'profiles/measured/xt4.json', dram_bytes: measured.dram_bytes },
    limits: ['DRAM size is measured (profiles/measured/xt4.json); other catalog hardware inputs are declarations, not measurements of this camera.',
      'Power SPI framing overrides the catalog peer: the X-T4 loader uses the four-byte checksum protocol; RTC and writes remain unmodeled.',
      'Flash is attached only in the explicit erased-NOR diagnostic profile; JEDEC/geometry/timing are catalog declarations, not measured identity or installed contents.',
      'SD media and USB are not attached; update-to-physical-flash placement is unverified.',
      'Frame inputs are synthetic; no physical sensor equivalence is claimed.'],
  })
}

if (import.meta.main) await generateBoardProfile()
