import { describe, it, expect } from 'vitest'
import { rotatedDims, normalizeRotation, coverScale, clampPan, OUTPUT_SIZE } from '../src/photo'

describe('normalizeRotation', () => {
  it('snaps to the nearest quarter turn and wraps', () => {
    expect(normalizeRotation(0)).toBe(0)
    expect(normalizeRotation(90)).toBe(90)
    expect(normalizeRotation(360)).toBe(0)
    expect(normalizeRotation(450)).toBe(90)
    expect(normalizeRotation(-90)).toBe(270)
    expect(normalizeRotation(44)).toBe(0)
    expect(normalizeRotation(46)).toBe(90)
    expect(normalizeRotation(undefined)).toBe(0)
  })
})

describe('rotatedDims', () => {
  it('swaps width/height on quarter turns only', () => {
    expect(rotatedDims(400, 300, 0)).toEqual({ w: 400, h: 300 })
    expect(rotatedDims(400, 300, 180)).toEqual({ w: 400, h: 300 })
    expect(rotatedDims(400, 300, 90)).toEqual({ w: 300, h: 400 })
    expect(rotatedDims(400, 300, 270)).toEqual({ w: 300, h: 400 })
  })
})

describe('coverScale', () => {
  it('scales the smaller dimension up to cover the frame', () => {
    // 1000x500 into 500 frame: needs 500/500=1 on height, 500/1000=0.5 on width
    // → the binding constraint is height, cover = 1.
    expect(coverScale(1000, 500, 0, 500)).toBe(1)
    // portrait 500x1000: width binds, cover = 500/500 = 1
    expect(coverScale(500, 1000, 0, 500)).toBe(1)
    // small square 250 into 500: must scale 2x
    expect(coverScale(250, 250, 0, 500)).toBe(2)
  })
  it('accounts for rotation (dims swap)', () => {
    // 1000x500 rotated 90 → effective 500x1000; width 500 binds → cover 1
    expect(coverScale(1000, 500, 90, 500)).toBe(1)
    // 2000x500 rotated 90 → effective 500x2000; cover = 500/500 = 1 (height 2000
    // already exceeds). Unrotated it would be 500/500=1 too, but check a case that
    // differs: 800x400 → unrotated cover = 500/400 = 1.25; rotated → 500x... eff
    // 400x800, width 400 binds → 500/400 = 1.25. Use asymmetric to prove the swap:
    expect(coverScale(1200, 400, 0, 600)).toBeCloseTo(1.5) // 600/400
    expect(coverScale(1200, 400, 90, 600)).toBeCloseTo(1.5) // eff 400x1200, 600/400
  })
  it('defaults the frame to OUTPUT_SIZE and guards zero dims', () => {
    expect(coverScale(OUTPUT_SIZE, OUTPUT_SIZE)).toBe(1)
    expect(coverScale(0, 100, 0, 500)).toBe(1)
    expect(coverScale(100, 100, 0, 0)).toBe(1)
  })
})

describe('clampPan', () => {
  it('locks pan to zero when the image exactly covers', () => {
    // 500x500 at scale 1 into 500 frame → no slack
    expect(clampPan(40, -30, 500, 500, 1, 0, 500)).toEqual({ x: 0, y: 0 })
  })
  it('allows pan within the overflow and clamps beyond it', () => {
    // 500x500 at scale 2 into 500 → drawn 1000px, slack (1000-500)/2 = 250 each way
    expect(clampPan(100, -100, 500, 500, 2, 0, 500)).toEqual({ x: 100, y: -100 })
    expect(clampPan(999, -999, 500, 500, 2, 0, 500)).toEqual({ x: 250, y: -250 })
  })
  it('uses rotated dims for the bounds', () => {
    // 1000x500 rotated 90 → eff 500x1000. At scale 1 into 500 frame:
    // x slack (500-500)/2 = 0, y slack (1000-500)/2 = 250.
    expect(clampPan(80, 80, 1000, 500, 1, 90, 500)).toEqual({ x: 0, y: 80 })
    expect(clampPan(80, 400, 1000, 500, 1, 90, 500)).toEqual({ x: 0, y: 250 })
  })
  it('treats missing offsets as zero', () => {
    expect(clampPan(undefined, undefined, 500, 500, 2, 0, 500)).toEqual({ x: 0, y: 0 })
  })
})

describe('rectangular frames (#75 family portrait)', () => {
  it('covers a 3:2 frame on whichever side is tighter', () => {
    // A square photo into 1200x800: width is the binding side.
    expect(coverScale(600, 600, 0, 1200, 800)).toBe(2)
    // A tall photo into the same frame: still width.
    expect(coverScale(400, 1000, 0, 1200, 800)).toBe(3)
    // A wide panorama: height binds.
    expect(coverScale(4000, 1000, 0, 1200, 800)).toBe(0.8)
    // A zero frame side is degenerate, not a divide-by-zero.
    expect(coverScale(600, 600, 0, 1200, 0)).toBe(1)
  })
  it('clamps pan per axis against the frame height', () => {
    // 600x600 at scale 2 → 1200x1200 drawn into 1200x800: no x slack, 200 y slack.
    expect(clampPan(50, 500, 600, 600, 2, 0, 1200, 800)).toEqual({ x: 0, y: 200 })
  })
})
