// Pure crop/rotate/zoom geometry for the pet-portrait editor (#21). Kept out of
// the Vue component so it is unit-testable without a canvas: the component just
// applies these numbers to a 2D context. All lengths are in OUTPUT canvas pixels
// (the editor uses one square canvas sized OUTPUT_SIZE, displayed small via CSS),
// so the same transform that previews the crop also exports it — no second pass.

/** Square side of the stored portrait, in pixels. A cropped square keeps the
 *  volume small while looking crisp in the round/rounded frame. */
export const OUTPUT_SIZE = 512

export interface Dims { w: number; h: number }
export interface Pan { x: number; y: number }

/** Image dimensions after a 0/90/180/270° rotation — width/height swap on the
 *  quarter turns. `rotation` is normalized to a multiple of 90 first. */
export function rotatedDims(w: number, h: number, rotation: number): Dims {
  return normalizeRotation(rotation) % 180 === 90 ? { w: h, h: w } : { w, h }
}

/** Snap any angle to the nearest {0,90,180,270}; the editor only rotates in
 *  quarter turns (predictable, no gaps to fill). Negatives wrap. */
export function normalizeRotation(rotation: number | null | undefined): number {
  return (((Math.round((rotation || 0) / 90) * 90) % 360) + 360) % 360
}

/** Minimum scale so the (possibly rotated) image fully covers the frame — the
 *  portrait must never show empty corners. The frame is square unless `frameH`
 *  is given (the 3:2 family portrait, #75). */
export function coverScale(imgW: number, imgH: number, rotation = 0, frame = OUTPUT_SIZE, frameH = frame): number {
  if (!imgW || !imgH || !frame || !frameH) return 1
  const d = rotatedDims(imgW, imgH, rotation)
  return Math.max(frame / d.w, frameH / d.h)
}

/** Clamp a pan offset so the scaled/rotated image always covers the frame (no
 *  gap can be dragged into view). Returns the corrected {x,y} in frame pixels. */
export function clampPan(
  px: number | null | undefined,
  py: number | null | undefined,
  imgW: number,
  imgH: number,
  scale: number,
  rotation: number,
  frame = OUTPUT_SIZE,
  frameH = frame,
): Pan {
  const d = rotatedDims(imgW, imgH, rotation)
  const maxX = Math.max(0, (d.w * scale - frame) / 2)
  const maxY = Math.max(0, (d.h * scale - frameH) / 2)
  const clamp = (v: number | null | undefined, m: number) => {
    const r = Math.min(m, Math.max(-m, v || 0))
    return r === 0 ? 0 : r // normalize -0 → 0 so callers never see a signed zero
  }
  return { x: clamp(px, maxX), y: clamp(py, maxY) }
}
