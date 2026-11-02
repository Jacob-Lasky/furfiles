<script setup lang="ts">
import { computed } from 'vue'
import { CATEGORIES, money, renderMarkdown, regimenKindLabel, protectionLabel, contactRoleLabel, fileSize } from '../format'
import { saveLabel, recordsNeedingPet, hasWork, resultSummary, petFieldLabel, petFieldValue, petName, shortDate, endApplies, reassignClaim, reassignPolicy } from '../importer'
import type { ImportEntry } from '../importer'
import { display } from '../store'
import type { PetSummary } from '../types'

// One dropped file's review card (#23). The plan is edited IN PLACE (toggles,
// pet picks, titles): it is page-owned reactive state and this card is its
// editor, so there is one copy of each plan. Nothing is written until Save.
const props = defineProps<{ entry: ImportEntry; pets: PetSummary[] }>()
const emit = defineEmits<{ save: []; remove: []; retry: [] }>()

const plan = computed(() => props.entry.plan)
const needPet = computed(() => (plan.value ? recordsNeedingPet(plan.value) : 0))
const canSave = computed(() => !!plan.value && hasWork(plan.value) && needPet.value === 0)
const isDup = computed(() => !!plan.value && plan.value.file_duplicates.length > 0)
const name = (id: string | null | undefined) => petName(props.pets, id)
// A proposed profile value in the words the profile will show (#72).
const fieldValue = (u: { pet_id: string; field: string }, v: string | null) =>
  petFieldValue(u.field, v, props.pets.find((p) => p.id === u.pet_id)?.species, display.breeder_terms)
const clinicName = computed(() => {
  const c = plan.value?.clinic
  return c ? c.matched_name || c.name : ''
})
const hasUpdates = computed(() => {
  const p = plan.value
  return !!p && (p.care_team.length + p.clinic_links.length + p.weights.length + p.reminders.length +
    p.completed_reminders.length + p.pet_updates.length + p.regimens.length) > 0
})
// Old-policy ends for the pet each replacement is CURRENTLY assigned to; the
// rest stay hidden (they're the other pets' candidates for an unassigned policy).
const visibleEnds = computed(() => {
  const p = plan.value
  return p ? p.policy_ends.filter((e) => p.policies[e.replaced_by]?.pet_id === e.pet_id) : []
})
function onVisit(c: { record_id: number | null; record_title: string | null; visit_options: { record_id: number; title: string }[] }) {
  c.record_title = c.visit_options.find((v) => v.record_id === c.record_id)?.title ?? null
}
const hasInsurance = computed(() => {
  const p = plan.value
  return !!p && p.claims.length + p.policies.length + visibleEnds.value.length > 0
})
</script>

