<script setup lang="ts">
// View / add / edit / delete a shared contact (#20, #44): a vet, emergency vet,
// specialist, groomer, or (via #42) daycare/boarding facility. One sheet serves
// all of it so the fields can't drift. An existing contact opens in a read-only
// VIEW first (#44) — name, role, hours, requirements, notes laid out nicely —
// with an explicit Edit button into the form (Delete/Cancel/Save); Cancel returns
// to the view. Creating a new contact goes straight to the form, and Cancel there
// closes the sheet (there is no prior view to return to).
//
// #42: an optional "import from a website" step (when AI is configured) fetches a
// facility's page and pre-fills the fields + a requirements checklist for the
// user to confirm — nothing is saved until they tap Save.
import { ref, reactive, onMounted } from 'vue'
import { api } from '../api'
import { contactRoleLabel, renderMarkdown, labeledToRows, rowsToLabeled, mergeLabeledRows } from '../format'
import type { LabeledRow } from '../format'
import { CONTACT_ROLES } from '../types'
import type { Contact, NewContactInput } from '../types'
import SheetShell from './SheetShell.vue'

const props = withDefaults(defineProps<{
  // The contact to view/edit, or null to create a new one.
  contact?: Contact | null
  // The role a freshly-created contact defaults to (e.g. from a role filter).
  defaultRole?: string
}>(), { contact: null, defaultRole: 'vet' })
const emit = defineEmits<{
  close: []
  saved: [contact: Contact]
  deleted: [id: string]
}>()

const editing = !!props.contact
// Existing contacts rest in the read-only view; a new contact starts in the form.
const mode = ref<'view' | 'edit'>(editing ? 'view' : 'edit')
const saving = ref(false)
const deleting = ref(false)
const error = ref('')

const form = reactive({
  name: props.contact?.name || '',
  role: props.contact?.role || props.defaultRole,
  doctor: props.contact?.doctor || '',
  phone: props.contact?.phone || '',
  email: props.contact?.email || '',
  address: props.contact?.address || '',
  website: props.contact?.website || '',
  notes: props.contact?.notes || '',
})
// Editable requirement/hours rows share the LabeledDetail round-trip with
// InsuranceSheet's coverages/exclusions (helpers in format.ts). add/remove are
// trivial array ops kept inline.
function addRow(rows: LabeledRow[]) { rows.push({ label: '', detail: '' }) }
function removeRow(rows: LabeledRow[], i: number) { rows.splice(i, 1) }

const reqs = ref<LabeledRow[]>(labeledToRows(props.contact?.requirements))
const hours = ref<LabeledRow[]>(labeledToRows(props.contact?.hours))

// ---- import from a website (#42) ----
const canImport = ref(false) // AI ingestion enabled + a key stored
const importUrl = ref('')
const importing = ref(false)

onMounted(async () => {
  try {
    const s = await api.ingestion()
    canImport.value = !!(s && s.enabled && s.has_key)
  } catch (_) { canImport.value = false }
})

// Cancel from the edit form: an existing contact drops back to its view, a new
// one closes the sheet (there is no view to return to).
function cancelEdit() {
  if (editing) mode.value = 'view'
  else emit('close')
}

async function parseUrl() {
  const url = importUrl.value.trim()
  if (!url) { error.value = 'Paste the facility’s website address first.'; return }
  importing.value = true
  error.value = ''
  try {
    const res = await api.parseContactUrl(url)
    const f = res.fields
    // Pre-fill only fields the parse found, without clobbering anything the user
    // already typed. The website they pasted becomes the contact's website.
    if (f.name && !form.name.trim()) form.name = f.name
    // The role only on a NEW contact still at its default: an existing contact's
    // role, or one the person already picked, is theirs.
    if (f.role && (CONTACT_ROLES as readonly string[]).includes(f.role) && !editing && form.role === props.defaultRole) form.role = f.role
    if (f.doctor && !form.doctor.trim()) form.doctor = f.doctor
    if (f.phone && !form.phone.trim()) form.phone = f.phone
    if (f.email && !form.email.trim()) form.email = f.email
    if (f.address && !form.address.trim()) form.address = f.address
    if (f.notes && !form.notes.trim()) form.notes = f.notes
    if (!form.website.trim()) form.website = url
    // Append parsed requirements + hours that aren't already listed.
    mergeLabeledRows(reqs.value, f.requirements)
    mergeLabeledRows(hours.value, f.hours)
  } catch (e) {
    error.value = e.message || 'Could not read that website.'
  } finally {
    importing.value = false
  }
}

