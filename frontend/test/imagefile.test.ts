import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const jpegBlob = () => new Blob([new Uint8Array([0xff, 0xd8, 0xff, 0xe0])], { type: 'image/jpeg' })
vi.mock('heic2any', () => ({ default: vi.fn(async () => jpegBlob()) }))
import heic2any from 'heic2any'
import { looksHeic, documentFile, decodeImage, heicToJpeg, HEIC_FAILED, MAX_DOCUMENT_EDGE } from '../src/imagefile'

// Image stand-in: `decodes(blobSize?)` decides per load whether it works. jsdom
// never fires load/error itself.
let loads: boolean[] = []
let dims = { w: 800, h: 600 }
function stubImage() {
  vi.stubGlobal('Image', class {
    onload?: () => void
    onerror?: () => void
    naturalWidth = dims.w
    naturalHeight = dims.h
    set src(_v: string) {
      const ok = loads.length ? loads.shift() : true
      setTimeout(() => (ok ? this.onload?.() : this.onerror?.()))
    }
  })
}

const heic = (name = 'IMG_1.HEIC', type = 'image/heic') => new File([new Uint8Array(32)], name, { type })

describe('looksHeic', () => {
  it('goes by MIME or extension, either one', () => {
    expect(looksHeic({ name: 'a.heic', type: '' })).toBe(true)
    expect(looksHeic({ name: 'a.HEIF', type: '' })).toBe(true)
    expect(looksHeic({ name: 'a', type: 'image/heif' })).toBe(true)
    expect(looksHeic({ name: 'a.jpg', type: 'image/jpeg' })).toBe(false)
    expect(looksHeic({})).toBe(false)
  })
})

