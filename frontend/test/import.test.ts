import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'

vi.mock('../src/api', () => ({
  api: { pets: vi.fn(), ingestion: vi.fn(), importAnalyze: vi.fn(), importCommit: vi.fn() },
}))
import { api } from '../src/api'
// The real libheif wasm never loads under test; the mock stands in for the JPEG it returns.
vi.mock('heic2any', () => ({ default: vi.fn(async () => new Blob([new Uint8Array([0xff, 0xd8, 0xff, 0xe0])], { type: 'image/jpeg' })) }))
import heic2any from 'heic2any'
import Import from '../src/views/Import.vue'
import ImportFileCard from '../src/components/ImportFileCard.vue'
import {
  mediaTypeOf, importable, unsupportedReason, planCounts, hasWork, recordsNeedingPet,
  saveLabel, resultSummary, petFieldLabel, petFieldValue, petName, shortDate, runLimited,
} from '../src/importer'
import type { ImportPlan, ImportResult } from '../src/types'

enableAutoUnmount(beforeEach)

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/', component: { template: '<div/>' } },
    { path: '/import', component: { template: '<div/>' } },
    { path: '/pets/:id', component: { template: '<div/>' } },
    { path: '/settings', component: { template: '<div/>' } },
  ],
})
const opts = { global: { plugins: [router] } }

const PETS = [
  { id: 'lulu', name: 'Lulu', species: 'Dog', archived_at: null },
  { id: 'kipper', name: 'Kipper', species: 'Cat', archived_at: null },
  { id: 'koa', name: 'Koa', species: 'Horse', archived_at: '2026-08-01' },
]

function plan(over: Partial<ImportPlan> = {}): ImportPlan {
  return {
    sha256: 'abc', file_duplicates: [], summary: 'An invoice for Lulu.',
    clinic: { selected: true, contact_id: 'amc', matched_name: 'AMC of Springfield', name: 'Animal Medical Center of Springfield', role: 'vet', phone: null, email: null, address: null, website: null },
    records: [
      { selected: true, pet_id: 'lulu', pet_name_on_document: 'Lulu', occurred_on: '2026-09-12', ends_on: null, title: 'Panacur dispensed', category: 'Prescriptions', cost_cents: 7250, line_items: [{ description: 'Panacur', amount_cents: 7250 }], notes: 'Three-day course.', extra: { Lot: 'A1' }, duplicate_of: null },
    ],
    care_team: [{ selected: true, pet_id: 'lulu' }],
    clinic_links: [{ selected: true, record_id: 11, pet_id: 'lulu', title: 'Recheck exam', occurred_on: '2026-09-11' }],
    weights: [
      { selected: false, pet_id: 'lulu', recorded_on: '2026-09-11', value: 55.4, unit: 'lb', existing_id: 3, existing_value: 55.8, date_is_inferred: false },
      { selected: false, pet_id: 'kipper', recorded_on: '2026-10-06', value: 14.5, unit: 'lb', existing_id: null, existing_value: null, date_is_inferred: true },
    ],
    reminders: [
      { selected: true, pet_id: 'kipper', due_on: '2026-11-17', title: 'Feline Annual Exam', duplicate_of: null, same_day: 'Annual exam' },
      { selected: false, pet_id: 'lulu', due_on: '2027-03-05', title: 'Lepto - Annual', duplicate_of: 7 },
    ],
    completed_reminders: [{ selected: true, reminder_id: 2, pet_id: 'lulu', title: 'SNAP 4Dx', due_on: '2026-11-05', reason: 'Run 9/4' }],
    pet_updates: [
      { selected: true, pet_id: 'kipper', field: 'microchip', current: null, value: '000000000000201', reason: null },
      { selected: false, pet_id: 'kipper', field: 'name', current: 'Kipper', value: 'Sir Whiskers', reason: null },
    ],
    regimens: [
      { selected: true, pet_id: 'kipper', kind: 'preventative', product: 'Revolution Plus', amount: null, frequency: 'monthly', protects: ['flea', 'tick'], started_on: null, ended_on: null, notes: null, duplicate_of: null },
      { selected: false, pet_id: 'kipper', kind: 'food', product: "Hill's 7+", amount: null, frequency: null, protects: [], started_on: null, ended_on: null, notes: null, duplicate_of: 'r1' },
    ],
    claims: [], policies: [], policy_ends: [],
    low_confidence_fields: ['weight date'],
    provider: 'anthropic', model: 'claude-sonnet-5',
    ...over,
  }
}

