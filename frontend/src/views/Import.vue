<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { api } from '../api'
import { readDataUrl } from '../format'
import { documentFile, looksHeic } from '../imagefile'
import { mediaTypeOf, importable, unsupportedReason, runLimited, hasWork, recordsNeedingPet } from '../importer'
import type { ImportEntry } from '../importer'
import { bumpData } from '../store'
import type { PetSummary } from '../types'
import ImportFileCard from '../components/ImportFileCard.vue'

// Bulk import (#23): ONE place to drop every document — no pet picked first.
// Each file is read by the model into a plan (records for whichever pets it
// concerns, plus weights, reminders, profile and care updates), checked against
// what's already saved, and shown for review. Nothing is written until Save.

const ANALYZE_CONCURRENCY = 2

const pets = ref<PetSummary[]>([])
const entries = ref<ImportEntry[]>([])
const canImport = ref<boolean | null>(null) // null = still checking
const dragging = ref(false)
const savingAll = ref(false)
let nextId = 1

onMounted(async () => {
  try { pets.value = (await api.pets(true)) || [] } catch (_) { pets.value = [] }
  try {
    const s = await api.ingestion()
    canImport.value = !!(s && s.enabled && s.has_key)
  } catch (_) { canImport.value = false }
})

async function analyze(e: ImportEntry) {
  e.status = 'analyzing'
  e.error = ''
  try {
    e.plan = await api.importAnalyze({ filename: e.name, media_type: e.media_type, data_base64: e.data })
    e.status = 'ready'
  } catch (err) {
    e.status = 'error'
    e.error = err.message || 'Could not read this document.'
  }
}

// Get one dropped file ready to send: an iPhone photo becomes a JPEG here, once,
// so analyze and commit both send the SAME bytes (the server hashes them for its
// duplicate check). Sets the entry to 'queued' on success, else 'unsupported'/'error'.
async function prepare(e: ImportEntry, f: File) {
  let file = f
  if (looksHeic(f)) {
    e.status = 'converting'
    try {
      file = await documentFile(f)
    } catch (err) {
      e.status = 'error'
      e.error = err.message
      return
    }
    e.name = file.name
    e.media_type = file.type
    e.size = file.size
  }
  if (!importable(e.media_type)) {
    e.status = 'unsupported'
    e.error = unsupportedReason(e.media_type)
    return
  }
  try {
    e.data = (await readDataUrl(file)).split(',')[1] || ''
    e.status = 'queued'
  } catch (_) {
    e.status = 'error'
    e.error = 'Could not read this file from your device.'
  }
}

async function addFiles(files: File[]) {
  const fresh: [ImportEntry, File][] = []
  for (const f of files) {
    entries.value.push({
      id: nextId++, name: f.name, media_type: mediaTypeOf(f), size: f.size, data: '',
      status: 'queued', plan: null, result: null, error: '',
    })
    // The reactive proxy, not the pushed literal: later writes must be seen by the page.
    fresh.push([entries.value[entries.value.length - 1], f])
  }
  // Prepare and analyze each file as a unit, two at a time, so the first
  // analysis starts without waiting for every HEIC in the drop to convert.
  // A card the person removed while it waited is never converted or sent to the provider.
  const present = (e: ImportEntry) => entries.value.some((x) => x.id === e.id)
  await runLimited(fresh, ANALYZE_CONCURRENCY, async ([e, f]) => {
    if (!present(e)) return
    await prepare(e, f)
    if (e.status === 'queued' && present(e)) await analyze(e)
  })
}

function onPick(ev: Event) {
  const input = ev.target as HTMLInputElement
  const files = Array.from(input.files || [])
  input.value = '' // allow re-picking the same file
  if (files.length) addFiles(files)
}

function onDrop(ev: DragEvent) {
  dragging.value = false
  const files = Array.from(ev.dataTransfer?.files || [])
  if (files.length) addFiles(files)
}

