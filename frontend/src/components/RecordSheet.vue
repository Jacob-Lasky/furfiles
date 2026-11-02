<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import { api } from '../api'
import {
  todayISO, fileSize, isImage, isPdf, money, categoryColor, contactRoleLabel,
  centsFromDollars, dollarsFromCents, itemsToLineItems, lineItemsToItems, rowsToExtra, extraToRows,
} from '../format'
import type { RecordFormFields, LineItemRow, ExtraRow } from '../format'
import type { RecordLike, MedicalRecord, Attachment, Contact } from '../types'
import SheetShell from './SheetShell.vue'
import RecordForm from './RecordForm.vue'
import RecordDetails from './RecordDetails.vue'

// View / edit / delete an existing record (#15, #44). Opened by tapping a timeline
// record or a calendar event (#16), so it works standalone from either surface.
// Opens in a read-only, screenshot-worthy VIEW first (#44): the record laid out
// nicely with its attachments rendered inline (images + PDFs, not new-tab links).
// An explicit Edit button flips it in place to the edit form (Delete/Cancel/Save),
// which shares RecordForm with the add sheet so the fields can't drift. Cancel
// returns to the view rather than closing — the view is the resting state.
const props = withDefaults(defineProps<{
  petId: string
  petName?: string
  // The record to view/edit. May be a timeline RecordDetail (with `attachments`
  // and a resolved `contact`) or a calendar MedicalRecord (with neither) —
  // attachments are fetched, and the clinic resolved from contact_id, if absent.
  record: RecordLike
}>(), { petName: '' })
const emit = defineEmits<{
  close: []
  saved: [payload: { petId: string; record: MedicalRecord }]
  deleted: [payload: { petId: string; id: number }]
}>()

const mode = ref<'view' | 'edit'>('view')
const saving = ref(false)
const deleting = ref(false)
const error = ref('')
const attachments = ref<Attachment[]>([])
const contacts = ref<Contact[]>([]) // shared clinics/vets to pick from (#20)

const form = reactive<RecordFormFields>({
  occurred_on: props.record.occurred_on || todayISO(),
  ends_on: props.record.ends_on || '',
  title: props.record.title || '',
  category: props.record.category || 'Appointments',
  cost: dollarsFromCents(props.record.cost_cents),
  notes: props.record.notes || '',
  contact_id: props.record.contact_id || '',
})
const items = ref<LineItemRow[]>(lineItemsToItems(props.record.line_items))
const rows = ref<ExtraRow[]>(extraToRows(props.record.extra))

// The clinic/vet shown in the view: the record's resolved contact when it carried
// one (timeline), else looked up from contact_id in the shared list (calendar).
const clinic = computed<Contact | null>(() => {
  const rc = (props.record as { contact?: Contact | null }).contact
  if (rc) return rc
  if (!props.record.contact_id) return null
  return contacts.value.find((c) => c.id === props.record.contact_id) || null
})

onMounted(async () => {
  if (Array.isArray(props.record.attachments)) {
    attachments.value = [...props.record.attachments]
  } else {
    try { attachments.value = (await api.recordAttachments(props.record.id)) || [] } catch (_) { /* non-fatal */ }
  }
  try { contacts.value = (await api.contacts()) || [] } catch (_) { /* non-fatal */ }
})

async function removeAttachment(a: Attachment) {
  try {
    await api.deleteAttachment(a.id)
    attachments.value = attachments.value.filter((x) => x.id !== a.id)
  } catch (e) {
    error.value = e.message || 'Could not remove that file.'
  }
}

async function save() {
  if (!form.title.trim()) { error.value = 'Give the record a title.'; return }
  saving.value = true
  error.value = ''
  try {
    const lineItems = itemsToLineItems(items.value)
    const breakdownCents = lineItems.reduce((s, li) => s + (li.amount_cents || 0), 0)
    // Cost = the field if set, else the sum of the breakdown, else cleared.
    const cost_cents = form.cost !== '' ? centsFromDollars(form.cost) : lineItems.length ? breakdownCents : null
    // A full overwrite: send extra/line_items even when empty (null / []) so
    // clearing them on the edit sheet actually clears them server-side.
    const updated = await api.updateRecord(props.petId, props.record.id, {
      occurred_on: form.occurred_on,
      ends_on: form.ends_on || null,
      title: form.title.trim(),
      category: form.category,
      cost_cents,
      notes: form.notes.trim() || null,
      contact_id: form.contact_id || null,
      extra: rowsToExtra(rows.value) ?? null,
      line_items: lineItems,
    })
    emit('saved', { petId: props.petId, record: updated })
  } catch (e) {
    error.value = e.message || 'Could not save.'
  } finally {
    saving.value = false
  }
}

async function remove() {
  if (!window.confirm(`Delete “${props.record.title}”? This can't be undone.`)) return
  deleting.value = true
  error.value = ''
  try {
    await api.deleteRecord(props.petId, props.record.id)
    emit('deleted', { petId: props.petId, id: props.record.id })
  } catch (e) {
    error.value = e.message || 'Could not delete.'
    deleting.value = false
  }
}
</script>

