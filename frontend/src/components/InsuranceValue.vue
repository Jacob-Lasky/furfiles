<script setup lang="ts">
// One policy term's value (#74), shown in the policy sheet's view: paid in vs
// paid back as the Costs page's covered-vs-out-of-pocket bar, the net, the
// accident & illness break-even with its formula, the preventative allowance,
// and the visits the payouts came from. Plain numbers, no verdict.
import { computed } from 'vue'
import { money, longDate } from '../format'
import {
  valueBar, netSentence, breakEvenSentence, missingSentence, assumedSentence,
  preventativeSentence, termLabel,
} from '../insuranceValue'
import type { PolicyValue } from '../types'

const props = defineProps<{ value: PolicyValue }>()

const v = computed(() => props.value)
const wellness = computed(() => v.value.kind === 'wellness')
const live = computed(() => v.value.phase === 'current')
// A wellness plan with no payouts has nothing to show on the "back" side.
const showBack = computed(() => !wellness.value || v.value.paid_back_cents > 0)
const visits = computed(() => {
  // One visit can carry two payout rows (part by claim note, part by date).
  const n = new Set(v.value.payouts.map((p) => p.record_id)).size
  if (!n) return live.value ? 'no payouts yet' : 'no payouts'
  return n === 1 ? 'on 1 visit' : `on ${n} visits`
})
const bar = computed(() => valueBar(v.value.paid_in_cents, v.value.paid_back_cents))
const breakEven = computed(() => (v.value.break_even ? breakEvenSentence(v.value.break_even) : null))
// Per-limit "left" chips only when exact (nothing paid back yet this term).
const leftChips = computed(() =>
  live.value || v.value.phase === 'upcoming'
    ? (v.value.preventative?.limits || []).filter((l) => l.unused_cents != null)
    : [],
)
const termCost = computed(() => {
  const t = v.value.term_premium_cents
  if (t == null) return ''
  return v.value.phase === 'ended' ? 'for the term' : `of ${money(t)} this term`
})
</script>

<template>
  <div class="cv-block ivx" data-testid="insurance-value">
    <h4>{{ showBack ? 'Paid in & paid back' : 'Paid in' }} <small class="ivx-term">{{ termLabel(v) }}</small></h4>

    <div class="ivx-nums">
      <div>
        <span class="ivx-lab">Paid in</span>
        <b class="mono" data-testid="insurance-value-in">{{ money(v.paid_in_cents) }}</b>
        <small v-if="termCost">{{ termCost }}</small>
      </div>
      <div v-if="showBack">
        <span class="ivx-lab">Paid back</span>
        <b class="mono ivx-back" data-testid="insurance-value-back">{{ money(v.paid_back_cents) }}</b>
        <small>{{ visits }}</small>
      </div>
    </div>

    <div v-if="bar && showBack" class="covered" data-testid="insurance-value-bar">
      <div v-if="bar.back" class="seg" :style="{ background: 'var(--good)', width: bar.back + '%' }">{{ money(v.paid_back_cents) }} back</div>
      <div v-if="bar.rest" class="seg" :style="{ background: 'var(--warn)', color: '#3a2a10', width: bar.rest + '%' }">
        {{ money((v.paid_in_cents || 0) - v.paid_back_cents) }} not back{{ live ? ' yet' : '' }}
      </div>
    </div>
    <p v-if="showBack" class="ivx-net" data-testid="insurance-value-net">{{ netSentence(v.net_cents, live) }}</p>

    <p v-if="wellness" class="ivx-p" data-testid="insurance-value-wellness">
      <template v-if="v.paid_back_cents > 0">Wellness plans mostly pay at the counter; the payouts below are claims recorded against this plan. What the fee includes is under What's covered.</template>
      <template v-else>Wellness plans are paid at the counter, not by claims, so there's no payout to add up here. What the fee includes is under What's covered.</template>
    </p>

    <p v-if="breakEven" class="ivx-p" data-testid="insurance-value-breakeven">
      <b>{{ breakEven.lead }}</b>
      <span v-if="breakEven.formula" class="ivx-formula mono">{{ breakEven.formula }}</span>
    </p>

    <template v-if="v.preventative">
      <p class="ivx-p" data-testid="insurance-value-prev">{{ preventativeSentence(v.preventative, v.phase) }}</p>
      <div v-if="leftChips.length" class="reqchips">
        <span v-for="l in leftChips" :key="l.label" class="reqchip prot">{{ money(l.unused_cents) }} of {{ l.label }} unused</span>
      </div>
    </template>

    <p v-if="missingSentence(v.missing)" class="ivx-p ivx-muted" data-testid="insurance-value-missing">{{ missingSentence(v.missing) }}</p>
    <p v-if="assumedSentence(v)" class="ivx-p ivx-muted">{{ assumedSentence(v) }}</p>

    <div v-if="v.payouts.length" class="ivx-pays">
      <div v-for="p in v.payouts" :key="`${p.record_id}-${p.matched_by}`" class="ivx-pay" data-testid="insurance-value-payout">
        <span class="ivx-date">{{ longDate(p.occurred_on) }}</span>
        <span class="ivx-title">{{ p.title }}</span>
        <span class="mono ivx-back">{{ money(p.amount_cents) }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ivx-term { text-transform: none; letter-spacing: 0; font-weight: 500; color: var(--ink-2); margin-left: 6px; }
.ivx-nums { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.ivx-nums > div { display: flex; flex-direction: column; gap: 1px; }
.ivx-lab { font-size: 11.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--ink-3); font-weight: 700; }
.ivx-nums b { font-size: 20px; font-weight: 800; }
.ivx-nums small { font-size: 12px; color: var(--ink-2); }
.ivx-back { color: var(--good); }
.ivx-net { font-size: 13px; color: var(--ink-2); margin: 2px 0 0; }
.ivx-p { font-size: 13.5px; color: var(--ink); margin: 10px 0 0; line-height: 1.45; }
.ivx-p b { font-weight: 650; }
.ivx-formula { display: block; font-size: 12px; color: var(--ink-2); margin-top: 2px; }
.ivx-muted { color: var(--ink-2); font-size: 12.5px; }
.reqchips { margin-top: 8px; }
.ivx-pays { margin-top: 12px; border-top: 1px solid var(--border); }
.ivx-pay { display: flex; gap: 10px; align-items: baseline; padding: 7px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
.ivx-pay:last-child { border-bottom: none; }
.ivx-date { flex: none; color: var(--ink-3); font-size: 12px; min-width: 88px; }
.ivx-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink-2); }
.ivx-pay .mono { font-weight: 700; }
</style>
