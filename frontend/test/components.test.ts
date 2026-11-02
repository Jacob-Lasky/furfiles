import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'

vi.mock('../src/api', () => ({
  api: {
    health: vi.fn(), users: vi.fn(), pets: vi.fn(), pet: vi.fn(),
    createPet: vi.fn(), updatePet: vi.fn(), archivePet: vi.fn(), unarchivePet: vi.fn(), deletePet: vi.fn(),
    records: vi.fn(), addRecord: vi.fn(), updateRecord: vi.fn(), deleteRecord: vi.fn(),
    weights: vi.fn(), addWeight: vi.fn(), costs: vi.fn(),
    // #4: ingestion + attachments
    ingestion: vi.fn(), saveIngestion: vi.fn(), clearIngestion: vi.fn(), extract: vi.fn(),
    ingestionModels: vi.fn(), // #67 model picker
    recordAttachments: vi.fn(), addAttachment: vi.fn(), deleteAttachment: vi.fn(),
    attachmentUrl: vi.fn((id) => `/api/attachments/${id}`),
    // #21: pet portrait photos
    setPetPhoto: vi.fn(), deletePetPhoto: vi.fn(),
    petPhotoUrl: vi.fn((pet) => (pet && pet.photo_path ? `/api/pets/${pet.id}/photo?v=${pet.photo_path}` : null)),
    // #41: diagnostics capture
    reportDiag: vi.fn(),
    // #26: reminders / calendar events
    reminders: vi.fn(), allReminders: vi.fn(), addReminder: vi.fn(),
    updateReminder: vi.fn(), deleteReminder: vi.fn(),
    // #20: shared contacts + per-pet care team
    contacts: vi.fn(() => Promise.resolve([])), contact: vi.fn(),
    createContact: vi.fn(), updateContact: vi.fn(), deleteContact: vi.fn(),
    petContacts: vi.fn(), linkPetContact: vi.fn(), unlinkPetContact: vi.fn(),
    parseContactUrl: vi.fn(), // #42 facility web parser
    // #8/#29: insurance & wellness plans
    petInsurance: vi.fn(), createInsurance: vi.fn(), insurance: vi.fn(),
    updateInsurance: vi.fn(), deleteInsurance: vi.fn(),
    setInsuranceDocument: vi.fn(), deleteInsuranceDocument: vi.fn(),
    extractInsurance: vi.fn(),
    insuranceValue: vi.fn(), petInsuranceValue: vi.fn(), householdInsuranceValue: vi.fn(), // #74
    insuranceDocumentUrl: vi.fn((p) => (p && p.document_name ? `/api/insurance/${p.id}/document?v=${p.updated_at}` : null)),
    // #63: ongoing care (food, preventatives, medications, supplements)
    petRegimens: vi.fn(), createRegimen: vi.fn(), regimen: vi.fn(),
    updateRegimen: vi.fn(), deleteRegimen: vi.fn(),
    // #72: household display preferences
    displaySettings: vi.fn(), saveDisplaySettings: vi.fn(),
  },
}))
import { api } from '../src/api'
// heic2any is lazy-imported by the photo editor; mock it so HEIC tests don't
// load the real libheif wasm. Returns the blob as-is (stands in for the JPEG).
vi.mock('heic2any', () => ({ default: vi.fn(async ({ blob }) => blob) }))
import heic2any from 'heic2any'
import { nextTick } from 'vue'
import { bumpData, display, setDisplay, loadDisplay, pageActions } from '../src/store'
import PetCard from '../src/components/PetCard.vue'
import SpeciesMark from '../src/components/SpeciesMark.vue'
import AddRecordSheet from '../src/components/AddRecordSheet.vue'
import RecordSheet from '../src/components/RecordSheet.vue'
import ReminderSheet from '../src/components/ReminderSheet.vue'
import AddPetSheet from '../src/components/AddPetSheet.vue'
import PhotoEditorSheet from '../src/components/PhotoEditorSheet.vue'
import ContactSheet from '../src/components/ContactSheet.vue'
import InsuranceSheet from '../src/components/InsuranceSheet.vue'
import Home from '../src/views/Home.vue'
import Pet from '../src/views/Pet.vue'
import PetSheet from '../src/components/PetSheet.vue'
import RegimenSheet from '../src/components/RegimenSheet.vue'
import ArchiveSheet from '../src/components/ArchiveSheet.vue'
import SexSuggestionChip from '../src/components/SexSuggestionChip.vue'
import Costs from '../src/views/Costs.vue'
import Calendar from '../src/views/Calendar.vue'
import Contacts from '../src/views/Contacts.vue'
import Settings from '../src/views/Settings.vue'

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/', component: { template: '<div/>' } },
    { path: '/pets/:id', component: { template: '<div/>' } },
    { path: '/settings', component: { template: '<div/>' } },
    { path: '/import', component: { template: '<div/>' } },
  ],
})
const opts = { global: { plugins: [router] } }

// The + speed-dial (#77) lives in the App shell; a view only registers its own
// actions. Run one the way a tap on its pill would.
function dialKeys(): string[] { return pageActions.value.map((a) => a.key) }
async function runDial(key: string) {
  const a = pageActions.value.find((x) => x.key === key)
  if (!a) throw new Error(`no dial action "${key}" (have: ${dialKeys().join(', ')})`)
  a.run()
  await nextTick()
}

const BISCUIT = {
  id: 'biscuit', name: 'Biscuit', species: 'Dog', breed: 'Golden Retriever',
  birth_date: '2022-03-14', weight: 32.4, weight_unit: 'kg', next_reminder: null,
  insurer: 'Trupanion', insurer_reimburse: '90%', archived_at: null,
}
const REMINDER = { id: 5, pet_id: 'biscuit', due_on: '2099-01-01', title: 'Rabies booster', kind: 'vaccine', done: false, source: 'seed', created_at: '' }

// Unmount wrappers between tests so components watching shared store state
// (Pet ↔ dataVersion) from earlier tests don't react to a later bumpData().
enableAutoUnmount(afterEach)
beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset && fn.mockReset()
  // attachmentUrl / petPhotoUrl are pure URL builders, not data calls — restore
  // their impls after the blanket reset so templates render real hrefs/srcs.
  api.attachmentUrl.mockImplementation((id) => `/api/attachments/${id}`)
  api.petPhotoUrl.mockImplementation((pet) => (pet && pet.photo_path ? `/api/pets/${pet.id}/photo?v=${pet.photo_path}` : null))
  api.insuranceDocumentUrl.mockImplementation((p) => (p && p.document_name ? `/api/insurance/${p.id}/document?v=${p.updated_at}` : null))
})

describe('PetCard', () => {
  it('shows the pet identity and the next reminder (#35)', () => {
    const w = mount(PetCard, { props: { pet: { ...BISCUIT, next_reminder: REMINDER } }, ...opts })
    expect(w.text()).toContain('Biscuit')
    expect(w.text()).toContain('Dog')
    // #12: durable probe anchors
    expect(w.find('[data-testid="pet-card"]').exists()).toBe(true)
    const chip = w.find('[data-testid="pet-card-next"]')
    expect(chip.text()).toContain('Rabies booster')
    expect(chip.text()).toContain('due') // neutral relative phrase, not a grade
  })
  it('reads "all caught up" when nothing is due (#35)', () => {
    const w = mount(PetCard, { props: { pet: BISCUIT }, ...opts })
    expect(w.find('[data-testid="pet-card-next"]').text()).toContain('All caught up')
  })
  it('draws the species-matched portrait track', () => {
    const w = mount(PetCard, { props: { pet: BISCUIT }, ...opts })
    expect(w.find('.portrait [data-track]').attributes('data-track')).toBe('dog')
    const horse = mount(PetCard, { props: { pet: { ...BISCUIT, species: 'Horse' } }, ...opts })
    expect(horse.find('.portrait [data-track]').attributes('data-track')).toBe('horse')
  })
  it('swaps the gradient placeholder for a real photo when one is set (#21)', () => {
    const noPhoto = mount(PetCard, { props: { pet: BISCUIT }, ...opts })
    expect(noPhoto.find('.pphoto').exists()).toBe(false)
    expect(noPhoto.find('.portrait [data-track]').exists()).toBe(true) // placeholder track
    const withPhoto = mount(PetCard, { props: { pet: { ...BISCUIT, photo_path: 'photos/biscuit/x.jpg' } }, ...opts })
    expect(withPhoto.find('img.pphoto').attributes('src')).toBe('/api/pets/biscuit/photo?v=photos/biscuit/x.jpg')
    expect(withPhoto.find('.portrait [data-track]').exists()).toBe(false) // placeholder gone
    expect(withPhoto.find('.portrait .init').exists()).toBe(false)
  })
})

describe('SpeciesMark', () => {
  it('renders the matching track for each supported species', () => {
    for (const [species, track] of [
      ['Dog', 'dog'], ['Cat', 'cat'], ['Horse', 'horse'], ['Bird', 'bird'], ['Fish', 'fish'],
    ]) {
      const w = mount(SpeciesMark, { props: { species } })
      expect(w.find('[data-track]').attributes('data-track')).toBe(track)
    }
  })
  it('falls back to the paw for unknown or missing species', () => {
    expect(mount(SpeciesMark, { props: { species: 'Rabbit' } }).find('[data-track]').attributes('data-track')).toBe('paw')
    expect(mount(SpeciesMark, {}).find('[data-track]').attributes('data-track')).toBe('paw')
  })
  it('forwards the paw class through to the rendered root for styling', () => {
    const known = mount(SpeciesMark, { props: { species: 'Cat' }, attrs: { class: 'paw' } })
    expect(known.find('svg').classes()).toContain('paw')
    const fallback = mount(SpeciesMark, { props: { species: 'Rabbit' }, attrs: { class: 'paw' } })
    expect(fallback.find('svg').classes()).toContain('paw')
  })
})

describe('AddPetSheet', () => {
  it('validates, submits, and emits created', async () => {
    api.createPet.mockResolvedValue({ id: 'nimbus' })
    const w = mount(AddPetSheet, opts)
    // empty name blocks
    await w.find('.btn.primary').trigger('click')
    expect(api.createPet).not.toHaveBeenCalled()
    expect(w.text()).toContain('Give your pet a name')
    // fill + submit
    await w.find('input').setValue('Nimbus')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.createPet).toHaveBeenCalled()
    expect(w.emitted().created[0][0]).toEqual({ id: 'nimbus' })
  })

  it('sends every optional field it collects', async () => {
    api.createPet.mockResolvedValue({ id: 'nimbus' })
    const w = mount(AddPetSheet, opts)
    await w.find('[data-testid="add-pet-name"]').setValue('Nimbus')
    await w.find('[data-testid="add-pet-species"]').setValue('Cat')
    await w.find('[data-testid="add-pet-breed"]').setValue('Ragdoll')
    await w.find('[data-testid="add-pet-sex"]').setValue('female_spayed')
    await w.find('[data-testid="add-pet-birth"]').setValue('2025-11-04')
    await w.find('[data-testid="add-pet-weight"]').setValue('4.2')
    await w.find('[data-testid="add-pet-unit"]').setValue('lb')
    await w.find('[data-testid="add-pet-save"]').trigger('click')
    await flushPromises()
    expect(api.createPet).toHaveBeenCalledWith({
      name: 'Nimbus', species: 'Cat', breed: 'Ragdoll', sex: 'female_spayed',
      birth_date: '2025-11-04', weight: 4.2, weight_unit: 'lb',
    })
  })

  it('omits a weight unit when no weight was given', async () => {
    api.createPet.mockResolvedValue({ id: 'nimbus' })
    const w = mount(AddPetSheet, opts)
    await w.find('[data-testid="add-pet-name"]').setValue('Nimbus')
    await w.find('[data-testid="add-pet-save"]').trigger('click')
    await flushPromises()
    expect(api.createPet.mock.calls[0][0]).toMatchObject({ weight: null, weight_unit: null })
  })

  it('Cancel closes without creating', async () => {
    const w = mount(AddPetSheet, opts)
    await w.find('[data-testid="add-pet-cancel"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
    expect(api.createPet).not.toHaveBeenCalled()
  })

  it('surfaces a save error', async () => {
    api.createPet.mockRejectedValue(new Error('save failed'))
    const w = mount(AddPetSheet, opts)
    await w.find('input').setValue('Nimbus')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('save failed')
  })
})