<template>
  <article class="card imp" :class="`imp-${entry.status}`" data-testid="import-file" :data-status="entry.status">
    <header class="imp-head">
      <div class="fthumb doc"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><path d="M14 2v6h6"/></svg></div>
      <div class="fmeta">
        <b :title="entry.name">{{ entry.name }}</b>
        <small>{{ fileSize(entry.size) }}<template v-if="plan && plan.model"> · read by {{ plan.model }}</template></small>
      </div>
      <button v-if="entry.status !== 'saving'" class="fx" aria-label="Remove file" data-testid="import-remove" @click="emit('remove')">×</button>
    </header>

    <div v-if="entry.status === 'queued'" class="imp-state">Waiting its turn…</div>
    <div v-else-if="entry.status === 'converting'" class="imp-state" data-testid="import-converting"><span class="imp-spin"></span>Converting the iPhone photo to a JPEG…</div>
    <div v-else-if="entry.status === 'analyzing'" class="imp-state"><span class="imp-spin"></span>Reading the document and checking it against your records…</div>
    <div v-else-if="entry.status === 'unsupported' || entry.status === 'error'" class="imp-state err" data-testid="import-error">
      {{ entry.error }}
      <button v-if="entry.status === 'error' && entry.data" class="addrow" data-testid="import-retry" @click="emit('retry')">Try again</button>
    </div>
    <div v-else-if="entry.status === 'saved' && entry.result" class="imp-state ok" data-testid="import-saved">
      {{ resultSummary(entry.result) }}
      <span class="imp-links">
        <router-link v-for="pid in [...new Set(entry.result.records.map((r) => r.pet_id))]" :key="pid" :to="`/pets/${pid}`">See {{ name(pid) }}</router-link>
      </span>
    </div>

    <template v-if="plan && (entry.status === 'ready' || entry.status === 'saving')">
      <p v-if="plan.summary" class="imp-summary">{{ plan.summary }}</p>

      <div v-if="isDup" class="imp-dup" data-testid="import-file-dup">
        <b>Already in FurFiles.</b> This exact file is attached to
        <template v-for="(d, i) in plan.file_duplicates" :key="d.attachment_id">
          <router-link :to="`/pets/${d.pet_id}`">{{ d.pet_name }} · {{ d.title }} ({{ shortDate(d.occurred_on) }})</router-link><span v-if="i < plan.file_duplicates.length - 1">, </span>
        </template>. Its records aren't selected again; anything ticked below is new information from it.
      </div>

      <div v-if="plan.clinic" class="imp-clinic" data-testid="import-clinic">
        <template v-if="plan.clinic.contact_id">At <b>{{ clinicName }}</b> <span class="pill ok">saved contact</span></template>
        <label v-else class="au-row">
          <input v-model="plan.clinic.selected" type="checkbox" data-testid="import-clinic-new" />
          <span>Add <b>{{ plan.clinic.name }}</b> to Contacts as a {{ contactRoleLabel(plan.clinic.role).toLowerCase() }}</span>
        </label>
      </div>

      <section v-if="plan.records.length" class="imp-sec">
        <h4>Records <span>{{ plan.records.length }}</span></h4>
        <div v-for="(r, i) in plan.records" :key="i" class="imp-rec" :class="{ off: !r.selected }" data-testid="import-record">
          <div class="imp-rec-top">
            <input v-model="r.selected" type="checkbox" :aria-label="`Import ${r.title}`" data-testid="import-record-toggle" />
            <select v-model="r.pet_id" class="imp-pet" :class="{ missing: r.selected && !r.pet_id }" data-testid="import-record-pet">
              <option :value="null" disabled>Whose is this?</option>
              <option v-for="p in pets" :key="p.id" :value="p.id">{{ p.name }}{{ p.archived_at ? ' (archived)' : '' }}</option>
            </select>
            <input v-model="r.occurred_on" type="date" class="imp-date" aria-label="Date" />
            <span v-if="r.cost_cents != null" class="mono imp-cost">{{ money(r.cost_cents) }}</span>
          </div>
          <input v-model="r.title" class="imp-title" aria-label="Title" />
          <div class="imp-meta">
            <select v-model="r.category" class="imp-cat" aria-label="Category">
              <option v-for="c in CATEGORIES" :key="c" :value="c">{{ c }}</option>
            </select>
            <span v-if="r.pet_name_on_document && r.pet_name_on_document !== name(r.pet_id)" class="imp-docname">“{{ r.pet_name_on_document }}” on the document</span>
          </div>
          <div v-if="r.duplicate_of" class="imp-note warn" data-testid="import-record-dup">
            {{ r.duplicate_of.reason }}: <router-link :to="`/pets/${r.pet_id}`">{{ r.duplicate_of.title }} ({{ shortDate(r.duplicate_of.occurred_on) }})</router-link>
          </div>
          <details v-if="r.notes || r.line_items.length || Object.keys(r.extra).length" class="imp-more">
            <summary>Details</summary>
            <!-- renderMarkdown escapes first and allows a fixed tag set; the notes are untrusted model output. -->
            <div v-if="r.notes" class="mdnotes" v-html="renderMarkdown(r.notes)"></div>
            <div v-for="(li, j) in r.line_items" :key="j" class="imp-li"><span>{{ li.description }}</span><span class="mono">{{ li.amount_cents != null ? money(li.amount_cents) : '' }}</span></div>
            <div v-if="Object.keys(r.extra).length" class="kvchips"><span v-for="(v, k) in r.extra" :key="k" class="kvchip"><b>{{ k }}</b>{{ v }}</span></div>
          </details>
        </div>
      </section>

      <section v-if="hasUpdates" class="imp-sec">
        <h4>Also update</h4>
        <div class="alsoupdate">
          <label v-for="(l, i) in plan.care_team" :key="`ct${i}`" class="au-row" data-testid="import-careteam">
            <input v-model="l.selected" type="checkbox" />
            <span>Add <b>{{ clinicName }}</b> to {{ name(l.pet_id) }}’s care team</span>
          </label>
          <label v-for="(l, i) in plan.clinic_links" :key="`cl${i}`" class="au-row" data-testid="import-clinic-link">
            <input v-model="l.selected" type="checkbox" />
            <span>Mark {{ name(l.pet_id) }}’s saved visit “{{ l.title }}” ({{ shortDate(l.occurred_on) }}) as at <b>{{ clinicName }}</b></span>
          </label>
          <label v-for="(w, i) in plan.weights" :key="`w${i}`" class="au-row" data-testid="import-weight">
            <input v-model="w.selected" type="checkbox" />
            <span v-if="w.existing_id != null">Correct {{ name(w.pet_id) }}’s weight on {{ shortDate(w.recorded_on) }}: <s class="mono">{{ w.existing_value }}</s> → <b class="mono">{{ w.value }} {{ w.unit }}</b></span>
            <span v-else>Log {{ name(w.pet_id) }} at <b class="mono">{{ w.value }} {{ w.unit }}</b> on {{ shortDate(w.recorded_on) }}<span v-if="w.date_is_inferred" class="imp-aside"> (the document doesn’t say when this was measured)</span></span>
          </label>
          <label v-for="(r, i) in plan.reminders" :key="`r${i}`" class="au-row" :class="{ off: r.duplicate_of != null }" data-testid="import-reminder">
            <input v-model="r.selected" type="checkbox" :disabled="r.duplicate_of != null" />
            <span>Remind: {{ name(r.pet_id) }} · <b>{{ r.title }}</b> · {{ shortDate(r.due_on) }}<span v-if="r.duplicate_of != null" class="imp-aside"> (already on the calendar)</span><span v-else-if="r.same_day" class="imp-aside"> ({{ name(r.pet_id) }} already has “{{ r.same_day }}” that day — the same thing?)</span></span>
          </label>
          <label v-for="(c, i) in plan.completed_reminders" :key="`c${i}`" class="au-row" data-testid="import-done">
            <input v-model="c.selected" type="checkbox" />
            <span>Mark done: {{ name(c.pet_id) }} · <b>{{ c.title }}</b> (due {{ shortDate(c.due_on) }})<span v-if="c.reason" class="imp-aside"> — {{ c.reason }}</span></span>
          </label>
          <label v-for="(u, i) in plan.pet_updates" :key="`u${i}`" class="au-row" data-testid="import-petupdate">
            <input v-model="u.selected" type="checkbox" />
            <span>{{ name(u.pet_id) }} · {{ petFieldLabel(u.field) }}: <template v-if="u.current"><s>{{ fieldValue(u, u.current) }}</s> → </template><b>{{ fieldValue(u, u.value) }}</b><span v-if="u.reason" class="imp-aside"> — {{ u.reason }}</span></span>
          </label>
          <label v-for="(g, i) in plan.regimens" :key="`g${i}`" class="au-row" :class="{ off: g.duplicate_of != null }" data-testid="import-regimen">
            <input v-model="g.selected" type="checkbox" :disabled="g.duplicate_of != null" />
            <span>{{ name(g.pet_id) }} is on <b>{{ g.product }}</b> ({{ regimenKindLabel(g.kind).toLowerCase() }}<template v-if="g.frequency">, {{ g.frequency }}</template>)<template v-if="g.protects.length"> · {{ g.protects.map(protectionLabel).join(', ') }}</template><span v-if="g.duplicate_of != null" class="imp-aside"> (already listed)</span><span v-else-if="g.replaces" class="imp-aside"> (a dose change: ends the current entry)</span></span>
          </label>
        </div>
      </section>

      <section v-if="hasInsurance" class="imp-sec">
        <h4>Insurance</h4>
        <div class="alsoupdate">
          <div v-for="(c, i) in plan.claims" :key="`claim${i}`" class="au-row" :class="{ off: c.already_recorded }" data-testid="import-claim">
            <input v-model="c.selected" type="checkbox" :disabled="c.already_recorded" :aria-label="`Record claim ${c.claim_number}`" />
            <span>
              <!-- Always correctable: a claim the model gave the wrong pet. A new
                   pet means the matched visit no longer applies. -->
              <select v-model="c.pet_id" class="imp-pet" :class="{ missing: c.selected && !c.pet_id }" data-testid="import-claim-pet" @change="reassignClaim(c)">
                <option :value="null" disabled>Whose?</option>
                <option v-for="p in pets" :key="p.id" :value="p.id">{{ p.name }}</option>
              </select>
              {{ c.insurer || 'Insurance' }} paid <b class="mono">{{ money(c.paid_cents) }}</b> for {{ shortDate(c.treatment_date) }}
              <template v-if="c.visit_options.length > 1"> → added to
                <select v-model="c.record_id" class="imp-pet" data-testid="import-claim-visit" @change="onVisit(c)">
                  <option v-for="v in c.visit_options" :key="v.record_id" :value="v.record_id">{{ v.title }}{{ v.cost_cents != null ? ` (${money(v.cost_cents)})` : '' }}</option>
                </select>
              </template>
              <template v-else-if="c.record_id != null"> → added to “{{ c.record_title }}”</template>
              <template v-else> → saved as a visit (none saved that day)</template>
              <span class="imp-aside"> · claim {{ c.claim_number }}<template v-if="c.already_recorded"> (already recorded)</template></span>
            </span>
          </div>
          <!-- A div, not a label: the row holds its own pet picker. -->
          <div v-for="(p, i) in plan.policies" :key="`po${i}`" class="au-row" :class="{ off: p.existing_id != null }" data-testid="import-policy">
            <input v-model="p.selected" type="checkbox" :disabled="p.existing_id != null" :aria-label="`Save policy ${p.policy_number || p.provider}`" />
            <span><select v-model="p.pet_id" class="imp-pet" :class="{ missing: p.selected && !p.pet_id }" data-testid="import-policy-pet" @change="reassignPolicy(plan, i, p)">
                <option :value="null" disabled>Whose?</option>
                <option v-for="pt in pets" :key="pt.id" :value="pt.id">{{ pt.name }}</option>
              </select> new {{ p.kind === 'wellness' ? 'wellness plan' : 'policy' }}: <b>{{ p.provider }}</b><template v-if="p.policy_number">{{ ' ' + p.policy_number }}</template><template v-if="p.start_date">, {{ shortDate(p.start_date) }}<template v-if="p.end_date"> – {{ shortDate(p.end_date) }}</template></template><template v-if="p.premium_cents != null"> · {{ money(p.premium_cents) }}/{{ p.premium_interval || 'yr' }}</template><template v-if="p.reimburse_percent != null"> · {{ p.reimburse_percent }}%</template><template v-if="p.deductible_cents != null"> · {{ money(p.deductible_cents) }} deductible</template><span v-if="p.existing_id != null" class="imp-aside"> (already saved)</span></span>
          </div>
          <label v-for="(e, i) in visibleEnds" :key="`pe${i}`" class="au-row" :class="{ off: !endApplies(plan, e) }" data-testid="import-policy-end">
            <input v-model="e.selected" type="checkbox" :disabled="!endApplies(plan, e)" />
            <span>End {{ name(e.pet_id) }}’s {{ e.provider }}<template v-if="e.policy_number">{{ ' ' + e.policy_number }}</template> on {{ shortDate(e.end_date) }} <span class="imp-aside">(replaced by the new policy)</span></span>
          </label>
        </div>
      </section>

      <div v-if="plan.low_confidence_fields.length" class="doublecheck">Double-check: {{ plan.low_confidence_fields.join(', ') }}</div>

      <footer class="imp-foot">
        <span v-if="needPet" class="imp-need" data-testid="import-need-pet">Pick a pet for {{ needPet }} record{{ needPet === 1 ? '' : 's' }} first</span>
        <button class="btn primary" :disabled="!canSave || entry.status === 'saving'" data-testid="import-save" @click="emit('save')">
          {{ entry.status === 'saving' ? 'Saving…' : saveLabel(plan) }}
        </button>
      </footer>
    </template>
  </article>
