import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { PAW_PATH } from '../src/format'

// The app icon is the white paw on coral. These tests lock the two invariants
// that keep it honest: the icon's paw is the SAME PAW_PATH the app renders (no
// drift between the tab icon and the in-app mark), and index.html + the web
// manifest actually point at assets that exist on disk.
//
// Paths resolve from process.cwd() (the frontend project root under vitest), NOT
// `new URL(x, import.meta.url)`: Vite rewrites that pattern as an asset import
// for static string literals, so a dynamic `${f}` path would silently resolve
// against a different base than a literal one.
const read = (rel) => readFileSync(join(process.cwd(), rel))
const readText = (rel) => read(rel).toString('utf8')

describe('app icon: favicon.svg', () => {
  const svg = readText('public/favicon.svg')

  it('draws the canonical paw, not a redrawn one', () => {
    const d = svg.match(/<path[^>]*\sd="([^"]+)"/)[1]
    expect(d).toBe(PAW_PATH)
  })

  it('uses the coral brand field', () => {
    expect(svg).toMatch(/fill="#F2624C"/i)
    expect(svg).toMatch(/fill="#FFFFFF"/i)
  })
})

describe('app icon: index.html wiring', () => {
  const html = readText('index.html')

  it('links every icon surface', () => {
    expect(html).toMatch(/<link rel="icon" href="\/favicon\.svg" type="image\/svg\+xml"/)
    expect(html).toMatch(/<link rel="icon" href="\/favicon\.ico"/)
    expect(html).toMatch(/<link rel="apple-touch-icon" href="\/apple-touch-icon\.png"/)
    expect(html).toMatch(/<link rel="manifest" href="\/site\.webmanifest"/)
  })

  it('keeps the coral theme-color', () => {
    expect(html).toMatch(/<meta name="theme-color" content="#F2624C"/)
  })
})

describe('app icon: web manifest', () => {
  const manifest = JSON.parse(readText('public/site.webmanifest'))

  it('is branded FurFiles with the coral theme', () => {
    expect(manifest.name).toBe('FurFiles')
    expect(manifest.theme_color).toBe('#F2624C')
    expect(manifest.display).toBe('standalone')
  })

  it('offers the home-screen icon sizes plus a maskable variant', () => {
    const bySize = manifest.icons.map((i) => `${i.sizes}:${i.purpose || ''}`)
    expect(bySize).toContain('192x192:any')
    expect(bySize).toContain('512x512:any')
    expect(manifest.icons.some((i) => i.purpose === 'maskable')).toBe(true)
  })
})

describe('app icon: referenced raster assets exist', () => {
  // The generated rasters are committed (not built), so a broken regen could
  // leave index.html / the manifest pointing at a file that 404s. Assert every
  // referenced asset is actually on disk and non-empty.
  const rasters = ['favicon.ico', 'apple-touch-icon.png', 'icon-192.png', 'icon-512.png', 'maskable-512.png']

  it.each(rasters)('ships a non-empty %s', (f) => {
    expect(read(join('public', f)).length).toBeGreaterThan(0)
  })
})