describe('AddRecordSheet', () => {
  const PETS = [{ id: 'biscuit', name: 'Biscuit' }, { id: 'miso', name: 'Miso' }]
  beforeEach(() => {
    localStorage.clear()
    api.ingestion.mockResolvedValue({ enabled: false, has_key: false, provider: 'anthropic', model: 'claude-sonnet-5' })
  })

  // Put a File onto the hidden input and fire change; the component reads it via
  // Blob.arrayBuffer()+btoa (both available in jsdom).
  async function attach(w, bytes, name, type) {
    const file = new File([new Uint8Array(bytes)], name, { type })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    // The component reads the file asynchronously (Blob→base64), so its name
    // only renders after a later macrotask. Poll for that instead of guessing a
    // single tick — a fixed one-tick wait is racy and flakes under coverage
    // instrumentation (slower execution) or CI load.
    for (let i = 0; i < 50 && !w.text().includes(name); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
  }

  it('loads pets, validates, converts cost to cents, and emits created', async () => {
    api.pets.mockResolvedValue(PETS)
    api.addRecord.mockResolvedValue({ id: 7 })
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    // defaults to the first pet
    expect(w.find('select').element.value).toBe('biscuit')
    // empty title blocks
    await w.find('.btn.primary').trigger('click')
    expect(api.addRecord).not.toHaveBeenCalled()
    expect(w.text()).toContain('Give the record a title')
    // fill title + cost, submit
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Wellness exam')
    await w.find('input[type="number"]').setValue('185.50')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.addRecord).toHaveBeenCalledWith('biscuit', expect.objectContaining({
      title: 'Wellness exam', category: 'Appointments', cost_cents: 18550, source: 'manual',
    }))
    expect(api.addAttachment).not.toHaveBeenCalled() // no files → no uploads
    expect(w.emitted().created[0][0]).toEqual({ petId: 'biscuit', record: { id: 7 } })
    // remembers the last-used pet
    expect(localStorage.getItem('furfiles-last-pet')).toBe('biscuit')
    // #12: durable probe anchors
    expect(w.find('[data-testid="add-sheet"]').exists()).toBe(true)
    expect(w.find('[data-testid="add-submit"]').exists()).toBe(true)
    expect(w.find('[data-testid="record-title-input"]').exists()).toBe(true)
  })

  it('offers linked contacts as the clinic and sends contact_id (#20)', async () => {
    api.pets.mockResolvedValue(PETS)
    api.contacts.mockResolvedValue([
      { id: 'c1', name: 'Cedar Creek', role: 'vet', doctor: null, phone: null, email: null,
        address: null, website: null, notes: null, created_by: null, created_at: '', updated_at: '' },
    ])
    api.addRecord.mockResolvedValue({ id: 8 })
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    const picker = w.find('[data-testid="record-contact-select"]')
    expect(picker.exists()).toBe(true)
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Checkup')
    await picker.setValue('c1')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.addRecord).toHaveBeenCalledWith('biscuit', expect.objectContaining({ contact_id: 'c1' }))
  })

  it('defaults the pet to the profile you are on', async () => {
    api.pets.mockResolvedValue(PETS)
    const w = mount(AddRecordSheet, { props: { defaultPetId: 'miso' }, ...opts })
    await flushPromises()
    expect(w.find('select').element.value).toBe('miso')
  })

  it('prefers the last-used pet when no profile default', async () => {
    localStorage.setItem('furfiles-last-pet', 'miso')
    api.pets.mockResolvedValue(PETS)
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    expect(w.find('select').element.value).toBe('miso')
  })

  it('blocks and disables save when there are no pets', async () => {
    api.pets.mockResolvedValue([])
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    expect(w.text()).toContain('No pets yet')
    expect(w.find('.btn.primary').attributes('disabled')).toBeDefined()
  })

  it('surfaces a save error', async () => {
    api.pets.mockResolvedValue(PETS)
    api.addRecord.mockRejectedValue(new Error('record save failed'))
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Checkup')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('record save failed')
    expect(w.emitted().created).toBeUndefined()
  })

  it('lets you add, edit, and remove custom detail rows, saving them as extra', async () => {
    api.pets.mockResolvedValue(PETS)
    api.addRecord.mockResolvedValue({ id: 3 })
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Rabies booster')
    await w.findAll('.addrow').find((b) => b.text().includes('Add a field')).trigger('click')
    await w.find('.kv-k').setValue('Rabies tag')
    await w.find('.kv-v').setValue('12345')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.addRecord).toHaveBeenCalledWith('biscuit', expect.objectContaining({
      extra: { 'Rabies tag': '12345' },
    }))
  })

  it('converts an iPhone HEIC to a JPEG before it is attached, scanned and saved (#71)', async () => {
    // native decode fails once (a real HEIC Chromium can't read), then the
    // heic2any JPEG reads back fine.
    let nativeFails = 1
    vi.stubGlobal('Image', class {
      constructor() { this.naturalWidth = 800; this.naturalHeight = 600 }
      set src(_v) { setTimeout(() => (nativeFails-- > 0 ? this.onerror?.() : this.onload?.())) }
    })
    api.pets.mockResolvedValue(PETS)
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'claude-sonnet-5' })
    api.extract.mockResolvedValue({
      provider: 'anthropic', model: 'claude-sonnet-5',
      fields: { title: 'Rabies booster', occurred_on: '2026-07-02', category: 'Vaccination', low_confidence_fields: [] },
    })
    api.addRecord.mockResolvedValue({ id: 9 })
    api.addAttachment.mockResolvedValue({ id: 'att-1' })
    heic2any.mockClear()
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await attach(w, [1, 2, 3], 'IMG_0042.HEIC', 'image/heic')
    vi.unstubAllGlobals()
    expect(heic2any).toHaveBeenCalledTimes(1)
    expect(w.find('.filepick').text()).not.toContain('Converting')
    expect(w.text()).toContain('IMG_0042.jpg') // the stored name, not the HEIC
    expect(w.find('.fthumb').element.getAttribute('src')).toContain('data:image/jpeg')
    await w.find('.scanbtn').trigger('click')
    await flushPromises()
    expect(api.extract.mock.calls[0][0]).toMatchObject({ filename: 'IMG_0042.jpg', media_type: 'image/jpeg' })
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.addAttachment.mock.calls[0][1]).toMatchObject({ filename: 'IMG_0042.jpg', media_type: 'image/jpeg' })
  })

  it('holds Save while a HEIC is still converting, so the photo is not left behind (#71)', async () => {
    let nativeFails = 1
    vi.stubGlobal('Image', class {
      constructor() { this.naturalWidth = 800; this.naturalHeight = 600 }
      set src(_v) { setTimeout(() => (nativeFails-- > 0 ? this.onerror?.() : this.onload?.())) }
    })
    let finish
    heic2any.mockImplementationOnce(() => new Promise((res) => { finish = () => res(new Blob([new Uint8Array([1])])) }))
    api.pets.mockResolvedValue(PETS)
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    const file = new File([new Uint8Array([1])], 'slow.heic', { type: 'image/heic' })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    for (let i = 0; i < 50 && !w.find('.filepick').text().includes('Converting'); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
    expect(w.find('.filepick').text()).toContain('Converting')
    expect(w.find('[data-testid="add-submit"]').attributes('disabled')).toBeDefined()
    expect(input.attributes('disabled')).toBeDefined()
    for (let i = 0; i < 50 && !finish; i++) { await new Promise((r) => setTimeout(r)); await flushPromises() }
    finish() // libheif is done; the converted JPEG now reads back and attaches
    for (let i = 0; i < 50 && w.find('.filepick').text().includes('Converting'); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
    vi.unstubAllGlobals()
    expect(w.text()).toContain('slow.jpg')
    expect(w.find('[data-testid="add-submit"]').attributes('disabled')).toBeUndefined()
  })

  it('says so, and attaches nothing, when a HEIC cannot be opened at all (#71)', async () => {
    vi.stubGlobal('Image', class { set src(_v) { setTimeout(() => this.onerror?.()) } })
    heic2any.mockRejectedValueOnce(new Error('libheif boom'))
    api.pets.mockResolvedValue(PETS)
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    const file = new File([new Uint8Array([1])], 'broken.heic', { type: 'image/heic' })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    for (let i = 0; i < 50 && !w.text().includes('Export it'); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
    vi.unstubAllGlobals()
    expect(w.text()).toContain("couldn't be opened")
    expect(w.find('.frow').exists()).toBe(false)
  })

  it('hides the extract button unless document scanning is enabled', async () => {
    api.pets.mockResolvedValue(PETS)
    const w = mount(AddRecordSheet, opts) // ingestion disabled (default)
    await flushPromises()
    await attach(w, [104, 105], 'receipt.pdf', 'application/pdf')
    expect(w.find('.scanbtn').exists()).toBe(false)
    expect(w.text()).toContain('Turn on document scanning') // points to Settings
    // the file is still attached (documents work without the LLM)
    expect(w.text()).toContain('receipt.pdf')
  })

  it('shows a failed scan\'s provider message and pre-fills nothing (#87)', async () => {
    api.pets.mockResolvedValue(PETS)
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'claude-sonnet-5-5' })
    // The exact message Jake saw before #87 was fixed, relayed by the backend's 502.
    api.extract.mockRejectedValue(new Error('400 — tool_choice: type "tool" and "any" are not supported for this model.'))
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await attach(w, [104, 105], 'invoice.pdf', 'application/pdf')
    await w.find('[data-testid="add-extract-btn"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="add-error"]').text()).toContain('not supported for this model')
    expect(w.find('[data-testid="record-title-input"]').element.value).toBe('')
    expect(w.find('[data-testid="add-extract-btn"]').attributes('disabled')).toBeUndefined() // can retry
  })

  it('extracts from a document, prefills fields, and attaches it as a scan on save', async () => {
    api.pets.mockResolvedValue(PETS)
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'claude-sonnet-5' })
    api.extract.mockResolvedValue({
      provider: 'anthropic', model: 'claude-sonnet-5',
      fields: {
        title: 'Dental cleaning', occurred_on: '2026-07-02', category: 'Appointments',
        cost: 420, clinic: 'Cedar Creek', notes: 'extraction',
        weight: { value: 46.2, unit: 'lb' },
        reminders: [
          { due_on: '2026-09-05', title: 'Fecal exam' },
          { due_on: '2026-12-22', title: 'Bordetella booster' },
        ],
        line_items: [{ description: 'Dental cleaning', amount: 320 }, { description: 'Extraction', amount: 100 }],
        extra_fields: { 'Rabies tag': '12345' }, low_confidence_fields: ['cost'],
      },
    })
    // one reminder is already on the calendar → should be flagged + not re-created
    api.reminders.mockResolvedValue([{ id: 1, due_on: '2026-12-22', title: 'Bordetella booster', done: false }])
    api.addRecord.mockResolvedValue({ id: 9 })
    api.addAttachment.mockResolvedValue({ id: 'att-1' })
    api.addWeight.mockResolvedValue({ id: 3 })
    api.addReminder.mockResolvedValue({ id: 7 })

    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await attach(w, [104, 105], 'invoice.pdf', 'application/pdf')

    // extract button appears once a scannable file is present + scanning is on
    expect(w.find('[data-testid="add-file-input"]').exists()).toBe(true)
    expect(w.find('[data-testid="add-extract-btn"]').exists()).toBe(true)
    await w.find('[data-testid="add-extract-btn"]').trigger('click')
    await flushPromises()

    // fields pre-filled from the model
    expect(w.find('[data-testid="record-title-input"]').element.value).toBe('Dental cleaning')
    expect(w.find('[data-testid="record-date-input"]').element.value).toBe('2026-07-02')
    expect(w.find('[data-testid="record-cost-input"]').element.value).toBe('420')
    expect(w.find('[data-testid="add-error"]').exists()).toBe(false)
    // clinic folded into the notes textarea
    expect(w.find('textarea').element.value).toBe('Cedar Creek — extraction')
    expect(w.text()).toContain('Double-check: cost') // low-confidence surfaced
    expect(w.find('.kv-k').element.value).toBe('Rabies tag') // extra field row added
    // cost breakdown rows populated from line_items
    const descs = w.findAll('.li-desc').map((i) => i.element.value)
    expect(descs).toEqual(['Dental cleaning', 'Extraction'])

    // #26: "also update" proposals — weight + reminders, dupe flagged
    expect(w.find('.alsoupdate').exists()).toBe(true)
    expect(w.find('.au-wval').element.value).toBe('46.2')
    expect(w.findAll('.au-rem')).toHaveLength(2)
    expect(w.findAll('.au-dup')).toHaveLength(1) // Bordetella already on calendar

    // save: record + line items + doc, PLUS weight logged and only the NEW reminder added
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.addRecord).toHaveBeenCalledWith('biscuit', expect.objectContaining({
      title: 'Dental cleaning', source: 'scan', extra: { 'Rabies tag': '12345' },
      line_items: [
        { description: 'Dental cleaning', amount_cents: 32000 },
        { description: 'Extraction', amount_cents: 10000 },
      ],
    }))
    expect(api.addAttachment).toHaveBeenCalledWith(9, expect.objectContaining({
      filename: 'invoice.pdf', media_type: 'application/pdf', source: 'scan',
    }))
    expect(api.addWeight).toHaveBeenCalledWith('biscuit', expect.objectContaining({ value: 46.2, unit: 'lb' }))
    // only the non-duplicate reminder is created
    expect(api.addReminder).toHaveBeenCalledTimes(1)
    expect(api.addReminder).toHaveBeenCalledWith('biscuit', expect.objectContaining({ title: 'Fecal exam', due_on: '2026-09-05' }))
    expect(w.emitted().created[0][0]).toEqual({ petId: 'biscuit', record: { id: 9 } })
  })

  it('auto-sums the total from a manually entered cost breakdown', async () => {
    api.pets.mockResolvedValue(PETS)
    api.addRecord.mockResolvedValue({ id: 5 })
    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Visit')
    const addItem = w.findAll('.addrow').find((b) => b.text().includes('cost item'))
    await addItem.trigger('click')
    await addItem.trigger('click')
    const descs = w.findAll('.li-desc')
    const amts = w.findAll('.li-amt')
    await descs[0].setValue('Exam'); await amts[0].setValue('50')
    await descs[1].setValue('Meds'); await amts[1].setValue('34.20')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.addRecord).toHaveBeenCalledWith('biscuit', expect.objectContaining({
      cost_cents: 8420, // 50.00 + 34.20 auto-summed because the Cost field was blank
      line_items: [
        { description: 'Exam', amount_cents: 5000 },
        { description: 'Meds', amount_cents: 3420 },
      ],
    }))
  })

  it('does not re-create the record when a later attachment upload is retried', async () => {
    api.pets.mockResolvedValue(PETS)
    api.addRecord.mockResolvedValue({ id: 11 })
    api.addAttachment
      .mockRejectedValueOnce(new Error('upload blipped'))
      .mockResolvedValueOnce({ id: 'att-2' })

    const w = mount(AddRecordSheet, opts)
    await flushPromises()
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Visit')
    await attach(w, [1, 2, 3], 'x-ray.png', 'image/png')

    await w.find('.btn.primary').trigger('click') // creates record, upload fails
    await flushPromises()
    expect(w.text()).toContain('upload blipped')
    expect(api.addRecord).toHaveBeenCalledTimes(1)
    expect(w.emitted().created).toBeUndefined()

    await w.find('.btn.primary').trigger('click') // retry
    await flushPromises()
    expect(api.addRecord).toHaveBeenCalledTimes(1) // NOT created again
    expect(api.addAttachment).toHaveBeenCalledTimes(2)
    expect(w.emitted().created[0][0]).toEqual({ petId: 'biscuit', record: { id: 11 } })
  })
})

describe('RecordSheet (#15, #44)', () => {
  const REC = {
    id: 7, pet_id: 'biscuit', occurred_on: '2026-07-02', ends_on: null,
    title: 'Dental', category: 'Appointments', cost_cents: 42000, covered_cents: null,
    notes: 'clean', extra: { 'Rabies tag': '12345' },
    line_items: [{ description: 'Exam', amount_cents: 8500 }],
    attachments: [{ id: 'att-1', filename: 'invoice.pdf', media_type: 'application/pdf', size_bytes: 20480 }],
  }

  // The sheet opens read-only (#44); the edit form is gated behind the Edit button.
  async function mountEditing(record = REC, extra = {}) {
    const w = mount(RecordSheet, { props: { petId: 'biscuit', record, ...extra }, ...opts })
    await flushPromises()
    await w.find('[data-testid="record-edit"]').trigger('click')
    return w
  }

  it('opens read-only (#44): lays out the record and renders a PDF inline', async () => {
    const w = mount(RecordSheet, { props: { petId: 'biscuit', petName: 'Biscuit', record: REC }, ...opts })
    await flushPromises()
    // the view, not the form
    expect(w.find('[data-testid="record-view"]').exists()).toBe(true)
    expect(w.find('input[placeholder="Annual wellness exam"]').exists()).toBe(false)
    // the record laid out: title, cost, notes, breakdown, custom field
    expect(w.text()).toContain('Dental')
    expect(w.text()).toContain('$420')
    expect(w.text()).toContain('clean')
    expect(w.text()).toContain('Exam')
    expect(w.text()).toContain('$85')
    expect(w.text()).toContain('Rabies tag')
    expect(w.text()).toContain('12345')
    // the PDF renders inline (iframe), pointed at the streaming endpoint
    const pdf = w.find('[data-testid="record-view-pdf"]')
    expect(pdf.exists()).toBe(true)
    expect(pdf.attributes('src')).toBe('/api/attachments/att-1')
  })

  it('renders an image attachment inline in the view (#44)', async () => {
    const rec = { ...REC, attachments: [{ id: 'att-2', filename: 'xray.png', media_type: 'image/png', size_bytes: 51200 }] }
    const w = mount(RecordSheet, { props: { petId: 'biscuit', record: rec }, ...opts })
    await flushPromises()
    expect(w.find('.rv-img').attributes('src')).toBe('/api/attachments/att-2')
    expect(w.find('[data-testid="record-view-pdf"]').exists()).toBe(false)
  })

  it('shows the clinic the record happened at in the view (#20/#44)', async () => {
    const rec = { ...REC, contact_id: 'c1', contact: { ...CEDAR } }
    const w = mount(RecordSheet, { props: { petId: 'biscuit', record: rec }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="record-view"]').text()).toContain('Cedar Creek')
  })

  it('closes from the view via Close (#44)', async () => {
    const w = mount(RecordSheet, { props: { petId: 'biscuit', record: REC }, ...opts })
    await flushPromises()
    await w.find('[data-testid="record-close"]').trigger('click')
    expect(w.emitted().close).toBeTruthy()
  })

  it('Edit flips to the form, prefilled, and saves a full overwrite', async () => {
    api.updateRecord.mockResolvedValue({ ...REC, title: 'Dental cleaning' })
    const w = await mountEditing(REC, { petName: 'Biscuit' })
    expect(w.find('input[placeholder="Annual wellness exam"]').element.value).toBe('Dental')
    expect(w.find('.kv-k').element.value).toBe('Rabies tag')
    expect(w.find('.li-desc').element.value).toBe('Exam')
    expect(w.find('input[type="number"]').element.value).toBe('420') // cost, cents → dollars
    expect(w.text()).toContain('invoice.pdf') // existing attachment shown

    await w.find('input[placeholder="Annual wellness exam"]').setValue('Dental cleaning')
    await w.find('[data-testid="record-save"]').trigger('click')
    await flushPromises()
    expect(api.updateRecord).toHaveBeenCalledWith('biscuit', 7, expect.objectContaining({
      title: 'Dental cleaning', ends_on: null, cost_cents: 42000,
      extra: { 'Rabies tag': '12345' },
      line_items: [{ description: 'Exam', amount_cents: 8500 }],
    }))
    expect(w.emitted().saved[0][0]).toEqual({ petId: 'biscuit', record: expect.objectContaining({ title: 'Dental cleaning' }) })
  })

  it('clears cost and custom fields when they are blanked (null / [] overwrite)', async () => {
    api.updateRecord.mockResolvedValue({ ...REC })
    const w = await mountEditing()
    await w.find('.lirow .fx').trigger('click') // remove the only line item
    await w.find('.kvrow .fx').trigger('click') // remove the only detail row
    await w.find('input[type="number"]').setValue('') // blank the cost
    await w.find('[data-testid="record-save"]').trigger('click')
    await flushPromises()
    expect(api.updateRecord).toHaveBeenCalledWith('biscuit', 7, expect.objectContaining({
      cost_cents: null, extra: null, line_items: [],
    }))
  })

  it('deletes after confirmation and emits deleted', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    api.deleteRecord.mockResolvedValue(null)
    const w = await mountEditing()
    await w.find('[data-testid="record-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteRecord).toHaveBeenCalledWith('biscuit', 7)
    expect(w.emitted().deleted[0][0]).toEqual({ petId: 'biscuit', id: 7 })
  })

  it('does not delete when the confirmation is dismissed', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    const w = await mountEditing()
    await w.find('[data-testid="record-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteRecord).not.toHaveBeenCalled()
  })

  it('removes an existing attachment immediately (edit mode)', async () => {
    api.deleteAttachment.mockResolvedValue(null)
    const w = await mountEditing()
    expect(w.text()).toContain('invoice.pdf')
    await w.find('.attwrap .fx').trigger('click')
    await flushPromises()
    expect(api.deleteAttachment).toHaveBeenCalledWith('att-1')
    expect(w.text()).not.toContain('invoice.pdf')
  })

  it('fetches attachments when the record did not carry them (calendar path)', async () => {
    api.recordAttachments.mockResolvedValue([{ id: 'att-9', filename: 'lab.pdf', media_type: 'application/pdf', size_bytes: 100 }])
    const w = mount(RecordSheet, { props: { petId: 'biscuit', record: { ...REC, attachments: undefined } }, ...opts })
    await flushPromises()
    expect(api.recordAttachments).toHaveBeenCalledWith(7)
    expect(w.text()).toContain('lab.pdf') // rendered inline in the view
  })

  it('surfaces a save error and does not emit', async () => {
    api.updateRecord.mockRejectedValue(new Error('save boom'))
    const w = await mountEditing()
    await w.find('[data-testid="record-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('save boom')
    expect(w.emitted().saved).toBeUndefined()
  })

  it('Cancel returns to the view without saving or deleting (#44)', async () => {
    const w = await mountEditing()
    expect(w.find('input[placeholder="Annual wellness exam"]').exists()).toBe(true) // in edit
    await w.find('[data-testid="record-cancel"]').trigger('click')
    // back to the read-only view, sheet still open
    expect(w.find('[data-testid="record-view"]').exists()).toBe(true)
    expect(w.find('input[placeholder="Annual wellness exam"]').exists()).toBe(false)
    expect(w.emitted().close).toBeUndefined()
    expect(api.updateRecord).not.toHaveBeenCalled()
    expect(api.deleteRecord).not.toHaveBeenCalled()
  })
})

describe('ReminderSheet (#16)', () => {
  const REM = { id: 5, pet_id: 'biscuit', pet_name: 'Biscuit', due_on: '2026-09-05', title: 'Rabies due', done: false }

  it('toggles done and emits changed', async () => {
    api.updateReminder.mockResolvedValue({})
    const w = mount(ReminderSheet, { props: { reminder: REM }, ...opts })
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.updateReminder).toHaveBeenCalledWith(5, { done: true })
    expect(w.emitted().changed).toBeTruthy()
  })

  it('deletes after confirmation and emits deleted', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    api.deleteReminder.mockResolvedValue(null)
    const w = mount(ReminderSheet, { props: { reminder: REM }, ...opts })
    await w.find('.btn.danger').trigger('click')
    await flushPromises()
    expect(api.deleteReminder).toHaveBeenCalledWith(5)
    expect(w.emitted().deleted).toBeTruthy()
  })

  it('does not delete when the confirmation is dismissed', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    const w = mount(ReminderSheet, { props: { reminder: REM }, ...opts })
    await w.find('.btn.danger').trigger('click')
    await flushPromises()
    expect(api.deleteReminder).not.toHaveBeenCalled()
  })

  it('surfaces an update error and does not emit', async () => {
    api.updateReminder.mockRejectedValue(new Error('rem boom'))
    const w = mount(ReminderSheet, { props: { reminder: REM }, ...opts })
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('rem boom')
    expect(w.emitted().changed).toBeUndefined()
  })

  it('links to the pet profile and closes on that click', async () => {
    const w = mount(ReminderSheet, { props: { reminder: REM }, ...opts })
    expect(w.find('.rs-link').attributes('href')).toContain('/pets/biscuit')
    await w.find('.rs-link').trigger('click')
    expect(w.emitted().close).toBeTruthy()
  })
})