async function save(e: ImportEntry) {
  if (!e.plan) return
  e.status = 'saving'
  e.error = ''
  try {
    e.result = await api.importCommit({ filename: e.name, media_type: e.media_type, data_base64: e.data, plan: e.plan })
    e.status = 'saved'
    e.data = '' // the bytes are on the server now; don't hold megabytes in memory
    bumpData()
    // A rename or a new pet-level fact shows up in later plans' pickers.
    try { pets.value = (await api.pets(true)) || pets.value } catch (_) { /* keep the old list */ }
  } catch (err) {
    e.status = 'ready'
    e.error = err.message || 'Could not save.'
  }
}

function remove(e: ImportEntry) {
  entries.value = entries.value.filter((x) => x.id !== e.id)
}

// Files that can be saved as they stand: read, something selected, every
// selected record has a pet.
const saveable = computed(() =>
  entries.value.filter((e) => e.status === 'ready' && e.plan && hasWork(e.plan) && recordsNeedingPet(e.plan) === 0),
)
const busy = computed(() => entries.value.some((e) => e.status === 'analyzing' || e.status === 'queued' || e.status === 'converting'))

async function saveAll() {
  savingAll.value = true
  // One at a time: a later file's duplicate check should see what an earlier one saved.
  for (const e of saveable.value.slice()) await save(e)
  savingAll.value = false
}
</script>

<template>
  <div data-testid="import-page">
    <div class="apphead">
      <div>
        <div class="eyebrow">Import</div>
        <h1>Drop in your records</h1>
        <div class="sub">Invoices, visit summaries, vaccine certificates, whole clinic histories — for any of your pets, all at once.</div>
      </div>
    </div>

    <div class="pad">
      <div v-if="canImport === false" class="card card-pad imp-setup" data-testid="import-setup">
        <b>Importing reads your documents with AI, which isn't set up yet.</b>
        <p>Add your API key in <router-link to="/settings">Settings</router-link> and switch scanning on. Your key stays on this server.</p>
      </div>

      <template v-else>
        <label
          class="drop imp-drop"
          :class="{ over: dragging }"
          data-testid="import-drop"
          @dragover.prevent="dragging = true"
          @dragleave.prevent="dragging = false"
          @drop.prevent="onDrop"
        >
          <input type="file" multiple accept="application/pdf,image/*" class="imp-input" data-testid="import-input" @change="onPick" />
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M17 8l-5-5-5 5M12 3v12"/></svg>
          <div class="t">Drop files here, or tap to choose</div>
          <div class="s">PDFs and photos, iPhone ones included. FurFiles works out whose each record is, skips what you already have, and shows you everything before saving.</div>
        </label>
        <div class="privacy">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
          <span>Nothing's saved until you say so. Each document goes to your AI provider to be read, along with your pets' profiles and a list of what's already saved (record titles, dates and totals, reminders, weights, clinics) so it can match pets and skip duplicates.</span>
        </div>

        <div v-if="entries.length" class="imp-bar">
          <span>{{ entries.length }} file{{ entries.length === 1 ? '' : 's' }}<template v-if="busy"> · reading…</template></span>
          <button v-if="saveable.length > 1" class="btn primary" :disabled="savingAll" data-testid="import-save-all" @click="saveAll">
            {{ savingAll ? 'Saving…' : `Save all ${saveable.length} ready` }}
          </button>
        </div>

        <div class="imp-list">
          <div v-for="e in entries" :key="e.id">
            <ImportFileCard :entry="e" :pets="pets" @save="save(e)" @remove="remove(e)" @retry="analyze(e)" />
            <div v-if="e.status === 'ready' && e.error" class="errbox imp-saveerr">{{ e.error }}</div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.imp-setup p { margin: 6px 0 0; color: var(--ink-2); font-size: 14px; }
.imp-setup a { color: var(--brand); font-weight: 600; }
.imp-drop { display: block; cursor: pointer; margin-top: 10px; transition: border-color .15s, background .15s; }
.imp-drop.over { border-color: var(--brand); background: var(--brand-soft); }
.imp-input { position: absolute; width: 1px; height: 1px; opacity: 0; pointer-events: none; }
.imp-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin: 22px 0 12px; font-size: 13px; color: var(--ink-3); font-weight: 600; }
.imp-bar .btn { flex: 0 0 auto; padding-left: 20px; padding-right: 20px; }
.imp-saveerr { padding: 0 0 14px; margin-top: -6px; text-align: left; }
</style>
