<script setup lang="ts">
// Pet-portrait editor (#21): pick an image, then crop / rotate / zoom to a
// square and save it as the pet's photo. The crop happens client-side on one
// canvas (sized to the output, shown small via CSS), so the same transform that
// previews the crop also exports it — the server just stores the finished
// image. Geometry lives in photo.ts so it stays testable without a canvas.
// The family portrait (#75) reuses it with a 3:2 frame and its own `upload`.
import { ref, reactive, computed, nextTick } from 'vue'
import { api } from '../api'
import { OUTPUT_SIZE, coverScale, clampPan } from '../photo'
import { isImage } from '../format'
import { decodeImage, looksHeic } from '../imagefile'
import type { PetPhotoInput } from '../types'
import SheetShell from './SheetShell.vue'

const props = withDefaults(defineProps<{
  petId?: string
  petName?: string
  hasPhoto?: boolean
  /** Output frame in pixels; square OUTPUT_SIZE unless given. */
  width?: number
  height?: number
  title?: string
  hint?: string
  /** Where the cropped JPEG goes; defaults to the pet's photo endpoint. */
  upload?: ((data: PetPhotoInput) => Promise<unknown>) | null
}>(), {
  petId: '', petName: '', hasPhoto: false, width: OUTPUT_SIZE, height: OUTPUT_SIZE,
  title: '', hint: 'Pick a picture, then drag, zoom, and rotate to frame it.', upload: null,
})
const emit = defineEmits<{ close: []; saved: [] }>()

const W = computed(() => props.width)
const H = computed(() => props.height)
// A landscape frame gets a wider preview than the square portrait's 300px.
const frameStyle = computed(() =>
  W.value === H.value ? undefined : { aspectRatio: `${W.value} / ${H.value}`, maxWidth: '440px' },
)
const canvas = ref<HTMLCanvasElement | null>(null)
const img = ref<HTMLImageElement | null>(null) // the loaded HTMLImageElement
const saving = ref(false)
const removing = ref(false)
const converting = ref(false) // decoding a HEIC before it can be framed
const error = ref('')
const zoom = ref(1) // slider multiplier over the cover scale (1 = fully zoomed out)

// Live transform, all in canvas (output) pixels. cover is the min scale that
// keeps the frame covered; scale = cover * zoom; pan is the drag offset.
const t = reactive({ imgW: 0, imgH: 0, rotation: 0, cover: 1, pan: { x: 0, y: 0 } })
const scale = computed(() => t.cover * zoom.value)
const ready = computed(() => t.imgW > 0)

function ctx2d() {
  return canvas.value ? canvas.value.getContext('2d') : null
}

// Redraw the preview: clear, then draw the image centered under the current
// rotation/scale/pan. A null context (e.g. jsdom) is a no-op — the geometry is
// covered by photo.js tests and the pixels by the /test-ux probe.
function draw() {
  const c = ctx2d()
  if (!c || !img.value) return
  c.clearRect(0, 0, W.value, H.value)
  c.save()
  c.translate(W.value / 2 + t.pan.x, H.value / 2 + t.pan.y)
  c.rotate((t.rotation * Math.PI) / 180)
  c.scale(scale.value, scale.value)
  c.drawImage(img.value, -t.imgW / 2, -t.imgH / 2)
  c.restore()
}

function recompute() {
  t.cover = coverScale(t.imgW, t.imgH, t.rotation, W.value, H.value)
  t.pan = clampPan(t.pan.x, t.pan.y, t.imgW, t.imgH, scale.value, t.rotation, W.value, H.value)
  draw()
}

async function onFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = (input.files || [])[0]
  input.value = '' // allow re-picking the same file
  if (!file) return
  // Reject only obvious non-images; HEIC may arrive as image/heic OR (with an
  // empty type) just a .heic name, so `looksHeic` keeps those in play.
  if (!looksHeic(file) && !isImage(file.type)) {
    error.value = 'Pick an image file (JPEG, PNG, WebP, HEIC, or GIF).'
    return
  }
  error.value = ''
  try {
    // Native decode first, heic2any only on failure (see imagefile.ts).
    const im = await decodeImage(file, () => { converting.value = true })
    img.value = im
    t.imgW = im.naturalWidth || im.width
    t.imgH = im.naturalHeight || im.height
    t.rotation = 0
    zoom.value = 1
    t.pan = { x: 0, y: 0 }
    await nextTick()
    recompute()
  } catch (err) {
    error.value = "That photo couldn't be opened. If it's a HEIC, try exporting it as JPEG."
    // Capture the exact file that failed so it can be pulled and reproduced (#41).
    api.reportDiag(
      { context: 'photo-editor:load', message: String((err && err.message) || err), stack: err && err.stack },
      file,
    )
  } finally {
    converting.value = false
  }
}