describe('PhotoEditorSheet (#21)', () => {
  let getContextSpy, toDataURLSpy
  beforeEach(() => {
    // jsdom has no real 2D canvas — stub the drawing surface. Pixel correctness
    // is verified by /test-ux; here we exercise the component's flow + geometry.
    const fakeCtx = {
      clearRect: vi.fn(), save: vi.fn(), restore: vi.fn(),
      translate: vi.fn(), rotate: vi.fn(), scale: vi.fn(), drawImage: vi.fn(),
    }
    getContextSpy = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(fakeCtx)
    toDataURLSpy = vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/jpeg;base64,QUJD')
    // new Image() never fires onload in jsdom; fake it with fixed dimensions.
    vi.stubGlobal('Image', class {
      constructor() { this.naturalWidth = 800; this.naturalHeight = 600 }
      set src(_v) { if (this.onload) setTimeout(() => this.onload()) }
    })
  })
  afterEach(() => {
    getContextSpy.mockRestore()
    toDataURLSpy.mockRestore()
    vi.unstubAllGlobals()
  })
  beforeEach(() => { heic2any.mockClear() })

  async function pickImage(w, type = 'image/png', name = 'pedro.png') {
    const file = new File([new Uint8Array([1, 2, 3])], name, { type })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    for (let i = 0; i < 50 && !w.find('.cropctl').exists(); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
  }

  const base = { props: { petId: 'pedro', petName: 'Pedro' } }

  it('blocks saving until a photo is chosen', async () => {
    const w = mount(PhotoEditorSheet, base)
    expect(w.find('.btn.primary').attributes('disabled')).toBeDefined()
    // controls only appear once an image is loaded
    expect(w.find('.cropctl').exists()).toBe(false)
    expect(w.text()).toContain('Choose a photo to begin')
  })

  it('rejects a non-image file', async () => {
    const w = mount(PhotoEditorSheet, base)
    const file = new File([new Uint8Array([1])], 'invoice.pdf', { type: 'application/pdf' })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    await flushPromises()
    expect(w.text()).toContain('Pick an image file')
    expect(w.find('.cropctl').exists()).toBe(false)
  })

  it('picks, frames (rotate + zoom + drag), and saves a square jpeg', async () => {
    api.setPetPhoto.mockResolvedValue({ id: 'pedro', photo_path: 'photos/pedro/x.jpg' })
    const w = mount(PhotoEditorSheet, base)
    await pickImage(w)
    expect(w.find('.cropctl').exists()).toBe(true) // image loaded → controls show

    await w.find('.rotate').trigger('click') // rotate a quarter turn
    await w.find('.zoom').setValue(2) // zoom in
    // drag to pan — dispatch native MouseEvents so clientX/Y are set (test-utils
    // can't assign the read-only coords on a synthetic pointer event).
    const canvasEl = w.find('.cropcanvas').element
    canvasEl.dispatchEvent(new MouseEvent('pointerdown', { clientX: 10, clientY: 10, bubbles: true }))
    canvasEl.dispatchEvent(new MouseEvent('pointermove', { clientX: 40, clientY: 25, bubbles: true }))
    canvasEl.dispatchEvent(new MouseEvent('pointerup', { bubbles: true }))
    await flushPromises()

    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.setPetPhoto).toHaveBeenCalledWith('pedro', { media_type: 'image/jpeg', data_base64: 'QUJD' })
    expect(w.emitted().saved).toBeTruthy()
  })

  it('surfaces a save error and keeps the sheet open', async () => {
    api.setPetPhoto.mockRejectedValue(new Error('photo save failed'))
    const w = mount(PhotoEditorSheet, base)
    await pickImage(w)
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('photo save failed')
    expect(w.emitted().saved).toBeUndefined()
  })

  it('removes an existing photo', async () => {
    api.deletePetPhoto.mockResolvedValue({ id: 'pedro', photo_path: null })
    const w = mount(PhotoEditorSheet, { props: { petId: 'pedro', petName: 'Pedro', hasPhoto: true } })
    const remove = w.findAll('.btn.sec').find((b) => b.text().includes('Remove'))
    expect(remove).toBeTruthy()
    await remove.trigger('click')
    await flushPromises()
    expect(api.deletePetPhoto).toHaveBeenCalledWith('pedro')
    expect(w.emitted().saved).toBeTruthy()
  })

  it('surfaces a remove error and stays open', async () => {
    api.deletePetPhoto.mockRejectedValue(new Error('remove failed'))
    const w = mount(PhotoEditorSheet, { props: { petId: 'pedro', hasPhoto: true } })
    await w.findAll('.btn.sec').find((b) => b.text().includes('Remove')).trigger('click')
    await flushPromises()
    expect(w.text()).toContain('remove failed')
    expect(w.emitted().saved).toBeUndefined()
  })

  it('surfaces an image that cannot be decoded', async () => {
    // Image that fires onerror instead of onload
    vi.stubGlobal('Image', class {
      set src(_v) { if (this.onerror) setTimeout(() => this.onerror()) }
    })
    const w = mount(PhotoEditorSheet, base)
    const file = new File([new Uint8Array([1])], 'broken.png', { type: 'image/png' })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    for (let i = 0; i < 50 && !w.text().includes("couldn't be opened"); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
    expect(w.text()).toContain("couldn't be opened")
    expect(w.find('.cropctl').exists()).toBe(false)
  })

  it('degrades gracefully when the canvas has no 2D context', async () => {
    getContextSpy.mockReturnValue(null) // draw() becomes a no-op, must not throw
    const w = mount(PhotoEditorSheet, base)
    await pickImage(w)
    expect(w.find('.cropctl').exists()).toBe(true) // still usable
  })

  it('loads a phone JPEG-mislabeled-as-HEIC natively, without heic2any (#21 regression)', async () => {
    // Firefox on Android hands over a JPEG named .heic with type image/heic. The
    // native decoder reads it fine, so heic2any must NOT be invoked — it would
    // throw "already browser readable" on an actual JPEG (Jake's phone bug).
    api.setPetPhoto.mockResolvedValue({ id: 'koa', photo_path: 'photos/koa/x.jpg' })
    const w = mount(PhotoEditorSheet, { props: { petId: 'koa', petName: 'Koa' } })
    await pickImage(w, 'image/heic', 'IMG_6488.HEIC')
    expect(heic2any).not.toHaveBeenCalled()
    expect(w.find('.cropctl').exists()).toBe(true)
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.setPetPhoto).toHaveBeenCalledWith('koa', { media_type: 'image/jpeg', data_base64: 'QUJD' })
  })

  it('converts a real HEIC the browser cannot decode, via heic2any', async () => {
    // native decode fails once (a real HEIC the browser can't read), then the
    // heic2any-converted JPEG decodes.
    let nativeFails = 1
    vi.stubGlobal('Image', class {
      constructor() { this.naturalWidth = 800; this.naturalHeight = 600 }
      set src(_v) {
        if (nativeFails-- > 0) { if (this.onerror) setTimeout(() => this.onerror()) }
        else if (this.onload) setTimeout(() => this.onload())
      }
    })
    const w = mount(PhotoEditorSheet, base)
    await pickImage(w, 'image/heic', 'real.heic')
    expect(heic2any).toHaveBeenCalledTimes(1)
    expect(w.find('.cropctl').exists()).toBe(true)
  })

  it('reports the file and shows a hint when a photo cannot be opened at all', async () => {
    // native decode fails AND conversion fails → user-facing hint + capture (#41).
    vi.stubGlobal('Image', class { set src(_v) { if (this.onerror) setTimeout(() => this.onerror()) } })
    heic2any.mockRejectedValueOnce(new Error('libheif boom'))
    const w = mount(PhotoEditorSheet, base)
    const file = new File([new Uint8Array([1])], 'broken.heic', { type: 'image/heic' })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    for (let i = 0; i < 50 && !w.text().includes('exporting it as JPEG'); i++) {
      await new Promise((r) => setTimeout(r))
      await flushPromises()
    }
    expect(w.text()).toContain("couldn't be opened")
    expect(w.find('.cropctl').exists()).toBe(false)
    expect(api.reportDiag).toHaveBeenCalled()
    const [rep, sentFile] = api.reportDiag.mock.calls[0]
    expect(rep.context).toBe('photo-editor:load')
    expect(sentFile.name).toBe('broken.heic')
  })
})

describe('InsuranceSheet (#8/#29)', () => {
  const POLICY = {
    id: 'pol-1', pet_id: 'lulu', kind: 'insurance', provider: 'Lemonade',
    plan_name: 'Pet + Preventative', policy_number: 'DEMO-POL-0401', status: 'active',
    start_date: '2026-02-10', end_date: '2027-02-10',
    premium_cents: 91136, premium_interval: 'year', reimburse_percent: 90,
    deductible_cents: 50000, annual_limit_cents: 5000000, notes: 'Illness waiting period 15 days',
    coverages: [{ label: 'Wellness exam', detail: 'Up to $70/yr' }],
    exclusions: [{ label: 'Pre-existing', detail: null }],
    document_name: 'policy.pdf', document_media_type: 'application/pdf', document_size_bytes: 5,
    created_by: null, created_at: '', updated_at: '2026-07-06T00:00:00',
  }
  const base = { props: { petId: 'lulu', petName: 'Lulu' }, ...opts }
  beforeEach(() => {
    api.ingestion.mockResolvedValue({ enabled: false, has_key: false, provider: 'anthropic', model: 'claude-sonnet-5' })
  })

  async function attach(w, name = 'policy.pdf', type = 'application/pdf') {
    const file = new File([new Uint8Array([104, 105])], name, { type })
    const input = w.find('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    for (let i = 0; i < 50 && !w.text().includes(name); i++) {
      await new Promise((r) => setTimeout(r)); await flushPromises()
    }
  }

  it('opens an existing policy read-only, laying out coverage + inline PDF', async () => {
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="insurance-view"]').exists()).toBe(true)
    expect(w.find('[data-testid="insurance-provider-input"]').exists()).toBe(false) // not the form
    expect(w.text()).toContain('Lemonade')
    expect(w.text()).toContain('DEMO-POL-0401')
    expect(w.text()).toContain('90%')
    expect(w.text()).toContain('$500') // deductible
    expect(w.text()).toContain('$911.36/yr') // premium + interval
    expect(w.text()).toContain('Wellness exam')
    expect(w.text()).toContain('Pre-existing')
    const pdf = w.find('[data-testid="insurance-view-doc"] iframe')
    expect(pdf.attributes('src')).toBe('/api/insurance/pol-1/document?v=2026-07-06T00:00:00')
  })

  it('Edit flips to the form, prefilled (cents → dollars)', async () => {
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    expect(w.find('[data-testid="insurance-provider-input"]').element.value).toBe('Lemonade')
    expect(w.find('[data-testid="insurance-reimburse-input"]').element.value).toBe('90')
    expect(w.find('[data-testid="insurance-premium-input"]').element.value).toBe('911.36')
  })

  it('Cancel returns to the view for an existing policy', async () => {
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    await w.find('[data-testid="insurance-cancel"]').trigger('click')
    expect(w.find('[data-testid="insurance-view"]').exists()).toBe(true)
    expect(w.emitted().close).toBeUndefined()
  })

  it('validates provider, creates a policy, and syncs the pet insurer', async () => {
    api.createInsurance.mockResolvedValue({ ...POLICY, id: 'pol-new' })
    api.updatePet.mockResolvedValue({})
    const w = mount(InsuranceSheet, base)
    await flushPromises()
    // empty provider blocks
    await w.find('[data-testid="insurance-save"]').trigger('click')
    expect(api.createInsurance).not.toHaveBeenCalled()
    expect(w.text()).toContain('Give this policy a provider')
    // fill provider + reimburse, add a coverage row, submit
    await w.find('[data-testid="insurance-provider-input"]').setValue('Lemonade')
    await w.find('[data-testid="insurance-reimburse-input"]').setValue('90')
    await w.find('[data-testid="add-coverage"]').trigger('click')
    const cov = w.find('[data-testid="coverage-row"]')
    await cov.find('.kv-k').setValue('Wellness exam')
    await cov.find('.kv-v').setValue('Up to $70/yr')
    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(api.createInsurance).toHaveBeenCalledWith('lulu', expect.objectContaining({
      provider: 'Lemonade', kind: 'insurance', reimburse_percent: 90,
      coverages: [{ label: 'Wellness exam', detail: 'Up to $70/yr' }],
    }))
    // #26/#29: also surfaced on the pet's profile (default-on for insurance)
    expect(api.updatePet).toHaveBeenCalledWith('lulu', { insurer: 'Lemonade', insurer_reimburse: '90%' })
    expect(w.emitted().saved).toBeTruthy()
  })

  it('extracts from a document, prefills, and stores the doc + policy on save', async () => {
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'claude-sonnet-5' })
    api.extractInsurance.mockResolvedValue({
      provider: 'anthropic', model: 'claude-sonnet-5',
      fields: {
        kind: 'insurance', provider: 'Lemonade', plan_name: 'Pet + Preventative',
        policy_number: 'DEMO-POL-0401', start_date: '2026-02-10', end_date: '2027-02-10',
        premium: 911.36, premium_interval: 'year', reimburse_percent: 90,
        deductible: 500, annual_limit: 50000, notes: 'Waiting period',
        coverages: [{ label: 'Wellness exam', detail: 'Up to $70/yr' }],
        exclusions: [{ label: 'Pre-existing', detail: null }],
        low_confidence_fields: ['annual_limit'],
      },
    })
    api.createInsurance.mockResolvedValue({ ...POLICY, id: 'pol-new' })
    api.setInsuranceDocument.mockResolvedValue({ ...POLICY, id: 'pol-new' })
    api.updatePet.mockResolvedValue({})

    const w = mount(InsuranceSheet, base)
    await flushPromises()
    await attach(w, 'policy.pdf', 'application/pdf')
    expect(w.find('[data-testid="insurance-extract-btn"]').exists()).toBe(true)
    await w.find('[data-testid="insurance-extract-btn"]').trigger('click')
    await flushPromises()

    // fields pre-filled from the model
    expect(w.find('[data-testid="insurance-provider-input"]').element.value).toBe('Lemonade')
    expect(w.find('[data-testid="insurance-reimburse-input"]').element.value).toBe('90')
    expect(w.find('[data-testid="insurance-premium-input"]').element.value).toBe('911.36')
    expect(w.find('[data-testid="coverage-row"] .kv-k').element.value).toBe('Wellness exam')
    expect(w.text()).toContain('Double-check: annual_limit')

    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(api.createInsurance).toHaveBeenCalledWith('lulu', expect.objectContaining({
      provider: 'Lemonade', reimburse_percent: 90, premium_cents: 91136, deductible_cents: 50000,
    }))
    // the same file is stored as the policy document
    expect(api.setInsuranceDocument).toHaveBeenCalledWith('pol-new', expect.objectContaining({
      filename: 'policy.pdf', media_type: 'application/pdf',
    }))
    expect(api.updatePet).toHaveBeenCalledWith('lulu', { insurer: 'Lemonade', insurer_reimburse: '90%' })
  })

  it('deletes after confirmation and emits deleted', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    api.deleteInsurance.mockResolvedValue(null)
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    await w.find('[data-testid="insurance-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteInsurance).toHaveBeenCalledWith('pol-1')
    expect(w.emitted().deleted[0][0]).toBe('pol-1')
  })

  it('surfaces a save error and does not emit', async () => {
    api.createInsurance.mockRejectedValue(new Error('policy save failed'))
    const w = mount(InsuranceSheet, base)
    await flushPromises()
    await w.find('[data-testid="insurance-provider-input"]').setValue('Lemonade')
    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('policy save failed')
    expect(w.emitted().saved).toBeUndefined()
  })

  it('renders a wellness plan with an inline image document', async () => {
    const wellness = {
      ...POLICY, id: 'pol-2', kind: 'wellness', provider: 'Banfield', plan_name: 'Puppy Gold',
      status: 'expired', reimburse_percent: null, deductible_cents: null, annual_limit_cents: null,
      premium_cents: 5395, premium_interval: 'month', exclusions: null,
      coverages: [{ label: 'Exams', detail: 'Unlimited' }],
      document_name: 'plan.png', document_media_type: 'image/png',
    }
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: wellness }, ...opts })
    await flushPromises()
    expect(w.text()).toContain('Wellness plan')
    expect(w.text()).toContain('Expired')
    expect(w.text()).toContain('$53.95/mo')
    expect(w.find('.ins-badge').attributes('data-kind')).toBe('wellness')
    // image renders inline, not as a PDF iframe
    expect(w.find('[data-testid="insurance-view-doc"] .doc-img').attributes('src'))
      .toBe('/api/insurance/pol-2/document?v=2026-07-06T00:00:00')
    expect(w.find('[data-testid="insurance-view-doc"] iframe').exists()).toBe(false)
  })

  it('edits an existing policy (update, not create) and removes its document', async () => {
    api.updateInsurance.mockResolvedValue({ ...POLICY })
    api.deleteInsuranceDocument.mockResolvedValue({ ...POLICY, document_name: null })
    api.updatePet.mockResolvedValue({})
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    expect(w.text()).toContain('policy.pdf') // the attached document shown
    await w.find('[data-testid="insurance-doc-remove"]').trigger('click')
    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(api.updateInsurance).toHaveBeenCalledWith('pol-1', expect.objectContaining({ provider: 'Lemonade' }))
    expect(api.createInsurance).not.toHaveBeenCalled()
    expect(api.deleteInsuranceDocument).toHaveBeenCalledWith('pol-1')
    expect(w.emitted().saved).toBeTruthy()
  })

  it('closes on Cancel for a brand-new policy', async () => {
    const w = mount(InsuranceSheet, base)
    await flushPromises()
    await w.find('[data-testid="insurance-cancel"]').trigger('click')
    expect(w.emitted().close).toBeTruthy()
  })

  it('does not delete when the confirmation is dismissed', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    await w.find('[data-testid="insurance-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteInsurance).not.toHaveBeenCalled()
  })

  it('surfaces an extract error', async () => {
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'x' })
    api.extractInsurance.mockRejectedValue(new Error('read boom'))
    const w = mount(InsuranceSheet, base)
    await flushPromises()
    await attach(w, 'p.pdf', 'application/pdf')
    await w.find('[data-testid="insurance-extract-btn"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('read boom')
  })

  it('clears the pet\'s stored reimbursement when the policy has none', async () => {
    // Null here CLEARS (pet edits are null-vs-absent since #63), so the profile
    // can't keep a previous insurer's percentage next to a new provider.
    api.updateInsurance.mockResolvedValue({ ...POLICY })
    api.updatePet.mockResolvedValue({})
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    await w.find('[data-testid="insurance-reimburse-input"]').setValue('')
    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(api.updatePet).toHaveBeenCalledWith('lulu', { insurer: 'Lemonade', insurer_reimburse: null })
  })

  it('replaces the document by picking a new file while editing', async () => {
    api.updateInsurance.mockResolvedValue({ ...POLICY })
    api.setInsuranceDocument.mockResolvedValue({ ...POLICY })
    api.updatePet.mockResolvedValue({})
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: POLICY }, ...opts })
    await flushPromises()
    await w.find('[data-testid="insurance-edit"]').trigger('click')
    await attach(w, 'renewal.pdf', 'application/pdf')
    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(api.setInsuranceDocument).toHaveBeenCalledWith('pol-1', expect.objectContaining({ filename: 'renewal.pdf' }))
  })

  it('shows a download link for a non-previewable document', async () => {
    const p = { ...POLICY, id: 'pol-3', document_name: 'policy.docx', document_media_type: 'application/octet-stream' }
    const w = mount(InsuranceSheet, { props: { ...base.props, policy: p }, ...opts })
    await flushPromises()
    const link = w.find('[data-testid="insurance-view-doc"] .doc-link')
    expect(link.exists()).toBe(true)
    expect(link.attributes('href')).toBe('/api/insurance/pol-3/document?v=2026-07-06T00:00:00')
  })

  it('can opt out of syncing the policy to the pet profile', async () => {
    api.createInsurance.mockResolvedValue({ ...POLICY, id: 'pol-new' })
    const w = mount(InsuranceSheet, base)
    await flushPromises()
    await w.find('[data-testid="insurance-provider-input"]').setValue('Lemonade')
    await w.find('[data-testid="insurance-also-pet"]').setValue(false)
    await w.find('[data-testid="insurance-save"]').trigger('click')
    await flushPromises()
    expect(api.createInsurance).toHaveBeenCalled()
    expect(api.updatePet).not.toHaveBeenCalled()
  })
})

