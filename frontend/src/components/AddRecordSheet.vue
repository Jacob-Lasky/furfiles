<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import { api } from '../api'
import { documentFile, looksHeic } from '../imagefile'
import { todayISO, CATEGORIES, fileSize, isImage, readDataUrl, centsFromDollars, itemsToLineItems, rowsToExtra } from '../format'
import type { RecordFormFields, LineItemRow, ExtraRow } from '../format'
import type { PetSummary, MedicalRecord, Reminder, Contact } from '../types'
import SheetShell from './SheetShell.vue'
import RecordForm from './RecordForm.vue'

// One modal for BOTH ways to add a record (#4): type it in, or drop a document
// and let the model pre-fill it. Documents are first-class — you can attach a
// photo/PDF to a manual record too; LLM autofill is just a layer on top. The
// record fields themselves live in the shared RecordForm (#15) so the add and
// edit sheets can't drift.
const props = withDefaults(defineProps<{ defaultPetId?: string | null }>(), { defaultPetId: null })
const emit = defineEmits<{
  close: []
  created: [payload: { petId: string; record: MedicalRecord }]
}>()

const LAST_PET_KEY = 'furfiles-last-pet'

// A file chosen in the picker, held until the record is saved then uploaded.
interface PickedFile { name: string; media_type: string; size: number; data: string; scanned: boolean; uploaded: boolean }
// A scan's proposed weight log (#26), pending user confirmation.
interface WeightProposal { value: string; unit: string; keep: boolean; logged: boolean }
// A scan's proposed calendar reminder (#26); `duplicate` flags one already set.
interface ReminderProposal { due_on: string; title: string; keep: boolean; duplicate: boolean; created: boolean }

const saving = ref(false)
const error = ref('')
const pets = ref<PetSummary[]>([])
const contacts = ref<Contact[]>([]) // shared clinics/vets to pick from (#20)
const loadingPets = ref(true)
const canExtract = ref(false) // ingestion enabled + a key is stored
const extracting = ref(false)
const converting = ref(false) // turning an iPhone HEIC into a JPEG before it's attached
const lowConf = ref<string[]>([]) // field labels the model was unsure about
// Proposed updates to OTHER parts of the app from a scan (#26), confirmed by the
// user before anything is written.
const weightProposal = ref<WeightProposal | null>(null)
const reminderProposals = ref<ReminderProposal[]>([])

const form = reactive<RecordFormFields & { pet_id: string }>({
  pet_id: '', occurred_on: todayISO(), ends_on: '', title: '', category: CATEGORIES[0], cost: '', notes: '', contact_id: '',
})
// Editable label/value detail rows — the per-record custom fields (#4). Filled
// by extraction, and freely renamable/addable/removable by hand.
const rows = ref<ExtraRow[]>([])
// Editable {desc, amount(dollars-string)} cost-breakdown rows; the total
// auto-sums from these when the Cost field is left blank.
const items = ref<LineItemRow[]>([])
// Files chosen but not yet uploaded; attached to the record after it is saved.
const files = ref<PickedFile[]>([])
// Once the record row exists, keep it so a retry (e.g. after an upload hiccup)
// doesn't create a duplicate.
const createdRecord = ref<MedicalRecord | null>(null)

function lastPet(): string | null {
  try { return localStorage.getItem(LAST_PET_KEY) } catch (_) { return null }
}
function rememberPet(id: string) {
  try { localStorage.setItem(LAST_PET_KEY, id) } catch (_) { /* private mode */ }
}

onMounted(async () => {
  try {
    pets.value = (await api.pets(false)) || []
  } catch (e) {
    error.value = e.message || 'Could not load pets.'
  } finally {
    loadingPets.value = false
  }
  const ids = pets.value.map((p) => p.id)
  const preferred = [props.defaultPetId, lastPet()].find((id) => id && ids.includes(id))
  form.pet_id = preferred || ids[0] || ''
  // Shared contacts to offer as the clinic/vet (#20); best-effort.
  try { contacts.value = (await api.contacts()) || [] } catch (_) { /* non-fatal */ }
  // Is document scanning available? (optional, off by default)
  try {
    const s = await api.ingestion()
    canExtract.value = !!(s && s.enabled && s.has_key)
  } catch (_) { canExtract.value = false }
})

// ---- attachments ----

function fileToBase64(file: Blob): Promise<string> {
  // readDataUrl yields "data:<mime>;base64,<data>"; keep just the data tail.
  return readDataUrl(file).then((u) => u.split(',')[1] || '')
}

function extractable(mediaType: string) {
  return mediaType === 'application/pdf' || (mediaType || '').startsWith('image/')
}

async function onFiles(e: Event) {
  const input = e.target as HTMLInputElement
  const chosen = Array.from(input.files || [])
  input.value = '' // allow re-picking the same file
  error.value = ''
  for (const picked of chosen) {
    try {
      // An iPhone HEIC becomes a JPEG here, so the preview, the scan and the stored
      // attachment are all the one readable file (#71). Anything else passes through.
      if (looksHeic(picked)) converting.value = true
      const file = await documentFile(picked)
      const media_type =
        file.type || (file.name.toLowerCase().endsWith('.pdf') ? 'application/pdf' : 'application/octet-stream')
      const data = await fileToBase64(file)
      files.value.push({
        name: file.name, media_type, size: file.size, data, scanned: false, uploaded: false,
      })
    } catch (err) {
      error.value = looksHeic(picked) ? err.message : `Could not read "${picked.name}".`
    } finally {
      converting.value = false
    }
  }
}

