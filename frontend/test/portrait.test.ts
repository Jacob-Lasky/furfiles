import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'

vi.mock('../src/api', () => ({
  api: {
    pets: vi.fn(), household: vi.fn(), setHouseholdPortrait: vi.fn(), deleteHouseholdPortrait: vi.fn(),
    householdPortraitUrl: vi.fn(), petPhotoUrl: vi.fn(), setPetPhoto: vi.fn(), reportDiag: vi.fn(),
  },
}))
import { api } from '../src/api'
import FamilyPortraitSheet from '../src/components/FamilyPortraitSheet.vue'
import PhotoEditorSheet from '../src/components/PhotoEditorSheet.vue'
import Home from '../src/views/Home.vue'
import AddPetSheet from '../src/components/AddPetSheet.vue'

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: '/', component: { template: '<div/>' } }, { path: '/pets/:id', component: { template: '<div/>' } }],
})
const opts = { global: { plugins: [router] } }

const pet = (id: string, name: string, extra = {}) => ({
  id, name, species: 'Dog', photo_path: null, gradient_a: '#F3B44C', gradient_b: '#E0762F',
  archived_at: null, archive_kind: null, next_reminder: null, ...extra,
})
const PEDRO = pet('pedro', 'Pedro', { photo_path: 'photos/pedro/a.jpg' })
const LULU = pet('lulu', 'Lulu')
const KIPPER = pet('kipper', 'Kipper', { species: 'Cat' })
const KOA = pet('koa', 'Koa', { archived_at: '2026-08-01', archive_kind: 'rehomed' })
const GENERATED = { source: 'generated', pet_ids: ['kipper', 'lulu', 'pedro'], version: 'v1', updated_at: '2026-10-11 10:00:00', roster_changed: false }
const UPLOADED = { ...GENERATED, source: 'uploaded', pet_ids: [], version: 'v2' }

let ctx
function fakeCtx() {
  const gradient = () => ({ addColorStop: vi.fn() })
  return {
    createLinearGradient: vi.fn(gradient), createRadialGradient: vi.fn(gradient),
    fillRect: vi.fn(), save: vi.fn(), restore: vi.fn(), beginPath: vi.fn(), closePath: vi.fn(),
    arc: vi.fn(), arcTo: vi.fn(), moveTo: vi.fn(), fill: vi.fn(), clip: vi.fn(), drawImage: vi.fn(),
    fillText: vi.fn(), measureText: vi.fn((t) => ({ width: t.length * 10 })),
    clearRect: vi.fn(), translate: vi.fn(), rotate: vi.fn(), scale: vi.fn(),
  }
}

enableAutoUnmount(afterEach)
beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
  api.petPhotoUrl.mockImplementation((p) => (p && p.photo_path ? `/api/pets/${p.id}/photo?v=${p.photo_path}` : null))
  api.householdPortraitUrl.mockImplementation((p) => (p ? `/api/household/portrait?v=${p.version}` : null))
  api.household.mockResolvedValue({ portrait: null })
  ctx = fakeCtx()
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(ctx)
  vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/jpeg;base64,QUJD')
  // jsdom never loads images; photos "load" at 512px, a src containing
  // "broken" fails, and SVG data URLs (species marks) load too.
  vi.stubGlobal('Image', class {
    width = 0; height = 0; naturalWidth = 0; naturalHeight = 0
    set src(v) {
      setTimeout(() => {
        if (String(v).includes('broken')) { this.onerror && this.onerror(); return }
        this.width = this.height = this.naturalWidth = this.naturalHeight = 512
        this.onload && this.onload()
      })
    }
  })
  try { localStorage.clear() } catch (_) { /* no storage */ }
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

async function settle(w, until: () => boolean = () => false) {
  for (let i = 0; i < 30 && !until(); i++) {
    await new Promise((r) => setTimeout(r))
    await flushPromises()
  }
  return w
}