describe('Home', () => {
  it('renders active pets, memorial, and a calm banner when nothing is due (#35)', async () => {
    api.pets.mockResolvedValue([
      { ...BISCUIT },
      { id: 'rex', name: 'Rex', species: 'Dog', next_reminder: null, archived_at: '2024-01-01', memorial_note: 'Good boy.' },
    ])
    const w = mount(Home, opts)
    await flushPromises()
    expect(w.text()).toContain('Biscuit')
    expect(w.text()).toContain('Remembering')
    expect(w.text()).toContain('Rex')
    expect(w.text()).toContain('all caught up')
  })

  it('surfaces the pet with the soonest reminder in the banner (#35)', async () => {
    api.pets.mockResolvedValue([{ ...BISCUIT, next_reminder: { ...REMINDER } }])
    const w = mount(Home, opts)
    await flushPromises()
    expect(w.text()).toContain('Biscuit: Rabies booster')
    expect(w.text()).not.toContain('could use a little care') // no grading
  })

  it('shows an error when the API fails', async () => {
    api.pets.mockRejectedValue(new Error('boom'))
    const w = mount(Home, opts)
    await flushPromises()
    expect(w.text()).toContain('boom')
  })

  it('opens the add-pet sheet from the trailing card', async () => {
    api.pets.mockResolvedValue([{ ...BISCUIT }])
    const w = mount(Home, opts)
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(false)
    await w.find('.pcard.padd').trigger('click')
    expect(w.find('.mback').exists()).toBe(true)
    expect(w.text()).toContain('Add a pet')
  })

  it('offers "Add a pet" in the speed-dial while mounted, and withdraws it after (#77)', async () => {
    api.pets.mockResolvedValue([{ ...BISCUIT }])
    const w = mount(Home, opts)
    await flushPromises()
    expect(dialKeys()).toEqual(['add-pet'])
    expect(pageActions.value[0].label).toBe('Add a pet')
    await runDial('add-pet')
    expect(w.findComponent(AddPetSheet).exists()).toBe(true)
    w.unmount()
    expect(dialKeys()).toEqual([])
  })
})

describe('Pet', () => {
  it('renders the profile with records and weights', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT,
      gotcha_date: '2022-06-01',
      records: [{
        id: 1, title: 'Dental', occurred_on: '2026-07-02', ends_on: '2026-07-06', category: 'Appointments',
        cost_cents: 42000, covered_cents: 37800, source: 'scan',
        notes: '**Bloodwork** normal\n- Recheck in 2 weeks',
        extra: { 'Rabies tag': '12345' },
        line_items: [{ description: 'Exam', amount_cents: 8500 }],
        attachments: [
          { id: 'att-1', filename: 'invoice.pdf', media_type: 'application/pdf', size_bytes: 20480 },
          { id: 'att-2', filename: 'xray.png', media_type: 'image/png', size_bytes: 51200 },
        ],
      }],
      weights: [{ value: 31, unit: 'kg' }, { value: 32, unit: 'kg' }, { value: 32.4, unit: 'kg' }],
      reminders: [{ id: 5, due_on: '2026-09-05', title: 'Fecal exam', done: false, source: 'scan' }],
      next_reminder: { id: 5, pet_id: 'biscuit', due_on: '2026-09-05', title: 'Fecal exam', kind: null, done: false, source: 'scan', created_at: '' },
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.text()).toContain('Biscuit')
    expect(w.find('[data-testid="next-up"]').text()).toContain('Fecal exam') // #35 what's next
    expect(w.find('.rem').exists()).toBe(true) // reminders section
    expect(w.text()).toContain('Fecal exam')
    expect(w.text()).toContain('Dental')
    expect(w.text()).toContain('$420')
    expect(w.text()).toContain('from a scanned document')
    expect(w.text()).toContain('Home since Jun 2022') // gotcha day surfaces
    expect(w.find('.trend').exists()).toBe(true)
    // #4: custom-field chip and both attachments render, linked to the API
    expect(w.text()).toContain('Rabies tag')
    expect(w.text()).toContain('invoice.pdf')
    expect(w.find('a.att[href="/api/attachments/att-1"]').exists()).toBe(true)
    expect(w.find('.att img').attributes('src')).toBe('/api/attachments/att-2') // image thumbnail
    // notes render as markdown, and the cost breakdown shows
    expect(w.html()).toContain('<strong>Bloodwork</strong>')
    expect(w.html()).toContain('<li>Recheck in 2 weeks</li>')
    expect(w.text()).toContain('Exam')
    expect(w.text()).toContain('$85')
    expect(w.text()).toContain('2026-07-06') // #15: the record's date range end
    // #32: birthday + gotcha day surface as timeline milestones
    expect(w.text()).toContain('Biscuit was born')
    expect(w.text()).toContain('Biscuit came home')
    expect(w.findAll('.tevent.milestone').length).toBe(2)
    // #12: durable probe anchors
    expect(w.find('[data-testid="pet-name"]').text()).toBe('Biscuit')
    expect(w.find('[data-testid="weight-trend"]').exists()).toBe(true)
    expect(w.find('[data-testid="timeline-event"]').exists()).toBe(true)
    expect(w.find('[data-testid="timeline-milestone"]').exists()).toBe(true)
  })

  it('opens the record sheet read-only when a timeline record is tapped (#15/#44)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, weights: [], reminders: [],
      records: [{ id: 1, title: 'Dental', occurred_on: '2026-07-02', category: 'Appointments', cost_cents: 42000, attachments: [] }],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(false)
    await w.find('.tevent.clickable').trigger('click')
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(true)
    // #44: opens the read-only view first (Edit is inside), not the edit form
    expect(w.find('[data-testid="record-view"]').exists()).toBe(true)
    expect(w.find('[data-testid="record-edit"]').exists()).toBe(true)
    expect(w.find('.sheet').text()).toContain('Dental')
  })

  it('shows an error when the pet cannot be loaded', async () => {
    api.pet.mockRejectedValue(new Error("no pet 'ghost'"))
    const w = mount(Pet, { props: { id: 'ghost' }, ...opts })
    await flushPromises()
    expect(w.text()).toContain("no pet 'ghost'")
  })

  it('shows the real photo in the hero and opens the editor on tap (#21)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, photo_path: 'photos/biscuit/x.jpg',
      records: [], weights: [], reminders: [],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    // hero shows the photo, not the initial placeholder
    expect(w.find('.phero-portrait img.pphoto').attributes('src')).toBe('/api/pets/biscuit/photo?v=photos/biscuit/x.jpg')
    expect(w.find('.phero-portrait .init').exists()).toBe(false)
    // the portrait is a button; tapping opens the editor sheet
    expect(w.findComponent(PhotoEditorSheet).exists()).toBe(false)
    await w.find('button.phero-portrait').trigger('click')
    const editor = w.findComponent(PhotoEditorSheet)
    expect(editor.exists()).toBe(true)
    expect(editor.props()).toMatchObject({ petId: 'biscuit', petName: 'Biscuit', hasPhoto: true })
    // saving from the editor closes it and reloads the profile
    editor.vm.$emit('saved')
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(2)
    expect(w.findComponent(PhotoEditorSheet).exists()).toBe(false)
  })

  it('falls back to the gradient placeholder when there is no photo (#21)', async () => {
    api.pet.mockResolvedValue({ ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [] })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.find('.phero-portrait img.pphoto').exists()).toBe(false)
    expect(w.find('.phero-portrait .init').text()).toBe('B')
  })

  it('marks a reminder done from the profile (#26)', async () => {
    const pet = {
      ...BISCUIT, gotcha_date: null, records: [], weights: [],
      reminders: [{ id: 9, due_on: '2026-09-05', title: 'Fecal exam', done: false, source: 'scan' }],
    }
    api.pet.mockResolvedValue(pet)
    api.updateReminder.mockResolvedValue({ ...pet.reminders[0], done: true })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    await w.find('.rem-check').trigger('click')
    await flushPromises()
    expect(api.updateReminder).toHaveBeenCalledWith(9, { done: true })
    expect(api.pet).toHaveBeenCalledTimes(2) // reloaded after toggle
  })

  it('reloads its timeline when the shared data signal bumps', async () => {
    api.pet.mockResolvedValue({ ...BISCUIT, gotcha_date: null, records: [], weights: [] })
    mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(1)
    bumpData() // a record was added while viewing this same pet
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(2)
  })

  it('lists insurance policies, reflects the tile, and opens the sheet (#8/#29)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [{
        id: 'pol-1', pet_id: 'biscuit', kind: 'insurance', provider: 'Lemonade',
        plan_name: 'Preventative', policy_number: 'POL04', status: 'active',
        reimburse_percent: 90, deductible_cents: 50000, premium_cents: 91136, premium_interval: 'year',
        annual_limit_cents: null, start_date: null, end_date: null, notes: null, coverages: null, exclusions: null,
        document_name: 'policy.pdf', document_media_type: 'application/pdf', document_size_bytes: 5,
        created_by: null, created_at: '', updated_at: '2026-07-06',
      }],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    const row = w.find('[data-testid="insurance-policy-row"]')
    expect(row.exists()).toBe(true)
    expect(row.text()).toContain('Lemonade')
    expect(row.text()).toContain('90% back')
    expect(row.text()).toContain('$500 deductible')
    expect(w.text()).toContain('90% reimbursement') // tile reflects the active policy
    // tapping opens the view sheet
    expect(w.findComponent(InsuranceSheet).exists()).toBe(false)
    await row.trigger('click')
    await flushPromises()
    expect(w.findComponent(InsuranceSheet).exists()).toBe(true)
  })

  // ---- standing care fields + ongoing care (#63) ----

  it('shows colour on the hero and allergies as a medical alert (#63)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [],
      color: 'Blue Merle',
      allergies: [{ label: 'Chicken', detail: 'hives' }, { label: 'Beef', detail: null }],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="pet-color-tag"]').text()).toBe('Blue Merle')
    const alert = w.find('[data-testid="allergy-alert"]')
    expect(alert.exists()).toBe(true)
    expect(alert.text()).toContain('Chicken · hives')
    expect(alert.text()).toContain('Beef')
  })

  it('hides the allergy alert when nothing is recorded (#63)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [], color: null, allergies: null,
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="allergy-alert"]').exists()).toBe(false)
    expect(w.find('[data-testid="pet-color-tag"]').exists()).toBe(false)
  })

  it('lists ongoing care, separating current from stopped (#63)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [],
      regimens: [
        { id: 'r1', pet_id: 'biscuit', kind: 'food', product: "Hill's Puppy", amount: '1 cup', frequency: '2x/day', protects: null, started_on: null, ended_on: null, notes: null, created_by: null, created_at: '', updated_at: '' },
        { id: 'r2', pet_id: 'biscuit', kind: 'preventative', product: 'Simparica Trio', amount: '1 pill', frequency: 'monthly', protects: ['flea', 'tick', 'heartworm'], started_on: null, ended_on: null, notes: null, created_by: null, created_at: '', updated_at: '' },
        { id: 'r3', pet_id: 'biscuit', kind: 'medication', product: 'Apoquel', amount: null, frequency: null, protects: null, started_on: null, ended_on: '2026-02-01', notes: null, created_by: null, created_at: '', updated_at: '' },
      ],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    const current = w.findAll('[data-testid="care-regimen"]')
    expect(current).toHaveLength(2)
    expect(current[0].text()).toContain("Hill's Puppy")
    expect(current[0].text()).toContain('1 cup · 2x/day')
    // One product, three protections — the reason this is a list, not columns.
    expect(current[1].text()).toContain('Simparica Trio')
    expect(current[1].text()).toContain('Flea')
    expect(current[1].text()).toContain('Tick')
    expect(current[1].text()).toContain('Heartworm')
    const past = w.findAll('[data-testid="care-regimen-past"]')
    expect(past).toHaveLength(1)
    expect(past[0].text()).toContain('stopped 2026-02-01')
  })

  it('renders protections as COVERAGE chips, never as hazard chips (#63)', async () => {
    // Protection is good news and sits one card below the red allergy chips;
    // it must not reuse the hazard/neutral chip tint, and it must match the
    // green the edit form uses for the same fact.
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [],
      allergies: [{ label: 'Chicken', detail: null }],
      regimens: [
        { id: 'r2', pet_id: 'biscuit', kind: 'preventative', product: 'Simparica Trio', amount: null, frequency: null, protects: ['flea', 'tick'], started_on: null, ended_on: null, notes: null, created_by: null, created_at: '', updated_at: '' },
      ],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    const chips = w.findAll('[data-testid="care-regimen"] .reqchip')
    expect(chips).toHaveLength(2)
    for (const c of chips) expect(c.classes()).toContain('prot')
    // Allergy chips are a different class entirely, so the two can't converge.
    expect(w.find('[data-testid="allergy-alert"] .allergy-chip').exists()).toBe(true)
    expect(w.find('[data-testid="allergy-alert"] .reqchip').exists()).toBe(false)
  })

  it('opens the care sheet on a row, and the add sheet on the button (#63)', async () => {
    const reg = { id: 'r1', pet_id: 'biscuit', kind: 'food', product: 'Kibble', amount: null, frequency: null, protects: null, started_on: null, ended_on: null, notes: null, created_by: null, created_at: '', updated_at: '' }
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [reg],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.findComponent(RegimenSheet).exists()).toBe(false)
    await w.find('[data-testid="care-regimen"]').trigger('click')
    expect(w.findComponent(RegimenSheet).props('regimen')).toEqual(reg)
    // Saving reloads the pet so the section reflects the change.
    w.findComponent(RegimenSheet).vm.$emit('saved')
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(2)
    // The add button opens the same sheet with no entry (create mode).
    await w.find('[data-testid="add-regimen"]').trigger('click')
    expect(w.findComponent(RegimenSheet).props('regimen')).toBe(null)
  })

  it('says so when there is no ongoing care on file (#63)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.text()).toContain('No food, preventatives or medications on file yet')
  })

  it('closes every open sheet when the route moves to another pet (#63)', async () => {
    // The component is reused across pets, so a sheet left open kept editing
    // the PREVIOUS pet's data under the NEW pet's name: saving wrote A's
    // profile onto B, and deleting removed A's care entry under a confirmation
    // that said B.
    const reg = { id: 'r1', pet_id: 'biscuit', kind: 'food', product: 'Kibble', amount: null, frequency: null, protects: null, started_on: null, ended_on: null, notes: null, created_by: null, created_at: '', updated_at: '' }
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [reg],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    await runDial('edit-pet')
    expect(w.findComponent(PetSheet).exists()).toBe(true)

    api.pet.mockResolvedValue({
      ...BISCUIT, id: 'miso', name: 'Miso', gotcha_date: null, records: [], weights: [],
      reminders: [], insurance_policies: [], regimens: [],
    })
    await w.setProps({ id: 'miso' })
    await flushPromises()
    expect(w.findComponent(PetSheet).exists()).toBe(false)
    expect(w.findComponent(RegimenSheet).exists()).toBe(false)

    // And the same for a care sheet opened on the first pet.
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [reg],
    })
    await w.setProps({ id: 'biscuit' })
    await flushPromises()
    await w.find('[data-testid="care-regimen"]').trigger('click')
    expect(w.findComponent(RegimenSheet).exists()).toBe(true)
    api.pet.mockResolvedValue({
      ...BISCUIT, id: 'miso', name: 'Miso', gotcha_date: null, records: [], weights: [],
      reminders: [], insurance_policies: [], regimens: [],
    })
    await w.setProps({ id: 'miso' })
    await flushPromises()
    expect(w.findComponent(RegimenSheet).exists()).toBe(false)
  })

  it('opens the profile editor and reloads after saving (#63)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.findComponent(PetSheet).exists()).toBe(false)
    // No in-page edit button any more: the speed-dial is the one place (#77).
    expect(w.find('[data-testid="edit-profile"]').exists()).toBe(false)
    expect(pageActions.value.map((a) => a.label)).toEqual(['Edit Biscuit'])
    await runDial('edit-pet')
    expect(w.findComponent(PetSheet).exists()).toBe(true)
    w.findComponent(PetSheet).vm.$emit('saved')
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(2)
  })

  it('withholds "Edit" while another pet is still loading, and on unmount (#77)', async () => {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [],
      insurance_policies: [], regimens: [],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(dialKeys()).toEqual(['edit-pet'])
    // Mid-navigation the old pet is still on screen; editing it would write
    // Biscuit's profile under Miso's route.
    let resolveMiso: (v: unknown) => void = () => {}
    api.pet.mockReturnValue(new Promise((r) => { resolveMiso = r }))
    await w.setProps({ id: 'miso' })
    expect(dialKeys()).toEqual([])
    resolveMiso({ ...BISCUIT, id: 'miso', name: 'Miso', gotcha_date: null, records: [], weights: [], reminders: [], insurance_policies: [], regimens: [] })
    await flushPromises()
    expect(pageActions.value.map((a) => a.label)).toEqual(['Edit Miso'])
    w.unmount()
    expect(dialKeys()).toEqual([])
  })

  it('ignores a slow reply for the previous pet that lands last (#77)', async () => {
    const detail = (id: string, name: string) => ({ ...BISCUIT, id, name, gotcha_date: null, records: [], weights: [], reminders: [], insurance_policies: [], regimens: [] })
    let resolveA: (v: unknown) => void = () => {}
    api.pet.mockReturnValueOnce(new Promise((r) => { resolveA = r }))
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    api.pet.mockResolvedValueOnce(detail('miso', 'Miso'))
    await w.setProps({ id: 'miso' })
    await flushPromises()
    resolveA(detail('biscuit', 'Biscuit')) // A's reply arrives after B's
    await flushPromises()
    expect(w.find('[data-testid="pet-name"]').text()).toBe('Miso')
    expect(pageActions.value.map((a) => a.label)).toEqual(['Edit Miso'])
    expect(w.find('.loading').exists()).toBe(false)
  })

  it('ignores a failed reply for the previous pet that lands last', async () => {
    let rejectA: (e: unknown) => void = () => {}
    api.pet.mockReturnValueOnce(new Promise((_, rej) => { rejectA = rej }))
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    api.pet.mockResolvedValueOnce({ ...BISCUIT, id: 'miso', name: 'Miso', gotcha_date: null, records: [], weights: [], reminders: [], insurance_policies: [], regimens: [] })
    await w.setProps({ id: 'miso' })
    await flushPromises()
    rejectA(new Error('stale boom'))
    await flushPromises()
    expect(w.text()).not.toContain('stale boom')
    expect(w.find('[data-testid="pet-name"]').text()).toBe('Miso')
  })

  it('opens the add-insurance sheet and reloads after saving (#8/#29)', async () => {
    api.pet.mockResolvedValue({ ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [], insurance_policies: [] })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.text()).toContain('No policies or plans on file yet')
    await w.find('[data-testid="add-insurance"]').trigger('click')
    const sheet = w.findComponent(InsuranceSheet)
    expect(sheet.exists()).toBe(true)
    sheet.vm.$emit('saved')
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(2) // reloaded after save
  })
})

