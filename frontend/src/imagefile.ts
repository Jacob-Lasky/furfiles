// Decoding image files the browser may or may not understand, shared by the pet
// portrait editor (#21), the bulk importer (#23) and the single-record scan (#71).
//
// iPhones save photos as HEIC. The model only reads JPEG/PNG/WebP/GIF, and most
// desktop browsers can't decode HEIC into an <img>. DO NOT "detect HEIC by name
// or MIME, then always convert": Firefox on Android hands the picker a JPEG that
// is still named `.heic` and typed `image/heic`, and forcing that readable file
// through libheif throws "already browser readable". The order is always
// NATIVE DECODE FIRST (Safari and Apple decode HEIC natively and skip the wasm
// entirely), and only a decode FAILURE falls back to the lazy-imported heic2any.
// The name/MIME hint here only decides whether a file is worth inspecting at all.
import { readDataUrl } from './format'

/** Longest edge of a converted document photo. 12 MP phone shots are ~4000 px;
 *  3000 keeps small print on a form legible while holding the JPEG well under the
 *  provider's per-image size limit. */
export const MAX_DOCUMENT_EDGE = 3000
const JPEG_QUALITY = 0.9

/** Shown when neither the browser nor libheif can open the file. */
export const HEIC_FAILED =
  "That photo couldn't be opened. Export it as a JPEG (or share it as a PDF) and add that instead."

/** Is this file claiming to be HEIC/HEIF, by MIME or extension? A hint, never proof:
 *  iOS sometimes hands over an empty `type`, and Android Firefox mislabels JPEGs. */
export function looksHeic(file: { name?: string; type?: string }): boolean {
  return /image\/hei[cf]/i.test(file.type || '') || /\.hei[cf]$/i.test(file.name || '')
}

/** Load a blob into an <img> using ONLY the browser's own decoder. */
export function blobToImage(blob: Blob): Promise<HTMLImageElement> {
  return readDataUrl(blob).then(
    (url) =>
      new Promise<HTMLImageElement>((resolve, reject) => {
        const im = new Image()
        im.onload = () => resolve(im)
        im.onerror = () => reject(new Error('That image could not be opened.'))
        im.src = url
      }),
  )
}

/** Convert a HEIC blob to JPEG with heic2any (libheif as wasm). Lazy-imported so
 *  Vite code-splits the ~1.35 MB chunk: only a file the browser can't decode
 *  downloads it. */
export async function heicToJpeg(file: Blob): Promise<Blob> {
  const { default: heic2any } = await import('heic2any')
  const out = await heic2any({ blob: file, toType: 'image/jpeg', quality: JPEG_QUALITY })
  return Array.isArray(out) ? out[0] : out
}

/** Decode to an <img>: native first, heic2any only if native decode throws.
 *  `onConvert` fires just before the slow wasm path so the UI can say so. */
export async function decodeImage(file: Blob, onConvert?: () => void): Promise<HTMLImageElement> {
  try {
    return await blobToImage(file)
  } catch (_) {
    onConvert?.()
    return blobToImage(await heicToJpeg(file))
  }
}

type Sniffed = { type: string; ext: string } | null

/** What the BYTES say the file is, for the formats the model reads. */
async function sniff(file: Blob): Promise<Sniffed> {
  const b = new Uint8Array(await file.slice(0, 12).arrayBuffer())
  const at = (i: number, s: string) => [...s].every((c, k) => b[i + k] === c.charCodeAt(0))
  if (b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return { type: 'image/jpeg', ext: 'jpg' }
  if (b[0] === 0x89 && at(1, 'PNG')) return { type: 'image/png', ext: 'png' }
  if (at(0, 'GIF8')) return { type: 'image/gif', ext: 'gif' }
  if (at(0, 'RIFF') && at(8, 'WEBP')) return { type: 'image/webp', ext: 'webp' }
  return null
}

function stemOf(name: string): string {
  return name.replace(/\.[^./\\]+$/, '') || 'photo'
}

function toJpeg(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('JPEG encode failed'))), 'image/jpeg', JPEG_QUALITY),
  )
}

/** Draw an already-decoded image to a JPEG no longer than `MAX_DOCUMENT_EDGE`. */
async function renderJpeg(im: HTMLImageElement): Promise<Blob> {
  const w = im.naturalWidth || im.width
  const h = im.naturalHeight || im.height
  const k = Math.min(1, MAX_DOCUMENT_EDGE / Math.max(w, h))
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.round(w * k))
  canvas.height = Math.max(1, Math.round(h * k))
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('no canvas')
  ctx.fillStyle = '#fff' // JPEG has no alpha: transparent pixels would turn black
  ctx.fillRect(0, 0, canvas.width, canvas.height)
  ctx.drawImage(im, 0, 0, canvas.width, canvas.height)
  return toJpeg(canvas)
}

/** Check a libheif JPEG reads back (an unreadable one is a failed conversion, so
 *  this throws), and re-draw it smaller when a 12 MP phone shot would top the
 *  provider's per-image limit. Without a canvas the oversized JPEG is kept as is. */
async function verifyAndShrink(jpeg: Blob): Promise<Blob> {
  const im = await blobToImage(jpeg)
  if (Math.max(im.naturalWidth || im.width, im.naturalHeight || im.height) <= MAX_DOCUMENT_EDGE) return jpeg
  try {
    return await renderJpeg(im)
  } catch (_) {
    return jpeg
  }
}

/**
 * Turn a document photo into a file the model can read. A file that doesn't
 * claim to be HEIC is returned untouched. One that does is inspected:
 *  1. its BYTES are JPEG/PNG/WebP/GIF (Android Firefox mislabel) -> same bytes,
 *     the true type and extension, no re-encode;
 *  2. the browser decodes it (Safari/Apple) -> re-drawn as a JPEG;
 *  3. only if that throws -> heic2any, then a downscale if it came out huge.
 * The result is `<original-stem>.jpg`, image/jpeg. Throws `HEIC_FAILED` when no
 * decoder can open it. Convert ONCE and reuse the returned file for every later
 * step (analyze, commit, attach): the server hashes those bytes for the
 * duplicate check, so the hash stays the same across steps.
 */
export async function documentFile(file: File, onConvert?: () => void): Promise<File> {
  if (!looksHeic(file)) return file
  const jpeg = (b: Blob) => new File([b], `${stemOf(file.name)}.jpg`, { type: 'image/jpeg' })
  try {
    const real = await sniff(file)
    if (real) return new File([file], `${stemOf(file.name)}.${real.ext}`, { type: real.type })
    let im: HTMLImageElement | null = null
    try {
      im = await blobToImage(file)
    } catch (_) { /* the browser can't read it: libheif below */ }
    if (im) {
      try {
        return jpeg(await renderJpeg(im))
      } catch (_) { /* decoded but can't re-encode (no canvas): libheif is the only way left */ }
    }
    onConvert?.()
    return jpeg(await verifyAndShrink(await heicToJpeg(file)))
  } catch (_) {
    throw new Error(HEIC_FAILED)
  }
}