const RESULT: ImportResult = {
  records: [{ id: 30, pet_id: 'lulu' } as never], contact: null, care_team_links: 1, clinic_links: 2,
  weights_added: 1, weights_corrected: 1, reminders_added: 2, reminders_completed: 1,
  pet_updates: 1, regimens_added: 1, regimens_ended: 0, claims_recorded: 2, policies_added: 1, policies_ended: 1, skipped_existing: 0,
}

// ---------- pure helpers ----------

describe('importer helpers', () => {
  it('derives a media type from the extension when the browser gives none', () => {
    expect(mediaTypeOf({ name: 'AVIEE553AD0.PDF', type: '' })).toBe('application/pdf')
    expect(mediaTypeOf({ name: 'x.jpeg', type: '' })).toBe('image/jpeg')
    expect(mediaTypeOf({ name: 'x.heic', type: '' })).toBe('image/heic')
    expect(mediaTypeOf({ name: 'x', type: '' })).toBe('application/octet-stream')
    expect(mediaTypeOf({ name: 'x.pdf', type: 'image/png' })).toBe('image/png')
    expect(importable('application/pdf')).toBe(true)
    expect(importable('image/heic')).toBe(false) // never sent as-is: converted first
    expect(unsupportedReason('text/plain')).toContain('Only PDFs')
    expect(unsupportedReason('text/plain')).toContain('HEIC')
  })

  it('counts exactly what a save will write', () => {
    const p = plan()
    // 1 record; updates: care team 1 + reminders 1 + done 1 + pet 1 + regimen 1 (existing clinic adds nothing)
    expect(planCounts(p)).toEqual({ records: 1, updates: 6 })
    expect(saveLabel(p)).toBe('Save 1 record + 6 updates')
    expect(hasWork(p)).toBe(true)
    const newClinic = plan({ clinic: { ...p.clinic!, contact_id: null } })
    expect(planCounts(newClinic).updates).toBe(7)
    const none = plan({ records: [], care_team: [], clinic_links: [], reminders: [], completed_reminders: [], pet_updates: [], regimens: [], clinic: null })
    expect(hasWork(none)).toBe(false)
    expect(saveLabel(none)).toBe('Nothing selected')
    const onlyRecords = plan({ care_team: [], clinic_links: [], reminders: [], completed_reminders: [], pet_updates: [], regimens: [] })
    expect(saveLabel(onlyRecords)).toBe('Save 1 record')
  })

  it('blocks a save until every selected record has a pet', () => {
    const p = plan()
    expect(recordsNeedingPet(p)).toBe(0)
    p.records[0].pet_id = null
    expect(recordsNeedingPet(p)).toBe(1)
    p.records[0].selected = false
    expect(recordsNeedingPet(p)).toBe(0)
  })

  it('summarises a commit and labels things in plain words', () => {
    expect(resultSummary(RESULT)).toBe('Saved 1 record, 1 care-team link, 2 visits linked to the clinic, 2 weights, 2 reminders, 1 reminder marked done, 1 profile update, 1 care item, 2 insurance claims, 1 policy, 1 old policy ended.')
    expect(resultSummary({ ...RESULT, records: [], care_team_links: 0, clinic_links: 0, weights_added: 0, weights_corrected: 0, reminders_added: 0, reminders_completed: 0, pet_updates: 0, regimens_added: 0, claims_recorded: 0, policies_added: 0, policies_ended: 0, contact: { id: 'c' } as never })).toBe('Saved a new contact.')
    expect(resultSummary({ ...RESULT, records: [], care_team_links: 0, clinic_links: 0, weights_added: 0, weights_corrected: 0, reminders_added: 0, reminders_completed: 0, pet_updates: 0, regimens_added: 0, claims_recorded: 0, policies_added: 0, policies_ended: 0 })).toBe('Nothing to save.')
    expect(petFieldLabel('birth_date')).toBe('Birthday')
    expect(petFieldLabel('odd')).toBe('odd')
    // #72: a proposed sex reads the way the profile will show it.
    expect(petFieldValue('sex', 'female_spayed', 'Dog')).toBe('Spayed female')
    expect(petFieldValue('sex', 'male_neutered', 'Horse')).toBe('Gelding')
    expect(petFieldValue('sex', 'male', 'Cat', true)).toBe('Tom')
    expect(petFieldValue('breed', 'Mix', 'Dog')).toBe('Mix')
    expect(petFieldValue('sex', null, 'Dog')).toBe('')
    expect(petName(PETS as never, 'lulu')).toBe('Lulu')
    expect(petName(PETS as never, null)).toBe('Unassigned')
    expect(petName(PETS as never, 'ghost')).toBe('ghost')
    expect(shortDate('2026-09-04')).toMatch(/2026/)
    expect(shortDate('')).toBe('')
    expect(shortDate('soon')).toBe('soon')
  })

  it('runs work with bounded concurrency and finishes everything', async () => {
    let inFlight = 0
    let peak = 0
    const done: number[] = []
    await runLimited([1, 2, 3, 4, 5], 2, async (n) => {
      inFlight++; peak = Math.max(peak, inFlight)
      await new Promise((r) => setTimeout(r, 1))
      inFlight--; done.push(n)
    })
    expect(peak).toBe(2)
    expect(done.sort()).toEqual([1, 2, 3, 4, 5])
    await runLimited([], 2, async () => { throw new Error('never') })
  })
})

