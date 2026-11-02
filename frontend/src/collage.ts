// The generated family portrait (#75): a designed collage of the ACTIVE pets'
// profile photos, composed on a canvas in the browser. The geometry (how many
// sit in each row, where, how big) is plain numbers so it is unit-testable
// without a canvas, like photo.ts; `drawCollage` only applies it to a 2D
// context. Every pet gets the same size seat: nobody is the centrepiece.
import { coverScale } from './photo'

/** Output size of the generated portrait: 3:2 landscape, the shape of a group
 *  photo, and the shape the upload crop uses too. */
export const COLLAGE_W = 1200
export const COLLAGE_H = 800

/** A circular seat: centre and radius, in output pixels. */
export interface Seat { x: number; y: number; r: number }

// Hand-tuned rows (back row first) for the counts a household usually has, so
// each one reads as composed rather than as a grid with a gap in it. Three is
// a pyramid, not a row: a row of three in a 3:2 frame leaves most of it empty.
const ROWS: Record<number, number[]> = { 1: [1], 2: [2], 3: [1, 2], 4: [2, 2], 5: [3, 2], 6: [3, 3] }

// Unit geometry (radius 1). PITCH_X leaves a small gap between neighbours in a
// row once the white ring is drawn. PITCH_Y lets a row nest into the gaps of
// the row behind it (rows are always half a seat apart) and tuck in slightly
// in front of it, like people in a group photo. A pair stands shoulder to
// shoulder (PAIR_PITCH, rings just overlapping) so two pets fill the frame.
const PITCH_X = 2.3
const PITCH_Y = 1.62
const PAIR_PITCH = 1.85
/** Largest radius as a share of the height, so a lone pet doesn't touch the edges. */
const MAX_R = 0.42

/** Pets per row, back row first. Beyond the hand-tuned counts: rows of up to
 *  four, the fuller rows at the back. */
export function rowSizes(n: number): number[] {
  if (n <= 0) return []
  if (ROWS[n]) return ROWS[n]
  const rows = Math.ceil(n / 4)
  const base = Math.floor(n / rows)
  const extra = n % rows
  return Array.from({ length: rows }, (_, i) => base + (i < extra ? 1 : 0))
}

/** Seats for `n` pets in a `w`×`h` frame, in roster order (back row first, left
 *  to right). The group is scaled to fit inside the margins and centred. */
export function collageSeats(n: number, w = COLLAGE_W, h = COLLAGE_H): Seat[] {
  const rows = rowSizes(n)
  if (!rows.length) return []
  const unit: { x: number; y: number }[] = []
  let offset = 0
  rows.forEach((count, ri) => {
    // Rows of different lengths are already half a seat apart once centred;
    // equal rows are nudged half a seat, alternating sides so a third row
    // lines up under the first instead of drifting into a diagonal.
    if (ri > 0 && count === rows[ri - 1]) offset += ri % 2 ? PITCH_X / 2 : -PITCH_X / 2
    const pitch = rows.length === 1 && count === 2 ? PAIR_PITCH : PITCH_X
    const half = ((count - 1) * pitch) / 2
    for (let i = 0; i < count; i++) unit.push({ x: i * pitch - half + offset, y: ri * PITCH_Y })
  })
  const minX = Math.min(...unit.map((p) => p.x)) - 1
  const maxX = Math.max(...unit.map((p) => p.x)) + 1
  const minY = Math.min(...unit.map((p) => p.y)) - 1
  const maxY = Math.max(...unit.map((p) => p.y)) + 1
  const r = Math.min((w * 0.9) / (maxX - minX), (h * 0.86) / (maxY - minY), h * MAX_R)
  const cx = w / 2 - ((minX + maxX) / 2) * r
  const cy = h / 2 - ((minY + maxY) / 2) * r
  return unit.map((p) => ({ x: cx + p.x * r, y: cy + p.y * r, r }))
}

/** `name`, cut with an ellipsis until it measures at most `maxW`; '' when not
 *  even one letter and the ellipsis fit (the caller then drops the label). */
export function fitText(measure: (t: string) => number, name: string, maxW: number): string {
  if (measure(name) <= maxW) return name
  let t = name
  while (t.length > 1 && measure(`${t}…`) > maxW) t = t.slice(0, -1)
  const out = `${t.trimEnd()}…`
  return measure(out) <= maxW ? out : ''
}

/** A `#RGB` / `#RRGGBB` colour as lowercase `#rrggbb`, or null for anything
 *  else. Pet gradients are hex (backend defaults and the UI), but the API takes
 *  free text, and an unparseable colour would make the canvas throw. */
export function hexColor(c: string | null | undefined): string | null {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec((c || '').trim())
  if (!m) return null
  const h = m[1].length === 3 ? m[1].replace(/./g, (d) => d + d) : m[1]
  return `#${h.toLowerCase()}`
}

/** A hex colour plus an alpha, as `#rrggbbaa`; null when it isn't hex. */
export function withAlpha(c: string, alpha: number): string | null {
  const hex = hexColor(c)
  if (!hex) return null
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
  return `${hex}${a.toString(16).padStart(2, '0')}`
}

const FALLBACK: [string, string] = ['#f2624c', '#db4e38']

type Img = CanvasImageSource & { width: number; height: number }

/** One pet in the portrait: their photo, or their gradient + species mark. */
export interface CollageMember {
  name: string
  colors: [string, string]
  photo: Img | null
  mark?: CanvasImageSource | null
}

export interface CollageOptions {
  width?: number
  height?: number
  names?: boolean
  font?: string
}

