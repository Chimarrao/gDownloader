import { existsSync, readdirSync, statSync } from 'fs'
import { resolve } from 'path'
import { defineConfig } from 'electron-vite'
import vue from '@vitejs/plugin-vue'

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
function legacyOrphanGuard() {
  const forbiddenFiles = [
    'src/renderer/src/api/client.ts',
    'src/renderer/src/api/types.ts',
    'src/main/downloads-store.ts',
    'src/main/history-store.ts',
    'src/main/settings-store.ts',
  ]
  const suspiciousDirectories = [
    'src/renderer/src/api',
  ]

  return {
    name: 'legacy-orphan-guard',
    buildStart(this: { error(message: string): never }) {
      for (const file of forbiddenFiles) {
        if (existsSync(resolve(file))) {
          this.error(`Arquivo legado órfão detectado: ${file}`)
        }
      }

      for (const directory of suspiciousDirectories) {
        const absolute = resolve(directory)
        if (!existsSync(absolute) || !statSync(absolute).isDirectory()) {
          continue
        }
        const files = readdirSync(absolute).filter((entry) => !entry.startsWith('.'))
        if (files.length > 0) {
          this.error(`Diretório legado deve estar vazio ou removido: ${directory}`)
        }
      }
    },
  }
}

export default defineConfig({
  main: {
    build: {
      rollupOptions: {
        input: {
          index: resolve('src/main/index.ts')
        }
      }
    }
  },
  preload: {
    build: {
      rollupOptions: {
        input: {
          index: resolve('src/preload/index.ts')
        }
      }
    }
  },
  renderer: {
    resolve: {
      alias: {
        '@renderer': resolve('src/renderer/src')
      }
    },
    plugins: [legacyOrphanGuard(), vue()]
  }
})