// ---------- the file card ----------

function entry(over = {}) {
  return { id: 1, name: 'AVIEE553AD0.PDF', media_type: 'application/pdf', size: 8100, data: 'eA==', status: 'ready', plan: plan(), result: null, error: '', ...over }
}

describe('ImportFileCard', () => {
  it('words a proposed sex the way the profile shows it, with the reason (#72)', () => {
    const p = plan({ pet_updates: [
      { selected: true, pet_id: 'lulu', field: 'sex', current: 'female', value: 'female_spayed', reason: 'Spay performed 9/21' },
      { selected: false, pet_id: 'koa', field: 'sex', current: null, value: 'male_neutered', reason: null },
    ] })
    const w = mount(ImportFileCard, { props: { entry: entry({ plan: p }), pets: PETS }, ...opts })
    const rows = w.findAll('[data-testid="import-petupdate"]')
    expect(rows[0].text()).toContain('Sex: Female → Spayed female')
    expect(rows[0].text()).toContain('Spay performed 9/21')
    expect(rows[0].find('s').text()).toBe('Female')
    expect((rows[0].find('input').element as HTMLInputElement).checked).toBe(true)
    expect(rows[1].text()).toContain('Koa · Sex: Gelding')
  })

  it('shows the plan, its proposals and an honest save label', () => {
    const w = mount(ImportFileCard, { props: { entry: entry(), pets: PETS }, ...opts })
    expect(w.text()).toContain('An invoice for Lulu.')
    expect(w.text()).toContain('AMC of Springfield')
    expect(w.findAll('[data-testid="import-record"]').length).toBe(1)
    expect(w.find('[data-testid="import-careteam"]').text()).toContain('Lulu’s care team')
    const weights = w.findAll('[data-testid="import-weight"]')
    expect(weights[0].text()).toContain('Correct Lulu’s weight')
    expect(weights[1].text()).toContain('doesn’t say when')
    expect(w.findAll('[data-testid="import-reminder"]')[1].text()).toContain('already on the calendar')
    expect(w.findAll('[data-testid="import-reminder"]')[0].text()).toContain('already has “Annual exam” that day')
    expect(w.find('[data-testid="import-done"]').text()).toContain('SNAP 4Dx')
    expect(w.findAll('[data-testid="import-petupdate"]')[1].text()).toContain('Sir Whiskers')
    expect(w.findAll('[data-testid="import-regimen"]')[0].text()).toContain('Flea')
    expect(w.findAll('[data-testid="import-regimen"]')[1].text()).toContain('already listed')
    expect(w.find('.doublecheck').text()).toContain('weight date')
    expect(w.find('[data-testid="import-save"]').text()).toBe('Save 1 record + 6 updates')
    expect(w.find('[data-testid="import-clinic-link"]').text()).toContain('“Recheck exam”')
    expect(w.text()).toContain('Koa (archived)')
  })

  it('blocks saving until an unassigned record gets a pet, then emits save', async () => {
    const e = entry()
    e.plan.records[0].pet_id = null
    const w = mount(ImportFileCard, { props: { entry: e, pets: PETS }, ...opts })
    expect(w.find('[data-testid="import-need-pet"]').exists()).toBe(true)
    expect(w.find('[data-testid="import-save"]').attributes('disabled')).toBeDefined()
    await w.find('[data-testid="import-record-pet"]').setValue('lulu')
    expect(w.find('[data-testid="import-need-pet"]').exists()).toBe(false)
    await w.find('[data-testid="import-save"]').trigger('click')
    expect(w.emitted('save')).toHaveLength(1)
    // Toggling a record off updates the label in place.
    await w.find('[data-testid="import-record-toggle"]').setValue(false)
    expect(w.find('[data-testid="import-save"]').text()).toBe('Save 6 updates')
    await w.find('[data-testid="import-remove"]').trigger('click')
    expect(w.emitted('remove')).toHaveLength(1)
  })

  it('explains a file that is already in FurFiles and a new clinic', () => {
    const p = plan({
      file_duplicates: [{ attachment_id: 'a', record_id: 11, pet_id: 'lulu', pet_name: 'Lulu', title: 'Recheck exam', occurred_on: '2026-09-11', filename: 'x.pdf' }],
      clinic: { selected: true, contact_id: null, matched_name: null, name: 'Oak Hollow Animal Clinic', role: 'vet', phone: null, email: null, address: null, website: null },
    })
    p.records[0].selected = false
    p.records[0].duplicate_of = { record_id: 11, title: 'Recheck exam', occurred_on: '2026-09-11', reason: 'Same pet, date and total' }
    p.records[0].pet_name_on_document = 'Sir Whiskers'
    const w = mount(ImportFileCard, { props: { entry: entry({ plan: p }), pets: PETS }, ...opts })
    expect(w.find('[data-testid="import-file-dup"]').text()).toContain('Lulu · Recheck exam')
    expect(w.find('[data-testid="import-record-dup"]').text()).toContain('Same pet, date and total')
    expect(w.find('[data-testid="import-clinic-new"]').exists()).toBe(true)
    expect(w.text()).toContain('“Sir Whiskers” on the document')
  })

  it('shows insurance claims and policies, and blocks a claim with no pet', async () => {
    const p = plan({
      records: [], care_team: [], clinic_links: [], weights: [], reminders: [], completed_reminders: [], pet_updates: [], regimens: [],
      clinic: null,
      claims: [
        { selected: true, pet_id: 'lulu', claim_number: 'CLM952', policy_number: 'POL04', insurer: 'Lemonade', treatment_date: '2026-09-04', items: [], paid_cents: 17712, record_id: 12, record_title: 'Comprehensive exam', visit_options: [{ record_id: 12, title: 'Comprehensive exam', cost_cents: 50603 }, { record_id: 14, title: 'Nail trim', cost_cents: 2000 }], already_recorded: false },
        { selected: true, pet_id: null, claim_number: 'CLM260', policy_number: null, insurer: null, treatment_date: '2026-03-05', items: [], paid_cents: 17200, record_id: null, record_title: null, visit_options: [], already_recorded: false },
        { selected: false, pet_id: 'lulu', claim_number: 'CLM818', policy_number: null, insurer: 'Lemonade', treatment_date: '2026-02-12', items: [], paid_cents: 5000, record_id: 3, record_title: 'Wellness exam', visit_options: [], already_recorded: true },
      ],
      policies: [
        { selected: true, pet_id: 'lulu', kind: 'insurance', provider: 'Lemonade', plan_name: null, policy_number: 'POL78', start_date: '2026-09-14', end_date: '2027-09-14', premium_cents: 75033, premium_interval: 'year', reimburse_percent: 90, deductible_cents: 50000, annual_limit_cents: null, notes: null, coverages: [], exclusions: [], existing_id: null },
        { selected: false, pet_id: 'lulu', kind: 'wellness', provider: 'Oak Hollow', plan_name: null, policy_number: null, start_date: null, end_date: null, premium_cents: null, premium_interval: null, reimburse_percent: null, deductible_cents: null, annual_limit_cents: null, notes: null, coverages: [], exclusions: [], existing_id: 'w1' },
      ],
      policy_ends: [{ selected: true, policy_id: 'old', pet_id: 'lulu', provider: 'Lemonade', policy_number: 'POL04', end_date: '2026-09-14', replaced_by: 0, seen_end_date: '2027-02-10' }],
    })
    const w = mount(ImportFileCard, { props: { entry: entry({ plan: p }), pets: PETS }, ...opts })
    const claims = w.findAll('[data-testid="import-claim"]')
    // Several visits that day: a visit picker, defaulting to the best guess.
    const visit = claims[0].find('[data-testid="import-claim-visit"]')
    expect((visit.element as HTMLSelectElement).value).toBe('12')
    await visit.setValue(14)
    expect(p.claims[0].record_title).toBe('Nail trim')
    expect(claims[1].text()).toContain('saved as a visit')
    expect(claims[2].text()).toContain('already recorded')
    // A real space between provider and number (Vue drops a whitespace-only text
    // node before an interpolation; the screenshot showed "LemonadePOL78").
    expect(w.findAll('[data-testid="import-policy"]')[0].text()).toContain('Lemonade POL78')
    expect(w.find('[data-testid="import-policy-end"]').text()).toContain('Lemonade POL04')
    // Policies are re-assignable too.
    expect(w.findAll('[data-testid="import-policy-pet"]').length).toBe(2)
    expect(w.findAll('[data-testid="import-policy"]')[1].text()).toContain('wellness plan')
    expect(w.find('[data-testid="import-policy-end"]').text()).toContain('POL04')
    // The claim without a pet blocks the save until one is picked.
    expect(w.find('[data-testid="import-need-pet"]').exists()).toBe(true)
    const pickers = w.findAll('[data-testid="import-claim-pet"]')
    await pickers[1].setValue('kipper')
    expect(w.find('[data-testid="import-need-pet"]').exists()).toBe(false)
    expect(w.find('[data-testid="import-save"]').text()).toBe('Save 4 updates')
    // Re-assigning a matched claim to another pet drops its matched visit.
    await pickers[0].setValue('pedro')
    expect(claims[0].text()).toContain('saved as a visit')
    // Re-assigning the new policy to Pedro hides Lulu's end; back to Lulu re-ticks it.
    await w.findAll('[data-testid="import-policy-pet"]')[0].setValue('pedro')
    expect(w.find('[data-testid="import-policy-end"]').exists()).toBe(false)
    await w.findAll('[data-testid="import-policy-pet"]')[0].setValue('lulu')
    expect((w.find('[data-testid="import-policy-end"] input').element as HTMLInputElement).checked).toBe(true)
    // Unticking the new policy disables (and stops counting) the old policy's end.
    await w.findAll('[data-testid="import-policy"]')[0].find('input').setValue(false)
    expect(w.find('[data-testid="import-policy-end"] input').attributes('disabled')).toBeDefined()
    expect(w.find('[data-testid="import-save"]').text()).toBe('Save 2 updates')
  })

  it('renders each lifecycle state', async () => {
    const states: [string, object, string][] = [
      ['queued', {}, 'Waiting'],
      ['converting', {}, 'Converting the iPhone photo'],
      ['analyzing', {}, 'Reading the document'],
      ['unsupported', { error: 'HEIC nope' }, 'HEIC nope'],
      ['error', { error: 'boom' }, 'boom'],
      ['saved', { result: RESULT }, 'Saved 1 record'],
      ['saving', {}, 'Saving…'],
    ]
    for (const [status, extra, text] of states) {
      const w = mount(ImportFileCard, { props: { entry: entry({ status, ...extra }), pets: PETS }, ...opts })
      expect(w.text()).toContain(text)
      if (status === 'error') {
        await w.find('[data-testid="import-retry"]').trigger('click')
        expect(w.emitted('retry')).toHaveLength(1)
      }
      if (status === 'saved') expect(w.find('[data-testid="import-saved"] a').text()).toBe('See Lulu')
      w.unmount()
    }
  })
})

