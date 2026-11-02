<script setup lang="ts">
// View / add / edit / delete one ONGOING CARE entry (#63) — the pet's food, a
// preventative, a regular medication, a supplement. Same view-then-edit shape as
// ContactSheet and InsuranceSheet (#44): an existing entry opens read-only with
// an explicit Edit into the form (Delete/Cancel/Save) and Cancel returns to the
// view; a new entry goes straight to the form, where Cancel closes.
//
// Food and medication share ONE form because a vet intake form asks for them
// identically — "(diet name, amount, frequency)" and "(drug name, dose,
// frequency)". `protects` only appears for a preventative, since that is what
// answers "Is your pet current on Heartworm Prevention? Brand: ___", and one
// product routinely covers all three (Simparica Trio).
import { ref, reactive, computed } from 'vue'
import { api } from '../api'
import { regimenKindLabel, regimenKindIcon, protectionLabel, regimenDose, renderMarkdown } from '../format'
import { REGIMEN_KINDS, PROTECTIONS } from '../types'
import type { Regimen, NewRegimenInput } from '../types'
import SheetShell from './SheetShell.vue'

const props = withDefaults(defineProps<{
  petId: string
  petName: string
  /** The entry to view/edit, or null to create a new one. */
  regimen?: Regimen | null
}>(), { regimen: null })
const emit = defineEmits<{
  close: []
  saved: [regimen: Regimen]
  deleted: [id: string]
}>()

const editing = !!props.regimen
const mode = ref<'view' | 'edit'>(editing ? 'view' : 'edit')
const saving = ref(false)
const deleting = ref(false)
const error = ref('')

const kindLabel = regimenKindLabel
const kindIcon = regimenKindIcon
const dose = regimenDose

const form = reactive({
  kind: props.regimen?.kind || 'food',
  product: props.regimen?.product || '',
  amount: props.regimen?.amount || '',
  frequency: props.regimen?.frequency || '',
  started_on: props.regimen?.started_on || '',
  ended_on: props.regimen?.ended_on || '',
  notes: props.regimen?.notes || '',
})

// Protection checkboxes. Only sent for a preventative — a food that protects
// against heartworm is a data-entry mistake, not a thing.
const protects = ref<string[]>([...(props.regimen?.protects || [])])
function toggleProtection(p: string) {
  const i = protects.value.indexOf(p)
  if (i >= 0) protects.value.splice(i, 1)
  else protects.value.push(p)
}

// "Still on it" is derived from `ended_on` being empty — there is no separate
// boolean to drift. Unticking the checkbox asks for the date it stopped.
const stillOn = computed({
  get: () => !form.ended_on,
  set: (on: boolean) => {
    form.ended_on = on ? '' : new Date().toISOString().slice(0, 10)
  },
})

const placeholder = computed(() => {
  switch (form.kind) {
    case 'food': return "Hill's Science Diet Large Breed Puppy"
    case 'preventative': return 'Simparica Trio'
    case 'supplement': return 'Cosequin'
    default: return 'Apoquel'
  }
})

function cancelEdit() {
  if (editing) mode.value = 'view'
  else emit('close')
}

async function submit() {
  if (!form.product.trim()) {
    error.value = form.kind === 'food' ? 'Name the food.' : 'Name the product.'
    return
  }
  saving.value = true
  error.value = ''
  try {
    const payload: NewRegimenInput = {
      kind: form.kind,
      product: form.product.trim(),
      amount: form.amount.trim() || null,
      frequency: form.frequency.trim() || null,
      // Only a preventative carries protections; anything else clears them.
      protects: form.kind === 'preventative' ? protects.value : null,
      started_on: form.started_on || null,
      ended_on: form.ended_on || null,
      notes: form.notes.trim() || null,
    }
    const saved = props.regimen
      ? await api.updateRegimen(props.regimen.id, payload)
      : await api.createRegimen(props.petId, payload)
    emit('saved', saved)
  } catch (e) {
    error.value = (e as Error).message || 'Could not save.'
  } finally {
    saving.value = false
  }
}