function removeFile(i: number) {
  files.value.splice(i, 1)
}

function previewUrl(f: PickedFile) {
  return `data:${f.media_type};base64,${f.data}`
}

// First attached document we could read with the model.
const scannableIndex = computed(() => files.value.findIndex((f) => extractable(f.media_type)))

// ---- extraction ----

async function runExtract() {
  const i = scannableIndex.value
  if (i < 0) return
  extracting.value = true
  error.value = ''
  try {
    const f = files.value[i]
    const res = await api.extract({ filename: f.name, media_type: f.media_type, data_base64: f.data })
    const x = res.fields
    if (x.title) form.title = x.title
    if (x.occurred_on) form.occurred_on = x.occurred_on
    if (x.category && CATEGORIES.includes(x.category)) form.category = x.category
    if (x.cost != null) form.cost = String(x.cost)
    // Fold the clinic into notes so nothing is lost, without a dedicated column.
    const noteParts: string[] = []
    if (x.clinic) noteParts.push(x.clinic)
    if (x.notes) noteParts.push(x.notes)
    if (noteParts.length) form.notes = noteParts.join(' — ')
    // Cost breakdown → editable line-item rows (model gives dollar amounts).
    if (Array.isArray(x.line_items) && x.line_items.length) {
      items.value = x.line_items.map((li) => ({
        desc: li.description || '',
        amount: li.amount != null ? String(li.amount) : '',
      }))
    }
    // Merge extra fields into the editable rows (existing rows win, so a manual
    // edit isn't clobbered by re-running extraction).
    const have = new Set(rows.value.map((r) => r.key.trim().toLowerCase()))
    for (const [k, v] of Object.entries(x.extra_fields || {})) {
      if (!have.has(k.trim().toLowerCase())) rows.value.push({ key: k, val: String(v) })
    }
    // Weight → propose logging it to the pet's history.
    if (x.weight && x.weight.value != null) {
      weightProposal.value = { value: String(x.weight.value), unit: x.weight.unit || 'lb', keep: true, logged: false }
    }
    // Reminders → propose calendar entries, flagging ones already scheduled.
    if (Array.isArray(x.reminders) && x.reminders.length) {
      let existing: Reminder[] = []
      try { existing = (await api.reminders(form.pet_id)) || [] } catch (_) { /* best-effort */ }
      const key = (d: string | null, t: string) => `${d}|${(t || '').trim().toLowerCase()}`
      const have = new Set(existing.map((r) => key(r.due_on, r.title)))
      reminderProposals.value = x.reminders
        .filter((r) => r.due_on) // a calendar entry needs a date
        .map((r) => {
          const duplicate = have.has(key(r.due_on, r.title))
          return { due_on: r.due_on ?? '', title: r.title || '', duplicate, keep: !duplicate, created: false }
        })
    }
    lowConf.value = Array.isArray(x.low_confidence_fields) ? x.low_confidence_fields : []
    f.scanned = true // this file is the scanned source
  } catch (e) {
    error.value = e.message || 'Could not read that document.'
  } finally {
    extracting.value = false
  }
}

const usedScan = computed(() => files.value.some((f) => f.scanned))

