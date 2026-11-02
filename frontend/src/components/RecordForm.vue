<script setup lang="ts">
// The shared record-field editor (#15): title, date range, category, cost,
// itemized breakdown, notes, and custom detail rows. Both the add-record sheet
// and the edit-record sheet embed this so the two can never drift. It's purely
// presentational — the parent owns `form` (reactive), `items`, and `rows` and
// this mutates them in place (Vue reactivity flows back), plus the save logic.
import { CATEGORIES, contactRoleLabel } from '../format'
import type { RecordFormFields, LineItemRow, ExtraRow } from '../format'
import type { Contact } from '../types'

const props = withDefaults(defineProps<{
  form: RecordFormFields
  items: LineItemRow[] // cost breakdown
  rows: ExtraRow[] // custom detail fields
  contacts?: Contact[] // shared clinics/vets to pick from (#20)
}>(), { contacts: () => [] })

function addItem() { props.items.push({ desc: '', amount: '' }) }
function removeItem(i: number) { props.items.splice(i, 1) }
function addRow() { props.rows.push({ key: '', val: '' }) }
function removeRow(i: number) { props.rows.splice(i, 1) }
</script>

<template>
  <div class="field">
    <label>Title</label>
    <input v-model="form.title" placeholder="Annual wellness exam" data-testid="record-title-input" />
  </div>
  <div class="field-row">
    <div class="field">
      <label>Date</label>
      <input v-model="form.occurred_on" type="date" data-testid="record-date-input" />
    </div>
    <div class="field">
      <label>Ends <span class="lbl-opt">optional</span></label>
      <input v-model="form.ends_on" type="date" aria-label="End date" />
    </div>
  </div>
  <div class="field">
    <label>Category</label>
    <select v-model="form.category">
      <option v-for="c in CATEGORIES" :key="c" :value="c">{{ c }}</option>
    </select>
  </div>
  <!-- Clinic / vet: an optional link to a shared contact (#20). Only shown once
       there's a contact to pick; managed on the Contacts page. -->
  <div v-if="props.contacts.length" class="field">
    <label>Clinic / vet <span class="lbl-opt">optional</span></label>
    <select v-model="form.contact_id" data-testid="record-contact-select">
      <option value="">None</option>
      <option v-for="c in props.contacts" :key="c.id" :value="c.id">
        {{ c.name }} · {{ contactRoleLabel(c.role) }}
      </option>
    </select>
  </div>
  <div class="field">
    <label>Cost</label>
    <input v-model="form.cost" type="number" step="0.01" :placeholder="items.length ? 'auto-summed from breakdown' : '185.00'" data-testid="record-cost-input" />
  </div>

  <!-- Cost breakdown: itemized charges (auto-fills the total when blank). -->
  <div class="field">
    <label>Cost breakdown</label>
    <div v-for="(it, i) in items" :key="i" class="lirow">
      <input v-model="it.desc" class="li-desc" placeholder="Item (e.g. Cephalexin 500mg)" aria-label="Item description" />
      <input v-model="it.amount" class="li-amt" type="number" step="0.01" placeholder="0.00" aria-label="Item amount" />
      <button class="fx" type="button" aria-label="Remove item" @click="removeItem(i)">×</button>
    </div>
    <button class="addrow" type="button" @click="addItem">+ Add a cost item</button>
  </div>

  <div class="field">
    <label>Notes</label>
    <textarea v-model="form.notes" rows="4" placeholder="Bloodwork all normal. Markdown supported — **bold**, - bullet lists."></textarea>
  </div>

  <!-- Custom detail fields: rename, edit, add, remove — all yours. -->
  <div class="field">
    <label>Details</label>
    <div v-for="(r, i) in rows" :key="i" class="kvrow">
      <input v-model="r.key" class="kv-k" placeholder="Label" aria-label="Field label" />
      <input v-model="r.val" class="kv-v" placeholder="Value" aria-label="Field value" />
      <button class="fx" type="button" aria-label="Remove field" @click="removeRow(i)">×</button>
    </div>
    <button class="addrow" type="button" @click="addRow">+ Add a field</button>
  </div>
</template>
