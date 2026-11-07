import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'
import { createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'

export default defineConfig({
  plugins: [
    {
      name: 'local-camera-files',
      configureServer(server) {
        const files = new Map([
          ['/local/FWUP0024.DAT', fileURLToPath(new URL('./FWUP0024.DAT', import.meta.url))],
          ['/local/xt4-rec.checkpoint', fileURLToPath(new URL('./.cache/xt4-rec.checkpoint', import.meta.url))],
        ])
        server.middlewares.use((request, response, next) => {
          const path = request.url?.split('?')[0] ?? ''
          if (!path.startsWith('/local/')) return next()
          // Firmware presets (manifests, images, app bundles) from `bun run firmware:build`.
          const built = path.match(/^\/local\/firmware\/((?:[a-z0-9-]+\/)*[a-z0-9-]+\.(?:bin|json|js|pak))$/)
          const file = built ? fileURLToPath(new URL(`./firmware/dist/${built[1]}`, import.meta.url)) : files.get(path)
          if (!file || !['GET', 'HEAD'].includes(request.method ?? '')) { response.statusCode = 404; response.end(); return }
          void stat(file).then(info => {
            if (!info.isFile()) { response.statusCode = 404; response.end(); return }
            response.setHeader('Content-Type', 'application/octet-stream')
            response.setHeader('Content-Length', info.size)
            response.setHeader('Cache-Control', 'no-store')
            if (request.method === 'HEAD') { response.end(); return }
            createReadStream(file).on('error', () => response.destroy()).pipe(response)
          }).catch(() => { response.statusCode = 404; response.end() })
        })
      },
    },
    react(),
    tailwindcss(),
  ],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/target/**', '**/.cache/**', '**/references/**', '**/research/**', '**/RESEARCH.md', '**/README.md'] },
  },
})
