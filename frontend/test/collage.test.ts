import { describe, it, expect, vi } from 'vitest'
import {
  rowSizes, collageSeats, fitText, withAlpha, hexColor, drawCollage, COLLAGE_W, COLLAGE_H, COLLAGE_FONT,
} from '../src/collage'

describe('rowSizes', () => {
  it('uses the hand-tuned rows for a usual household', () => {
    expect(rowSizes(0)).toEqual([])
    expect(rowSizes(-2)).toEqual([])
    expect(rowSizes(1)).toEqual([1])
    expect(rowSizes(2)).toEqual([2])
    expect(rowSizes(3)).toEqual([1, 2]) // a pyramid fills a 3:2 frame; a row of three doesn't
    expect(rowSizes(4)).toEqual([2, 2])
    expect(rowSizes(5)).toEqual([3, 2])
    expect(rowSizes(6)).toEqual([3, 3])
  })
  it('falls back to rows of up to four, fuller rows at the back', () => {
    expect(rowSizes(7)).toEqual([4, 3])
    expect(rowSizes(8)).toEqual([4, 4])
    expect(rowSizes(9)).toEqual([3, 3, 3])
    expect(rowSizes(10)).toEqual([4, 3, 3])
    for (let n = 7; n <= 20; n++) {
      const rows = rowSizes(n)
      expect(rows.reduce((a, b) => a + b, 0)).toBe(n)
      expect(Math.max(...rows)).toBeLessThanOrEqual(4)
    }
  })
})

describe('collageSeats', () => {
  const RING = 1.055 // the white ring drawn around each seat

  it('seats everyone at one size, inside the frame, tucked in but never hidden', () => {
    for (let n = 1; n <= 12; n++) {
      const seats = collageSeats(n)
      expect(seats).toHaveLength(n)
      const r = seats[0].r
      for (const s of seats) {
        expect(s.r).toBe(r) // nobody is the centrepiece
        expect(s.x - s.r * RING).toBeGreaterThanOrEqual(0)
        expect(s.x + s.r * RING).toBeLessThanOrEqual(COLLAGE_W)
        expect(s.y - s.r * RING).toBeGreaterThanOrEqual(0)
        expect(s.y + s.r * RING).toBeLessThanOrEqual(COLLAGE_H)
      }
      for (let i = 0; i < n; i++) {
        for (let j = i + 1; j < n; j++) {
          // Neighbours may tuck in front of each other like a group photo, but
          // no photo loses more than a sliver (centres at least 1.85 radii apart).
          const d = Math.hypot(seats[i].x - seats[j].x, seats[i].y - seats[j].y)
          expect(d, `n=${n} seats ${i},${j}`).toBeGreaterThanOrEqual(1.85 * r - 1e-9)
        }
      }
    }
  })

  it('centres the group in the frame', () => {
    for (const n of [1, 2, 3, 4, 5, 6, 9]) {
      const seats = collageSeats(n)
      const r = seats[0].r
      const minX = Math.min(...seats.map((s) => s.x)) - r
      const maxX = Math.max(...seats.map((s) => s.x)) + r
      const minY = Math.min(...seats.map((s) => s.y)) - r
      const maxY = Math.max(...seats.map((s) => s.y)) + r
      expect((minX + maxX) / 2).toBeCloseTo(COLLAGE_W / 2, 6)
      expect((minY + maxY) / 2).toBeCloseTo(COLLAGE_H / 2, 6)
    }
  })

  // The small households (1-3, the common case) must FILL the frame, not sit
  // as small circles in a field of background: the group spans at least 70% of
  // the height, and a lone pet or a pair is at least 70% tall themselves.
  const span = (n: number) => {
    const seats = collageSeats(n)
    const r = seats[0].r
    return (Math.max(...seats.map((s) => s.y)) - Math.min(...seats.map((s) => s.y)) + 2 * r) / COLLAGE_H
  }
  it('fills the frame for one, two and three pets', () => {
    const [s] = collageSeats(1)
    expect(s).toEqual({ x: COLLAGE_W / 2, y: COLLAGE_H / 2, r: COLLAGE_H * 0.42 })
    const two = collageSeats(2)
    expect(two[0].y).toBe(two[1].y)
    expect(two[0].x).toBeLessThan(two[1].x)
    expect((2 * two[0].r) / COLLAGE_H).toBeGreaterThanOrEqual(0.7)
    for (const n of [1, 2, 3]) expect(span(n), `n=${n}`).toBeGreaterThanOrEqual(0.7)
  })

  it('stands three as a pyramid: one at the back, two in front', () => {
    const [top, left, right] = collageSeats(3)
    expect(left.y).toBeCloseTo(right.y, 6)
    expect(top.y).toBeLessThan(left.y)
    expect(top.x).toBeCloseTo((left.x + right.x) / 2, 6)
  })

  it('offsets equal rows by half a seat, alternating, so a third row lines up with the first', () => {
    const four = collageSeats(4)
    const pitch = four[1].x - four[0].x
    expect(four[2].x - four[0].x).toBeCloseTo(pitch / 2, 6)
    const nine = collageSeats(9)
    expect(nine[6].x).toBeCloseTo(nine[0].x, 6)
    expect(nine[3].x).not.toBeCloseTo(nine[0].x, 1)
    // An unequal back row is already staggered by centring.
    const five = collageSeats(5)
    expect(five[3].x - five[0].x).toBeCloseTo((five[1].x - five[0].x) / 2, 6)
  })

  it('returns nothing for an empty roster and honours a custom frame', () => {
    expect(collageSeats(0)).toEqual([])
    const [s] = collageSeats(1, 600, 400)
    expect(s).toEqual({ x: 300, y: 200, r: 400 * 0.42 })
  })
})

