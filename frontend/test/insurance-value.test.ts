// Insurance value (#74): the wording helpers and the two surfaces that show
// them (the policy sheet's view and the Costs card). The numbers are the API's,
// copied from the backend's hand-worked Lulu fixtures.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'

vi.mock('../src/api', () => ({
  api: {
    costs: vi.fn(), householdInsuranceValue: vi.fn(), insuranceValue: vi.fn(),
    ingestion: vi.fn(), insuranceDocumentUrl: vi.fn(() => null),
  },
}))
import { api } from '../src/api'
import {
  valueBar, netSentence, breakEvenSentence, missingSentence, assumedSentence,
  preventativeSentence, termLabel,
} from '../src/insuranceValue'
import InsuranceValue from '../src/components/InsuranceValue.vue'
import InsuranceSheet from '../src/components/InsuranceSheet.vue'
import Costs from '../src/views/Costs.vue'

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: '/', component: { template: '<div/>' } }, { path: '/pets/:id', component: { template: '<div/>' } }],
})
const opts = { global: { plugins: [router] } }

const LIMITS = [
  { label: 'Wellness Exam', limit_cents: 7000, unused_cents: null },
  { label: 'Wellness Testing', limit_cents: 18000, unused_cents: null },
  { label: 'Vaccines', limit_cents: 10500, unused_cents: null },
]
const CUR = {
  policy_id: 'cur', pet_id: 'lulu', kind: 'insurance', provider: 'Lemonade', plan_name: null,
  policy_number: 'DEMO-POL-0778', status: 'active', start_date: '2026-09-14', end_date: '2027-09-14',
  phase: 'current' as const, annual_premium_cents: 75033, term_premium_cents: 75033,
  paid_in_cents: 5756, paid_back_cents: 18000, net_cents: 12244,
  payouts: [{ record_id: 4, occurred_on: '2026-09-18', title: 'Blood test', amount_cents: 18000, matched_by: 'claim' as const }],
  break_even: { bill_cents: 133370, premium_cents: 75033, deductible_cents: 50000, reimburse_percent: 90, annual_limit_cents: 5000000, reachable: true },
  preventative: { allowance_cents: 35500, unused_at_least_cents: 17500, limits: LIMITS },
  missing: [], assumed: [],
}
const OLD = {
  ...CUR, policy_id: 'old', policy_number: 'DEMO-POL-0401', status: 'expired',
  start_date: '2025-09-14', end_date: '2026-09-14', phase: 'ended' as const,
  annual_premium_cents: 91136, term_premium_cents: 91136, paid_in_cents: 91136,
  paid_back_cents: 40412, net_cents: -50724,
  payouts: [
    { record_id: 1, occurred_on: '2026-02-12', title: 'Wellness exam', amount_cents: 5000, matched_by: 'claim' as const },
    { record_id: 2, occurred_on: '2026-05-20', title: 'Ear infection', amount_cents: 17700, matched_by: 'term' as const },
    { record_id: 3, occurred_on: '2026-09-04', title: 'Comprehensive exam', amount_cents: 17712, matched_by: 'claim' as const },
  ],
  break_even: { ...CUR.break_even, bill_cents: 151263, premium_cents: 91136 },
  preventative: { allowance_cents: 35500, unused_at_least_cents: 0, limits: LIMITS },
}

enableAutoUnmount(afterEach)
beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset && fn.mockReset()
  api.ingestion.mockResolvedValue({ enabled: false, has_key: false })
})