async function submit() {
  if (!form.pet_id) { error.value = 'Pick which pet this is for.'; return }
  if (!form.title.trim()) { error.value = 'Give the record a title.'; return }
  saving.value = true
  error.value = ''
  try {
    if (!createdRecord.value) {
      const lineItems = itemsToLineItems(items.value)
      // Total = the Cost field if set, else the sum of the breakdown.
      const breakdownCents = lineItems.reduce((s, li) => s + (li.amount_cents || 0), 0)
      const cost_cents = form.cost
        ? centsFromDollars(form.cost)
        : lineItems.length
          ? breakdownCents
          : null
      const payload = {
        occurred_on: form.occurred_on || todayISO(),
        ends_on: form.ends_on || null,
        title: form.title.trim(),
        category: form.category,
        cost_cents,
        notes: form.notes.trim() || null,
        source: usedScan.value ? 'scan' : 'manual',
        contact_id: form.contact_id || null,
        extra: rowsToExtra(rows.value),
        line_items: lineItems.length ? lineItems : undefined,
      }
      createdRecord.value = await api.addRecord(form.pet_id, payload)
    }
    const created = createdRecord.value
    if (!created) return
    const rid = created.id
    // Upload any not-yet-uploaded files; a retry only re-sends the failures.
    for (const f of files.value) {
      if (f.uploaded) continue
      await api.addAttachment(rid, {
        filename: f.name, media_type: f.media_type, data_base64: f.data,
        source: f.scanned ? 'scan' : 'upload',
      })
      f.uploaded = true
    }
    // Confirmed cross-app updates (#26). Guarded by flags so a retry after a
    // mid-way failure doesn't double-write (weight has no server-side dedup).
    const wp = weightProposal.value
    if (wp && wp.keep && !wp.logged && wp.value) {
      await api.addWeight(form.pet_id, {
        recorded_on: form.occurred_on || todayISO(),
        value: parseFloat(wp.value),
        unit: wp.unit || 'lb',
      })
      wp.logged = true
    }
    for (const r of reminderProposals.value) {
      if (r.keep && !r.created && r.title.trim() && r.due_on) {
        await api.addReminder(form.pet_id, { due_on: r.due_on, title: r.title.trim(), source: 'scan' })
        r.created = true
      }
    }
    rememberPet(form.pet_id)
    emit('created', { petId: form.pet_id, record: created })
  } catch (e) {
    error.value = e.message || 'Could not save.'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <SheetShell testid="add-sheet" @close="emit('close')">
    <div class="sheet-head">
      <h3>Add a record</h3>
      <small>A visit, a dose, a receipt — kept forever.</small>
    </div>
    <!-- Bulk import (#23) is the faster path for a stack of documents: no pet
         to pick, one file can hold many records. Offer it where people start. -->
    <router-link to="/import" class="imp-link" data-testid="add-import-link" @click="emit('close')">
      Got a stack of documents, maybe for several pets? <b>Import them all at once</b>
    </router-link>

    <div class="sheet-body">
      <div v-if="!loadingPets && !pets.length" class="drop" style="cursor:default">
        <div class="t">No pets yet</div>
        <div class="s">Add a pet first — every record belongs to one.</div>
      </div>

      <template v-else>
        <div class="field">
          <label>Pet</label>
          <select v-model="form.pet_id">
            <option v-for="p in pets" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </div>

        <!-- Documents: attach photos/PDFs; optionally let the model read one. -->
        <div class="field">
          <label>Documents</label>
          <div v-for="(f, i) in files" :key="i" class="frow">
            <img v-if="isImage(f.media_type)" class="fthumb" :src="previewUrl(f)" alt="" />
            <span v-else class="fthumb doc">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><path d="M14 2v6h6"/></svg>
            </span>
            <span class="fmeta"><b>{{ f.name }}</b><small>{{ fileSize(f.size) }}<span v-if="f.scanned"> · scanned</span></small></span>
            <button class="fx" type="button" aria-label="Remove file" @click="removeFile(i)">×</button>
          </div>

          <label class="filepick">
            <input type="file" multiple :disabled="converting" @change="onFiles" style="display:none" data-testid="add-file-input" />
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M12 5v14M5 12h14"/></svg>
            {{ converting ? 'Converting the iPhone photo…' : 'Add a photo or document' }}
          </label>

          <div v-if="canExtract && scannableIndex >= 0" class="scanbar">
            <button class="scanbtn" type="button" :disabled="extracting" @click="runExtract" data-testid="add-extract-btn">
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l2.2 6.5L21 10l-6.8 1.5L12 18l-2.2-6.5L3 10l6.8-1.5z"/></svg>
              {{ extracting ? 'Reading…' : 'Extract details with AI' }}
            </button>
            <p class="privacy">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
              <span>The document is sent to your configured AI model to read it. Nothing is saved until you tap Save.</span>
            </p>
          </div>
          <p v-else-if="scannableIndex >= 0" class="hint">
            Want this read automatically? Turn on document scanning in <router-link to="/settings">Settings</router-link>.
          </p>
        </div>

        <RecordForm :form="form" :items="items" :rows="rows" :contacts="contacts" />

        <p v-if="lowConf.length" class="doublecheck">Double-check: {{ lowConf.join(', ') }}</p>

        <!-- Cross-app updates proposed by a scan (#26); nothing written til Save. -->
        <div v-if="weightProposal || reminderProposals.length" class="field alsoupdate">
          <label>Also update from this document</label>
          <div v-if="weightProposal" class="au-row">
            <input type="checkbox" v-model="weightProposal.keep" aria-label="Log weight" />
            <span class="au-lbl">Log weight</span>
            <input v-model="weightProposal.value" type="number" step="0.1" class="au-wval" aria-label="Weight value" />
            <select v-model="weightProposal.unit" class="au-wunit" aria-label="Weight unit"><option>lb</option><option>kg</option></select>
          </div>
          <div v-for="(r, i) in reminderProposals" :key="i" class="au-row">
            <input type="checkbox" v-model="r.keep" :aria-label="`Add reminder ${r.title}`" />
            <span class="au-rem"><b>{{ r.due_on }}</b> {{ r.title }}</span>
            <span v-if="r.duplicate" class="au-dup">already on calendar</span>
          </div>
        </div>

        <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0" data-testid="add-error">{{ error }}</p>
      </template>
    </div>

    <div class="sheet-foot">
      <button class="btn sec" type="button" @click="emit('close')">Cancel</button>
      <button class="btn primary" data-testid="add-submit" :disabled="saving || converting || !pets.length" @click="submit">
        {{ saving ? 'Saving…' : 'Save record' }}
      </button>
    </div>
  </SheetShell>
</template>