describe('FamilyPortraitSheet (#75)', () => {
  it('composes everyone, notes who has no photo, and saves a generated portrait', async () => {
    api.setHouseholdPortrait.mockResolvedValue({ portrait: GENERATED })
    const w = mount(FamilyPortraitSheet, { props: { pets: [PEDRO, LULU, KIPPER] } })
    expect(w.text()).toContain('Gathering everyone')
    expect(w.find('[data-testid="portrait-save"]').attributes('disabled')).toBeDefined()
    await settle(w, () => !w.text().includes('Gathering'))

    expect(w.text()).toContain('No AI')
    expect(w.get('[data-testid="portrait-missing"]').text()).toContain('No profile photos to use for Lulu and Kipper')
    // Pedro's photo + Lulu's and Kipper's rasterised species marks.
    expect(ctx.drawImage).toHaveBeenCalledTimes(3)
    // The hidden marks are the real SpeciesMark, one per pet.
    expect(w.findAll('[data-pet]').map((e) => e.attributes('data-track'))).toEqual(['dog', 'dog', 'cat'])
    // Names are on by default; turning them off repaints without the pills.
    expect(ctx.fillText.mock.calls.map((c) => c[0])).toEqual(expect.arrayContaining(['Pedro', 'Lulu', 'Kipper']))
    ctx.fillText.mockClear()
    await w.get('[data-testid="portrait-names"]').setValue(false)
    expect(ctx.fillText.mock.calls.map((c) => c[0])).toEqual(['L', 'K'])

    await w.get('[data-testid="portrait-save"]').trigger('click')
    await flushPromises()
    expect(api.setHouseholdPortrait).toHaveBeenCalledWith({
      media_type: 'image/jpeg', data_base64: 'QUJD', source: 'generated', pet_ids: ['pedro', 'lulu', 'kipper'],
    })
    expect(w.emitted().saved[0]).toEqual([{ portrait: GENERATED }])
  })

  it('treats a photo that will not load like a missing one: mark drawn, and named', async () => {
    const w = mount(FamilyPortraitSheet, { props: { pets: [pet('rex', 'Rex', { photo_path: 'broken' }), LULU] } })
    await settle(w, () => !w.text().includes('Gathering'))
    // Both fall back to the colour portrait WITH its species mark.
    expect(ctx.drawImage).toHaveBeenCalledTimes(2)
    expect(w.get('[data-testid="portrait-missing"]').text()).toContain('for Rex and Lulu')
  })

  it('uses singular copy for one missing photo', async () => {
    const w = mount(FamilyPortraitSheet, { props: { pets: [PEDRO, LULU] } })
    await settle(w, () => !w.text().includes('Gathering'))
    expect(w.get('[data-testid="portrait-missing"]').text()).toBe(
      'No profile photo to use for Lulu, so a colour portrait stands in. Add one from their profile, then regenerate.',
    )
  })

  it('saves the roster it painted, even if the list changes while open', async () => {
    api.setHouseholdPortrait.mockResolvedValue({ portrait: GENERATED })
    const w = mount(FamilyPortraitSheet, { props: { pets: [PEDRO, LULU] } })
    await settle(w, () => !w.text().includes('Gathering'))
    await w.setProps({ pets: [PEDRO] })
    await w.get('[data-testid="portrait-save"]').trigger('click')
    await flushPromises()
    expect(api.setHouseholdPortrait.mock.calls[0][0].pet_ids).toEqual(['pedro', 'lulu'])
  })

  it('lists three or more missing photos with commas', async () => {
    const w = mount(FamilyPortraitSheet, { props: { pets: [LULU, KIPPER, pet('koa', 'Koa')] } })
    await settle(w, () => !w.text().includes('Gathering'))
    expect(w.text()).toContain('for Lulu, Kipper and Koa')
  })

  it('surfaces a save error and stays open; hands off to upload', async () => {
    api.setHouseholdPortrait.mockRejectedValue(new Error('no active pet \'lulu\''))
    const w = mount(FamilyPortraitSheet, { props: { pets: [PEDRO] } })
    await settle(w, () => !w.text().includes('Gathering'))
    expect(w.find('[data-testid="portrait-missing"]').exists()).toBe(false)
    await w.get('[data-testid="portrait-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain("no active pet 'lulu'")
    expect(w.emitted().saved).toBeUndefined()
    await w.get('[data-testid="portrait-use-photo"]').trigger('click')
    expect(w.emitted().upload).toBeTruthy()
  })

  it('falls back to a generic message for an error without one', async () => {
    api.setHouseholdPortrait.mockRejectedValue({})
    const w = mount(FamilyPortraitSheet, { props: { pets: [PEDRO] } })
    await settle(w, () => !w.text().includes('Gathering'))
    await w.get('[data-testid="portrait-save"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Could not save the portrait.')
  })

  it('degrades when the canvas has no 2D context', async () => {
    HTMLCanvasElement.prototype.getContext.mockReturnValue(null)
    const w = mount(FamilyPortraitSheet, { props: { pets: [PEDRO, LULU] } })
    await settle(w, () => !w.text().includes('Gathering'))
    expect(w.find('[data-testid="portrait-save"]').attributes('disabled')).toBeUndefined()
  })
})

describe('PhotoEditorSheet with a custom frame + target (#75)', () => {
  it('crops to 3:2 and hands the JPEG to the given upload', async () => {
    const upload = vi.fn(() => Promise.resolve())
    const w = mount(PhotoEditorSheet, {
      props: { width: 1200, height: 800, title: 'Family portrait', hint: 'Frame the crew.', upload },
    })
    expect(w.text()).toContain('Family portrait')
    expect(w.text()).toContain('Frame the crew.')
    const c = w.get('canvas')
    expect([c.attributes('width'), c.attributes('height')]).toEqual(['1200', '800'])
    expect(w.get('.cropwrap').attributes('style')).toContain('aspect-ratio: 1200 / 800')

    const file = new File([new Uint8Array([1, 2, 3])], 'crew.jpg', { type: 'image/jpeg' })
    const input = w.get('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
    await input.trigger('change')
    await settle(w, () => w.find('.cropctl').exists())
    const el = c.element
    el.dispatchEvent(new MouseEvent('pointerdown', { clientX: 0, clientY: 0, bubbles: true }))
    el.dispatchEvent(new MouseEvent('pointermove', { clientX: 5, clientY: 400, bubbles: true }))
    el.dispatchEvent(new MouseEvent('pointerup', { bubbles: true }))
    await w.get('.btn.primary').trigger('click')
    await flushPromises()
    expect(upload).toHaveBeenCalledWith({ media_type: 'image/jpeg', data_base64: 'QUJD' })
    expect(api.setPetPhoto).not.toHaveBeenCalled()
    expect(w.emitted().saved).toBeTruthy()
    // The square pet editor keeps its default frame (no inline aspect).
    const sq = mount(PhotoEditorSheet, { props: { petId: 'pedro' } })
    expect(sq.get('.cropwrap').attributes('style')).toBeUndefined()
  })
})

describe('Home family portrait (#75)', () => {
  async function home(pets = [PEDRO, LULU, KIPPER, KOA]) {
    api.pets.mockResolvedValue(pets)
    const w = mount(Home, opts)
    await flushPromises()
    return w
  }

  it('invites a portrait when there is none, and can be waved off', async () => {
    const w = await home()
    const invite = w.get('[data-testid="portrait-invite"]')
    expect(invite.text()).toContain('Make a family portrait')
    expect(invite.findAll('.ff')).toHaveLength(3) // faces of the active pets
    expect(invite.find('img').attributes('src')).toContain('/api/pets/pedro/photo')
    expect(w.find('[data-testid="portrait-later"]').exists()).toBe(false)

    await w.get('[data-testid="portrait-invite-dismiss"]').trigger('click')
    expect(w.find('[data-testid="portrait-invite"]').exists()).toBe(false)
    expect(localStorage.getItem('furfiles-portrait-invite-dismissed')).toBe('1')
    // ...but stays one quiet tap away.
    await w.get('[data-testid="portrait-later"]').trigger('click')
    expect(w.findComponent(FamilyPortraitSheet).exists()).toBe(true)
    expect(w.findComponent(FamilyPortraitSheet).props('pets').map((p) => p.id)).toEqual(['pedro', 'lulu', 'kipper'])
  })

  it('remembers a dismissed invitation across visits', async () => {
    localStorage.setItem('furfiles-portrait-invite-dismissed', '1')
    const w = await home()
    expect(w.find('[data-testid="portrait-invite"]').exists()).toBe(false)
    expect(w.find('[data-testid="portrait-later"]').exists()).toBe(true)
  })

  it('shows no invitation without active pets, or when the portrait fails to load', async () => {
    const empty = await home([KOA])
    expect(empty.find('[data-testid="portrait-invite"]').exists()).toBe(false)
    api.household.mockRejectedValue(new Error('down'))
    const w = await home()
    expect(w.find('[data-testid="portrait-invite"]').exists()).toBe(true)
    expect(w.text()).not.toContain('down')
  })

  it('generates from the invitation and shows the saved portrait', async () => {
    const w = await home()
    await w.get('[data-testid="portrait-invite-generate"]').trigger('click')
    const sheet = w.findComponent(FamilyPortraitSheet)
    sheet.vm.$emit('saved', { portrait: GENERATED })
    await flushPromises()
    expect(w.findComponent(FamilyPortraitSheet).exists()).toBe(false)
    const fig = w.get('[data-testid="family-portrait"]')
    expect(fig.attributes('data-source')).toBe('generated')
    const img = fig.get('img')
    expect(img.attributes('src')).toBe('/api/household/portrait?v=v1')
    // Named from the portrait's own pet_ids (sorted), not today's roster.
    expect(img.attributes('alt')).toBe('Family portrait of Kipper, Lulu and Pedro')
    expect(w.find('[data-testid="portrait-stale"]').exists()).toBe(false)
    expect(w.get('[data-testid="portrait-regenerate"]').classes()).not.toContain('hot')
    expect(w.get('[data-testid="portrait-replace"]').text()).toBe('Use a photo')
  })

  it('offers to regenerate once the roster has changed, and hands off to upload', async () => {
    api.household.mockResolvedValue({ portrait: { ...GENERATED, roster_changed: true } })
    const w = await home([PEDRO])
    expect(w.get('[data-testid="portrait-stale"]').text()).toContain('changed since this was made')
    // Lulu and Kipper are gone from Home's list (deleted), so only Pedro is named.
    expect(w.get('img[alt="Family portrait of Pedro"]').exists()).toBe(true)
    const regen = w.get('[data-testid="portrait-regenerate"]')
    expect(regen.classes()).toContain('hot')
    await regen.trigger('click')
    const sheet = w.findComponent(FamilyPortraitSheet)
    expect(sheet.exists()).toBe(true)
    sheet.vm.$emit('upload')
    await flushPromises()
    expect(w.findComponent(FamilyPortraitSheet).exists()).toBe(false)
    expect(w.findComponent(PhotoEditorSheet).props()).toMatchObject({ width: 1200, height: 800, title: 'Family portrait' })
  })

  it('uploads a replacement through the crop editor', async () => {
    api.household.mockResolvedValue({ portrait: UPLOADED })
    api.setHouseholdPortrait.mockResolvedValue({ portrait: { ...UPLOADED, version: 'v3' } })
    const w = await home()
    expect(w.find('[data-testid="portrait-regenerate"]').exists()).toBe(false)
    expect(w.get('[data-testid="family-portrait"] img').attributes('alt')).toBe('Family portrait')
    const replace = w.get('[data-testid="portrait-replace"]')
    expect(replace.text()).toBe('Replace')
    await replace.trigger('click')
    const editor = w.findComponent(PhotoEditorSheet)
    await editor.props('upload')({ media_type: 'image/jpeg', data_base64: 'QUJD' })
    expect(api.setHouseholdPortrait).toHaveBeenCalledWith({ media_type: 'image/jpeg', data_base64: 'QUJD', source: 'uploaded' })
    editor.vm.$emit('saved')
    await flushPromises()
    expect(w.findComponent(PhotoEditorSheet).exists()).toBe(false)
    expect(w.get('[data-testid="family-portrait"] img').attributes('src')).toBe('/api/household/portrait?v=v3')
  })

  it('removes the portrait after confirming, and reports a failure inline', async () => {
    api.household.mockResolvedValue({ portrait: GENERATED })
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const w = await home()
    await w.get('[data-testid="portrait-remove"]').trigger('click')
    expect(api.deleteHouseholdPortrait).not.toHaveBeenCalled()

    confirm.mockReturnValue(true)
    api.deleteHouseholdPortrait.mockRejectedValueOnce(new Error('disk full'))
    await w.get('[data-testid="portrait-remove"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Could not remove it: disk full')

    api.deleteHouseholdPortrait.mockResolvedValue({ portrait: null })
    await w.get('[data-testid="portrait-remove"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="family-portrait"]').exists()).toBe(false)
    expect(w.find('[data-testid="portrait-invite"]').exists()).toBe(true)
  })

  it('lands on a new pet\'s profile once it is added', async () => {
    const w = await home()
    await w.get('[data-testid="add-pet-card"]').trigger('click')
    w.findComponent(AddPetSheet).vm.$emit('created', { id: 'nimbus' })
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/pets/nimbus')
    expect(w.findComponent(AddPetSheet).exists()).toBe(false)
  })

  it('keeps a save that lands before the initial portrait GET returns', async () => {
    let resolveGet
    api.household.mockReturnValue(new Promise((r) => { resolveGet = r }))
    api.pets.mockResolvedValue([PEDRO])
    const w = mount(Home, opts)
    await flushPromises()
    await w.get('[data-testid="portrait-invite-generate"]').trigger('click')
    w.findComponent(FamilyPortraitSheet).vm.$emit('saved', { portrait: GENERATED })
    await flushPromises()
    resolveGet({ portrait: null }) // the stale answer arrives last
    await flushPromises()
    expect(w.find('[data-testid="family-portrait"]').exists()).toBe(true)
  })

  it('names an archived pet who is still in the picture', async () => {
    api.household.mockResolvedValue({ portrait: { ...GENERATED, pet_ids: ['koa', 'pedro'], roster_changed: true } })
    const w = await home()
    expect(w.get('[data-testid="family-portrait"] img').attributes('alt')).toBe('Family portrait of Koa and Pedro')
  })

  it('closes the sheets without saving', async () => {
    const w = await home()
    await w.get('[data-testid="portrait-invite-upload"]').trigger('click')
    w.findComponent(PhotoEditorSheet).vm.$emit('close')
    await flushPromises()
    expect(w.findComponent(PhotoEditorSheet).exists()).toBe(false)
    await w.get('[data-testid="portrait-invite-generate"]').trigger('click')
    w.findComponent(FamilyPortraitSheet).vm.$emit('close')
    await flushPromises()
    expect(w.findComponent(FamilyPortraitSheet).exists()).toBe(false)
  })
})
