import { describe, it, expect, beforeEach, vi } from 'vitest'
import { api } from '../src/api'

function ok(body, status = 200) {
  return Promise.resolve({ ok: status >= 200 && status < 300, status, json: () => Promise.resolve(body) })
}

describe('api client: family portrait (#75)', () => {
  beforeEach(() => {
    global.fetch = vi.fn(() => ok({ portrait: null }))
  })

  it('reads, sets and removes the household portrait', async () => {
    expect(await api.household()).toEqual({ portrait: null })
    expect(global.fetch).toHaveBeenLastCalledWith('/api/household', expect.objectContaining({ method: 'GET' }))

    const body = { media_type: 'image/jpeg', data_base64: 'QUJD', source: 'generated' as const, pet_ids: ['pedro'] }
    await api.setHouseholdPortrait(body)
    const [url, opts] = global.fetch.mock.calls[1]
    expect(url).toBe('/api/household/portrait')
    expect(opts.method).toBe('PUT')
    expect(JSON.parse(opts.body)).toEqual(body)

    await api.deleteHouseholdPortrait()
    expect(global.fetch).toHaveBeenLastCalledWith('/api/household/portrait', expect.objectContaining({ method: 'DELETE' }))
  })

  it('builds a cache-busted image URL from the version', () => {
    expect(api.householdPortraitUrl({ version: 'a b' })).toBe('/api/household/portrait?v=a%20b')
    expect(api.householdPortraitUrl(null)).toBe(null)
    expect(api.householdPortraitUrl(undefined)).toBe(null)
  })
})
