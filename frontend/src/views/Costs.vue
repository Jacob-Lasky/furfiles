<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { api } from '../api'
import { money, categoryColor } from '../format'
import { valueBar, netSentence, preventativeSentence } from '../insuranceValue'
import type { CostsSummary, InsuranceValueSummary, PetInsuranceValue } from '../types'

const data = ref<CostsSummary | null>(null)
// Insurance value per pet (#74). Loaded on its own so a failure here never
// hides the spend donut; the card is simply left out.
const value = ref<InsuranceValueSummary | null>(null)
const loading = ref(true)
const error = ref('')

const R = 40
const C = 2 * Math.PI * R

const segments = computed(() => {
  if (!data.value) return []
  const total = data.value.by_category.reduce((s, c) => s + c.cost_cents, 0) || 1
  let acc = 0
  return data.value.by_category.map((c, i) => {
    const len = (C * c.cost_cents) / total
    const seg = { ...c, color: categoryColor(c.category, i), len, offset: -acc }
    acc += len
    return seg
  })
})
const coveredPct = computed(() => {
  if (!data.value || !data.value.total_cents) return 0
  return Math.round((data.value.covered_cents / data.value.total_cents) * 100)
})

// One short line under a pet's bar about its current term: what's still there
// to use, else what one bill would take to come out even. Empty when neither
// is known.
function termHint(p: PetInsuranceValue): string {
  const live = p.policies.filter((t) => t.phase === 'current')
  const cur = live.find((t) => t.kind === 'insurance') || live[0]
  if (!cur) return ''
  if (cur.preventative && cur.preventative.unused_at_least_cents > 0) return preventativeSentence(cur.preventative, cur.phase)
  if (cur.break_even?.bill_cents != null) return `This term breaks even with one eligible bill of ${money(cur.break_even.bill_cents)}.`
  return ''
}
// A wellness plan pays at the counter, so the API keeps its fee out of the
// paid in / paid back totals; a pet with only a wellness plan gets its fee
// stated, with no bar or net to misread.
function wellnessLine(p: PetInsuranceValue, insured: boolean): string {
  const fee = money(p.wellness_paid_in_cents)
  if (!p.wellness_paid_in_cents) return ''
  return insured
    ? `Plus ${fee} in wellness-plan fees, paid at the counter.`
    : `${fee} in wellness-plan fees so far. Paid at the counter, so there are no claims to add up.`
}
const valueRows = computed(() =>
  (value.value?.pets || []).map((p) => {
    const insured = p.policies.some((t) => t.kind === 'insurance')
    // A policy with no recorded premium leaves paid in short, so a bar or net
    // over it would overstate what came back; the row says what's missing.
    const complete = insured && !p.paid_in_incomplete
    return {
      p,
      insured,
      complete,
      bar: complete ? valueBar(p.paid_in_cents, p.paid_back_cents) : null,
      hint: termHint(p),
      wellness: wellnessLine(p, insured),
    }
  }),
)
async function loadValue() {
  try { value.value = (await api.householdInsuranceValue()) || null } catch (_) { value.value = null }
}

onMounted(async () => {
  loadValue()
  try { data.value = await api.costs() } catch (e) { error.value = e.message } finally { loading.value = false }
})
</script>