describe('documentFile', () => {
  let toBlob: ReturnType<typeof vi.spyOn>
  let getContext: ReturnType<typeof vi.spyOn>
  let ctx: { drawImage: ReturnType<typeof vi.fn>; fillRect: ReturnType<typeof vi.fn>; fillStyle: string }
  beforeEach(() => {
    loads = []
    dims = { w: 800, h: 600 }
    stubImage()
    vi.mocked(heic2any).mockClear()
    ctx = { drawImage: vi.fn(), fillRect: vi.fn(), fillStyle: '' }
    getContext = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(ctx as never)
    toBlob = vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation(function (cb: BlobCallback) {
      cb(new Blob(['native-jpeg'], { type: 'image/jpeg' }))
    } as never)
  })
  afterEach(() => {
    getContext.mockRestore()
    toBlob.mockRestore()
    vi.unstubAllGlobals()
  })

  it('returns a non-HEIC file untouched', async () => {
    const f = new File(['x'], 'a.png', { type: 'image/png' })
    expect(await documentFile(f)).toBe(f)
  })

  it('keeps the bytes of an already-JPEG file named .heic, retyped and renamed', async () => {
    const f = new File([new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0])], 'IMG_9.heic', { type: 'image/heic' })
    const out = await documentFile(f)
    expect(out.name).toBe('IMG_9.jpg')
    expect(out.type).toBe('image/jpeg')
    expect(out.size).toBe(f.size)
    expect(heic2any).not.toHaveBeenCalled()
  })

  it.each([
    ['PNG', [0x89, 0x50, 0x4e, 0x47], 'image/png', 'png'],
    ['GIF', [0x47, 0x49, 0x46, 0x38], 'image/gif', 'gif'],
    ['WebP', [0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x45, 0x42, 0x50], 'image/webp', 'webp'],
  ])('recognises a mislabeled %s by its bytes', async (_n, magic, type, ext) => {
    const out = await documentFile(new File([new Uint8Array(magic)], 'x.heif', { type: 'image/heif' }))
    expect(out.type).toBe(type)
    expect(out.name).toBe(`x.${ext}`)
  })

  it('re-draws a HEIC the browser decodes natively (Safari) as a JPEG, no wasm', async () => {
    const out = await documentFile(heic())
    expect(heic2any).not.toHaveBeenCalled()
    expect(out.name).toBe('IMG_1.jpg')
    expect(out.type).toBe('image/jpeg')
    expect(await out.text()).toBe('native-jpeg')
    // painted on white first: JPEG has no alpha
    expect(ctx.fillStyle).toBe('#fff')
    expect(ctx.fillRect).toHaveBeenCalledWith(0, 0, 800, 600)
  })

  it('caps the longest edge when drawing a big native decode', async () => {
    dims = { w: 6000, h: 3000 }
    const canvases: HTMLCanvasElement[] = []
    toBlob.mockImplementation(function (this: HTMLCanvasElement, cb: BlobCallback) {
      canvases.push(this)
      cb(new Blob(['x'], { type: 'image/jpeg' }))
    } as never)
    await documentFile(heic())
    expect(canvases[0].width).toBe(MAX_DOCUMENT_EDGE)
    expect(canvases[0].height).toBe(MAX_DOCUMENT_EDGE / 2)
  })

  it('falls back to heic2any only when native decode fails', async () => {
    loads = [false, true] // native fails; the converted JPEG reads fine
    const onConvert = vi.fn()
    const out = await documentFile(heic('Scan 3.heif', 'image/heif'), onConvert)
    expect(onConvert).toHaveBeenCalledTimes(1)
    expect(heic2any).toHaveBeenCalledTimes(1)
    expect(out.name).toBe('Scan 3.jpg')
    expect(out.type).toBe('image/jpeg')
  })

  it('downsizes an oversized libheif result, but keeps a small one as it came', async () => {
    loads = [false, true]
    dims = { w: 4032, h: 3024 }
    const big = await documentFile(heic())
    expect(await big.text()).toBe('native-jpeg') // re-drawn smaller
    loads = [false, true]
    dims = { w: 1200, h: 900 }
    const small = await documentFile(heic())
    expect((await small.arrayBuffer()).byteLength).toBe(4) // the heic2any blob, untouched
  })

  it('uses libheif when the browser decodes but cannot re-encode', async () => {
    getContext.mockReturnValue(null as never)
    const out = await documentFile(heic())
    expect(heic2any).toHaveBeenCalledTimes(1)
    expect(out.type).toBe('image/jpeg')
  })

  it('treats a libheif result the browser cannot read back as a failed conversion', async () => {
    loads = [false, false] // native fails, and so does reading the converted file back
    await expect(documentFile(heic())).rejects.toThrow(HEIC_FAILED)
  })

  it('keeps an oversized libheif JPEG as is when there is no canvas to shrink it', async () => {
    loads = [false, true]
    dims = { w: 4032, h: 3024 }
    getContext.mockReturnValue(null as never)
    const out = await documentFile(heic())
    expect((await out.arrayBuffer()).byteLength).toBe(4)
  })

  it('throws one clear message when no decoder can open it', async () => {
    loads = [false]
    vi.mocked(heic2any).mockRejectedValueOnce(new Error('libheif boom'))
    await expect(documentFile(heic())).rejects.toThrow(HEIC_FAILED)
  })

  it('names an extensionless HEIC photo', async () => {
    loads = [false, true]
    expect((await documentFile(new File([new Uint8Array(8)], '', { type: 'image/heic' }))).name).toBe('photo.jpg')
  })

  it('handles a canvas that yields no blob', async () => {
    toBlob.mockImplementation(function (cb: BlobCallback) { cb(null) } as never)
    const out = await documentFile(heic()) // re-encode fails -> libheif
    expect(heic2any).toHaveBeenCalledTimes(1)
    expect(out.type).toBe('image/jpeg')
  })
})

describe('decodeImage / heicToJpeg', () => {
  beforeEach(() => { loads = []; stubImage(); vi.mocked(heic2any).mockClear() })
  afterEach(() => vi.unstubAllGlobals())

  it('decodes natively without converting', async () => {
    await decodeImage(new Blob(['x']))
    expect(heic2any).not.toHaveBeenCalled()
  })

  it('converts after a native failure', async () => {
    loads = [false, true]
    const onConvert = vi.fn()
    await decodeImage(new Blob(['x']), onConvert)
    expect(onConvert).toHaveBeenCalled()
    expect(heic2any).toHaveBeenCalledTimes(1)
  })

  it('unwraps heic2any array results', async () => {
    vi.mocked(heic2any).mockResolvedValueOnce([jpegBlob()] as never)
    expect(await heicToJpeg(new Blob(['x']))).toBeInstanceOf(Blob)
  })
})