describe('insurance value wording', () => {
  it('sizes the bar as paid back over paid in', () => {
    expect(valueBar(91136, 40412)).toEqual({ back: 44, rest: 56 })
    expect(valueBar(5756, 18000)).toEqual({ back: 100, rest: 0 }) // ahead: all green
    expect(valueBar(null, 18000)).toEqual({ back: 100, rest: 0 })
    expect(valueBar(0, 0)).toBeNull()
    expect(valueBar(undefined, 0)).toBeNull()
  })

  it('says the net plainly, either way', () => {
    expect(netSentence(-50724)).toBe('$507.24 more paid in than back')
    expect(netSentence(12244, true)).toBe('$122.44 more back than paid in so far')
    expect(netSentence(0)).toBe('Paid back exactly what was paid in')
    expect(netSentence(null)).toBe('')
  })

  it('shows the break-even with its formula, or why there is none', () => {
    expect(breakEvenSentence(OLD.break_even)).toEqual({
      lead: "One eligible bill of $1,512.63 would, on its own, return this term's premium.",
      formula: '$500 deductible + $911.36 ÷ 90%',
    })
    expect(breakEvenSentence({ ...OLD.break_even, reimburse_percent: 0, bill_cents: null, reachable: false }).lead)
      .toBe('At 0% reimbursement, no bill returns the premium.')
    expect(breakEvenSentence({ ...OLD.break_even, annual_limit_cents: 50000, bill_cents: null, reachable: false }).lead)
      .toBe('The $500 annual limit is below the $911.36 premium, so no single bill returns it.')
  })

  it('asks for what is missing, and owns up to assumptions', () => {
    expect(missingSentence(['reimbursement %', 'deductible'])).toBe('Add the reimbursement % and deductible to this policy to fill in the rest.')
    expect(missingSentence(['premium', 'start date', 'deductible'])).toBe('Add the premium, start date and deductible to this policy to fill in the rest.')
    expect(missingSentence([])).toBe('')
    expect(missingSentence(null)).toBe('')
    expect(assumedSentence({ assumed: ['end date', 'premium per year'], end_date: '2027-01-01' })).toBe('Assuming it renews Jan 1, 2027 and the premium is per year.')
    expect(assumedSentence({ assumed: [], end_date: null })).toBe('')
  })

  it('nudges about preventative allowance only while it can still be used', () => {
    const all = { allowance_cents: 35500, unused_at_least_cents: 35500, limits: LIMITS.map((l) => ({ ...l, unused_cents: l.limit_cents })) }
    expect(preventativeSentence(all, 'current')).toBe('No payouts recorded this term, so all $355 of preventative allowance looks unused.')
    expect(preventativeSentence(CUR.preventative, 'current')).toBe('Going by recorded payouts, at least $175 of the $355 preventative allowance is unused this term.')
    expect(preventativeSentence(OLD.preventative, 'current')).toBe('$355 of preventative allowance this term.')
    expect(preventativeSentence(CUR.preventative, 'ended')).toBe('$355 of preventative allowance for the term.')
    // A wellness plan: usage unknown (no per-limit amounts, lower bound 0).
    expect(preventativeSentence({ allowance_cents: 35500, unused_at_least_cents: 0, limits: LIMITS }, 'current')).toBe('$355 of preventative allowance this term.')
  })

  it('labels the term', () => {
    expect(termLabel(CUR)).toBe('This term')
    expect(termLabel({ phase: 'upcoming', start_date: '2027-09-14', end_date: null })).toBe('Starts Sep 14, 2027')
    expect(termLabel(OLD)).toBe('Sep 14, 2025 → Sep 14, 2026')
    expect(termLabel({ phase: 'unknown', start_date: null, end_date: null })).toBe('Term dates not recorded')
  })
})