describe('fitText / withAlpha', () => {
  const measure = (t: string) => t.length * 10
  it('keeps a name that fits and ellipsises one that does not', () => {
    expect(fitText(measure, 'Pedro', 100)).toBe('Pedro')
    expect(fitText(measure, 'Sir Reginald Fluffington', 100)).toBe('Sir Regin…')
    expect(fitText(measure, 'Ab cd', 40)).toBe('Ab…')
    expect(fitText(measure, 'Maximilian', 20)).toBe('M…')
    // Not even an initial fits: no label at all rather than one that overhangs.
    expect(fitText(measure, 'Maximilian', 5)).toBe('')
    expect(fitText(measure, 'M', 5)).toBe('')
  })
  it('normalises #RGB / #RRGGBB and rejects anything else', () => {
    expect(hexColor('#F3B44C')).toBe('#f3b44c')
    expect(hexColor(' #fff ')).toBe('#ffffff')
    expect(hexColor('#abc')).toBe('#aabbcc')
    expect(hexColor('tomato')).toBe(null)
    expect(hexColor('#12345')).toBe(null)
    expect(hexColor('')).toBe(null)
    expect(hexColor(null)).toBe(null)
  })
  it('appends an alpha byte to a hex colour, null otherwise', () => {
    expect(withAlpha('#F3B44C', 1)).toBe('#f3b44cff')
    expect(withAlpha('#F3B44C', 0)).toBe('#f3b44c00')
    expect(withAlpha('#f3b44c', 0.34)).toBe('#f3b44c57')
    expect(withAlpha('#F3B44C', 7)).toBe('#f3b44cff')
    expect(withAlpha('#fff', 0)).toBe('#ffffff00')
    expect(withAlpha('tomato', 0.5)).toBe(null)
  })
})

function fakeCtx() {
  const gradient = () => ({ addColorStop: vi.fn() })
  return {
    createLinearGradient: vi.fn(gradient), createRadialGradient: vi.fn(gradient),
    fillRect: vi.fn(), save: vi.fn(), restore: vi.fn(), beginPath: vi.fn(), closePath: vi.fn(),
    arc: vi.fn(), arcTo: vi.fn(), moveTo: vi.fn(), fill: vi.fn(), clip: vi.fn(),
    drawImage: vi.fn(), fillText: vi.fn(),
    measureText: vi.fn((t: string) => ({ width: t.length * 10 })),
    fillStyle: '', font: '', textAlign: '', textBaseline: '', globalAlpha: 1,
    shadowColor: '', shadowBlur: 0, shadowOffsetY: 0,
  }
}

