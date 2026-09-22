import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

const here = import.meta.dirname
const host = process.env.TAURI_DEV_HOST

// Each window is its own HTML entry point. `root: 'src'` keeps the dev URLs
// short (/popup/, /overlay/) so they match the paths in tauri.conf.json — at the
// cost of having to point the Svelte plugin back at the config in the repo root.
export default defineConfig({
  root: 'src',
  plugins: [svelte({ configFile: resolve(here, 'svelte.config.js') })],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'chrome105',
    // Boolean, not 'esbuild': Vite 8 minifies with oxc and no longer ships esbuild.
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      input: {
        settings: resolve(here, 'src/settings/index.html'),
        popup: resolve(here, 'src/popup/index.html'),
        overlay: resolve(here, 'src/overlay/index.html'),
      },
    },
  },
})