function rotate() {
  if (!ready.value) return
  t.rotation = (t.rotation + 90) % 360
  recompute()
}
function onZoom() {
  if (!ready.value) return
  recompute()
}

// Drag to pan. Deltas arrive in CSS pixels; convert to canvas pixels by the
// display ratio so a drag moves the image 1:1 under the finger.
const drag = { active: false, x: 0, y: 0 }
function ratio() {
  const w = canvas.value && canvas.value.clientWidth
  return w ? W.value / w : 1
}
function onDown(e: PointerEvent) {
  if (!ready.value) return
  drag.active = true
  drag.x = e.clientX
  drag.y = e.clientY
}
function onMove(e: PointerEvent) {
  if (!drag.active) return
  const f = ratio()
  t.pan = clampPan(
    t.pan.x + (e.clientX - drag.x) * f,
    t.pan.y + (e.clientY - drag.y) * f,
    t.imgW, t.imgH, scale.value, t.rotation, W.value, H.value,
  )
  drag.x = e.clientX
  drag.y = e.clientY
  draw()
}
function onUp() {
  drag.active = false
}

async function save() {
  if (!ready.value) { error.value = 'Choose a photo first.'; return }
  saving.value = true
  error.value = ''
  try {
    draw()
    const c = canvas.value
    if (!c) return
    // toDataURL → "data:image/jpeg;base64,<data>"; keep just the data.
    const dataUrl = c.toDataURL('image/jpeg', 0.9)
    const data_base64 = dataUrl.split(',')[1] || ''
    const data = { media_type: 'image/jpeg', data_base64 }
    await (props.upload ? props.upload(data) : api.setPetPhoto(props.petId, data))
    emit('saved')
  } catch (err) {
    error.value = err.message || 'Could not save the photo.'
    api.reportDiag({ context: 'photo-editor:save', message: String((err && err.message) || err), stack: err && err.stack })
  } finally {
    saving.value = false
  }
}

async function removePhoto() {
  removing.value = true
  error.value = ''
  try {
    await api.deletePetPhoto(props.petId)
    emit('saved')
  } catch (err) {
    error.value = err.message || 'Could not remove the photo.'
  } finally {
    removing.value = false
  }
}
</script>

<template>
  <SheetShell @close="emit('close')">
    <div class="sheet-head">
      <h3>{{ title || (petName ? `${petName}'s photo` : 'Pet photo') }}</h3>
      <small>{{ hint }}</small>
    </div>

    <div class="sheet-body">
      <div class="cropwrap" :style="frameStyle">
        <canvas
          ref="canvas" class="cropcanvas" :width="W" :height="H"
          :class="{ empty: !ready }"
          @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointerleave="onUp"
        ></canvas>
        <div v-if="!ready" class="cropempty">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 7h3l2-3h6l2 3h3a1 1 0 011 1v11a1 1 0 01-1 1H4a1 1 0 01-1-1V8a1 1 0 011-1z"/><circle cx="12" cy="13" r="4"/></svg>
          <span>{{ converting ? 'Converting photo…' : 'Choose a photo to begin' }}</span>
        </div>
      </div>

      <label class="filepick" :class="{ busy: converting }">
        <input type="file" accept="image/*,.heic,.heif" :disabled="converting" @change="onFile" style="display:none" />
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M12 5v14M5 12h14"/></svg>
        {{ converting ? 'Converting…' : ready ? 'Choose a different photo' : 'Choose a photo' }}
      </label>

      <div v-if="ready" class="cropctl">
        <button class="rotate" type="button" aria-label="Rotate" @click="rotate">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 4v6h-6"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
        </button>
        <input
          class="zoom" type="range" min="1" max="4" step="0.01"
          v-model.number="zoom" @input="onZoom" aria-label="Zoom"
        />
      </div>

      <p v-if="error" style="color:var(--crit);font-size:13px;margin:8px 0 0">{{ error }}</p>
    </div>

    <div class="sheet-foot">
      <button v-if="hasPhoto" class="btn sec" type="button" :disabled="removing || saving" @click="removePhoto">
        {{ removing ? 'Removing…' : 'Remove photo' }}
      </button>
      <button v-else class="btn sec" type="button" @click="emit('close')">Cancel</button>
      <button class="btn primary" :disabled="!ready || saving" @click="save">
        {{ saving ? 'Saving…' : 'Save photo' }}
      </button>
    </div>
  </SheetShell>
</template>
