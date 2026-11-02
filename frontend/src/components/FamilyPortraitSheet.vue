<script setup lang="ts">
// Family portrait generator (#75): composes the ACTIVE pets' profile photos into
// one designed collage on a canvas, previews it, and saves the JPEG through the
// same PUT an upload uses, tagged `generated` with the pets it was made from so
// Home can offer to regenerate once the roster changes. Deliberately NOT an
// image-generation model: that costs per image and would send the household's
// photos to a third party; this is free, deterministic, and stays on the server
// the household runs. Layout + painting live in collage.ts.
import { ref, computed, onMounted, nextTick, watch } from 'vue'
import { api } from '../api'
import { COLLAGE_W, COLLAGE_H, COLLAGE_FONT, drawCollage, type CollageMember } from '../collage'
import { andList, petColors } from '../format'
import type { Household, PetSummary } from '../types'
import SheetShell from './SheetShell.vue'
import SpeciesMark from './SpeciesMark.vue'

const props = defineProps<{ pets: PetSummary[] }>()
const emit = defineEmits<{ close: []; saved: [household: Household]; upload: [] }>()

// The roster is frozen when the sheet opens, so the ids saved always describe
// the pets actually painted, even if the parent's list changes underneath.
const roster = props.pets.slice()

const canvas = ref<HTMLCanvasElement | null>(null)
const marks = ref<HTMLElement | null>(null)
const names = ref(true)
const loading = ref(true)
const saving = ref(false)
const error = ref('')
let members: CollageMember[] = []
// Who ended up as a colour portrait: no photo set, or one that failed to load.
const missing = ref<string[]>([])
const missingNote = computed(() => {
  const n = missing.value
  if (!n.length) return ''
  const list = andList(n)
  return n.length === 1
    ? `No profile photo to use for ${list}, so a colour portrait stands in. Add one from their profile, then regenerate.`
    : `No profile photos to use for ${list}, so colour portraits stand in. Add them from each profile, then regenerate.`
})

// Resolves null instead of rejecting: a photo that won't load just falls back
// to the pet's colour portrait rather than blocking the whole collage.
function loadImage(src: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const im = new Image()
    im.onload = () => resolve(im)
    im.onerror = () => resolve(null)
    im.src = src
  })
}

// The species watermark is the real SpeciesMark component, rendered hidden
// below and rasterised from its SVG, so the canvas can't drift from the cards.
function markFor(id: string): Promise<HTMLImageElement | null> {
  // Pet ids are slugs (lowercase alnum + hyphens), safe inside the quoted selector.
  const svg = marks.value && marks.value.querySelector(`[data-pet="${id}"]`)
  if (!svg) return Promise.resolve(null)
  const xml = new XMLSerializer().serializeToString(svg)
  return loadImage(`data:image/svg+xml;charset=utf-8,${encodeURIComponent(xml)}`)
}

function font() {
  return getComputedStyle(document.documentElement).getPropertyValue('--font-d').trim() || COLLAGE_FONT
}

// A null context (e.g. jsdom) is a no-op: geometry is covered by collage.ts
// tests and the pixels by the screenshot probe.
function paint() {
  const c = canvas.value && canvas.value.getContext('2d')
  if (!c) return
  drawCollage(c, members, { names: names.value, font: font() })
}

onMounted(async () => {
  await nextTick()
  members = await Promise.all(
    roster.map(async (p) => {
      const url = api.petPhotoUrl(p)
      const photo = url ? await loadImage(url) : null
      return { name: p.name, colors: petColors(p), photo, mark: photo ? null : await markFor(p.id) }
    }),
  )
  missing.value = members.filter((m) => !m.photo).map((m) => m.name)
  loading.value = false
  paint()
})
watch(names, paint)

async function save() {
  saving.value = true
  error.value = ''
  try {
    paint()
    const c = canvas.value
    if (!c) return
    const data_base64 = c.toDataURL('image/jpeg', 0.9).split(',')[1] || ''
    const household = await api.setHouseholdPortrait({
      media_type: 'image/jpeg',
      data_base64,
      source: 'generated',
      pet_ids: roster.map((p) => p.id),
    })
    emit('saved', household)
  } catch (err) {
    error.value = err.message || 'Could not save the portrait.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <SheetShell testid="portrait-sheet" @close="emit('close')">
    <div class="sheet-head">
      <h3>Family portrait</h3>
      <small>Made right here from everyone's profile photos. No AI, and nothing is sent anywhere.</small>
    </div>

    <div class="sheet-body">
      <div class="cropwrap collage-wrap">
        <canvas ref="canvas" class="cropcanvas" :width="COLLAGE_W" :height="COLLAGE_H" data-testid="portrait-preview"></canvas>
        <div v-if="loading" class="cropempty"><span>Gathering everyone…</span></div>
      </div>
      <label class="toggle">
        <input v-model="names" type="checkbox" data-testid="portrait-names" />
        Show names
      </label>
      <p v-if="missingNote" class="collage-note" data-testid="portrait-missing">{{ missingNote }}</p>
      <p v-if="error" style="color:var(--crit);font-size:13px;margin:8px 0 0">{{ error }}</p>
      <div ref="marks" hidden>
        <SpeciesMark
          v-for="p in roster" :key="p.id" :species="p.species" :data-pet="p.id"
          width="240" height="240" style="color:#fff"
        />
      </div>
    </div>

    <div class="sheet-foot">
      <button class="btn sec" type="button" data-testid="portrait-use-photo" @click="emit('upload')">Use a photo instead</button>
      <button class="btn primary" data-testid="portrait-save" :disabled="loading || saving || !roster.length" @click="save">
        {{ saving ? 'Saving…' : 'Save portrait' }}
      </button>
    </div>
  </SheetShell>
</template>
