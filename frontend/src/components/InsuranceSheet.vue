<script setup lang="ts">
// View / add / edit / delete a pet's insurance policy or wellness plan (#8/#29).
// One sheet, same shape as ContactSheet (#44): an existing policy opens in a
// read-only VIEW first — coverage numbers, dates, what's covered/excluded, notes,
// and the policy document rendered inline — with an explicit Edit into the form
// (Delete/Cancel/Save); Cancel returns to the view. A new policy goes straight to
// the form, where Cancel closes. The view also shows the term's value (#74):
// paid in vs paid back and the break-even, from GET /insurance/:pid/value.
//
// The form's ONE file input is dual-purpose (like AddRecordSheet): the uploaded
// PDF/image becomes the stored policy document AND, when AI ingestion is on, the
// source the model reads to pre-fill the fields for the user to confirm. Money is
// entered in dollars and converted to cents on save (like a record's cost).
import { ref, reactive, computed, onMounted } from 'vue'
import { api } from '../api'
import {
  money, dollarsFromCents, centsFromDollars, readDataUrl, isPdf, isImage, renderMarkdown,
  labeledToRows, rowsToLabeled, mergeLabeledRows, insuranceKindLabel, premiumIntervalSuffix,
} from '../format'
import type { LabeledRow } from '../format'
import { INSURANCE_KINDS, INSURANCE_STATUSES } from '../types'
import type { InsurancePolicy, NewInsuranceInput, PolicyValue } from '../types'
import SheetShell from './SheetShell.vue'
import InsuranceValue from './InsuranceValue.vue'

const props = withDefaults(defineProps<{
  petId: string
  petName: string
  // The policy to view/edit, or null to create a new one.
  policy?: InsurancePolicy | null
}>(), { policy: null })
const emit = defineEmits<{
  close: []
  saved: [policy: InsurancePolicy]
  deleted: [id: string]
}>()

const editing = !!props.policy
const mode = ref<'view' | 'edit'>(editing ? 'view' : 'edit')
const saving = ref(false)
const deleting = ref(false)
const extracting = ref(false)
const error = ref('')

const kindLabel = insuranceKindLabel
const intervalSuffix = premiumIntervalSuffix
function statusLabel(s: string): string {
  return s ? s[0].toUpperCase() + s.slice(1) : s
}

const form = reactive({
  kind: props.policy?.kind || 'insurance',
  provider: props.policy?.provider || '',
  plan_name: props.policy?.plan_name || '',
  policy_number: props.policy?.policy_number || '',
  status: props.policy?.status || 'active',
  start_date: props.policy?.start_date || '',
  end_date: props.policy?.end_date || '',
  premium: dollarsFromCents(props.policy?.premium_cents),
  premium_interval: props.policy?.premium_interval || 'year',
  reimburse: props.policy?.reimburse_percent != null ? String(props.policy.reimburse_percent) : '',
  deductible: dollarsFromCents(props.policy?.deductible_cents),
  annual_limit: dollarsFromCents(props.policy?.annual_limit_cents),
  notes: props.policy?.notes || '',
})

// Editable coverage/exclusion rows share the LabeledDetail round-trip with
// ContactSheet's requirements/hours (helpers in format.ts). add/remove are
// trivial array ops kept inline.
function addRow(rows: LabeledRow[]) { rows.push({ label: '', detail: '' }) }
function removeRow(rows: LabeledRow[], i: number) { rows.splice(i, 1) }
const coverages = ref<LabeledRow[]>(labeledToRows(props.policy?.coverages))
const exclusions = ref<LabeledRow[]>(labeledToRows(props.policy?.exclusions))

// Also surface this policy on the pet's profile as their insurer (#26/#29 enrich
// the pet's insurer fields). Default on for a true insurance policy.
const alsoUpdatePet = ref(form.kind === 'insurance')

// ---- the policy document + AI extraction ----
const canExtract = ref(false) // AI ingestion enabled + a key stored
// A newly-picked file to store as the document on save (also the extract source).
const pickedFile = ref<{ name: string; media_type: string; data: string } | null>(null)
// Set when the user removes an existing document (applied on save).
const removeDoc = ref(false)
const lowConf = ref<string[]>([])

// The current, saved document (view mode + edit "already attached" state).
const docUrl = computed(() =>
  props.policy && props.policy.document_name && !removeDoc.value
    ? api.insuranceDocumentUrl(props.policy)
    : null,
)
const docMedia = computed(() => props.policy?.document_media_type || null)