describe('drawCollage', () => {
  const photo = { width: 512, height: 512 }
  const mark = { width: 240, height: 240 }
  const members = [
    { name: 'Pedro', colors: ['#F3B44C', '#E0762F'] as [string, string], photo },
    { name: 'lulu', colors: ['#F3B44C', '#E0762F'] as [string, string], photo: null, mark },
    { name: 'Kipper', colors: ['#DE93AE', '#B25E85'] as [string, string], photo: null },
  ]

  it('paints photos cover-fitted and colour portraits with an initial + species mark', () => {
    const ctx = fakeCtx()
    drawCollage(ctx as unknown as CanvasRenderingContext2D, members)
    const seats = ctx.arc.mock.calls
    // ring + clip per pet
    expect(seats).toHaveLength(6)
    // Pedro's photo is drawn as a square covering the seat; Lulu's mark is drawn too.
    expect(ctx.drawImage).toHaveBeenCalledTimes(2)
    const [img, , , dw, dh] = ctx.drawImage.mock.calls[0]
    expect(img).toBe(photo)
    expect(dw).toBeCloseTo(dh, 6)
    expect(ctx.drawImage.mock.calls[1][0]).toBe(mark)
    // Initials (upper-cased) for the two without photos; no names by default.
    expect(ctx.fillText.mock.calls.map((c) => c[0])).toEqual(['L', 'K'])
    expect(ctx.font).toContain(COLLAGE_FONT)
  })

  it('paints every name after every seat, so a front row never covers a back-row label', () => {
    const ctx = fakeCtx()
    const order: string[] = []
    ctx.arc.mockImplementation(() => order.push('seat'))
    ctx.arcTo.mockImplementation(() => order.push('pill'))
    drawCollage(ctx as unknown as CanvasRenderingContext2D, members, { names: true })
    expect(order.lastIndexOf('seat')).toBeLessThan(order.indexOf('pill'))
  })

  it('adds a name pill per pet when asked, ellipsising long names', () => {
    const ctx = fakeCtx()
    const long = [...members, { name: 'Sir Reginald Fluffington the Third', colors: ['#57C4CC', '#2E8FA8'] as [string, string], photo }]
    drawCollage(ctx as unknown as CanvasRenderingContext2D, long, { names: true, font: 'serif', width: 600, height: 400 })
    const texts = ctx.fillText.mock.calls.map((c) => c[0])
    expect(texts).toContain('Pedro')
    expect(texts).toContain('lulu')
    expect(texts).toContain('Kipper')
    const sir = texts.find((t) => t.startsWith('Sir') && t.endsWith('…'))
    expect(sir).toBeTruthy()
    // The whole pill (text + padding) stays within 1.5r, never overhanging the ring.
    const r = collageSeats(4, 600, 400)[0].r
    const fs = Math.round(Math.min(46, Math.max(18, r * 0.25)))
    expect(sir!.length * 10 + fs * 1.4).toBeLessThanOrEqual(r * 1.5)
    expect(ctx.font).toContain('serif')
    expect(ctx.arcTo).toHaveBeenCalled()
  })

  it('paints a colour the canvas could not parse in the brand coral instead', () => {
    const ctx = fakeCtx()
    const stops: string[] = []
    ctx.createLinearGradient.mockImplementation(() => ({ addColorStop: (_: number, c: string) => stops.push(c) }))
    ctx.createRadialGradient.mockImplementation(() => ({ addColorStop: (_: number, c: string) => stops.push(c) }))
    drawCollage(ctx as unknown as CanvasRenderingContext2D, [{ name: 'Odd', colors: ['not a colour', '#FFF'], photo: null }])
    expect(stops).toEqual(['#FDF8F4', '#F6E4D9', '#f2624c57', '#f2624c00', '#f2624c', '#ffffff'])
  })

  it('leaves seats unlabelled once they are too small for even an initial', () => {
    const ctx = fakeCtx()
    const crowd = Array.from({ length: 100 }, (_, i) => ({ name: `Pet ${i}`, colors: ['#57C4CC', '#2E8FA8'] as [string, string], photo }))
    drawCollage(ctx as unknown as CanvasRenderingContext2D, crowd, { names: true })
    expect(ctx.fillText).not.toHaveBeenCalled()
    expect(ctx.arcTo).not.toHaveBeenCalled()
  })

  it('treats a photo with no dimensions as missing', () => {
    const ctx = fakeCtx()
    drawCollage(ctx as unknown as CanvasRenderingContext2D, [{ name: 'Nemo', colors: ['#57C4CC', '#2E8FA8'], photo: { width: 0, height: 0 } }])
    expect(ctx.drawImage).not.toHaveBeenCalled()
    expect(ctx.fillText).toHaveBeenCalledWith('N', expect.any(Number), expect.any(Number))
  })
})