async function submit() {
  if (!form.name.trim()) { error.value = 'Give this contact a name.'; return }
  saving.value = true
  error.value = ''
  try {
    // Empty fields save as null (cleared) rather than empty strings. requirements
    // is always sent (empty list clears it server-side).
    const payload: NewContactInput = {
      name: form.name.trim(),
      role: form.role,
      doctor: form.doctor.trim() || null,
      phone: form.phone.trim() || null,
      email: form.email.trim() || null,
      address: form.address.trim() || null,
      website: form.website.trim() || null,
      notes: form.notes.trim() || null,
      requirements: rowsToLabeled(reqs.value),
      hours: rowsToLabeled(hours.value),
    }
    const saved = props.contact
      ? await api.updateContact(props.contact.id, payload)
      : await api.createContact(payload)
    emit('saved', saved)
  } catch (e) {
    error.value = e.message || 'Could not save.'
  } finally {
    saving.value = false
  }
}

async function remove() {
  if (!props.contact) return
  if (!window.confirm(`Delete “${props.contact.name}”? It will be unlinked from every pet, but their records stay.`)) return
  deleting.value = true
  error.value = ''
  try {
    await api.deleteContact(props.contact.id)
    emit('deleted', props.contact.id)
  } catch (e) {
    error.value = e.message || 'Could not delete.'
    deleting.value = false
  }
}
</script>