async function remove() {
  if (!props.regimen) return
  if (!window.confirm(`Remove ${props.regimen.product} from ${props.petName}'s ongoing care?`)) return
  deleting.value = true
  error.value = ''
  try {
    await api.deleteRegimen(props.regimen.id)
    emit('deleted', props.regimen.id)
  } catch (e) {
    error.value = (e as Error).message || 'Could not delete.'
    deleting.value = false
  }
}
</script>

<template>
  <SheetShell testid="regimen-sheet" @close="emit('close')">
    <!-- ============ VIEW: read-only, existing entries only =========== -->
    <template v-if="mode === 'view' && regimen">
      <div class="sheet-head">
        <div class="rv-title">
          <span class="care-badge" :data-kind="regimen.kind" aria-hidden="true">{{ kindIcon(regimen.kind) }}</span>
          <div>
            <h3>{{ regimen.product }}</h3>
            <small>
              {{ kindLabel(regimen.kind) }}
              <template v-if="dose(regimen)"> · {{ dose(regimen) }}</template>
              <template v-if="regimen.ended_on"> · stopped {{ regimen.ended_on }}</template>
            </small>
          </div>
        </div>
      </div>

      <div class="sheet-body" data-testid="regimen-view">
        <div class="cv-lines">
          <div class="cv-line">
            <span class="cv-lab">Status</span>
            <span :class="{ off: !!regimen.ended_on }">
              {{ regimen.ended_on ? `Stopped ${regimen.ended_on}` : `${petName} is currently on this` }}
            </span>
          </div>
          <div v-if="regimen.amount" class="cv-line"><span class="cv-lab">Amount</span><span>{{ regimen.amount }}</span></div>
          <div v-if="regimen.frequency" class="cv-line"><span class="cv-lab">How often</span><span>{{ regimen.frequency }}</span></div>
          <div v-if="regimen.started_on" class="cv-line"><span class="cv-lab">Started</span><span>{{ regimen.started_on }}</span></div>
        </div>

        <!-- What it protects against: the answer a vet or boarding facility wants. -->
        <div v-if="regimen.protects && regimen.protects.length" class="cv-block">
          <h4>Protects against</h4>
          <div class="reqchips">
            <span v-for="p in regimen.protects" :key="p" class="reqchip prot">{{ protectionLabel(p) }}</span>
          </div>
        </div>

        <div v-if="regimen.notes" class="cv-block">
          <h4>Notes</h4>
          <div class="mdnotes" v-html="renderMarkdown(regimen.notes)"></div>
        </div>
      </div>

      <div class="sheet-foot">
        <button class="btn sec" type="button" data-testid="regimen-close" @click="emit('close')">Close</button>
        <button class="btn primary" type="button" data-testid="regimen-edit" @click="mode = 'edit'">Edit</button>
      </div>
    </template>

    <!-- ============ EDIT / ADD: the form =========== -->
    <template v-else>
      <div class="sheet-head">
        <h3>{{ editing ? 'Edit ongoing care' : 'Add ongoing care' }}</h3>
        <small>Food, preventatives, medications and supplements {{ petName }} is on.</small>
      </div>

      <div class="sheet-body">
        <div class="field">
          <label>Kind</label>
          <div class="kindpick" data-testid="regimen-kind-pick">
            <button
              v-for="k in REGIMEN_KINDS" :key="k" type="button" class="kindbtn"
              :class="{ on: form.kind === k }" :data-kind="k" :aria-pressed="form.kind === k"
              @click="form.kind = k"
            >
              <span aria-hidden="true">{{ kindIcon(k) }}</span>{{ kindLabel(k) }}
            </button>
          </div>
        </div>

        <div class="field">
          <label>{{ form.kind === 'food' ? 'Food' : 'Product' }}</label>
          <input v-model="form.product" :placeholder="placeholder" data-testid="regimen-product-input" autofocus />
        </div>

        <div class="field-row">
          <div class="field">
            <label>{{ form.kind === 'food' ? 'Amount' : 'Dose' }} <span class="lbl-opt">optional</span></label>
            <input v-model="form.amount" :placeholder="form.kind === 'food' ? '1 cup' : '1 pill'" data-testid="regimen-amount-input" />
          </div>
          <div class="field">
            <label>How often <span class="lbl-opt">optional</span></label>
            <input v-model="form.frequency" :placeholder="form.kind === 'food' ? '2x/day' : 'monthly'" data-testid="regimen-frequency-input" />
          </div>
        </div>

        <!-- Protections: only meaningful for a preventative, and the reason this
             is one list rather than three columns — one product covers several. -->
        <div v-if="form.kind === 'preventative'" class="field">
          <label>Protects against</label>
          <div class="protpick" data-testid="regimen-protects">
            <button
              v-for="p in PROTECTIONS" :key="p" type="button" class="protbtn"
              :class="{ on: protects.includes(p) }" :aria-pressed="protects.includes(p)"
              :data-protection="p" @click="toggleProtection(p)"
            >{{ protectionLabel(p) }}</button>
          </div>
        </div>

        <div class="field-row">
          <div class="field">
            <label>Started <span class="lbl-opt">optional</span></label>
            <input v-model="form.started_on" type="date" />
          </div>
          <div class="field" v-if="!stillOn">
            <label>Stopped</label>
            <input v-model="form.ended_on" type="date" data-testid="regimen-ended-input" />
          </div>
        </div>

        <label class="alsorow">
          <input type="checkbox" v-model="stillOn" data-testid="regimen-still-on" />
          <span>{{ petName }} is currently on this</span>
        </label>

        <div class="field">
          <label>Notes <span class="lbl-opt">optional</span></label>
          <textarea v-model="form.notes" rows="3" placeholder="Given with food, vet's instructions, where you buy it…"></textarea>
        </div>

        <p v-if="error" style="color:var(--crit);font-size:13px;margin:4px 0 0">{{ error }}</p>
      </div>

      <div class="sheet-foot">
        <button v-if="editing" class="btn danger" type="button" data-testid="regimen-delete" :disabled="saving || deleting" @click="remove">
          {{ deleting ? 'Removing…' : 'Remove' }}
        </button>
        <button class="btn sec" type="button" data-testid="regimen-cancel" :disabled="saving || deleting" @click="cancelEdit">Cancel</button>
        <button class="btn primary" type="button" data-testid="regimen-save" :disabled="saving || deleting" @click="submit">
          {{ saving ? 'Saving…' : editing ? 'Save changes' : 'Add' }}
        </button>
      </div>
    </template>
  </SheetShell>