// What this term cost vs what it paid back (#74), for the view. Best-effort: a
// failure just leaves the section out, the policy itself still shows.
const value = ref<PolicyValue | null>(null)
async function loadValue(id: string) {
  try { value.value = (await api.insuranceValue(id)) || null } catch (_) { value.value = null }
}

onMounted(async () => {
  if (props.policy) loadValue(props.policy.id)
  try {
    const s = await api.ingestion()
    canExtract.value = !!(s && s.enabled && s.has_key)
  } catch (_) { canExtract.value = false }
})

function cancelEdit() {
  if (editing) mode.value = 'view'
  else emit('close')
}

async function onFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files && input.files[0]
  if (!file) return
  error.value = ''
  try {
    const dataUrl = await readDataUrl(file)
    pickedFile.value = {
      name: file.name || 'document',
      media_type: file.type || 'application/octet-stream',
      data: dataUrl.split(',')[1] || '',
    }
    removeDoc.value = false
  } catch (_) {
    error.value = 'Could not read that file.'
  }
}

async function runExtract() {
  if (!pickedFile.value) return
  extracting.value = true
  error.value = ''
  try {
    const res = await api.extractInsurance({
      filename: pickedFile.value.name,
      media_type: pickedFile.value.media_type,
      data_base64: pickedFile.value.data,
    })
    const f = res.fields
    // Pre-fill only fields the parse found, without clobbering user input.
    if (f.kind && INSURANCE_KINDS.includes(f.kind as never)) form.kind = f.kind
    if (f.provider && !form.provider.trim()) form.provider = f.provider
    if (f.plan_name && !form.plan_name.trim()) form.plan_name = f.plan_name
    if (f.policy_number && !form.policy_number.trim()) form.policy_number = f.policy_number
    if (f.start_date && !form.start_date) form.start_date = f.start_date
    if (f.end_date && !form.end_date) form.end_date = f.end_date
    if (f.premium != null && !form.premium.trim()) form.premium = f.premium.toFixed(2)
    if (f.premium_interval) form.premium_interval = f.premium_interval
    if (f.reimburse_percent != null && !form.reimburse.trim()) form.reimburse = String(f.reimburse_percent)
    if (f.deductible != null && !form.deductible.trim()) form.deductible = f.deductible.toFixed(2)
    if (f.annual_limit != null && !form.annual_limit.trim()) form.annual_limit = f.annual_limit.toFixed(2)
    if (f.notes && !form.notes.trim()) form.notes = f.notes
    mergeLabeledRows(coverages.value, f.coverages)
    mergeLabeledRows(exclusions.value, f.exclusions)
    lowConf.value = f.low_confidence_fields || []
    // A newly-read insurance policy is worth surfacing on the profile.
    if (form.kind === 'insurance') alsoUpdatePet.value = true
  } catch (e) {
    error.value = e.message || 'Could not read that document.'
  } finally {
    extracting.value = false
  }
}

// Guard against double-create if a later save step (document upload) fails and
// the user retries — reuse the already-created policy.
const createdId = ref<string | null>(props.policy?.id || null)

async function submit() {
  if (!form.provider.trim()) { error.value = 'Give this policy a provider.'; return }
  saving.value = true
  error.value = ''
  try {
    // A number-typed input can hand back a number, not a string; coerce before
    // any string op so a numeric reimburse/premium can't throw on `.trim()`.
    const premium = String(form.premium ?? '').trim()
    const reimburse = String(form.reimburse ?? '').trim()
    const payload: NewInsuranceInput = {
      kind: form.kind,
      provider: form.provider.trim(),
      plan_name: form.plan_name.trim() || null,
      policy_number: form.policy_number.trim() || null,
      status: form.status,
      start_date: form.start_date || null,
      end_date: form.end_date || null,
      premium_cents: centsFromDollars(premium),
      premium_interval: premium ? form.premium_interval : null,
      reimburse_percent: reimburse ? Math.round(Number(reimburse)) : null,
      deductible_cents: centsFromDollars(String(form.deductible ?? '')),
      annual_limit_cents: centsFromDollars(String(form.annual_limit ?? '')),
      notes: form.notes.trim() || null,
      coverages: rowsToLabeled(coverages.value),
      exclusions: rowsToLabeled(exclusions.value),
    }
    let saved = createdId.value
      ? await api.updateInsurance(createdId.value, payload)
      : await api.createInsurance(props.petId, payload)
    createdId.value = saved.id

    // Store / replace / remove the document (guarded so a retry won't re-upload).
    if (pickedFile.value) {
      saved = await api.setInsuranceDocument(saved.id, {
        filename: pickedFile.value.name,
        media_type: pickedFile.value.media_type,
        data_base64: pickedFile.value.data,
      })
      pickedFile.value = null
    } else if (removeDoc.value && props.policy?.document_name) {
      saved = await api.deleteInsuranceDocument(saved.id)
      removeDoc.value = false
    }

    // Optionally reflect the policy on the pet's profile (#26/#29). The null
    // reimbursement is deliberate and load-bearing since #63 made pet edits
    // null-vs-absent: a policy with no reimbursement % must CLEAR the pet's
    // stored one, or the profile keeps a stale percentage from a previous
    // insurer next to this policy's provider name. DO NOT swap the null for an
    // omitted key to "leave it alone" — that is the stale-value bug.
    if (alsoUpdatePet.value && form.kind === 'insurance') {
      try {
        await api.updatePet(props.petId, {
          insurer: form.provider.trim(),
          insurer_reimburse: reimburse ? `${Math.round(Number(reimburse))}%` : null,
        })
      } catch (_) { /* non-fatal: the policy is saved regardless */ }
    }
    emit('saved', saved)
  } catch (e) {
    error.value = e.message || 'Could not save.'
  } finally {
    saving.value = false
  }
}