<template>
  <SheetShell testid="contact-sheet" @close="emit('close')">
    <!-- ============ VIEW: read-only (#44), existing contacts only =========== -->
    <template v-if="mode === 'view' && contact">
      <div class="sheet-head">
        <div class="cv-title">
          <span class="role-badge" :data-role="contact.role" aria-hidden="true">{{ contact.name[0] }}</span>
          <div>
            <h3>{{ contact.name }}</h3>
            <small>{{ contactRoleLabel(contact.role) }}<template v-if="contact.doctor"> · {{ contact.doctor }}</template></small>
          </div>
        </div>
      </div>

      <div class="sheet-body" data-testid="contact-view">
        <div class="cv-lines">
          <a v-if="contact.phone" class="cv-line" :href="`tel:${contact.phone}`">
            <span class="cv-lab">Phone</span><span>{{ contact.phone }}</span>
          </a>
          <a v-if="contact.email" class="cv-line" :href="`mailto:${contact.email}`">
            <span class="cv-lab">Email</span><span>{{ contact.email }}</span>
          </a>
          <div v-if="contact.address" class="cv-line">
            <span class="cv-lab">Address</span><span>{{ contact.address }}</span>
          </div>
          <a v-if="contact.website" class="cv-line" :href="contact.website" target="_blank" rel="noopener">
            <span class="cv-lab">Website</span><span>{{ contact.website }}</span>
          </a>
        </div>

        <!-- Hours (#42 follow-up): open + drop-off/pick-up windows. -->
        <div v-if="contact.hours && contact.hours.length" class="cv-block">
          <h4>Hours</h4>
          <div v-for="(h, i) in contact.hours" :key="i" class="cv-kv">
            <b>{{ h.label }}</b><span v-if="h.detail">{{ h.detail }}</span>
          </div>
        </div>

        <!-- Requirements checklist (#42): what this facility needs of the pet. -->
        <div v-if="contact.requirements && contact.requirements.length" class="cv-block">
          <h4>Requirements</h4>
          <div class="reqchips">
            <span v-for="(r, i) in contact.requirements" :key="i" class="reqchip" :title="r.detail || ''">{{ r.label }}</span>
          </div>
        </div>

        <div v-if="contact.notes" class="cv-block">
          <h4>Notes</h4>
          <!-- notes render as markdown (safe subset; source may be LLM-parsed) -->
          <div class="mdnotes" v-html="renderMarkdown(contact.notes)"></div>
        </div>
      </div>

      <div class="sheet-foot">
        <button class="btn sec" type="button" data-testid="contact-close" @click="emit('close')">Close</button>
        <button class="btn primary" type="button" data-testid="contact-edit" @click="mode = 'edit'">Edit</button>
      </div>
    </template>

    <!-- ============ EDIT / ADD: the form (Delete / Cancel / Save) =========== -->
    <template v-else>
      <div class="sheet-head">
        <h3>{{ editing ? 'Edit contact' : 'Add a contact' }}</h3>
        <small>A vet, groomer, or facility — kept once, shared across your pets.</small>
      </div>

      <div class="sheet-body">
        <!-- Import from a website (#42): daycare/boarding requirements live on the
             facility's site, not on a document you can upload. -->
        <div v-if="canImport" class="field importbox">
          <label>Import from a website <span class="lbl-opt">optional</span></label>
          <div class="importrow">
            <input v-model="importUrl" type="url" data-testid="contact-import-url"
                   placeholder="https://facility.example/policies" @keydown.enter.prevent="parseUrl" />
            <button class="btn sec" type="button" data-testid="contact-import-btn" :disabled="importing" @click="parseUrl">
              {{ importing ? 'Reading…' : 'Fetch' }}
            </button>
          </div>
          <p class="privacy">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
            <span>The page is fetched and read by your configured AI model. Nothing is saved until you tap Save.</span>
          </p>
        </div>

        <div class="field-row">
          <div class="field">
            <label>Name</label>
            <input v-model="form.name" placeholder="Cedar Creek Animal Hospital" data-testid="contact-name-input" autofocus />
          </div>
          <div class="field" style="flex:0 0 140px">
            <label>Role</label>
            <select v-model="form.role" data-testid="contact-role-select">
              <option v-for="r in CONTACT_ROLES" :key="r" :value="r">{{ contactRoleLabel(r) }}</option>
            </select>
          </div>
        </div>
        <div class="field">
          <label>Doctor / contact person <span class="lbl-opt">optional</span></label>
          <input v-model="form.doctor" placeholder="Dr. Okafor" data-testid="contact-doctor-input" />
        </div>
        <div class="field-row">
          <div class="field">
            <label>Phone <span class="lbl-opt">optional</span></label>
            <input v-model="form.phone" type="tel" placeholder="(555) 010-0100" />
          </div>
          <div class="field">
            <label>Email <span class="lbl-opt">optional</span></label>
            <input v-model="form.email" type="email" placeholder="care@cedarcreek.example" />
          </div>
        </div>
        <div class="field">
          <label>Address <span class="lbl-opt">optional</span></label>
          <input v-model="form.address" placeholder="12 Oak Rd, Springfield" />
        </div>
        <div class="field">
          <label>Website <span class="lbl-opt">optional</span></label>
          <input v-model="form.website" type="url" placeholder="https://cedarcreek.example" />
        </div>
        <!-- Hours (#42 follow-up): open hours + drop-off / pick-up windows, structured. -->
        <div class="field">
          <label>Hours <span class="lbl-opt">open, drop-off, pick-up…</span></label>
          <div v-for="(h, i) in hours" :key="i" class="kvrow" data-testid="hours-row">
            <input v-model="h.label" class="kv-k" placeholder="e.g. Open / Drop-off" aria-label="Hours label" />
            <input v-model="h.detail" class="kv-v" placeholder="e.g. Mon–Fri 7am–7pm" aria-label="Hours detail" />
            <button class="fx" type="button" aria-label="Remove hours" @click="removeRow(hours, i)">×</button>
          </div>
          <button class="addrow" type="button" data-testid="add-hours" @click="addRow(hours)">+ Add hours</button>
        </div>

        <div class="field">
          <label>Notes <span class="lbl-opt">optional</span></label>
          <textarea v-model="form.notes" rows="3" placeholder="Parking, who to ask for, anything that isn't hours…"></textarea>
        </div>

        <!-- Requirements checklist (#42): what a facility requires of the pet. -->
        <div class="field">
          <label>Requirements <span class="lbl-opt">what this facility needs</span></label>
          <div v-for="(r, i) in reqs" :key="i" class="kvrow" data-testid="requirement-row">
            <input v-model="r.label" class="kv-k" placeholder="e.g. Vaccinations" aria-label="Requirement" />
            <input v-model="r.detail" class="kv-v" placeholder="e.g. Rabies, DHPP, Bordetella current" aria-label="Requirement detail" />
            <button class="fx" type="button" aria-label="Remove requirement" @click="removeRow(reqs, i)">×</button>
          </div>
          <button class="addrow" type="button" data-testid="add-requirement" @click="addRow(reqs)">+ Add a requirement</button>
        </div>

        <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
      </div>

      <div class="sheet-foot">
        <button v-if="editing" class="btn danger" type="button" data-testid="contact-delete" :disabled="saving || deleting" @click="remove">
          {{ deleting ? 'Deleting…' : 'Delete' }}
        </button>
        <button class="btn sec" type="button" data-testid="contact-cancel" :disabled="saving || deleting" @click="cancelEdit">Cancel</button>
        <button class="btn primary" type="button" data-testid="contact-save" :disabled="saving || deleting" @click="submit">
          {{ saving ? 'Saving…' : editing ? 'Save changes' : 'Add contact' }}
        </button>
      </div>
    </template>
  </SheetShell>
</template>

<style scoped>
.importbox { background: var(--surface-2); border-radius: 14px; padding: 12px; }
.importrow { display: flex; gap: 8px; }
.importrow input { flex: 1; }
.importrow .btn { flex: 0 0 auto; }
/* View mode (#44): read-only contact layout. */
.cv-title { display: flex; align-items: center; gap: 12px; }
.cv-title h3 { margin: 0; }
.cv-title small { color: var(--ink-2); }
/* View-mode layout (.cv-*, .reqchip*) is global in styles.css — shared with
   InsuranceSheet and RegimenSheet so the three views can't drift. */
</style>