</template>

<style scoped>
.imp { padding: 14px 16px; margin-bottom: 14px; }
.imp-head { display: flex; align-items: center; gap: 10px; }
.imp-state { margin-top: 10px; font-size: 13.5px; color: var(--ink-2); display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.imp-state.err { color: var(--crit); }
.imp-state.ok { color: var(--good); font-weight: 600; }
.imp-links { display: inline-flex; gap: 12px; }
.imp-links a { color: var(--brand); }
.imp-spin { width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--brand-soft); border-top-color: var(--brand); animation: sp .8s linear infinite; flex: none; }
.imp-summary { margin: 10px 0 0; font-size: 14px; color: var(--ink); }
.imp-dup { margin-top: 10px; font-size: 13px; padding: 10px 12px; border-radius: 12px; background: var(--good-soft); color: var(--ink); }
.imp-dup a, .imp-note a { color: var(--brand); font-weight: 600; }
.imp-clinic { margin-top: 10px; font-size: 13.5px; color: var(--ink-2); }
.imp-sec { margin-top: 14px; }
.imp-sec h4 { font-size: 12px; text-transform: uppercase; letter-spacing: .06em; color: var(--ink-3); margin-bottom: 8px; display: flex; gap: 6px; }
.imp-sec h4 span { color: var(--brand); }
.imp-rec { border: 1px solid var(--border); border-radius: 14px; padding: 10px 12px; margin-bottom: 8px; background: var(--surface); }
/* An unticked record stays readable (its duplicate note matters most then); only the controls recede. */
.imp-rec.off { background: var(--surface-2); }
.imp-rec.off .imp-rec-top, .imp-rec.off .imp-title, .imp-rec.off .imp-meta { opacity: .6; }
.imp-rec-top { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.imp-rec-top input[type="checkbox"] { width: 18px; height: 18px; accent-color: var(--brand); flex: none; }
.imp-pet, .imp-date, .imp-cat { padding: 6px 9px; border-radius: 10px; border: 1px solid var(--border-2); background: var(--surface-2); color: var(--ink); font-size: 13px; min-width: 0; }
.imp-pet { font-weight: 700; }
.imp-pet.missing { border-color: var(--crit); background: var(--crit-soft); color: var(--crit); }
.imp-cost { margin-left: auto; font-size: 13.5px; font-weight: 650; }
.imp-title { width: 100%; margin-top: 8px; padding: 8px 10px; border-radius: 10px; border: 1px solid transparent; background: transparent; color: var(--ink); font-size: 15px; font-weight: 650; font-family: var(--font-d); }
.imp-title:hover, .imp-title:focus { border-color: var(--border-2); background: var(--surface-2); outline: none; }
.imp-meta { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 4px; }
.imp-docname { font-size: 12px; color: var(--ink-3); }
.imp-note { margin-top: 8px; font-size: 12.5px; padding: 7px 10px; border-radius: 10px; }
.imp-note.warn { color: var(--ink-2); background: var(--warn-soft); }
.imp-more { margin-top: 6px; font-size: 13px; }
.imp-more summary { cursor: pointer; color: var(--ink-3); font-weight: 600; font-size: 12.5px; }
.imp-li { display: flex; justify-content: space-between; gap: 10px; font-size: 12.5px; color: var(--ink-2); padding: 2px 0; }
.au-row.off { opacity: .6; }
.imp-aside { color: var(--ink-3); font-size: 12.5px; }
.imp-foot { display: flex; align-items: center; justify-content: flex-end; gap: 12px; margin-top: 14px; flex-wrap: wrap; }
.imp-foot .btn { flex: 0 0 auto; padding-left: 22px; padding-right: 22px; }
.imp-need { font-size: 12.5px; color: var(--crit); font-weight: 600; }
</style>
