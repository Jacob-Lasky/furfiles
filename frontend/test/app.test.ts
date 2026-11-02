import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'

vi.mock('../src/api', () => ({
  api: { users: vi.fn(), pets: vi.fn(), addRecord: vi.fn(), petPhotoUrl: vi.fn(() => null) },
}))
import { api } from '../src/api'
import App from '../src/App.vue'
import { routes } from '../src/router'
import { pageActions } from '../src/store'

enableAutoUnmount(afterEach)

// jsdom has no matchMedia; App uses it for OS theme detection.
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation((q) => ({
    matches: false, media: q, addEventListener: vi.fn(), removeEventListener: vi.fn(),
  })),
})

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div>home</div>' } },
      { path: '/calendar', component: { template: '<div>cal</div>' } },
      { path: '/costs', component: { template: '<div>costs</div>' } },
      { path: '/contacts', component: { template: '<div>contacts</div>' } },
      { path: '/settings', component: { template: '<div>settings</div>' } },
      { path: '/import', component: { template: '<div>import</div>' } },
      { path: '/pets/:id', component: { template: '<div>pet</div>' } },
    ],
  })
}

describe('router config', () => {
  it('exposes deep-linkable routes', () => {
    const paths = routes.map((r) => r.path)
    expect(paths).toContain('/')
    expect(paths).toContain('/pets/:id')
    expect(paths).toContain('/calendar')
    expect(paths).toContain('/costs')
    expect(paths).toContain('/import')
  })
})

describe('App shell', () => {
  beforeEach(() => {
    api.users.mockReset()
    api.pets.mockReset()
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    document.documentElement.removeAttribute('data-theme')
  })

  it('renders nav, household, toggles theme and opens the add sheet', async () => {
    api.users.mockResolvedValue([{ id: 'jake', name: 'Jake', color: '#F2624C' }])
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const w = mount(App, { global: { plugins: [router] } })
    await flushPromises()

    expect(w.text()).toContain('FurFiles')
    expect(w.text()).toContain('Household')
    // theme toggle stamps the root attribute
    await w.find('.theme-toggle').trigger('click')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    await w.find('.theme-toggle').trigger('click')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')

    // the + is a speed-dial (#77); "Add a record" opens the RECORD sheet
    expect(w.find('.mback').exists()).toBe(false)
    await w.find('[data-testid="fab"]').trigger('click')
    await w.find('[data-testid="dial-add-record"]').trigger('click')
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(true)
    expect(w.text()).toContain('Add a record')
  })

  it('adds a record from the + button and lands on the pet', async () => {
    api.users.mockResolvedValue([])
    api.addRecord.mockResolvedValue({ id: 9 })
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const w = mount(App, { global: { plugins: [router] } })
    await flushPromises()

    await w.find('[data-testid="fab"]').trigger('click')
    await w.find('[data-testid="dial-add-record"]').trigger('click')
    await flushPromises() // record sheet loads its pet list
    await w.find('input[placeholder="Annual wellness exam"]').setValue('Checkup')
    await w.find('.btn.primary').trigger('click')
    await flushPromises()

    expect(api.addRecord).toHaveBeenCalledWith('biscuit', expect.objectContaining({ title: 'Checkup' }))
    expect(router.currentRoute.value.path).toBe('/pets/biscuit')
    expect(w.find('.mback').exists()).toBe(false) // sheet closed after save
  })

  it('survives the users call failing', async () => {
    api.users.mockRejectedValue(new Error('nope'))
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const w = mount(App, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.text()).toContain('FurFiles')
  })
})