async function remove() {
  if (!props.policy) return
  if (!window.confirm(`Delete this ${kindLabel(props.policy.kind).toLowerCase()} for ${props.petName}? The document is removed too.`)) return
  deleting.value = true
  error.value = ''
  try {
    await api.deleteInsurance(props.policy.id)
    emit('deleted', props.policy.id)
  } catch (e) {
    error.value = e.message || 'Could not delete.'
    deleting.value = false
  }
}
</script>

<template>
  <SheetShell testid="insurance-sheet" @close="emit('close')">
    <!-- ============ VIEW: read-only, existing policies only =========== -->
    <template v-if="mode === 'view' && policy">
      <div class="sheet-head">
        <div class="iv-title">
          <span class="ins-badge" :data-kind="policy.kind" aria-hidden="true">{{ policy.provider[0] }}</span>
          <div>
            <h3>{{ policy.provider }}</h3>
            <small>{{ kindLabel(policy.kind) }}<template v-if="policy.plan_name"> · {{ policy.plan_name }}</template><template v-if="policy.status !== 'active'"> · {{ statusLabel(policy.status) }}</template></small>
          </div>
        </div>
      </div>

      <div class="sheet-body" data-testid="insurance-view">
        <div class="cv-lines">
          <div v-if="policy.policy_number" class="cv-line"><span class="cv-lab">Policy #</span><span class="mono">{{ policy.policy_number }}</span></div>
          <div v-if="policy.reimburse_percent != null" class="cv-line"><span class="cv-lab">Reimburse</span><span>{{ policy.reimburse_percent }}%</span></div>
          <div v-if="policy.deductible_cents != null" class="cv-line"><span class="cv-lab">Deductible</span><span>{{ money(policy.deductible_cents) }}</span></div>
          <div v-if="policy.annual_limit_cents != null" class="cv-line"><span class="cv-lab">Annual limit</span><span>{{ money(policy.annual_limit_cents) }}</span></div>
          <div v-if="policy.premium_cents != null" class="cv-line"><span class="cv-lab">Premium</span><span>{{ money(policy.premium_cents) }}{{ intervalSuffix(policy.premium_interval) }}</span></div>
          <div v-if="policy.start_date || policy.end_date" class="cv-line"><span class="cv-lab">Term</span><span>{{ policy.start_date || '—' }} → {{ policy.end_date || '—' }}</span></div>
        </div>

        <InsuranceValue v-if="value" :value="value" />

        <div v-if="policy.coverages && policy.coverages.length" class="cv-block">
          <h4>What's covered</h4>
          <div v-for="(c, i) in policy.coverages" :key="i" class="cv-kv"><b>{{ c.label }}</b><span v-if="c.detail">{{ c.detail }}</span></div>
        </div>
        <div v-if="policy.exclusions && policy.exclusions.length" class="cv-block">
          <h4>Not covered</h4>
          <div class="reqchips">
            <span v-for="(x, i) in policy.exclusions" :key="i" class="reqchip excl" :title="x.detail || ''">{{ x.label }}</span>
          </div>
        </div>

        <div v-if="policy.notes" class="cv-block">
          <h4>Notes</h4>
          <div class="mdnotes" v-html="renderMarkdown(policy.notes)"></div>
        </div>

        <!-- The policy document, rendered inline (#44 pattern): PDF in an iframe,
             image inline, anything else a download link. -->
        <div v-if="docUrl" class="cv-block" data-testid="insurance-view-doc">
          <h4>Document <small class="doc-name">{{ policy.document_name }}</small></h4>
          <iframe v-if="isPdf(docMedia)" class="doc-frame" :src="docUrl" :title="policy.document_name || 'policy document'"></iframe>
          <a v-else-if="isImage(docMedia)" :href="docUrl" target="_blank" rel="noopener"><img class="doc-img" :src="docUrl" :alt="policy.document_name || 'policy document'" /></a>
          <a v-else class="doc-link" :href="docUrl" target="_blank" rel="noopener">{{ policy.document_name }}</a>
        </div>
      </div>

      <div class="sheet-foot">
        <button class="btn sec" type="button" data-testid="insurance-close" @click="emit('close')">Close</button>
        <button class="btn primary" type="button" data-testid="insurance-edit" @click="mode = 'edit'">Edit</button>
      </div>
    </template>

    <!-- ============ EDIT / ADD: the form =========== -->
    <template v-else>
      <div class="sheet-head">
        <h3>{{ editing ? 'Edit policy' : 'Add insurance' }}</h3>
        <small>A policy or wellness plan for {{ petName }} — with the document itself.</small>
      </div>

      <div class="sheet-body">
        <!-- Document upload + AI extract. The uploaded file is stored as the
             policy document and (when AI is on) read to pre-fill the fields. -->
        <div class="field importbox">
          <label>Policy document <span class="lbl-opt">PDF or image</span></label>
          <input type="file" accept="application/pdf,image/*" data-testid="insurance-file-input" @change="onFile" />
          <div v-if="pickedFile" class="pickedrow">
            <span class="picked-name">{{ pickedFile.name }}</span>
            <button v-if="canExtract" class="btn sec" type="button" data-testid="insurance-extract-btn" :disabled="extracting" @click="runExtract">
              {{ extracting ? 'Reading…' : 'Extract details with AI' }}
            </button>
          </div>
          <div v-else-if="docUrl" class="pickedrow">
            <span class="picked-name">{{ policy?.document_name }} <small>attached</small></span>
            <button class="btn sec" type="button" data-testid="insurance-doc-remove" @click="removeDoc = true">Remove</button>
          </div>
          <p v-if="canExtract" class="privacy">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
            <span>The document is read by your configured AI model. Nothing is saved until you tap Save.</span>
          </p>
        </div>

        <div class="field-row">
          <div class="field">
            <label>Provider</label>
            <input v-model="form.provider" placeholder="Lemonade" data-testid="insurance-provider-input" autofocus />
          </div>
          <div class="field" style="flex:0 0 150px">
            <label>Kind</label>
            <select v-model="form.kind" data-testid="insurance-kind-select">
              <option v-for="k in INSURANCE_KINDS" :key="k" :value="k">{{ kindLabel(k) }}</option>
            </select>
          </div>
        </div>
        <div class="field-row">
          <div class="field">
            <label>Plan name <span class="lbl-opt">optional</span></label>
            <input v-model="form.plan_name" placeholder="Pet Insurance + Preventative" />
          </div>
          <div class="field">
            <label>Policy # <span class="lbl-opt">optional</span></label>
            <input v-model="form.policy_number" placeholder="DEMO-POL-0401" />
          </div>
        </div>
        <div class="field-row">
          <div class="field" style="flex:0 0 140px">
            <label>Status</label>
            <select v-model="form.status" data-testid="insurance-status-select">
              <option v-for="s in INSURANCE_STATUSES" :key="s" :value="s">{{ statusLabel(s) }}</option>
            </select>
          </div>
          <div class="field">
            <label>Start <span class="lbl-opt">optional</span></label>
            <input v-model="form.start_date" type="date" />
          </div>
          <div class="field">
            <label>Renews <span class="lbl-opt">optional</span></label>
            <input v-model="form.end_date" type="date" />
          </div>
        </div>
        <div class="field-row" v-if="form.kind === 'insurance'">
          <div class="field" style="flex:0 0 120px">
            <label>Reimburse %</label>
            <input v-model="form.reimburse" type="number" min="0" max="100" inputmode="numeric" placeholder="90" data-testid="insurance-reimburse-input" />
          </div>
          <div class="field">
            <label>Deductible <span class="lbl-opt">$/yr</span></label>
            <input v-model="form.deductible" inputmode="decimal" placeholder="500" />
          </div>
          <div class="field">
            <label>Annual limit <span class="lbl-opt">$</span></label>
            <input v-model="form.annual_limit" inputmode="decimal" placeholder="50000" />
          </div>
        </div>
        <div class="field-row">
          <div class="field">
            <label>Premium <span class="lbl-opt">$</span></label>
            <input v-model="form.premium" inputmode="decimal" placeholder="911.36" data-testid="insurance-premium-input" />
          </div>
          <div class="field" style="flex:0 0 120px">
            <label>Per</label>
            <select v-model="form.premium_interval">
              <option value="year">Year</option>
              <option value="month">Month</option>
            </select>
          </div>
        </div>

        <!-- What's covered: coverage items + preventative sub-limits. -->
        <div class="field">
          <label>What's covered <span class="lbl-opt">exams, vaccines, sub-limits…</span></label>
          <div v-for="(c, i) in coverages" :key="i" class="kvrow" data-testid="coverage-row">
            <input v-model="c.label" class="kv-k" placeholder="e.g. Wellness exam" aria-label="Coverage" />
            <input v-model="c.detail" class="kv-v" placeholder="e.g. Up to $70/yr" aria-label="Coverage detail" />
            <button class="fx" type="button" aria-label="Remove coverage" @click="removeRow(coverages, i)">×</button>
          </div>
          <button class="addrow" type="button" data-testid="add-coverage" @click="addRow(coverages)">+ Add coverage</button>
        </div>
        <!-- What's not covered: exclusions. -->
        <div class="field">
          <label>Not covered <span class="lbl-opt">exclusions</span></label>
          <div v-for="(x, i) in exclusions" :key="i" class="kvrow" data-testid="exclusion-row">
            <input v-model="x.label" class="kv-k" placeholder="e.g. Pre-existing conditions" aria-label="Exclusion" />
            <input v-model="x.detail" class="kv-v" placeholder="optional detail" aria-label="Exclusion detail" />
            <button class="fx" type="button" aria-label="Remove exclusion" @click="removeRow(exclusions, i)">×</button>
          </div>
          <button class="addrow" type="button" data-testid="add-exclusion" @click="addRow(exclusions)">+ Add exclusion</button>
        </div>

        <div class="field">
          <label>Notes <span class="lbl-opt">optional</span></label>
          <textarea v-model="form.notes" rows="3" placeholder="Waiting periods, discounts, anything else…"></textarea>
        </div>

        <label v-if="form.kind === 'insurance'" class="alsorow">
          <input type="checkbox" v-model="alsoUpdatePet" data-testid="insurance-also-pet" />
          <span>Show on {{ petName }}'s profile as their insurer</span>
        </label>

        <p v-if="lowConf.length" class="lowconf">Double-check: {{ lowConf.join(', ') }}</p>
        <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
      </div>

      <div class="sheet-foot">
        <button v-if="editing" class="btn danger" type="button" data-testid="insurance-delete" :disabled="saving || deleting" @click="remove">
          {{ deleting ? 'Deleting…' : 'Delete' }}
        </button>
        <button class="btn sec" type="button" data-testid="insurance-cancel" :disabled="saving || deleting" @click="cancelEdit">Cancel</button>
        <button class="btn primary" type="button" data-testid="insurance-save" :disabled="saving || deleting" @click="submit">
          {{ saving ? 'Saving…' : editing ? 'Save changes' : 'Add policy' }}
        </button>
      </div>
    </template>
  </SheetShell>
</template>

<style scoped>
.importbox { background: var(--surface-2); border-radius: 14px; padding: 12px; }
.pickedrow { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 8px; }
.picked-name { font-size: 13px; color: var(--ink); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.picked-name small { color: var(--ink-3); font-weight: 500; }
.alsorow { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: var(--ink-2); margin-top: 4px; cursor: pointer; }
.alsorow input { width: auto; }
.lowconf { color: var(--warn, var(--brand-2)); font-size: 12.5px; margin: 6px 0 0; }
/* View mode: read-only policy layout (mirrors ContactSheet). */
.iv-title { display: flex; align-items: center; gap: 12px; }
.iv-title h3 { margin: 0; }
.iv-title small { color: var(--ink-2); }
.ins-badge { display: inline-grid; place-items: center; width: 40px; height: 40px; border-radius: 12px; font-weight: 800; color: #fff; background: var(--brand); }
.ins-badge[data-kind="wellness"] { background: var(--brand-2, var(--brand)); }
/* View-mode layout (.cv-*, .reqchip*) is global in styles.css — shared with
   ContactSheet and RegimenSheet. Only the document-name suffix is ours. */
.cv-block h4 .doc-name { text-transform: none; letter-spacing: 0; font-weight: 500; color: var(--ink-2); margin-left: 6px; }
.doc-frame { width: 100%; height: 420px; border: 1px solid var(--border); border-radius: 12px; background: #fff; }
.doc-img { max-width: 100%; border-radius: 12px; }
.doc-link { color: var(--brand); font-size: 14px; }
</style>