</template>

<style scoped>
.rv-title { display: flex; align-items: center; gap: 12px; }
.rv-title h3 { margin: 0; }
.rv-title small { color: var(--ink-2); }
.off { color: var(--ink-3); }
.alsorow { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: var(--ink-2); margin-top: 4px; cursor: pointer; }
.alsorow input { width: auto; }

/* Kind picker: the same chip-row shape as the record category picker. */
.kindpick { display: flex; flex-wrap: wrap; gap: 8px; }
.kindbtn {
  display: inline-flex; align-items: center; gap: 6px; padding: 8px 12px; cursor: pointer;
  border: 1.5px solid var(--line); border-radius: 999px; background: var(--surface);
  color: var(--ink-2); font: inherit; font-size: 13.5px; font-weight: 600;
}
.kindbtn.on { border-color: var(--brand); color: var(--brand); background: color-mix(in srgb, var(--brand) 10%, transparent); }

/* Protection toggles: multi-select, because one product covers several. */
.protpick { display: flex; flex-wrap: wrap; gap: 8px; }
.protbtn {
  padding: 7px 12px; cursor: pointer; border: 1.5px solid var(--line); border-radius: 999px;
  background: var(--surface); color: var(--ink-2); font: inherit; font-size: 13px; font-weight: 600;
}
.protbtn.on { border-color: var(--good); color: var(--good); background: color-mix(in srgb, var(--good) 12%, transparent); }
</style>