// ---------- the page ----------

// FileReader completes on a macrotask in jsdom, so microtask flushing alone
// never sees the file's bytes; let a few macrotasks run too.
async function settle() {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r))
    await flushPromises()
  }
}

function fileInput(w: ReturnType<typeof mount>, files: File[]) {
  const input = w.find('[data-testid="import-input"]')
  Object.defineProperty(input.element, 'files', { value: files, configurable: true })
  return input.trigger('change')
}

describe('Import page', () => {
  beforeEach(() => {
    vi.mocked(api.pets).mockReset().mockResolvedValue(PETS as never)
    vi.mocked(api.ingestion).mockReset().mockResolvedValue({ enabled: true, has_key: true } as never)
    vi.mocked(api.importAnalyze).mockReset()
    vi.mocked(api.importCommit).mockReset()
  })

  it('tells you to set up AI first when it is off', async () => {
    vi.mocked(api.ingestion).mockResolvedValue({ enabled: false, has_key: false } as never)
    const w = mount(Import, opts)
    await flushPromises()
    expect(w.find('[data-testid="import-setup"]').exists()).toBe(true)
    expect(w.find('[data-testid="import-drop"]').exists()).toBe(false)
  })

  it('treats a failed settings check as not set up', async () => {
    vi.mocked(api.ingestion).mockRejectedValue(new Error('down'))
    vi.mocked(api.pets).mockRejectedValue(new Error('down'))
    const w = mount(Import, opts)
    await flushPromises()
    expect(w.find('[data-testid="import-setup"]').exists()).toBe(true)
  })

  it('reads dropped files, refuses unsupported ones, saves, and can save all', async () => {
    vi.mocked(api.importAnalyze).mockImplementation(async (d) => {
      if (d.filename === 'bad.pdf') throw new Error('the model declined to read this document')
      return plan()
    })
    vi.mocked(api.importCommit).mockResolvedValue(RESULT)
    const w = mount(Import, opts)
    await flushPromises()
    await fileInput(w, [
      new File(['%PDF'], 'a.pdf', { type: 'application/pdf' }),
      new File(['%PDF'], 'b.PDF', { type: '' }),
      new File(['x'], 'notes.txt', { type: 'text/plain' }),
      new File(['%PDF'], 'bad.pdf', { type: 'application/pdf' }),
    ])
    await settle()
    const cards = w.findAll('[data-testid="import-file"]')
    expect(cards.map((c) => c.attributes('data-status'))).toEqual(['ready', 'ready', 'unsupported', 'error'])
    expect(api.importAnalyze).toHaveBeenCalledTimes(3)
    expect(vi.mocked(api.importAnalyze).mock.calls[1][0].media_type).toBe('application/pdf')
    expect(w.text()).toContain('declined')

    // Save one file on its own…
    await cards[0].find('[data-testid="import-save"]').trigger('click')
    await flushPromises()
    expect(api.importCommit).toHaveBeenCalledTimes(1)
    expect(vi.mocked(api.importCommit).mock.calls[0][0].filename).toBe('a.pdf')
    expect(w.findAll('[data-testid="import-file"]')[0].attributes('data-status')).toBe('saved')

    // …retry the failure, then save everything that's ready in one go.
    vi.mocked(api.importAnalyze).mockResolvedValue(plan())
    await w.findAll('[data-testid="import-retry"]')[0].trigger('click')
    await flushPromises()
    await w.find('[data-testid="import-save-all"]').trigger('click')
    await flushPromises()
    expect(api.importCommit).toHaveBeenCalledTimes(3)
    expect(w.findAll('[data-testid="import-file"]').filter((c) => c.attributes('data-status') === 'saved').length).toBe(3)

    // Removing a card drops it.
    await w.findAll('[data-testid="import-remove"]')[0].trigger('click')
    expect(w.findAll('[data-testid="import-file"]').length).toBe(3)
  })

  it('keeps a file reviewable when its save fails, and accepts a drag-and-drop', async () => {
    vi.mocked(api.importAnalyze).mockResolvedValue(plan())
    vi.mocked(api.importCommit).mockRejectedValue(new Error('choose which pet it belongs to'))
    const w = mount(Import, opts)
    await flushPromises()
    const drop = w.find('[data-testid="import-drop"]')
    await drop.trigger('dragover')
    expect(drop.classes()).toContain('over')
    await drop.trigger('dragleave')
    await drop.trigger('drop', { dataTransfer: { files: [new File(['%PDF'], 'x.pdf', { type: 'application/pdf' })] } })
    await settle()
    await w.find('[data-testid="import-save"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="import-file"]').attributes('data-status')).toBe('ready')
    expect(w.text()).toContain('choose which pet')
  })
})

