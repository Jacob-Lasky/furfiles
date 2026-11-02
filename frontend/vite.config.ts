import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

// During dev, proxy API calls to the Rust backend. In production the same Axum
// server serves this built bundle, so calls are same-origin.
export default defineConfig({
  plugins: [vue()],
  server: {
    port: 5173,
    proxy: { '/api': 'http://localhost:8080' },
  },
  build: { outDir: 'dist' },
  test: {
    environment: 'jsdom',
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,vue}'],
      // main.ts is the mount bootstrap; types.ts and *.d.ts are declarations
      // only — none have behavior to assert.
      exclude: ['src/main.ts', 'src/types.ts', 'src/**/*.d.ts'],
    },
  },
})