describe('InsuranceValue (policy view section)', () => {
  it('shows a current term: numbers, bar, break-even formula, nudge, payouts', () => {
    const w = mount(InsuranceValue, { props: { value: CUR } })
    expect(w.find('[data-testid="insurance-value-in"]').text()).toBe('$57.56')
    expect(w.text()).toContain('of $750.33 this term')
    expect(w.find('[data-testid="insurance-value-back"]').text()).toBe('$180')
    expect(w.find('[data-testid="insurance-value-net"]').text()).toBe('$122.44 more back than paid in so far')
    expect(w.find('[data-testid="insurance-value-breakeven"]').text()).toContain('One eligible bill of $1,333.70')
    expect(w.find('[data-testid="insurance-value-breakeven"]').text()).toContain('$500 deductible + $750.33 ÷ 90%')
    expect(w.find('[data-testid="insurance-value-prev"]').text()).toContain('at least $175')
    expect(w.findAll('[data-testid="insurance-value-payout"]').length).toBe(1)
    expect(w.text()).toContain('on 1 visit')
  })

  it('shows an ended term with what was not paid back, and no per-limit chips', () => {
    const w = mount(InsuranceValue, { props: { value: OLD } })
    const segs = w.findAll('[data-testid="insurance-value-bar"] .seg')
    expect(segs.length).toBe(2)
    expect(segs[0].text()).toBe('$404.12 back')
    expect(segs[1].text()).toBe('$507.24 not back')
    expect(w.text()).toContain('for the term')
    expect(w.text()).toContain('on 3 visits')
    expect(w.find('.reqchip').exists()).toBe(false)
  })

  it('lists exact sub-limits left when nothing has been paid back yet', () => {
    const fresh = {
      ...CUR, paid_back_cents: 0, net_cents: -5756, payouts: [],
      preventative: { allowance_cents: 35500, unused_at_least_cents: 35500, limits: LIMITS.map((l) => ({ ...l, unused_cents: l.limit_cents })) },
    }
    const w = mount(InsuranceValue, { props: { value: fresh } })
    const chips = w.findAll('.reqchip').map((c) => c.text())
    expect(chips).toEqual(['$70 of Wellness Exam unused', '$180 of Wellness Testing unused', '$105 of Vaccines unused'])
    expect(w.find('[data-testid="insurance-value-bar"] .seg').text()).toBe('$57.56 not back yet')
  })

  it('names what is missing instead of guessing a break-even', () => {
    const w = mount(InsuranceValue, {
      props: { value: { ...CUR, break_even: null, preventative: null, missing: ['reimbursement %'], assumed: ['end date'] } },
    })
    expect(w.find('[data-testid="insurance-value-breakeven"]').exists()).toBe(false)
    expect(w.find('[data-testid="insurance-value-missing"]').text()).toBe('Add the reimbursement % to this policy to fill in the rest.')
    expect(w.text()).toContain('Assuming it renews Sep 14, 2027.')
  })

  it('shows a wellness plan as its fee, with no claims bar', () => {
    const wellness = {
      ...CUR, kind: 'wellness', paid_in_cents: 47940, term_premium_cents: 47940, paid_back_cents: 0,
      net_cents: -47940, payouts: [], break_even: null, preventative: null, phase: 'ended' as const,
    }
    const w = mount(InsuranceValue, { props: { value: wellness } })
    expect(w.find('[data-testid="insurance-value-wellness"]').text()).toContain('paid at the counter')
    expect(w.find('[data-testid="insurance-value-bar"]').exists()).toBe(false)
    expect(w.find('[data-testid="insurance-value-net"]').exists()).toBe(false)
    expect(w.find('[data-testid="insurance-value-in"]').text()).toBe('$479.40')
    expect(w.find('[data-testid="insurance-value-back"]').exists()).toBe(false) // nothing to show back
    expect(w.find('h4').text()).toContain('Paid in')
    expect(w.find('h4').text()).not.toContain('paid back')
  })

  it('counts visits, not payout rows (one visit can be split by note and date)', () => {
    const split = {
      ...CUR,
      payouts: [
        { record_id: 7, occurred_on: '2026-09-20', title: 'Exam', amount_cents: 2000, matched_by: 'claim' as const },
        { record_id: 7, occurred_on: '2026-09-20', title: 'Exam', amount_cents: 4000, matched_by: 'term' as const },
      ],
    }
    expect(mount(InsuranceValue, { props: { value: split } }).text()).toContain('on 1 visit')
  })

  it('owns up when a wellness plan did record a payout', () => {
    const w = mount(InsuranceValue, { props: { value: { ...CUR, kind: 'wellness', break_even: null, preventative: null } } })
    expect(w.find('[data-testid="insurance-value-wellness"]').text()).toContain('payouts below are claims recorded against this plan')
    expect(w.find('[data-testid="insurance-value-back"]').exists()).toBe(true)
  })

  it('says when a term has no payouts yet, or at all', () => {
    const none = { ...CUR, paid_back_cents: 0, net_cents: -5756, payouts: [] }
    expect(mount(InsuranceValue, { props: { value: none } }).text()).toContain('no payouts yet')
    expect(mount(InsuranceValue, { props: { value: { ...none, phase: 'ended' as const } } }).text()).toContain('no payouts')
  })
})