export const COLLAGE_FONT = 'ui-rounded, "SF Pro Rounded", "Segoe UI", system-ui, sans-serif'

function circle(ctx: CanvasRenderingContext2D, x: number, y: number, r: number) {
  ctx.beginPath()
  ctx.arc(x, y, r, 0, Math.PI * 2)
  ctx.closePath()
}

function pill(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
  const r = h / 2
  ctx.beginPath()
  ctx.moveTo(x + r, y)
  ctx.arcTo(x + w, y, x + w, y + h, r)
  ctx.arcTo(x + w, y + h, x, y + h, r)
  ctx.arcTo(x, y + h, x, y, r)
  ctx.arcTo(x, y, x + w, y, r)
  ctx.closePath()
}

/** Paint the portrait: a warm ground, a soft glow of each pet's colour, then
 *  each pet in a ringed circle (photo cover-fitted, or gradient + initial +
 *  species mark like the profile placeholder), with an optional name pill. */
export function drawCollage(ctx: CanvasRenderingContext2D, members: CollageMember[], opts: CollageOptions = {}) {
  const W = opts.width ?? COLLAGE_W
  const H = opts.height ?? COLLAGE_H
  const font = opts.font || COLLAGE_FONT
  const seats = collageSeats(members.length, W, H)

  const ground = ctx.createLinearGradient(0, 0, W, H)
  ground.addColorStop(0, '#FDF8F4')
  ground.addColorStop(1, '#F6E4D9')
  ctx.fillStyle = ground
  ctx.fillRect(0, 0, W, H)

  // Brand coral stands in for a colour that isn't hex, so the canvas never
  // receives one it can't parse.
  const colors = members.map((m): [string, string] => [
    hexColor(m.colors[0]) || FALLBACK[0],
    hexColor(m.colors[1]) || FALLBACK[1],
  ])

  seats.forEach((s, i) => {
    const glow = ctx.createRadialGradient(s.x, s.y, s.r * 0.5, s.x, s.y, s.r * 2.2)
    glow.addColorStop(0, withAlpha(colors[i][0], 0.34)!)
    glow.addColorStop(1, withAlpha(colors[i][0], 0)!)
    ctx.fillStyle = glow
    ctx.fillRect(0, 0, W, H)
  })

  seats.forEach((s, i) => {
    const m = members[i]
    // White ring with a soft drop shadow, like a print on the table.
    ctx.save()
    ctx.shadowColor = 'rgba(70, 36, 20, 0.22)'
    ctx.shadowBlur = s.r * 0.2
    ctx.shadowOffsetY = s.r * 0.06
    circle(ctx, s.x, s.y, s.r * 1.055)
    ctx.fillStyle = '#FFFFFF'
    ctx.fill()
    ctx.restore()

    ctx.save()
    circle(ctx, s.x, s.y, s.r)
    ctx.clip()
    if (m.photo && m.photo.width && m.photo.height) {
      const k = coverScale(m.photo.width, m.photo.height, 0, s.r * 2, s.r * 2)
      const dw = m.photo.width * k
      const dh = m.photo.height * k
      ctx.drawImage(m.photo, s.x - dw / 2, s.y - dh / 2, dw, dh)
    } else {
      const g = ctx.createLinearGradient(s.x - s.r, s.y - s.r, s.x + s.r, s.y + s.r)
      g.addColorStop(0, colors[i][0])
      g.addColorStop(1, colors[i][1])
      ctx.fillStyle = g
      ctx.fillRect(s.x - s.r, s.y - s.r, s.r * 2, s.r * 2)
      if (m.mark) {
        ctx.globalAlpha = 0.2
        ctx.drawImage(m.mark, s.x + s.r * 0.12, s.y + s.r * 0.05, s.r * 0.85, s.r * 0.85)
        ctx.globalAlpha = 1
      }
      ctx.fillStyle = 'rgba(255, 255, 255, 0.95)'
      ctx.font = `800 ${Math.round(s.r * 0.95)}px ${font}`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      ctx.fillText(m.name.charAt(0).toUpperCase(), s.x, s.y - s.r * 0.08)
    }
    ctx.restore()

  })

  // Names go on last, over every seat: rows tuck in front of each other, and a
  // back-row label must not disappear behind the pet in front.
  if (!opts.names) return
  seats.forEach((s, i) => {
    const m = members[i]
    // Sized for the portrait as Home shows it (~400px wide), not at full size;
    // the floor only matters for a very large household.
    const fs = Math.round(Math.min(46, Math.max(10, s.r * 0.25)))
    ctx.save()
    ctx.font = `800 ${fs}px ${font}`
    // The whole pill stays within 1.5r so a long name can't overhang the ring;
    // when not even an initial fits, the pet goes unlabelled.
    const label = fitText((t) => ctx.measureText(t).width, m.name, s.r * 1.5 - fs * 1.4)
    if (!label) {
      ctx.restore()
      return
    }
    const pw = ctx.measureText(label).width + fs * 1.4
    const ph = fs * 1.8
    ctx.shadowColor = 'rgba(70, 36, 20, 0.18)'
    ctx.shadowBlur = fs * 0.5
    ctx.shadowOffsetY = fs * 0.12
    pill(ctx, s.x - pw / 2, s.y + s.r * 0.78 - ph / 2, pw, ph)
    ctx.fillStyle = 'rgba(255, 255, 255, 0.96)'
    ctx.fill()
    ctx.shadowColor = 'transparent'
    ctx.fillStyle = '#241C18'
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText(label, s.x, s.y + s.r * 0.78 + fs * 0.04)
    ctx.restore()
  })
}