describe('HEIC document photos (#71)', () => {
  beforeEach(() => {
    vi.mocked(api.pets).mockReset().mockResolvedValue(PETS as never)
    vi.mocked(api.ingestion).mockReset().mockResolvedValue({ enabled: true, has_key: true } as never)
    vi.mocked(api.importAnalyze).mockReset().mockResolvedValue(plan())
    vi.mocked(api.importCommit).mockReset().mockResolvedValue(RESULT)
    vi.mocked(heic2any).mockClear()
  })

  it('converts a HEIC the browser cannot decode, and analyzes and commits the same JPEG bytes', async () => {
    // jsdom's Image never loads (no onload/onerror), so native decode of a
    // non-JPEG blob is made to fail the way Chromium fails on a real HEIC.
    let nativeFails = 1 // the HEIC itself fails to decode; the converted JPEG reads back fine
    vi.stubGlobal('Image', class {
      naturalWidth = 800
      naturalHeight = 600
      set src(_v: string) { setTimeout(() => (nativeFails-- > 0 ? (this as any).onerror?.() : (this as any).onload?.())) }
    })
    const w = mount(Import, opts)
    await flushPromises()
    await fileInput(w, [new File(['not a jpeg'], 'IMG_0042.HEIC', { type: 'image/heic' })])
    await settle()
    vi.unstubAllGlobals()
    expect(heic2any).toHaveBeenCalledTimes(1)
    const card = w.find('[data-testid="import-file"]')
    expect(card.attributes('data-status')).toBe('ready')
    expect(card.text()).toContain('IMG_0042.jpg')
    const sent = vi.mocked(api.importAnalyze).mock.calls[0][0]
    expect(sent).toMatchObject({ filename: 'IMG_0042.jpg', media_type: 'image/jpeg' })
    await card.find('[data-testid="import-save"]').trigger('click')
    await flushPromises()
    // Same bytes both times: the server's SHA-256 duplicate check sees one file.
    expect(vi.mocked(api.importCommit).mock.calls[0][0]).toMatchObject({
      filename: 'IMG_0042.jpg', media_type: 'image/jpeg', data_base64: sent.data_base64,
    })
  })

  it('keeps a JPEG that merely carries a .heic name, without touching libheif', async () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 4, 5, 6, 7, 8])
    const w = mount(Import, opts)
    await flushPromises()
    await fileInput(w, [new File([jpeg], 'IMG_1.heic', { type: 'image/heic' })])
    await settle()
    expect(heic2any).not.toHaveBeenCalled()
    expect(vi.mocked(api.importAnalyze).mock.calls[0][0]).toMatchObject({ filename: 'IMG_1.jpg', media_type: 'image/jpeg' })
  })

  it('converts a HEIC by its extension when the picker typed it as octet-stream', async () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 4, 5, 6, 7, 8])
    const w = mount(Import, opts)
    await flushPromises()
    await fileInput(w, [new File([jpeg], 'IMG_2.HEIC', { type: 'application/octet-stream' })])
    await settle()
    expect(vi.mocked(api.importAnalyze).mock.calls[0][0]).toMatchObject({ filename: 'IMG_2.jpg', media_type: 'image/jpeg' })
  })

  it('never reads a file that was removed before its turn came', async () => {
    const pdf = (n: string) => new File(['%PDF'], n, { type: 'application/pdf' })
    let release!: () => void
    vi.mocked(api.importAnalyze).mockImplementation(() => new Promise((res) => { release = () => res(plan()) }))
    const w = mount(Import, opts)
    await flushPromises()
    // two lanes are busy with a.pdf and b.pdf; c.pdf waits its turn
    await fileInput(w, [pdf('a.pdf'), pdf('b.pdf'), pdf('c.pdf')])
    await settle()
    const cards = w.findAll('[data-testid="import-file"]')
    expect(cards.map((c) => c.attributes('data-status'))).toEqual(['analyzing', 'analyzing', 'queued'])
    await w.findAll('[data-testid="import-remove"]')[2].trigger('click')
    release()
    await settle()
    release()
    await settle()
    expect(vi.mocked(api.importAnalyze).mock.calls.map((c) => c[0].filename)).toEqual(['a.pdf', 'b.pdf'])
  })

  it('says plainly when neither decoder can open it, with no retry offered', async () => {
    vi.stubGlobal('Image', class { set src(_v: string) { setTimeout(() => (this as any).onerror?.()) } })
    vi.mocked(heic2any).mockRejectedValueOnce(new Error('libheif boom'))
    const w = mount(Import, opts)
    await flushPromises()
    await fileInput(w, [new File(['junk'], 'broken.heic', { type: 'image/heic' })])
    await settle()
    vi.unstubAllGlobals()
    expect(w.find('[data-testid="import-file"]').attributes('data-status')).toBe('error')
    expect(w.find('[data-testid="import-error"]').text()).toContain("couldn't be opened")
    expect(w.find('[data-testid="import-retry"]').exists()).toBe(false)
    expect(api.importAnalyze).not.toHaveBeenCalled()
  })
})