// The floating + as a speed-dial (#77): one place for every action.
describe('speed-dial', () => {
  beforeEach(() => {
    api.users.mockResolvedValue([])
    api.pets.mockResolvedValue([{ id: 'biscuit', name: 'Biscuit' }])
    pageActions.value = []
  })

  async function mountAt(path: string) {
    const router = makeRouter()
    router.push(path)
    await router.isReady()
    const w = mount(App, { global: { plugins: [router] }, attachTo: document.body })
    await flushPromises()
    return { w, router }
  }
  const fab = (w) => w.find('[data-testid="fab"]')
  const keys = (w) => w.findAll('[role="menuitem"]').map((b) => b.attributes('data-testid'))

  it('is a labelled menu button that toggles open and closed', async () => {
    const { w } = await mountAt('/')
    const t = fab(w)
    expect(t.attributes('aria-haspopup')).toBe('menu')
    expect(t.attributes('aria-controls')).toBe('speed-dial-menu')
    expect(w.find('#speed-dial-menu').attributes('role')).toBe('menu')
    expect(t.attributes('aria-expanded')).toBe('false')
    await t.trigger('click')
    expect(t.attributes('aria-expanded')).toBe('true')
    expect(w.find('.dial').classes()).toContain('open')
    // tapping the button again closes it (the × state)
    await t.trigger('click')
    expect(t.attributes('aria-expanded')).toBe('false')
    expect(w.find('[data-testid="dial-scrim"]').exists()).toBe(false)
  })

  it('lists page actions first and "Add a record" last, next to the button', async () => {
    const run = vi.fn()
    pageActions.value = [{ key: 'edit-pet', label: 'Edit Biscuit', icon: 'edit', run }]
    const { w } = await mountAt('/')
    expect(keys(w)).toEqual(['dial-edit-pet', 'dial-import', 'dial-add-record'])
    expect(w.find('[data-testid="dial-edit-pet"]').text()).toBe('Edit Biscuit')
    await fab(w).trigger('click')
    await w.find('[data-testid="dial-edit-pet"]').trigger('click')
    expect(run).toHaveBeenCalledOnce()
    expect(fab(w).attributes('aria-expanded')).toBe('false') // choosing closes it
  })

  it('renders a paw for the add-pet action and a stroke icon for the rest', async () => {
    pageActions.value = [{ key: 'add-pet', label: 'Add a pet', icon: 'pet', run: vi.fn() }]
    const { w } = await mountAt('/')
    expect(w.find('[data-testid="dial-add-pet"] svg').attributes('fill')).toBe('currentColor')
    expect(w.find('[data-testid="dial-import"] svg').attributes('fill')).toBe('none')
  })

  it('"Import documents" goes to /import, closes, and is not offered there', async () => {
    const { w, router } = await mountAt('/')
    await fab(w).trigger('click')
    await w.find('[data-testid="dial-import"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/import')
    expect(fab(w).attributes('aria-expanded')).toBe('false')
    expect(keys(w)).toEqual(['dial-add-record'])
  })

  it('closes on a tap outside, and on Escape, returning focus to the button', async () => {
    const { w } = await mountAt('/')
    await fab(w).trigger('click')
    await w.find('[data-testid="dial-scrim"]').trigger('click')
    expect(fab(w).attributes('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(fab(w).element)

    await fab(w).trigger('click')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(fab(w).attributes('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(fab(w).element)
    // once closed, Escape is no longer the dial's business
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(fab(w).attributes('aria-expanded')).toBe('false')
  })

  it('moves focus into the menu and through it with the keyboard', async () => {
    pageActions.value = [{ key: 'add-pet', label: 'Add a pet', icon: 'pet', run: vi.fn() }]
    const { w } = await mountAt('/')
    const item = (k: string) => w.find(`[data-testid="dial-${k}"]`).element
    await fab(w).trigger('click')
    await flushPromises()
    // focus lands on the action nearest the button: the primary one
    expect(document.activeElement).toBe(item('add-record'))
    const menu = w.find('[role="menu"]')
    await menu.trigger('keydown', { key: 'ArrowUp' })
    expect(document.activeElement).toBe(item('import'))
    await menu.trigger('keydown', { key: 'Home' })
    expect(document.activeElement).toBe(item('add-pet'))
    await menu.trigger('keydown', { key: 'ArrowUp' }) // wraps to the bottom
    expect(document.activeElement).toBe(item('add-record'))
    await menu.trigger('keydown', { key: 'ArrowDown' }) // wraps to the top
    expect(document.activeElement).toBe(item('add-pet'))
    await menu.trigger('keydown', { key: 'End' })
    expect(document.activeElement).toBe(item('add-record'))
    await menu.trigger('keydown', { key: 'x' }) // unhandled keys are ignored
    expect(fab(w).attributes('aria-expanded')).toBe('true')
    // Tab closes and lets the browser move focus on (APG): not cancelled.
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
    menu.element.dispatchEvent(tab)
    await flushPromises()
    expect(tab.defaultPrevented).toBe(false)
    expect(fab(w).attributes('aria-expanded')).toBe('false')
  })

  it('closes when the route changes underneath it, focus back on the +', async () => {
    const { w, router } = await mountAt('/')
    await fab(w).trigger('click')
    await flushPromises()
    expect(document.activeElement).toBe(w.find('[data-testid="dial-add-record"]').element)
    await router.push('/calendar')
    await flushPromises()
    expect(fab(w).attributes('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(fab(w).element) // not stranded in the hidden menu
  })

  it('with the real routes: Home adds "Add a pet", and focus goes into the sheet and back', async () => {
    const router = createRouter({ history: createMemoryHistory(), routes })
    router.push('/')
    await router.isReady()
    const w = mount(App, { global: { plugins: [router] }, attachTo: document.body })
    await flushPromises()
    expect(keys(w)).toEqual(['dial-add-pet', 'dial-import', 'dial-add-record'])
    await fab(w).trigger('click')
    await w.find('[data-testid="dial-add-pet"]').trigger('click')
    await flushPromises()
    const sheet = w.find('.mback .sheet')
    expect(sheet.text()).toContain('Add a pet')
    expect(document.activeElement).toBe(sheet.element) // not left behind the backdrop
    await w.find('.mback').trigger('click') // tap the backdrop
    await flushPromises()
    expect(w.find('.mback').exists()).toBe(false)
    expect(document.activeElement).toBe(fab(w).element)
  })
})