describe('Costs', () => {
  it('renders the donut legend and insurance rows', async () => {
    api.costs.mockResolvedValue({
      total_cents: 100000, covered_cents: 60000,
      by_category: [{ category: 'Appointments', cost_cents: 70000 }, { category: 'Prescriptions', cost_cents: 30000 }],
      by_pet: [{ pet_id: 'biscuit', name: 'Biscuit', insurer: 'Trupanion', cost_cents: 70000, covered_cents: 60000 }],
    })
    const w = mount(Costs, opts)
    await flushPromises()
    expect(w.text()).toContain('Appointments')
    expect(w.text()).toContain('Trupanion')
    expect(w.findAll('.donut circle').length).toBe(2)
  })
})

describe('Calendar', () => {
  const now = new Date()
  const ym = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`

  it('places records and reminders as events in the month grid', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    api.records.mockResolvedValue([{ id: 1, title: 'Exam', occurred_on: `${ym}-15`, category: 'Appointments', cost_cents: 18500 }])
    // #26: reminders merge into the calendar as dashed future events
    api.allReminders.mockResolvedValue([{ id: 1, pet_id: 'biscuit', pet_name: 'Biscuit', due_on: `${ym}-20`, title: 'Rabies due', done: false }])
    const w = mount(Calendar, opts)
    await flushPromises()
    expect(w.text()).toContain('Biscuit · Exam')
    expect(w.text()).toContain('Biscuit · Rabies due')
    expect(w.find('.cev.reminder').exists()).toBe(true)
    expect(w.findAll('.cday').length).toBeGreaterThan(27)
    // #12: durable probe anchors
    expect(w.findAll('[data-testid="calendar-day"]').length).toBeGreaterThan(27)
    expect(w.find('[data-testid="calendar-event"]').exists()).toBe(true)
  })

  it('switches to the agenda and splits upcoming from earlier (#6)', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    api.records.mockResolvedValue([{ id: 1, title: 'Old exam', occurred_on: '2020-01-01', category: 'Appointments', cost_cents: 5000 }])
    api.allReminders.mockResolvedValue([{ id: 2, pet_id: 'biscuit', pet_name: 'Biscuit', due_on: '2999-01-01', title: 'Future booster', done: false }])
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.findAll('.segmented button').find((b) => b.text() === 'Agenda').trigger('click')
    expect(w.find('.agenda').exists()).toBe(true)
    expect(w.text()).toContain('Upcoming')
    expect(w.text()).toContain('Future booster')
    expect(w.text()).toContain('Earlier')
    expect(w.text()).toContain('Old exam')
  })

  it('filters by type (#6): Reminders hides records', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    api.records.mockResolvedValue([{ id: 1, title: 'Exam', occurred_on: `${ym}-15`, category: 'Appointments', cost_cents: 18500 }])
    api.allReminders.mockResolvedValue([{ id: 2, pet_id: 'biscuit', pet_name: 'Biscuit', due_on: `${ym}-20`, title: 'Rabies due', done: false }])
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.find('select[aria-label="Filter by type"]').setValue('reminder')
    expect(w.text()).not.toContain('Exam')
    expect(w.text()).toContain('Rabies due')
  })

  it('filters by pet (#6)', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }, { id: 'miso', name: 'Miso' }])
    api.records.mockImplementation((id) =>
      Promise.resolve(id === 'biscuit'
        ? [{ id: 1, title: 'Biscuit exam', occurred_on: `${ym}-10`, category: 'Appointments' }]
        : [{ id: 2, title: 'Miso exam', occurred_on: `${ym}-12`, category: 'Appointments' }]))
    api.allReminders.mockResolvedValue([])
    const w = mount(Calendar, opts)
    await flushPromises()
    expect(w.text()).toContain('Biscuit exam')
    expect(w.text()).toContain('Miso exam')
    await w.find('select[aria-label="Filter by pet"]').setValue('miso')
    expect(w.text()).not.toContain('Biscuit exam')
    expect(w.text()).toContain('Miso exam')
  })

  it('opens the record view when a month event is tapped (#16/#44)', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    api.records.mockResolvedValue([{ id: 1, title: 'Exam', occurred_on: `${ym}-15`, category: 'Appointments', cost_cents: 18500 }])
    api.allReminders.mockResolvedValue([])
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.find('.cev').trigger('click')
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(true)
    // #44: the calendar now lands on the read-only view, not the edit form
    expect(w.find('[data-testid="record-view"]').exists()).toBe(true)
    expect(w.find('.sheet').text()).toContain('Exam')
  })

  it('opens the reminder sheet and toggles done (#16)', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    api.records.mockResolvedValue([])
    api.allReminders.mockResolvedValue([{ id: 5, pet_id: 'biscuit', pet_name: 'Biscuit', due_on: `${ym}-20`, title: 'Rabies due', done: false }])
    api.updateReminder.mockResolvedValue({})
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.find('.cev.reminder').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Reminder')
    expect(w.text()).toContain('Rabies due')
    await w.find('.mback .btn.primary').trigger('click') // Mark done
    await flushPromises()
    expect(api.updateReminder).toHaveBeenCalledWith(5, { done: true })
    expect(w.find('.mback').exists()).toBe(false) // sheet closed + calendar reloaded
  })

  it('shows a recurring milestone chip and navigates to the pet on tap (#32)', async () => {
    const mm = String(now.getMonth() + 1).padStart(2, '0')
    api.pets.mockResolvedValue([{ id: 'pedro', name: 'Pedro', birth_date: `2020-${mm}-05`, gotcha_date: null, archived_at: null }])
    api.records.mockResolvedValue([])
    api.allReminders.mockResolvedValue([])
    const w = mount(Calendar, opts)
    await flushPromises()
    const chip = w.find('.cev.milestone')
    expect(chip.exists()).toBe(true)
    expect(chip.text()).toContain('Pedro’s birthday')
    await chip.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/pets/pedro')
  })

  it('lists milestone anniversaries in the agenda upcoming section (#32)', async () => {
    api.pets.mockResolvedValue([{ id: 'pedro', name: 'Pedro', birth_date: '2020-05-05', gotcha_date: null, archived_at: null }])
    api.records.mockResolvedValue([])
    api.allReminders.mockResolvedValue([])
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.findAll('.segmented button').find((b) => b.text() === 'Agenda').trigger('click')
    expect(w.text()).toContain('Upcoming')
    expect(w.text()).toContain('Pedro’s birthday')
    expect(w.find('.arow.milestone').exists()).toBe(true)
  })

  it('shows an empty agenda when there is nothing to list (#6)', async () => {
    api.pets.mockResolvedValue([{ id: 'p', name: 'P', archived_at: null }])
    api.records.mockResolvedValue([])
    api.allReminders.mockResolvedValue([])
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.findAll('.segmented button').find((b) => b.text() === 'Agenda').trigger('click')
    expect(w.text()).toContain('Nothing to show yet')
  })

  it('steps through months with the nav buttons (#6)', async () => {
    api.pets.mockResolvedValue([])
    api.records.mockResolvedValue([])
    api.allReminders.mockResolvedValue([])
    const w = mount(Calendar, opts)
    await flushPromises()
    const label = w.find('h1').text()
    await w.find('[aria-label="Next month"]').trigger('click')
    expect(w.find('h1').text()).not.toBe(label)
    await w.find('[aria-label="Previous month"]').trigger('click')
    expect(w.find('h1').text()).toBe(label)
  })

  it('edits a record from the calendar, then closes and reloads (#16)', async () => {
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    api.records.mockResolvedValue([{ id: 1, title: 'Exam', occurred_on: `${ym}-15`, category: 'Appointments', cost_cents: 18500 }])
    api.allReminders.mockResolvedValue([])
    api.updateRecord.mockResolvedValue({ id: 1 })
    const w = mount(Calendar, opts)
    await flushPromises()
    await w.find('.cev').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="record-sheet"]').exists()).toBe(true)
    await w.find('[data-testid="record-edit"]').trigger('click') // #44: into the edit form
    await w.find('[data-testid="record-save"]').trigger('click')
    await flushPromises()
    expect(api.updateRecord).toHaveBeenCalled()
    expect(w.find('[data-testid="record-sheet"]').exists()).toBe(false) // closed + calendar reloaded
  })
})

describe('Settings', () => {
  const NO_KEY = { enabled: false, has_key: false, provider: 'anthropic', model: 'claude-sonnet-5' }
  const WITH_KEY = { enabled: true, has_key: true, provider: 'anthropic', model: 'claude-sonnet-5' }
  const MODELS = [
    { id: 'claude-sonnet-5-5', display_name: 'Claude Sonnet 5.5', created_at: '2026-09-28T00:00:00Z' },
    { id: 'claude-sonnet-5', display_name: 'Claude Sonnet 5', created_at: '2026-06-29T00:00:00Z' },
  ]
  const select = (w) => w.find('[data-testid="settings-model-select"]')
  const custom = (w) => w.find('[data-testid="settings-model-custom"]')

  it('loads current ingestion settings; with no key the model is typed', async () => {
    api.ingestion.mockResolvedValue(NO_KEY)
    const w = mount(Settings, opts)
    await flushPromises()
    expect(w.text()).toContain('Document scanning')
    expect(api.ingestionModels).not.toHaveBeenCalled() // nothing to list with
    expect(select(w).exists()).toBe(false)
    expect(custom(w).element.value).toBe('claude-sonnet-5')
    expect(w.find('[data-testid="settings-model-note"]').text()).toContain('Paste your API key')
    expect(w.find('.btn.sec').exists()).toBe(false) // no key yet → no remove button
  })

  it('shows the provider as plain text, not a dead control (#67)', async () => {
    api.ingestion.mockResolvedValue(NO_KEY)
    const w = mount(Settings, opts)
    await flushPromises()
    const p = w.find('[data-testid="settings-provider"]')
    expect(p.text()).toBe('Anthropic (Claude)')
    expect(p.element.tagName).toBe('DIV')
    expect(w.find('select[disabled]').exists()).toBe(false)
  })

  it('fills the picker from the stored key with the saved model selected (#67)', async () => {
    api.ingestion.mockResolvedValue(WITH_KEY)
    api.ingestionModels.mockResolvedValue(MODELS)
    api.saveIngestion.mockResolvedValue({ ...WITH_KEY, model: 'claude-sonnet-5-5' })
    const w = mount(Settings, opts)
    await flushPromises()
    expect(api.ingestionModels).toHaveBeenCalledWith(undefined) // stored key: GET
    const opts_ = select(w).findAll('option').map((o) => o.text())
    expect(opts_).toEqual(['Claude Sonnet 5.5', 'Claude Sonnet 5', 'Custom…'])
    expect(select(w).element.value).toBe('claude-sonnet-5')
    expect(custom(w).exists()).toBe(false) // only revealed by Custom…
    await select(w).setValue('claude-sonnet-5-5')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.saveIngestion).toHaveBeenCalledWith({ model: 'claude-sonnet-5-5', enabled: true })
    expect(select(w).element.value).toBe('claude-sonnet-5-5')
  })

  it('Custom… reveals the text input, and a saved model not in the list shows as Custom (#67)', async () => {
    api.ingestion.mockResolvedValue({ ...WITH_KEY, model: 'claude-private-preview' })
    api.ingestionModels.mockResolvedValue(MODELS)
    api.saveIngestion.mockResolvedValue({ ...WITH_KEY, model: 'claude-other' })
    const w = mount(Settings, opts)
    await flushPromises()
    expect(select(w).element.value).toBe('__custom__')
    expect(custom(w).element.value).toBe('claude-private-preview') // never rewritten
    // Pick a listed model, then back to Custom… → the input returns, editable.
    await select(w).setValue('claude-sonnet-5')
    expect(custom(w).exists()).toBe(false)
    await select(w).setValue('__custom__')
    await custom(w).setValue('claude-other')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.saveIngestion).toHaveBeenCalledWith({ model: 'claude-other', enabled: true })
  })

  it('falls back to the text input with the error when the list fails (#67)', async () => {
    api.ingestion.mockResolvedValue(WITH_KEY)
    api.ingestionModels.mockRejectedValue(new Error('The model provider returned 529 (overloaded_error).'))
    const w = mount(Settings, opts)
    await flushPromises()
    expect(select(w).exists()).toBe(false)
    expect(custom(w).element.value).toBe('claude-sonnet-5')
    const note = w.find('[data-testid="settings-model-note"]')
    expect(note.text()).toContain("Couldn't load the model list.")
    expect(note.text()).toContain('overloaded_error')
  })

  describe('a pasted, unsaved key (#67)', () => {
    beforeEach(() => { vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] }) })
    afterEach(() => { vi.useRealTimers() })

    it('lists models for the pasted key after a pause, without saving it', async () => {
      api.ingestion.mockResolvedValue(NO_KEY)
      api.ingestionModels.mockResolvedValue(MODELS)
      const w = mount(Settings, opts)
      await flushPromises()
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant')
      vi.advanceTimersByTime(1000)
      expect(api.ingestionModels).not.toHaveBeenCalled() // too short to be a key
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-pasted-key')
      vi.advanceTimersByTime(499)
      expect(api.ingestionModels).not.toHaveBeenCalled() // debounced
      vi.advanceTimersByTime(1)
      await flushPromises()
      expect(api.ingestionModels).toHaveBeenCalledTimes(1)
      expect(api.ingestionModels).toHaveBeenCalledWith('sk-ant-pasted-key')
      expect(api.saveIngestion).not.toHaveBeenCalled()
      expect(select(w).element.value).toBe('claude-sonnet-5')
    })

    // The real client throws an ApiError carrying the HTTP status.
    const apiError = (message, status) => Object.assign(new Error(message), { status })

    it('says inline when the pasted key is refused, blocks saving it, and clearing it resets', async () => {
      api.ingestion.mockResolvedValue(NO_KEY)
      api.ingestionModels.mockRejectedValue(apiError('That looks like a key for OpenAI, not Anthropic.', 400))
      const w = mount(Settings, opts)
      await flushPromises()
      await w.find('[data-testid="settings-api-key"]').setValue('sk-proj-abcdefghij')
      vi.advanceTimersByTime(500)
      await flushPromises()
      expect(w.find('[data-testid="settings-key-error"]').text()).toContain('OpenAI')
      expect(select(w).exists()).toBe(false)
      // A refused key can't be saved over whatever is stored.
      expect(w.find('.btn.primary').attributes('disabled')).toBeDefined()
      await w.find('[data-testid="settings-api-key"]').setValue('')
      await flushPromises()
      expect(w.find('[data-testid="settings-key-error"]').exists()).toBe(false)
      expect(w.find('.btn.primary').attributes('disabled')).toBeUndefined()
      expect(api.ingestionModels).toHaveBeenCalledTimes(1) // no stored key to fall back to
    })

    it('a provider outage for a pasted key shows the error but still allows saving', async () => {
      api.ingestion.mockResolvedValue(NO_KEY)
      api.ingestionModels.mockRejectedValue(apiError('The model provider returned 529 (overloaded_error).', 502))
      const w = mount(Settings, opts)
      await flushPromises()
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-pasted-key')
      vi.advanceTimersByTime(500)
      await flushPromises()
      expect(w.find('[data-testid="settings-key-error"]').text()).toContain('overloaded')
      expect(w.find('.btn.primary').attributes('disabled')).toBeUndefined()
      expect(custom(w).element.value).toBe('claude-sonnet-5') // typed fallback
    })

    it('drops an answer for a key that was edited before it arrived', async () => {
      api.ingestion.mockResolvedValue(NO_KEY)
      let resolveFirst
      api.ingestionModels.mockImplementationOnce(() => new Promise((r) => { resolveFirst = r }))
      const w = mount(Settings, opts)
      await flushPromises()
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-first-key')
      vi.advanceTimersByTime(500) // first request is in flight
      // Edited to something too short to look up: no new request will start,
      // so the edit itself must invalidate the old one.
      await w.find('[data-testid="settings-api-key"]').setValue('sk-a')
      resolveFirst(MODELS)
      await flushPromises()
      expect(select(w).exists()).toBe(false)
      expect(api.ingestionModels).toHaveBeenCalledTimes(1)
      expect(w.find('[data-testid="settings-model-note"]').text()).toContain('Paste your API key')
    })

    it('shows the list as loading while a pasted key waits to be looked up', async () => {
      api.ingestion.mockResolvedValue(WITH_KEY)
      api.ingestionModels.mockResolvedValue(MODELS)
      const w = mount(Settings, opts)
      await flushPromises()
      expect(select(w).exists()).toBe(true) // the stored key's list
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-replacement')
      // That list belonged to the old key; it goes until the new one answers.
      expect(select(w).exists()).toBe(false)
      expect(w.find('[data-testid="settings-model-note"]').text()).toContain('Loading')
      vi.advanceTimersByTime(500)
      await flushPromises()
      expect(api.ingestionModels).toHaveBeenLastCalledWith('sk-ant-replacement')
      expect(select(w).element.value).toBe('claude-sonnet-5')
    })

    it('ignores a slow answer for an earlier key', async () => {
      api.ingestion.mockResolvedValue(NO_KEY)
      let resolveFirst
      api.ingestionModels
        .mockImplementationOnce(() => new Promise((r) => { resolveFirst = r }))
        .mockResolvedValueOnce([MODELS[1]])
      const w = mount(Settings, opts)
      await flushPromises()
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-first-key')
      vi.advanceTimersByTime(500)
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-second-key')
      vi.advanceTimersByTime(500)
      await flushPromises()
      resolveFirst(MODELS) // the stale answer lands last
      await flushPromises()
      expect(select(w).findAll('option').map((o) => o.text())).toEqual(['Claude Sonnet 5', 'Custom…'])
    })

    it('after saving a pasted key, reloads the list from the stored key', async () => {
      api.ingestion.mockResolvedValue(NO_KEY)
      api.ingestionModels.mockResolvedValue(MODELS)
      api.saveIngestion.mockResolvedValue({ ...WITH_KEY, model: 'claude-sonnet-5' })
      const w = mount(Settings, opts)
      await flushPromises()
      await w.find('[data-testid="settings-api-key"]').setValue('sk-ant-pasted-key')
      vi.advanceTimersByTime(500)
      await flushPromises()
      await w.find('.btn.primary').trigger('click')
      await flushPromises()
      expect(api.ingestionModels).toHaveBeenLastCalledWith(undefined)
      expect(select(w).element.value).toBe('claude-sonnet-5')
    })
  })

  it('saves the key, model, and enabled flag (key sent only when entered)', async () => {
    api.ingestion.mockResolvedValue(NO_KEY)
    api.saveIngestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'claude-haiku-4-5-20251001' })
    api.ingestionModels.mockResolvedValue([])
    const w = mount(Settings, opts)
    await flushPromises()
    await custom(w).setValue('claude-haiku-4-5-20251001')
    await w.find('input[type="password"]').setValue('sk-ant-secret')
    await w.find('input[type="checkbox"]').setValue(true)
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(api.saveIngestion).toHaveBeenCalledWith({
      model: 'claude-haiku-4-5-20251001', enabled: true, api_key: 'sk-ant-secret',
    })
    expect(w.text()).toContain('Saved.')
    expect(w.find('input[type="password"]').element.value).toBe('') // cleared after save
    expect(w.find('.btn.sec').exists()).toBe(true) // now offers "Remove key"
  })

  it('surfaces a save error', async () => {
    api.ingestion.mockResolvedValue(NO_KEY)
    api.saveIngestion.mockRejectedValue(new Error('That looks like a key for OpenAI, not Anthropic.'))
    const w = mount(Settings, opts)
    await flushPromises()
    await w.find('.btn.primary').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('That looks like a key for OpenAI, not Anthropic.')
  })

  it('removes a stored key and drops back to the typed model', async () => {
    api.ingestion.mockResolvedValue(WITH_KEY)
    api.ingestionModels.mockResolvedValue(MODELS)
    api.clearIngestion.mockResolvedValue(NO_KEY)
    const w = mount(Settings, opts)
    await flushPromises()
    expect(select(w).exists()).toBe(true)
    await w.find('.btn.sec').trigger('click')
    await flushPromises()
    expect(api.clearIngestion).toHaveBeenCalled()
    expect(w.find('.btn.sec').exists()).toBe(false)
    expect(select(w).exists()).toBe(false)
    expect(custom(w).element.value).toBe('claude-sonnet-5')
  })

  it('surfaces a remove-key error', async () => {
    api.ingestion.mockResolvedValue(WITH_KEY)
    api.ingestionModels.mockResolvedValue(MODELS)
    api.clearIngestion.mockRejectedValue(new Error('disk full'))
    const w = mount(Settings, opts)
    await flushPromises()
    await w.find('.btn.sec').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('disk full')
  })

  it('surfaces a load error', async () => {
    api.ingestion.mockRejectedValue(new Error('settings unreachable'))
    const w = mount(Settings, opts)
    await flushPromises()
    expect(w.text()).toContain('settings unreachable')
  })
})

// ---------- shared contacts (#20) ----------

const CEDAR = {
  id: 'c1', name: 'Cedar Creek', role: 'vet', doctor: 'Dr. Okafor', phone: '555-0100',
  email: null, address: null, website: null, notes: null, created_by: null,
  created_at: '2026-07-01', updated_at: '2026-07-01',
}

describe('ContactSheet (#20)', () => {
  beforeEach(() => { vi.clearAllMocks() })

  // An existing contact opens read-only (#44); the form is behind Edit.
  async function mountEditing(contact) {
    const w = mount(ContactSheet, { props: { contact }, ...opts })
    await flushPromises()
    await w.find('[data-testid="contact-edit"]').trigger('click')
    return w
  }

  it('opens an existing contact in a read-only view (#44)', async () => {
    const vet = {
      ...CEDAR, email: 'care@cedar.example', website: 'https://cedar.example',
      hours: [{ label: 'Open', detail: 'Mon-Fri 7-7' }],
      requirements: [{ label: 'Vaccinations', detail: 'Rabies current' }], notes: 'Ask for Dr. O',
    }
    const w = mount(ContactSheet, { props: { contact: vet }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="contact-view"]').exists()).toBe(true)
    expect(w.find('[data-testid="contact-name-input"]').exists()).toBe(false) // not the form
    expect(w.text()).toContain('Cedar Creek')
    expect(w.text()).toContain('Dr. Okafor')
    expect(w.text()).toContain('555-0100')
    expect(w.text()).toContain('Open')
    expect(w.text()).toContain('Vaccinations')
    expect(w.text()).toContain('Ask for Dr. O')
    // phone + email are actionable links
    expect(w.find('a[href="tel:555-0100"]').exists()).toBe(true)
    expect(w.find('a[href="mailto:care@cedar.example"]').exists()).toBe(true)
  })

  it('closes from the contact view via Close (#44)', async () => {
    const w = mount(ContactSheet, { props: { contact: { ...CEDAR } }, ...opts })
    await flushPromises()
    await w.find('[data-testid="contact-close"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
  })

  it('Edit flips an existing contact to the form; Cancel returns to the view (#44)', async () => {
    const w = mount(ContactSheet, { props: { contact: { ...CEDAR } }, ...opts })
    await flushPromises()
    await w.find('[data-testid="contact-edit"]').trigger('click')
    expect(w.find('[data-testid="contact-name-input"]').exists()).toBe(true)
    expect(w.text()).toContain('Edit contact')
    await w.find('[data-testid="contact-cancel"]').trigger('click')
    expect(w.find('[data-testid="contact-view"]').exists()).toBe(true) // back to the view
    expect(w.emitted('close')).toBeUndefined()
  })

  it('Cancel closes a NEW contact — no view to return to (#44)', async () => {
    const w = mount(ContactSheet, opts)
    await flushPromises()
    expect(w.find('[data-testid="contact-name-input"]').exists()).toBe(true) // straight to the form
    await w.find('[data-testid="contact-cancel"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
  })

  it('creates a contact and emits saved', async () => {
    api.createContact.mockResolvedValue({ ...CEDAR })
    const w = mount(ContactSheet, opts)
    // header reads "Add", no delete button when creating
    expect(w.text()).toContain('Add a contact')
    expect(w.find('[data-testid="contact-delete"]').exists()).toBe(false)
    await w.find('[data-testid="contact-name-input"]').setValue('Cedar Creek')
    await w.find('[data-testid="contact-role-select"]').setValue('groomer')
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    expect(api.createContact).toHaveBeenCalledWith(expect.objectContaining({ name: 'Cedar Creek', role: 'groomer' }))
    expect(w.emitted('saved')?.[0]).toEqual([{ ...CEDAR }])
  })

  it('blank name is rejected before any request', async () => {
    const w = mount(ContactSheet, opts)
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    expect(api.createContact).not.toHaveBeenCalled()
    expect(w.text()).toContain('Give this contact a name.')
  })

  it('surfaces a save error and does not emit saved', async () => {
    api.createContact.mockRejectedValue(new Error('name is required'))
    const w = mount(ContactSheet, opts)
    await w.find('[data-testid="contact-name-input"]').setValue('X')
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('name is required')
    expect(w.emitted('saved')).toBeUndefined()
  })

  it('does not delete when the confirmation is dismissed', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    const w = await mountEditing({ ...CEDAR })
    await w.find('[data-testid="contact-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteContact).not.toHaveBeenCalled()
    expect(w.emitted('deleted')).toBeUndefined()
  })

  it('surfaces a delete error and does not emit deleted', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    api.deleteContact.mockRejectedValue(new Error('delete failed'))
    const w = await mountEditing({ ...CEDAR })
    await w.find('[data-testid="contact-delete"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('delete failed')
    expect(w.emitted('deleted')).toBeUndefined()
  })

  it('edits an existing contact (prefilled) and can delete it', async () => {
    api.updateContact.mockResolvedValue({ ...CEDAR, phone: '555-0199' })
    api.deleteContact.mockResolvedValue(null)
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    const w = await mountEditing({ ...CEDAR })
    expect(w.text()).toContain('Edit contact')
    // prefilled name + doctor
    expect((w.find('[data-testid="contact-name-input"]').element as HTMLInputElement).value).toBe('Cedar Creek')
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    expect(api.updateContact).toHaveBeenCalledWith('c1', expect.objectContaining({ name: 'Cedar Creek', doctor: 'Dr. Okafor' }))
    // delete confirms, calls the API, and emits deleted
    await w.find('[data-testid="contact-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteContact).toHaveBeenCalledWith('c1')
    expect(w.emitted('deleted')?.[0]).toEqual(['c1'])
  })

  // ---- #42: requirements + website import ----

  it('adds a requirement row and sends it on save (#42)', async () => {
    api.createContact.mockResolvedValue({ ...CEDAR })
    const w = mount(ContactSheet, opts)
    await flushPromises()
    await w.find('[data-testid="contact-name-input"]').setValue('Happy Tails Boarding')
    await w.find('[data-testid="add-requirement"]').trigger('click')
    const row = w.find('[data-testid="requirement-row"]')
    await row.find('.kv-k').setValue('Vaccinations')
    await row.find('.kv-v').setValue('Rabies + DHPP current')
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    expect(api.createContact).toHaveBeenCalledWith(expect.objectContaining({
      requirements: [{ label: 'Vaccinations', detail: 'Rabies + DHPP current' }],
    }))
  })

  it('adds an hours row and sends it on save (#42 follow-up)', async () => {
    api.createContact.mockResolvedValue({ ...CEDAR })
    const w = mount(ContactSheet, opts)
    await flushPromises()
    await w.find('[data-testid="contact-name-input"]').setValue('Oak Hollow Animal Clinic')
    await w.find('[data-testid="add-hours"]').trigger('click')
    const row = w.find('[data-testid="hours-row"]')
    await row.find('.kv-k').setValue('Open')
    await row.find('.kv-v').setValue('Mon-Fri 7am-7pm')
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    expect(api.createContact).toHaveBeenCalledWith(expect.objectContaining({
      hours: [{ label: 'Open', detail: 'Mon-Fri 7am-7pm' }],
    }))
  })

  it('prefills hours rows when editing (#42 follow-up)', async () => {
    const vet = { ...CEDAR, id: 'cv', hours: [{ label: 'Open', detail: 'Mon-Fri 7-7' }] }
    const w = await mountEditing(vet)
    const rows = w.findAll('[data-testid="hours-row"]')
    expect(rows.length).toBe(1)
    expect((rows[0].find('.kv-k').element as HTMLInputElement).value).toBe('Open')
  })

  it('prefills requirement rows when editing a facility (#42)', async () => {
    const boarding = {
      ...CEDAR, id: 'c9', name: 'Happy Tails', role: 'boarding',
      requirements: [{ label: 'Fecal test', detail: 'Negative within 30 days' }],
    }
    const w = await mountEditing(boarding)
    const rows = w.findAll('[data-testid="requirement-row"]')
    expect(rows.length).toBe(1)
    expect((rows[0].find('.kv-k').element as HTMLInputElement).value).toBe('Fecal test')
  })

  it('hides the import box unless AI ingestion is configured (#42)', async () => {
    api.ingestion.mockResolvedValue({ enabled: false, has_key: false, provider: 'anthropic', model: 'x' })
    const w = mount(ContactSheet, opts)
    await flushPromises()
    expect(w.find('[data-testid="contact-import-url"]').exists()).toBe(false)
  })

  it('imports from a website: prefills fields + requirements (#42)', async () => {
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'claude-sonnet-5' })
    api.parseContactUrl.mockResolvedValue({
      provider: 'anthropic', model: 'claude-sonnet-5',
      fields: {
        name: 'Happy Tails Boarding', role: 'boarding', doctor: 'Jane Doe', phone: '555-0100', email: null,
        address: '9 Kennel Ln', notes: 'Ample parking',
        requirements: [
          { label: 'Vaccinations', detail: 'Rabies, DHPP, Bordetella current' },
          { label: 'Fecal test', detail: 'Negative within 30 days' },
        ],
        hours: [
          { label: 'Open', detail: 'Mon-Fri 7am-7pm' },
          { label: 'Boarding pick-up', detail: 'by 12pm' },
        ],
      },
    })
    api.createContact.mockResolvedValue({ ...CEDAR })
    const w = mount(ContactSheet, opts)
    await flushPromises()
    // the import box is shown; fetch a URL
    const urlInput = w.find('[data-testid="contact-import-url"]')
    expect(urlInput.exists()).toBe(true)
    await urlInput.setValue('https://happytails.example/policies')
    await w.find('[data-testid="contact-import-btn"]').trigger('click')
    await flushPromises()
    expect(api.parseContactUrl).toHaveBeenCalledWith('https://happytails.example/policies')
    // fields pre-filled, website set to the pasted URL, requirement + hours rows added
    expect((w.find('[data-testid="contact-name-input"]').element as HTMLInputElement).value).toBe('Happy Tails Boarding')
    expect(w.findAll('[data-testid="requirement-row"]').length).toBe(2)
    expect(w.findAll('[data-testid="hours-row"]').length).toBe(2)
    // saving carries the imported requirements + hours + website
    await w.find('[data-testid="contact-save"]').trigger('click')
    await flushPromises()
    const payload = api.createContact.mock.calls[0][0]
    expect(payload.website).toBe('https://happytails.example/policies')
    // the role and contact person the parse found (a daycare website, 2026-10-11)
    expect(payload.role).toBe('boarding')
    expect(payload.doctor).toBe('Jane Doe')
    expect(payload.requirements.map((r) => r.label)).toEqual(['Vaccinations', 'Fecal test'])
    expect(payload.hours.map((h) => h.label)).toEqual(['Open', 'Boarding pick-up'])
  })

  it('keeps a role the person already picked when importing', async () => {
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'x' })
    api.parseContactUrl.mockResolvedValue({
      provider: 'anthropic', model: 'x',
      fields: { name: 'X', role: 'boarding', doctor: null, phone: null, email: null, address: null, notes: null, requirements: [], hours: [] },
    })
    const w = mount(ContactSheet, opts)
    await flushPromises()
    await w.find('[data-testid="contact-role-select"]').setValue('groomer')
    await w.find('[data-testid="contact-doctor-input"]').setValue('Dr. Typed')
    await w.find('[data-testid="contact-import-url"]').setValue('https://x.example')
    await w.find('[data-testid="contact-import-btn"]').trigger('click')
    await flushPromises()
    expect((w.find('[data-testid="contact-role-select"]').element as HTMLSelectElement).value).toBe('groomer')
    expect((w.find('[data-testid="contact-doctor-input"]').element as HTMLInputElement).value).toBe('Dr. Typed')
  })

  it('surfaces an import error (#42)', async () => {
    api.ingestion.mockResolvedValue({ enabled: true, has_key: true, provider: 'anthropic', model: 'x' })
    api.parseContactUrl.mockRejectedValue(new Error('that address is not allowed'))
    const w = mount(ContactSheet, opts)
    await flushPromises()
    await w.find('[data-testid="contact-import-url"]').setValue('http://127.0.0.1/')
    await w.find('[data-testid="contact-import-btn"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('that address is not allowed')
  })
})

describe('Contacts view (#20)', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('groups contacts by role and opens the contact sheet on a row tap', async () => {
    api.contacts.mockResolvedValue([
      { ...CEDAR },
      { ...CEDAR, id: 'c2', name: 'Corner Groomers', role: 'groomer', doctor: null },
    ])
    const w = mount(Contacts, opts)
    await flushPromises()
    expect(w.text()).toContain('Vet')
    expect(w.text()).toContain('Groomer')
    expect(w.findAll('[data-testid="contact-row"]').length).toBe(2)
    // tapping a row opens the contact sheet with that contact
    await w.findAll('[data-testid="contact-row"]')[0].trigger('click')
    const sheet = w.findComponent(ContactSheet)
    expect(sheet.exists()).toBe(true)
    expect(sheet.props('contact')).toMatchObject({ id: 'c1' })
  })

  it('shows an empty state and opens the add sheet', async () => {
    api.contacts.mockResolvedValue([])
    const w = mount(Contacts, opts)
    await flushPromises()
    expect(w.text()).toContain('No contacts yet')
    // Adding lives in the speed-dial (#77), not a page button.
    expect(w.find('[data-testid="add-contact"]').exists()).toBe(false)
    expect(pageActions.value.map((a) => a.label)).toEqual(['Add a contact'])
    await runDial('add-contact')
    const sheet = w.findComponent(ContactSheet)
    expect(sheet.exists()).toBe(true)
    expect(sheet.props('contact')).toBe(null)
  })

  it('reloads the list after a contact is saved from the add sheet', async () => {
    api.contacts.mockResolvedValue([])
    const w = mount(Contacts, opts)
    await flushPromises()
    await runDial('add-contact')
    const sheet = w.findComponent(ContactSheet)
    sheet.vm.$emit('saved', { ...CEDAR })
    await flushPromises()
    expect(api.contacts).toHaveBeenCalledTimes(2) // initial load + reload after save
    expect(w.findComponent(ContactSheet).exists()).toBe(false) // sheet closed
  })

  it('reloads the list after a contact is deleted from the edit sheet', async () => {
    api.contacts.mockResolvedValue([{ ...CEDAR }])
    const w = mount(Contacts, opts)
    await flushPromises()
    await w.findAll('[data-testid="contact-row"]')[0].trigger('click')
    const sheet = w.findComponent(ContactSheet)
    sheet.vm.$emit('deleted', 'c1')
    await flushPromises()
    expect(api.contacts).toHaveBeenCalledTimes(2)
    expect(w.findComponent(ContactSheet).exists()).toBe(false)
  })

  it('surfaces a load error', async () => {
    api.contacts.mockRejectedValue(new Error('contacts unreachable'))
    const w = mount(Contacts, opts)
    await flushPromises()
    expect(w.text()).toContain('contacts unreachable')
  })
})

describe('Pet care team (#20)', () => {
  beforeEach(() => { vi.clearAllMocks() })

  function mountPetWith(contacts) {
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, records: [], weights: [], reminders: [], contacts,
    })
    return mount(Pet, { props: { id: 'biscuit' }, ...opts })
  }

  it('lists the care team and unlinks a contact', async () => {
    api.contacts.mockResolvedValue([{ ...CEDAR }])
    api.unlinkPetContact.mockResolvedValue(null)
    const w = mountPetWith([{ ...CEDAR }])
    await flushPromises()
    expect(w.find('[data-testid="care-team"]').text()).toContain('Cedar Creek')
    expect(w.findAll('[data-testid="care-contact"]').length).toBe(1)
    await w.find('[data-testid="care-contact"] .fx').trigger('click')
    await flushPromises()
    expect(api.unlinkPetContact).toHaveBeenCalledWith('biscuit', 'c1')
    expect(api.pet).toHaveBeenCalledTimes(2) // reloaded after unlink
  })

  it('links an existing contact via the picker', async () => {
    // one contact exists but is NOT yet on the care team → offered in the picker
    api.contacts.mockResolvedValue([{ ...CEDAR }])
    api.linkPetContact.mockResolvedValue([{ ...CEDAR }])
    const w = mountPetWith([])
    await flushPromises()
    const picker = w.find('.ctlink')
    expect(picker.exists()).toBe(true)
    await picker.setValue('c1')
    await flushPromises()
    expect(api.linkPetContact).toHaveBeenCalledWith('biscuit', 'c1')
  })

  it('adds a new contact and links it in one step', async () => {
    api.contacts.mockResolvedValue([])
    api.createContact.mockResolvedValue({ ...CEDAR })
    api.linkPetContact.mockResolvedValue([{ ...CEDAR }])
    const w = mountPetWith([])
    await flushPromises()
    await w.find('[data-testid="add-care-contact"]').trigger('click')
    const sheet = w.findComponent(ContactSheet)
    expect(sheet.exists()).toBe(true)
    // simulate the sheet saving a newly-created contact
    sheet.vm.$emit('saved', { ...CEDAR })
    await flushPromises()
    expect(api.linkPetContact).toHaveBeenCalledWith('biscuit', 'c1')
  })

  it('edits a care-team contact and reloads on save, and on delete', async () => {
    api.contacts.mockResolvedValue([{ ...CEDAR }])
    api.updateContact.mockResolvedValue({ ...CEDAR })
    const w = mountPetWith([{ ...CEDAR }])
    await flushPromises()
    // tapping the row opens the contact sheet for that contact
    await w.find('[data-testid="care-contact"]').trigger('click')
    let sheet = w.findComponent(ContactSheet)
    expect(sheet.props('contact')).toMatchObject({ id: 'c1' })
    sheet.vm.$emit('saved', { ...CEDAR })
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(2) // reloaded after edit
    // and a delete from the sheet also reloads
    await w.find('[data-testid="care-contact"]').trigger('click')
    sheet = w.findComponent(ContactSheet)
    sheet.vm.$emit('deleted', 'c1')
    await flushPromises()
    expect(api.pet).toHaveBeenCalledTimes(3)
  })

  it('surfaces an error when unlinking fails', async () => {
    api.contacts.mockResolvedValue([{ ...CEDAR }])
    api.unlinkPetContact.mockRejectedValue(new Error('link busy'))
    const w = mountPetWith([{ ...CEDAR }])
    await flushPromises()
    await w.find('[data-testid="care-contact"] .fx').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="care-team"]').text()).toContain('link busy')
  })

  it('shows the linked clinic on a timeline record', async () => {
    api.contacts.mockResolvedValue([])
    api.pet.mockResolvedValue({
      ...BISCUIT, gotcha_date: null, weights: [], reminders: [], contacts: [{ ...CEDAR }],
      records: [{
        id: 1, title: 'Annual exam', occurred_on: '2026-07-02', category: 'Appointments',
        cost_cents: null, contact_id: 'c1', contact: { ...CEDAR }, attachments: [],
      }],
    })
    const w = mount(Pet, { props: { id: 'biscuit' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="timeline-event"]').text()).toContain('Cedar Creek')
  })
})

// ---------- PetSheet: the profile editor (#63) ----------
// Before this existed, microchip / birth date / gotcha day could only be set at
// creation and were then frozen — updatePet was never called from the UI.

describe('PetSheet', () => {
  const PET = {
    id: 'pedro', name: 'Pedro', species: 'Dog', breed: 'Mix, Australian Shep',
    color: 'Blue Merle', sex: 'male_neutered', birth_date: '2024-05-06',
    gotcha_date: null, microchip: '000000000000202', weight: null, weight_unit: null,
    allergies: [{ label: 'Chicken', detail: 'hives' }],
    insurer: null, insurer_reimburse: null, gradient_a: null, gradient_b: null,
    photo_path: null, archived_at: null, memorial_note: null,
    created_by: null, created_at: '', updated_at: '',
  }

  it('pre-fills every standing field from the pet', () => {
    const w = mount(PetSheet, { props: { pet: PET } })
    expect((w.find('[data-testid="pet-name-input"]').element as HTMLInputElement).value).toBe('Pedro')
    expect((w.find('[data-testid="pet-color-input"]').element as HTMLInputElement).value).toBe('Blue Merle')
    expect((w.find('[data-testid="pet-microchip-input"]').element as HTMLInputElement).value).toBe('000000000000202')
    expect((w.find('[data-testid="pet-birth-input"]').element as HTMLInputElement).value).toBe('2024-05-06')
    expect(w.findAll('[data-testid="allergy-row"]')).toHaveLength(1)
  })

  it('sends null for a blanked field so a value can be CLEARED', async () => {
    api.updatePet.mockResolvedValue({ ...PET, microchip: null })
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.find('[data-testid="pet-microchip-input"]').setValue('')
    await w.find('[data-testid="pet-color-input"]').setValue('')
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    const [id, payload] = api.updatePet.mock.calls[0]
    expect(id).toBe('pedro')
    expect(payload.microchip).toBe(null)
    expect(payload.color).toBe(null)
    expect(payload.name).toBe('Pedro') // untouched fields still sent
    expect(w.emitted('saved')).toBeTruthy()
  })

  it('round-trips allergy rows and drops blank ones', async () => {
    api.updatePet.mockResolvedValue(PET)
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.find('[data-testid="add-allergy"]').trigger('click')
    const rows = w.findAll('[data-testid="allergy-row"]')
    expect(rows).toHaveLength(2)
    // The new row is left blank; it must not reach the API.
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    expect(api.updatePet.mock.calls[0][1].allergies).toEqual([{ label: 'Chicken', detail: 'hives' }])
  })

  it('removing every allergy sends an empty list (the form\'s "No")', async () => {
    api.updatePet.mockResolvedValue(PET)
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.findAll('[data-testid="allergy-row"]')[0].find('.fx').trigger('click')
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    expect(api.updatePet.mock.calls[0][1].allergies).toEqual([])
  })

  it('refuses to save a blank name and never calls the API', async () => {
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.find('[data-testid="pet-name-input"]').setValue('   ')
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    expect(api.updatePet).not.toHaveBeenCalled()
    expect(w.text()).toContain('Give your pet a name')
  })

  it('keeps a species that is not in the preset list', () => {
    const w = mount(PetSheet, { props: { pet: { ...PET, species: 'Axolotl' } } })
    const opts = w.find('[data-testid="pet-species-select"]').findAll('option').map((o) => o.text())
    expect(opts[0]).toBe('Axolotl') // not silently rewritten to "Dog"
  })

  it('edits every remaining profile field and sends them all', async () => {
    api.updatePet.mockResolvedValue(PET)
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.find('[data-testid="pet-breed-input"]').setValue('Australian Shepherd')
    await w.find('[data-testid="pet-sex-input"]').setValue('female_spayed')
    await w.find('[data-testid="pet-gotcha-input"]').setValue('2024-08-01')
    await w.find('[data-testid="pet-birth-input"]').setValue('2024-06-03')
    await w.find('[data-testid="pet-species-select"]').setValue('Cat')
    // Edit the allergen and reaction inputs in place.
    const row = w.findAll('[data-testid="allergy-row"]')[0]
    await row.find('.kv-k').setValue('Salmon')
    await row.find('.kv-v').setValue('itchy paws')
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    expect(api.updatePet.mock.calls[0][1]).toMatchObject({
      breed: 'Australian Shepherd', sex: 'female_spayed',
      gotcha_date: '2024-08-01', birth_date: '2024-06-03', species: 'Cat',
      allergies: [{ label: 'Salmon', detail: 'itchy paws' }],
    })
  })

  it('Cancel closes without saving', async () => {
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.find('[data-testid="pet-cancel"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
    expect(api.updatePet).not.toHaveBeenCalled()
  })

  it('surfaces an API error instead of closing', async () => {
    api.updatePet.mockRejectedValue(new Error('nope'))
    const w = mount(PetSheet, { props: { pet: PET } })
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('nope')
    expect(w.emitted('saved')).toBeFalsy()
  })
})

// ---------- RegimenSheet: one form for food, preventatives, meds (#63) ----------

describe('RegimenSheet', () => {
  const TRIO = {
    id: 'r1', pet_id: 'pedro', kind: 'preventative', product: 'Simparica Trio',
    amount: '1 pill', frequency: 'monthly', protects: ['flea', 'tick', 'heartworm'],
    started_on: '2026-01-01', ended_on: null, notes: 'Given with food',
    created_by: null, created_at: '', updated_at: '',
  }
  const base = { petId: 'pedro', petName: 'Pedro' }

  it('opens an existing entry read-only, then edits in place (#44 shape)', async () => {
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    expect(w.find('[data-testid="regimen-view"]').exists()).toBe(true)
    expect(w.text()).toContain('Pedro is currently on this')
    expect(w.text()).toContain('Heartworm')
    expect(w.find('[data-testid="regimen-product-input"]').exists()).toBe(false)
    await w.find('[data-testid="regimen-edit"]').trigger('click')
    expect(w.find('[data-testid="regimen-product-input"]').exists()).toBe(true)
    // Cancel returns to the view rather than closing (the view is the rest state).
    await w.find('[data-testid="regimen-cancel"]').trigger('click')
    expect(w.find('[data-testid="regimen-view"]').exists()).toBe(true)
    expect(w.emitted('close')).toBeFalsy()
  })

  it('the view marks protections as coverage, matching the form toggles', async () => {
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    const chips = w.findAll('[data-testid="regimen-view"] .reqchip')
    expect(chips).toHaveLength(3)
    for (const c of chips) expect(c.classes()).toContain('prot')
  })

  it('a new entry opens straight in the form and Cancel closes it', async () => {
    const w = mount(RegimenSheet, { props: base })
    expect(w.find('[data-testid="regimen-view"]').exists()).toBe(false)
    await w.find('[data-testid="regimen-cancel"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
  })

  it('shows protections only for a preventative', async () => {
    const w = mount(RegimenSheet, { props: base })
    // Default kind is food — no protections to pick.
    expect(w.find('[data-testid="regimen-protects"]').exists()).toBe(false)
    await w.find('.kindbtn[data-kind="preventative"]').trigger('click')
    expect(w.find('[data-testid="regimen-protects"]').exists()).toBe(true)
  })

  it('saves one product covering several protections', async () => {
    api.createRegimen.mockResolvedValue(TRIO)
    const w = mount(RegimenSheet, { props: base })
    await w.find('.kindbtn[data-kind="preventative"]').trigger('click')
    await w.find('[data-testid="regimen-product-input"]').setValue('Simparica Trio')
    await w.find('[data-testid="regimen-amount-input"]').setValue('1 pill')
    await w.find('[data-testid="regimen-frequency-input"]').setValue('monthly')
    for (const p of ['flea', 'tick', 'heartworm']) {
      await w.find(`.protbtn[data-protection="${p}"]`).trigger('click')
    }
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    const [petId, payload] = api.createRegimen.mock.calls[0]
    expect(petId).toBe('pedro')
    expect(payload).toMatchObject({
      kind: 'preventative', product: 'Simparica Trio', amount: '1 pill',
      frequency: 'monthly', protects: ['flea', 'tick', 'heartworm'], ended_on: null,
    })
    expect(w.emitted('saved')).toBeTruthy()
  })

  it('clears protections when the kind is switched away from preventative', async () => {
    api.updateRegimen.mockResolvedValue(TRIO)
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    await w.find('[data-testid="regimen-edit"]').trigger('click')
    await w.find('.kindbtn[data-kind="food"]').trigger('click')
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    // A food that protects against heartworm is a data-entry mistake.
    expect(api.updateRegimen.mock.calls[0][1].protects).toBe(null)
  })

  it('toggling a protection off removes it', async () => {
    api.updateRegimen.mockResolvedValue(TRIO)
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    await w.find('[data-testid="regimen-edit"]').trigger('click')
    await w.find('.protbtn[data-protection="tick"]').trigger('click')
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(api.updateRegimen.mock.calls[0][1].protects).toEqual(['flea', 'heartworm'])
  })

  it('"currently on this" is derived from ended_on, both ways', async () => {
    api.updateRegimen.mockResolvedValue(TRIO)
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    await w.find('[data-testid="regimen-edit"]').trigger('click')
    // Ticked (no end date) → unticking stamps today and reveals the date field.
    expect(w.find('[data-testid="regimen-ended-input"]').exists()).toBe(false)
    await w.find('[data-testid="regimen-still-on"]').setValue(false)
    expect(w.find('[data-testid="regimen-ended-input"]').exists()).toBe(true)
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(api.updateRegimen.mock.calls[0][1].ended_on).toBe(new Date().toISOString().slice(0, 10))
  })

  it('re-ticking "currently on this" clears the end date (back on it)', async () => {
    const stopped = { ...TRIO, ended_on: '2026-02-01' }
    api.updateRegimen.mockResolvedValue(stopped)
    const w = mount(RegimenSheet, { props: { ...base, regimen: stopped } })
    await w.find('[data-testid="regimen-edit"]').trigger('click')
    await w.find('[data-testid="regimen-still-on"]').setValue(true)
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(api.updateRegimen.mock.calls[0][1].ended_on).toBe(null)
  })

  it('refuses a blank product and names the field for food', async () => {
    const w = mount(RegimenSheet, { props: base })
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(api.createRegimen).not.toHaveBeenCalled()
    expect(w.text()).toContain('Name the food')
  })

  it('deletes after confirmation only', async () => {
    api.deleteRegimen.mockResolvedValue(undefined)
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    await w.find('[data-testid="regimen-edit"]').trigger('click')
    await w.find('[data-testid="regimen-delete"]').trigger('click')
    expect(api.deleteRegimen).not.toHaveBeenCalled()
    confirm.mockReturnValue(true)
    await w.find('[data-testid="regimen-delete"]').trigger('click')
    await flushPromises()
    expect(api.deleteRegimen).toHaveBeenCalledWith('r1')
    expect(w.emitted('deleted')).toBeTruthy()
    confirm.mockRestore()
  })

  it('Close on the view emits close without editing', async () => {
    const w = mount(RegimenSheet, { props: { ...base, regimen: TRIO } })
    await w.find('[data-testid="regimen-close"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
    expect(api.updateRegimen).not.toHaveBeenCalled()
  })

  it('carries start date and notes through, and renders notes as markdown', async () => {
    api.createRegimen.mockResolvedValue(TRIO)
    const w = mount(RegimenSheet, { props: base })
    await w.find('[data-testid="regimen-product-input"]').setValue('Kibble')
    await w.find('input[type="date"]').setValue('2026-03-01')
    await w.find('textarea').setValue('**Twice** daily')
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(api.createRegimen.mock.calls[0][1]).toMatchObject({
      started_on: '2026-03-01', notes: '**Twice** daily',
    })
    // The view renders those notes through the markdown renderer.
    const v = mount(RegimenSheet, { props: { ...base, regimen: { ...TRIO, notes: '**Twice** daily' } } })
    expect(v.find('[data-testid="regimen-view"]').html()).toContain('<strong>Twice</strong>')
  })

  it('labels the product field per kind and prompts for a product, not a food', async () => {
    const w = mount(RegimenSheet, { props: base })
    expect(w.text()).toContain('Food') // default kind
    await w.find('.kindbtn[data-kind="medication"]').trigger('click')
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Name the product')
  })

  it('surfaces an API error instead of closing', async () => {
    api.createRegimen.mockRejectedValue(new Error('boom'))
    const w = mount(RegimenSheet, { props: base })
    await w.find('[data-testid="regimen-product-input"]').setValue('Kibble')
    await w.find('[data-testid="regimen-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('boom')
    expect(w.emitted('saved')).toBeFalsy()
  })
})

describe('Archiving a pet (#3, #37)', () => {
  const KOA = {
    id: 'koa', name: 'Koa', species: 'Horse', breed: 'Dutch Warmblood', color: null, sex: 'male_neutered',
    birth_date: null, gotcha_date: null, microchip: null, weight: null, weight_unit: null, allergies: null,
    insurer: null, insurer_reimburse: null, gradient_a: null, gradient_b: null, photo_path: null,
    archived_at: null, archive_kind: null, memorial_note: null, created_by: null, created_at: '', updated_at: '',
  }
  const DETAIL = { records: [], weights: [], reminders: [], contacts: [], insurance_policies: [], regimens: [], next_reminder: null }

  it('archives as passed by default, with today as the date', async () => {
    api.archivePet.mockResolvedValue({ ...KOA, archived_at: '2026-10-11', archive_kind: 'passed' })
    const w = mount(ArchiveSheet, { props: { pet: KOA } })
    expect(w.text()).toContain('Is Koa no longer with you?')
    expect(w.text()).toContain('Date of passing')
    await w.find('[data-testid="archive-save"]').trigger('click')
    await flushPromises()
    const [id, payload] = api.archivePet.mock.calls[0]
    expect(id).toBe('koa')
    expect(payload.kind).toBe('passed')
    expect(payload.archived_at).toMatch(/^\d{4}-\d{2}-\d{2}$/)
    expect(payload.memorial_note).toBe(null)
    expect(w.emitted('saved')).toBeTruthy()
  })

  it('rehomes with no memorial wording', async () => {
    api.archivePet.mockResolvedValue({ ...KOA, archived_at: '2026-08-01', archive_kind: 'rehomed' })
    const w = mount(ArchiveSheet, { props: { pet: KOA } })
    await w.find('[data-testid="archive-kind-rehomed"] input').setValue(true)
    expect(w.text()).toContain('Date they moved on')
    expect(w.text()).toContain('Where they went')
    expect(w.find('[data-testid="archive-save"]').text()).toBe('Move to past companions')
    await w.find('[data-testid="archive-date"]').setValue('2026-08-01')
    await w.find('[data-testid="archive-note"]').setValue('  Retired to a trainer in Riverbend ')
    await w.find('[data-testid="archive-save"]').trigger('click')
    await flushPromises()
    expect(api.archivePet.mock.calls[0][1]).toEqual({
      kind: 'rehomed', archived_at: '2026-08-01', memorial_note: 'Retired to a trainer in Riverbend',
    })
  })

  it('edits an existing archive, pre-filled, and surfaces an API error', async () => {
    api.archivePet.mockRejectedValue(new Error('archived_at must be an ISO date'))
    const pet = { ...KOA, archived_at: '2026-10-20', archive_kind: 'passed', memorial_note: 'A good boy.' }
    const w = mount(ArchiveSheet, { props: { pet } })
    expect(w.text()).toContain("Update Koa's page")
    expect((w.find('[data-testid="archive-date"]').element as HTMLInputElement).value).toBe('2026-10-20')
    expect((w.find('[data-testid="archive-note"]').element as HTMLTextAreaElement).value).toBe('A good boy.')
    expect(w.find('[data-testid="archive-save"]').text()).toBe('Save changes')
    await w.find('[data-testid="archive-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('archived_at must be an ISO date')
    expect(w.emitted('saved')).toBeFalsy()
    await w.find('[data-testid="archive-cancel"]').trigger('click')
    expect(w.emitted('close')).toBeTruthy()
  })

  it('offers the archive hand-off only for an active pet', async () => {
    const w = mount(PetSheet, { props: { pet: KOA } })
    await w.find('[data-testid="pet-archive-open"]').trigger('click')
    expect(w.emitted('archive')).toBeTruthy()
    const archived = mount(PetSheet, { props: { pet: { ...KOA, archived_at: '2026-08-01', archive_kind: 'rehomed' } } })
    expect(archived.find('[data-testid="pet-archive-open"]').exists()).toBe(false)
  })

  it('opens the archive sheet from the profile editor and reloads after saving', async () => {
    api.pet.mockResolvedValue({ ...KOA, ...DETAIL })
    api.archivePet.mockResolvedValue({ ...KOA, archived_at: '2026-08-01', archive_kind: 'rehomed' })
    const w = mount(Pet, { props: { id: 'koa' }, ...opts })
    await flushPromises()
    await runDial('edit-pet')
    await w.find('[data-testid="pet-archive-open"]').trigger('click')
    expect(w.find('[data-testid="pet-sheet"]').exists()).toBe(false) // one sheet at a time
    expect(w.find('[data-testid="archive-sheet"]').exists()).toBe(true)
    api.pet.mockResolvedValue({ ...KOA, ...DETAIL, archived_at: '2026-08-01', archive_kind: 'rehomed' })
    await w.find('[data-testid="archive-save"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="archive-sheet"]').exists()).toBe(false)
    expect(w.find('[data-testid="archived-banner"]').exists()).toBe(true)
  })

  it('shows a rehomed banner in place of Next up, and brings the pet back', async () => {
    api.pet.mockResolvedValue({ ...KOA, ...DETAIL, archived_at: '2026-08-01', archive_kind: 'rehomed', memorial_note: 'Retired to a trainer in Riverbend', reminders: [{ id: 9, pet_id: 'koa', due_on: '2027-01-01', title: 'Coggins', kind: null, done: false, source: 'manual', created_at: '' }] })
    const w = mount(Pet, { props: { id: 'koa' }, ...opts })
    await flushPromises()
    const banner = w.find('[data-testid="archived-banner"]')
    expect(banner.attributes('data-kind')).toBe('rehomed')
    expect(banner.text()).toContain('Moved on · Aug 1, 2026')
    expect(banner.text()).not.toContain('memory')
    expect(w.find('[data-testid="archived-note"]').text()).toBe('Retired to a trainer in Riverbend')
    expect(w.find('[data-testid="next-up"]').exists()).toBe(false)
    expect(dialKeys()).toEqual(['edit-pet']) // still editable (#37), from the speed-dial (#77)
    expect(w.find('[data-testid="reminders-paused"]').exists()).toBe(true) // not "1 upcoming"
    expect(w.text()).not.toContain('upcoming')

    await w.find('[data-testid="archived-edit"]').trigger('click')
    expect(w.find('[data-testid="archive-sheet"]').exists()).toBe(true)
    await w.find('[data-testid="archive-cancel"]').trigger('click')

    api.unarchivePet.mockResolvedValue({ ...KOA })
    api.pet.mockResolvedValue({ ...KOA, ...DETAIL, reminders: [{ id: 9, pet_id: 'koa', due_on: '2027-01-01', title: 'Coggins', kind: null, done: false, source: 'manual', created_at: '' }] })
    await w.find('[data-testid="archived-restore"]').trigger('click')
    await flushPromises()
    expect(api.unarchivePet).toHaveBeenCalledWith('koa')
    expect(w.text()).toContain('1 upcoming')
    expect(w.find('[data-testid="archived-banner"]').exists()).toBe(false)
    expect(w.find('[data-testid="next-up"]').exists()).toBe(true)
  })

  it('uses memorial wording for a pet that passed, and reports a failed restore', async () => {
    api.pet.mockResolvedValue({ ...KOA, ...DETAIL, archived_at: '2026-10-20', archive_kind: 'passed', memorial_note: null })
    api.unarchivePet.mockRejectedValue(new Error('nope'))
    const w = mount(Pet, { props: { id: 'koa' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="archived-banner"]').text()).toContain('In loving memory · Oct 20, 2026')
    await w.find('[data-testid="archived-restore"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="archived-banner"]').text()).toContain('nope')
  })

  it('splits Home into Remembering and Past companions, both linking to the pet', async () => {
    api.pets.mockResolvedValue([
      { ...BISCUIT },
      { id: 'rocco', name: 'Rocco', species: 'Cat', next_reminder: null, archived_at: '2026-10-20', archive_kind: 'passed', memorial_note: null },
      { id: 'koa', name: 'Koa', species: 'Horse', next_reminder: null, archived_at: '2026-08-01', archive_kind: 'rehomed', memorial_note: null, photo_path: 'photos/koa/x.jpg' },
    ])
    const w = mount(Home, opts)
    await flushPromises()
    const remember = w.findAll('[data-testid="home-remember-card"]')
    const past = w.findAll('[data-testid="home-past-card"]')
    expect(remember.map((c) => c.text())).toEqual([expect.stringContaining('Rocco')])
    expect(past).toHaveLength(1)
    expect(past[0].text()).toContain('Moved on Aug 1, 2026')
    expect(past[0].attributes('href')).toBe('/pets/koa')
    expect(remember[0].attributes('href')).toBe('/pets/rocco')
    expect(w.text()).toContain('Past companions')
    // An archived pet with a portrait shows it instead of the initial; one
    // without keeps the initial.
    expect(past[0].find('[data-testid="home-archived-photo"]').attributes('src')).toBe('/api/pets/koa/photo?v=photos/koa/x.jpg')
    expect(past[0].find('.init').exists()).toBe(false)
    expect(remember[0].find('[data-testid="home-archived-photo"]').exists()).toBe(false)
    expect(remember[0].find('.init').text()).toBe('R')
  })
})

// ---------- Sex as a closed set + life stages (#72, #73) ----------

describe('Sex and life stage', () => {
  afterEach(() => setDisplay({ breeder_terms: false }))

  it('the add-pet picker words the five values for the chosen species', async () => {
    const w = mount(AddPetSheet, opts)
    const labels = () => w.find('[data-testid="add-pet-sex"]').findAll('option').map((o) => o.text())
    expect(labels()).toEqual(['Not recorded', 'Male', 'Female', 'Neutered male', 'Spayed female', 'Unknown'])
    await w.find('[data-testid="add-pet-species"]').setValue('Horse')
    expect(labels()).toEqual(['Not recorded', 'Stallion', 'Mare', 'Gelding', 'Spayed mare', 'Unknown'])
    setDisplay({ breeder_terms: true })
    await w.find('[data-testid="add-pet-species"]').setValue('Cat')
    expect(labels().slice(1, 3)).toEqual(['Tom', 'Queen'])
  })

  it('the profile editor pre-selects the stored value and sends null for "Not recorded"', async () => {
    const pet = {
      id: 'koa', name: 'Koa', species: 'Horse', breed: null, color: null, sex: 'male_neutered',
      birth_date: null, gotcha_date: null, microchip: null, weight: null, weight_unit: null, allergies: null,
      insurer: null, insurer_reimburse: null, gradient_a: null, gradient_b: null, photo_path: null,
      archived_at: null, archive_kind: null, memorial_note: null, created_by: null, created_at: '', updated_at: '',
    }
    api.updatePet.mockResolvedValue(pet)
    const w = mount(PetSheet, { props: { pet } })
    const sel = w.find('[data-testid="pet-sex-input"]')
    expect((sel.element as HTMLSelectElement).value).toBe('male_neutered')
    expect(sel.find('option:checked').text()).toBe('Gelding')
    await sel.setValue('')
    await w.find('[data-testid="pet-save"]').trigger('click')
    await flushPromises()
    expect(api.updatePet.mock.calls[0][1].sex).toBe(null)
  })

  it('the pet card shows a life-stage tag, and none for a pet that has passed', () => {
    const young = new Date()
    young.setMonth(young.getMonth() - 3)
    const born = young.toISOString().slice(0, 10)
    const w = mount(PetCard, { props: { pet: { ...BISCUIT, birth_date: born, sex: 'female' } }, ...opts })
    expect(w.find('[data-testid="pet-card-stage"]').text()).toBe('Puppy')
    const gone = mount(PetCard, { props: { pet: { ...BISCUIT, birth_date: born, archived_at: '2026-01-01' } }, ...opts })
    expect(gone.find('[data-testid="pet-card-stage"]').exists()).toBe(false)
  })

  const LULU = {
    ...BISCUIT, id: 'lulu', name: 'Lulu', species: 'Dog', sex: 'female', birth_date: '2025-11-04',
    gotcha_date: null, weights: [], reminders: [], contacts: [], insurance_policies: [], regimens: [],
    records: [{ id: 9, title: 'Spay', occurred_on: '2026-09-21', category: 'Appointments', attachments: [] }],
    sex_suggestion: { sex: 'female_spayed', record_id: 9, record_title: 'Spay', occurred_on: '2026-09-21' },
  }

  it('the profile hero words the sex for the species and offers the spay update', async () => {
    api.pet.mockResolvedValue(LULU)
    const w = mount(Pet, { props: { id: 'lulu' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="pet-sex-tag"]').text()).toBe('Female')
    expect(w.find('[data-testid="pet-life-stage"]').exists()).toBe(true)
    const chip = w.find('[data-testid="sex-suggestion"]')
    expect(chip.text()).toContain('Update to Spayed female?')
    expect(chip.text()).toContain('Spay · Sep 21, 2026')

    // The record link opens the record it came from.
    await w.find('[data-testid="sex-suggestion-record"]').trigger('click')
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(true)
    expect(w.text()).toContain('Spay')

    // One tap: PATCH the sex, then reload (the server stops suggesting it).
    api.updatePet.mockResolvedValue({ ...LULU, sex: 'female_spayed' })
    api.pet.mockResolvedValue({ ...LULU, sex: 'female_spayed', sex_suggestion: null })
    await w.find('[data-testid="sex-suggestion-accept"]').trigger('click')
    await flushPromises()
    expect(api.updatePet).toHaveBeenCalledWith('lulu', { sex: 'female_spayed' })
    expect(w.find('[data-testid="sex-suggestion"]').exists()).toBe(false)
    expect(w.find('[data-testid="pet-sex-tag"]').text()).toBe('Spayed female')
  })

  it('a horse\'s adult stage is its sex word, so the hero shows it once', async () => {
    api.pet.mockResolvedValue({ ...LULU, id: 'koa', name: 'Koa', species: 'Horse', sex: 'male_neutered', birth_date: '2015-04-01', records: [], sex_suggestion: null })
    const w = mount(Pet, { props: { id: 'koa' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="pet-sex-tag"]').text()).toBe('Gelding')
    expect(w.find('[data-testid="pet-life-stage"]').exists()).toBe(false)
  })

  it('hides an "unknown" sex on the hero, and uses breeder terms when the household asks', async () => {
    api.pet.mockResolvedValue({ ...LULU, sex: 'unknown', sex_suggestion: null })
    const w = mount(Pet, { props: { id: 'lulu' }, ...opts })
    await flushPromises()
    expect(w.find('[data-testid="pet-sex-tag"]').exists()).toBe(false)
    setDisplay({ breeder_terms: true })
    api.pet.mockResolvedValue({ ...LULU, sex_suggestion: null })
    const b = mount(Pet, { props: { id: 'lulu' }, ...opts })
    await flushPromises()
    expect(b.find('[data-testid="pet-sex-tag"]').text()).toBe('Bitch')
  })

  it('the suggestion chip shows the API error and stays put', async () => {
    api.updatePet.mockRejectedValue(new Error('nope'))
    const w = mount(SexSuggestionChip, { props: { pet: LULU } })
    await w.find('[data-testid="sex-suggestion-accept"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('nope')
    expect(w.emitted('updated')).toBeFalsy()
    api.updatePet.mockRejectedValue({})
    await w.find('[data-testid="sex-suggestion-accept"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Could not update.')
    const none = mount(SexSuggestionChip, { props: { pet: { ...LULU, sex_suggestion: null } } })
    expect(none.find('[data-testid="sex-suggestion"]').exists()).toBe(false)
  })

  it('Settings toggles breeder terms through the API and updates the shared store', async () => {
    api.ingestion.mockResolvedValue({ enabled: false, provider: 'anthropic', model: 'm', has_key: false })
    api.saveDisplaySettings.mockResolvedValue({ breeder_terms: true })
    const w = mount(Settings, opts)
    await flushPromises()
    const box = w.find('[data-testid="breeder-terms-toggle"]')
    expect((box.element as HTMLInputElement).checked).toBe(false)
    await box.setValue(true)
    await flushPromises()
    expect(api.saveDisplaySettings).toHaveBeenCalledWith({ breeder_terms: true })
    expect(display.breeder_terms).toBe(true)
    api.saveDisplaySettings.mockRejectedValue(new Error('offline'))
    await box.setValue(false)
    await flushPromises()
    expect(w.text()).toContain('offline')
    expect(display.breeder_terms).toBe(true) // unchanged: the server didn't take it
  })

  it('a startup load that lands after a save does not undo it', async () => {
    let answer: (d: { breeder_terms: boolean }) => void = () => {}
    api.displaySettings.mockReturnValue(new Promise((r) => { answer = r }))
    const loading = loadDisplay()
    setDisplay({ breeder_terms: true }) // the Settings toggle's confirmed save
    answer({ breeder_terms: false }) // the older GET arrives late
    await loading
    expect(display.breeder_terms).toBe(true)
  })

  it('loadDisplay reads the household setting and keeps the defaults on failure', async () => {
    api.displaySettings.mockResolvedValue({ breeder_terms: true })
    await loadDisplay()
    expect(display.breeder_terms).toBe(true)
    setDisplay({ breeder_terms: false })
    api.displaySettings.mockRejectedValue(new Error('down'))
    await loadDisplay()
    expect(display.breeder_terms).toBe(false)
  })
})