<template>
  <div>
    <div class="apphead"><div><div class="eyebrow">All time</div><h1>Costs</h1><div class="sub">Where it goes, and what insurance gave back.</div></div></div>

    <div v-if="loading" class="loading">Adding it up…</div>
    <div v-else-if="error" class="errbox">{{ error }}</div>
    <div v-else-if="data" class="pad">
      <div class="cost-hero">
        <div class="card card-pad">
          <div class="donut-wrap">
            <svg class="donut" data-testid="cost-donut" viewBox="0 0 100 100">
              <circle v-for="s in segments" :key="s.category" cx="50" cy="50" :r="R" fill="none"
                :stroke="s.color" stroke-width="16"
                :stroke-dasharray="`${s.len.toFixed(1)} ${(C - s.len).toFixed(1)}`"
                :stroke-dashoffset="s.offset.toFixed(1)" transform="rotate(-90 50 50)" />
              <text x="50" y="46" text-anchor="middle" font-size="15" font-weight="800" fill="var(--ink)" style="font-family:var(--font-d)">{{ money(data.total_cents) }}</text>
              <text x="50" y="60" text-anchor="middle" font-size="7" fill="var(--ink-3)">all time</text>
            </svg>
            <div class="legend">
              <div v-for="s in segments" :key="s.category" class="legrow">
                <span class="sw" :style="{ background: s.color }"></span>
                <span class="nm">{{ s.category }}</span>
                <span class="amt">{{ money(s.cost_cents) }}</span>
              </div>
              <div v-if="!segments.length" style="color:var(--ink-3);font-size:13px">No costs recorded yet.</div>
            </div>
          </div>
        </div>

        <div class="card card-pad">
          <h4 style="font-size:15px">Insurance</h4>
          <div style="font-size:12.5px;color:var(--ink-2);margin-top:3px">{{ money(data.covered_cents) }} of {{ money(data.total_cents) }} came back.</div>
          <div class="covered">
            <div class="seg" :style="{ background: 'var(--good)', width: coveredPct + '%' }" v-if="coveredPct">{{ money(data.covered_cents) }} back</div>
            <div class="seg" :style="{ background: 'var(--warn)', color: '#3a2a10', width: (100 - coveredPct) + '%' }">{{ money(data.total_cents - data.covered_cents) }} you</div>
          </div>
          <div v-for="p in data.by_pet" :key="p.pet_id" class="ins-row" data-testid="insurance-row" :data-pet="p.pet_id">
            <div class="co">
              <span class="ins-logo" :style="p.insurer ? { background: 'var(--good)' } : {}">{{ p.insurer ? p.insurer[0] : '·' }}</span>
              <router-link :to="`/pets/${p.pet_id}`">{{ p.name }}<span style="color:var(--ink-3)"> · {{ p.insurer || 'no policy' }}</span></router-link>
            </div>
            <span class="mono" :style="{ color: p.covered_cents ? 'var(--good)' : 'var(--ink-3)', fontWeight: 700 }">
              {{ p.covered_cents ? money(p.covered_cents) : money(p.cost_cents) }}
            </span>
          </div>
        </div>

        <div v-if="valueRows.length" class="card card-pad ivcard" data-testid="insurance-value-card">
          <h4 style="font-size:15px">Insurance value</h4>
          <div style="font-size:12.5px;color:var(--ink-2);margin-top:3px">What each pet's policies cost, next to what they paid back.</div>
          <div class="ivgrid">
            <div v-for="{ p, insured, complete, bar, hint, wellness } in valueRows" :key="p.pet_id" class="ivrow" data-testid="insurance-value-row" :data-pet="p.pet_id">
              <div class="ivrow-head">
                <router-link :to="`/pets/${p.pet_id}`">{{ p.name }}</router-link>
                <span v-if="insured" class="mono">{{ money(p.paid_in_cents) }} in · <b>{{ money(p.paid_back_cents) }} back</b></span>
                <span v-else class="mono">{{ money(p.wellness_paid_in_cents) }} in</span>
              </div>
              <div v-if="bar" class="covered slim">
                <div class="seg" :style="{ background: 'var(--good)', width: bar.back + '%' }"></div>
                <div class="seg" :style="{ background: 'var(--warn)', width: bar.rest + '%' }"></div>
              </div>
              <div v-if="complete" class="ivrow-foot">{{ netSentence(p.net_cents, true) }}</div>
              <div v-else-if="insured" class="ivrow-foot">Paid in leaves out a policy with no premium or dates recorded, so there's no net yet.</div>
              <div v-if="wellness" class="ivrow-foot">{{ wellness }}</div>
              <div v-if="hint" class="ivrow-foot">{{ hint }}</div>
              <div v-if="p.unmatched_paid_back_cents" class="ivrow-foot">{{ money(p.unmatched_paid_back_cents) }} paid back on visits outside any recorded term.</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@media (min-width: 720px) { .ivcard { grid-column: 1 / -1; } }
.ivgrid { display: grid; gap: 4px 22px; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); margin-top: 6px; }
.ivrow { padding: 11px 0; border-top: 1px solid var(--border); }
.ivrow-head { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; font-size: 13.5px; }
.ivrow-head a { font-weight: 650; }
.ivrow-head .mono { font-size: 12.5px; color: var(--ink-2); white-space: nowrap; }
.ivrow-head b { color: var(--good); font-weight: 700; }
.covered.slim { height: 10px; border-radius: 6px; margin: 7px 0 5px; }
.ivrow-foot { font-size: 12.5px; color: var(--ink-2); margin-top: 2px; }
</style>