<template>
  <SheetShell testid="record-sheet" @close="emit('close')">
    <!-- ============ VIEW: read-only, screenshot-worthy (#44) ============ -->
    <template v-if="mode === 'view'">
      <div class="sheet-head">
        <h3>{{ record.title }}</h3>
        <small>
          <span class="rv-cat" :style="{ background: categoryColor(record.category) }"></span>{{ record.category }}
          · {{ record.occurred_on }}<span v-if="record.ends_on"> – {{ record.ends_on }}</span>
          <template v-if="petName"> · {{ petName }}</template>
        </small>
      </div>

      <div class="sheet-body" data-testid="record-view">
        <div v-if="record.cost_cents" class="rv-cost">
          {{ money(record.cost_cents) }}
          <small v-if="record.covered_cents">{{ money(record.covered_cents) }} back</small>
        </div>

        <!-- scan badge + notes + cost breakdown + custom fields, shared with the
             timeline so the two surfaces can't drift. -->
        <RecordDetails :record="record" />

        <!-- The clinic/vet this happened at (#20). -->
        <div v-if="clinic" class="rv-clinic">
          <span class="role-badge" :data-role="clinic.role" aria-hidden="true">{{ clinic.name[0] }}</span>
          <span class="rv-clinic-body">
            <b>{{ clinic.name }}</b>
            <small>{{ contactRoleLabel(clinic.role) }}<template v-if="clinic.phone"> · {{ clinic.phone }}</template></small>
          </span>
        </div>

        <!-- Attachments rendered inline (#44): images and PDFs show in-app; any
             other file type stays a download chip. -->
        <div v-if="attachments.length" class="rv-atts" data-testid="record-view-atts">
          <figure v-for="a in attachments" :key="a.id" class="rv-att">
            <img v-if="isImage(a.media_type)" class="rv-img" :src="api.attachmentUrl(a.id)" :alt="a.filename" />
            <iframe v-else-if="isPdf(a.media_type)" class="rv-pdf" :src="api.attachmentUrl(a.id)" :title="a.filename" data-testid="record-view-pdf"></iframe>
            <a v-else class="att" :href="api.attachmentUrl(a.id)" target="_blank" rel="noopener">
              <span class="att-doc"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><path d="M14 2v6h6"/></svg></span>
              <span class="att-name">{{ a.filename }}<small>{{ fileSize(a.size_bytes) }}</small></span>
            </a>
            <figcaption>
              <a :href="api.attachmentUrl(a.id)" target="_blank" rel="noopener">{{ a.filename }}</a>
              <small>{{ fileSize(a.size_bytes) }}</small>
            </figcaption>
          </figure>
        </div>

        <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
      </div>

      <div class="sheet-foot">
        <button class="btn sec" type="button" data-testid="record-close" @click="emit('close')">Close</button>
        <button class="btn primary" type="button" data-testid="record-edit" @click="mode = 'edit'">Edit</button>
      </div>
    </template>

    <!-- ============ EDIT: the existing form (Delete / Cancel / Save) ========= -->
    <template v-else>
      <div class="sheet-head">
        <h3>Edit record</h3>
        <small>{{ petName ? petName + ' · ' : '' }}change any detail, or delete it.</small>
      </div>

      <div class="sheet-body">
        <RecordForm :form="form" :items="items" :rows="rows" :contacts="contacts" />

        <!-- Existing attachments: view, or remove (removal is immediate). -->
        <div v-if="attachments.length" class="field">
          <label>Documents</label>
          <div class="atts">
            <span v-for="a in attachments" :key="a.id" class="attwrap">
              <a class="att" :href="api.attachmentUrl(a.id)" target="_blank" rel="noopener">
                <img v-if="isImage(a.media_type)" :src="api.attachmentUrl(a.id)" alt="" />
                <span v-else class="att-doc"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><path d="M14 2v6h6"/></svg></span>
                <span class="att-name">{{ a.filename }}<small>{{ fileSize(a.size_bytes) }}</small></span>
              </a>
              <button class="fx" type="button" :aria-label="`Remove ${a.filename}`" @click="removeAttachment(a)">×</button>
            </span>
          </div>
        </div>

        <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
      </div>

      <div class="sheet-foot">
        <button class="btn danger" type="button" data-testid="record-delete" :disabled="deleting || saving" @click="remove">
          {{ deleting ? 'Deleting…' : 'Delete' }}
        </button>
        <button class="btn sec" type="button" data-testid="record-cancel" :disabled="saving || deleting" @click="mode = 'view'">Cancel</button>
        <button class="btn primary" data-testid="record-save" :disabled="saving || deleting" @click="save">
          {{ saving ? 'Saving…' : 'Save changes' }}
        </button>
      </div>
    </template>
  </SheetShell>
</template>

<style scoped>
/* View mode (#44): the read-only, share-worthy layout. */
.rv-cat { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; vertical-align: middle; }
.rv-cost { font-family: var(--font-m); font-weight: 700; font-size: 26px; margin: 2px 0 6px; }
.rv-cost small { font-family: var(--font-b); font-weight: 600; font-size: 13px; color: var(--good); margin-left: 8px; }
.rv-clinic { display: flex; align-items: center; gap: 12px; margin-top: 14px; padding-top: 14px; border-top: 1px solid var(--border); }
.rv-clinic-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.rv-clinic-body b { font-size: 15px; }
.rv-clinic-body small { color: var(--ink-2); font-size: 12.5px; }
/* Attachments rendered inline: images and PDFs on their own, full-width. */
.rv-atts { display: flex; flex-direction: column; gap: 14px; margin-top: 16px; }
.rv-att { margin: 0; }
.rv-img { display: block; width: 100%; max-height: 60vh; object-fit: contain; border-radius: 12px; background: var(--surface-2); }
.rv-pdf { display: block; width: 100%; height: 62vh; border: 1px solid var(--border); border-radius: 12px; background: var(--surface-2); }
.rv-att figcaption { display: flex; align-items: baseline; gap: 8px; margin-top: 6px; font-size: 12.5px; }
.rv-att figcaption a { color: var(--brand); font-weight: 600; text-decoration: none; }
.rv-att figcaption small { color: var(--ink-3); }
</style>