describe('InsuranceSheet view + value (#74)', () => {
  const POLICY = {
    id: 'cur', pet_id: 'lulu', kind: 'insurance', provider: 'Lemonade', plan_name: null,
    policy_number: 'DEMO-POL-0778', status: 'active', start_date: '2026-09-14', end_date: '2027-09-14',
    premium_cents: 75033, premium_interval: 'year', reimburse_percent: 90, deductible_cents: 50000,
    annual_limit_cents: 5000000, notes: null, coverages: null, exclusions: null,
    document_name: null, document_media_type: null, document_size_bytes: null,
    created_by: null, created_at: '', updated_at: '',
  }
  it('loads the term value for an existing policy', async () => {
    api.insuranceValue.mockResolvedValue(CUR)
    const w = mount(InsuranceSheet, { props: { petId: 'lulu', petName: 'Lulu', policy: POLICY }, ...opts })
    await flushPromises()
    expect(api.insuranceValue).toHaveBeenCalledWith('cur')
    expect(w.find('[data-testid="insurance-value"]').exists()).toBe(true)
  })
  it('still shows the policy when the value cannot load', async () => {
    api.insuranceValue.mockRejectedValue(new Error('500'))
    const w = mount(InsuranceSheet, { props: { petId: 'lulu', petName: 'Lulu', policy: POLICY }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="insurance-view"]').exists()).toBe(true)
    expect(w.find('[data-testid="insurance-value"]').exists()).toBe(false)
  })
  it('does not ask for a value while adding a new policy', async () => {
    mount(InsuranceSheet, { props: { petId: 'lulu', petName: 'Lulu' }, ...opts })
    await flushPromises()
    expect(api.insuranceValue).not.toHaveBeenCalled()
  })
})

describe('Costs insurance value card (#74)', () => {
  const COSTS = { total_cents: 100000, covered_cents: 58412, by_category: [], by_pet: [] }
  const pet = (over = {}) => ({
    pet_id: 'lulu', name: 'Lulu', as_of: '2026-10-11', policies: [CUR, OLD],
    paid_in_cents: 96892, paid_back_cents: 58412, net_cents: -38480,
    paid_in_incomplete: false, unmatched_paid_back_cents: 0, wellness_paid_in_cents: 0, ...over,
  })

  it('shows each insured pet with totals, bar, net and a current-term hint', async () => {
    api.costs.mockResolvedValue(COSTS)
    api.householdInsuranceValue.mockResolvedValue({ as_of: '2026-10-11', pets: [pet({ unmatched_paid_back_cents: 8000 })] })
    const w = mount(Costs, opts)
    await flushPromises()
    const row = w.find('[data-testid="insurance-value-row"][data-pet="lulu"]')
    expect(row.text()).toContain('$968.92 in')
    expect(row.text()).toContain('$584.12 back')
    expect(row.text()).toContain('$384.80 more paid in than back so far')
    expect(row.text()).toContain('at least $175 of the $355 preventative allowance')
    expect(row.text()).toContain('$80 paid back on visits outside any recorded term.')
    expect(row.findAll('.covered .seg').length).toBe(2)
    expect(w.text()).toContain('All time') // the totals were never "this year"
  })

  it('falls back to the break-even hint, then to none', async () => {
    api.costs.mockResolvedValue(COSTS)
    const noPrev = { ...CUR, preventative: null }
    api.householdInsuranceValue.mockResolvedValue({
      as_of: '2026-10-11',
      pets: [
        pet({ policies: [noPrev] }),
        pet({ pet_id: 'pedro', name: 'Pedro', policies: [OLD], paid_in_cents: 0, paid_back_cents: 0, net_cents: 0 }),
      ],
    })
    const w = mount(Costs, opts)
    await flushPromises()
    expect(w.find('[data-pet="lulu"]').text()).toContain('This term breaks even with one eligible bill of $1,333.70.')
    const pedro = w.find('[data-pet="pedro"]')
    expect(pedro.text()).not.toContain('This term')
    expect(pedro.find('.covered').exists()).toBe(false) // nothing to draw
  })

  it('states a wellness plan as its fee, never as money not paid back', async () => {
    api.costs.mockResolvedValue(COSTS)
    const plan = { ...CUR, policy_id: 'wp', kind: 'wellness', break_even: null, preventative: null }
    api.householdInsuranceValue.mockResolvedValue({
      as_of: '2026-10-11',
      pets: [
        pet({ pet_id: 'pedro', name: 'Pedro', policies: [plan], paid_in_cents: 0, paid_back_cents: 0, net_cents: 0, wellness_paid_in_cents: 29552 }),
        pet({ wellness_paid_in_cents: 4000, policies: [plan, CUR] }),
      ],
    })
    const w = mount(Costs, opts)
    await flushPromises()
    const pedro = w.find('[data-pet="pedro"]')
    expect(pedro.text()).toContain('$295.52 in')
    expect(pedro.text()).toContain('$295.52 in wellness-plan fees so far. Paid at the counter, so there are no claims to add up.')
    expect(pedro.text()).not.toContain('more paid in than back')
    expect(pedro.find('.covered').exists()).toBe(false)
    const lulu = w.find('[data-pet="lulu"]')
    expect(lulu.text()).toContain('Plus $40 in wellness-plan fees, paid at the counter.')
    expect(lulu.text()).toContain('at least $175') // the hint reads the current INSURANCE term
  })

  it('shows no net when a policy has no premium recorded', async () => {
    api.costs.mockResolvedValue(COSTS)
    api.householdInsuranceValue.mockResolvedValue({ as_of: '2026-10-11', pets: [pet({ paid_in_incomplete: true })] })
    const w = mount(Costs, opts)
    await flushPromises()
    const row = w.find('[data-pet="lulu"]')
    expect(row.text()).toContain("no premium or dates recorded, so there's no net yet")
    expect(row.text()).not.toContain('more paid in than back')
    expect(row.find('.covered').exists()).toBe(false)
  })

  it('leaves the card out when no pet has a policy, or the value fails', async () => {
    api.costs.mockResolvedValue(COSTS)
    api.householdInsuranceValue.mockResolvedValue({ as_of: '2026-10-11', pets: [] })
    let w = mount(Costs, opts)
    await flushPromises()
    expect(w.find('[data-testid="insurance-value-card"]').exists()).toBe(false)
    api.householdInsuranceValue.mockRejectedValue(new Error('boom'))
    w = mount(Costs, opts)
    await flushPromises()
    expect(w.find('[data-testid="insurance-value-card"]').exists()).toBe(false)
    expect(w.find('[data-testid="cost-donut"]').exists()).toBe(true)
  })
})
