import { describe, it, expect, beforeEach, vi } from 'vitest'
import { api, ApiError } from '../src/api'

function ok(body, status = 200) {
  return Promise.resolve({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  })
}

describe('api client', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('GETs pets, with and without archived', async () => {
    global.fetch.mockReturnValueOnce(ok([{ id: 'a' }]))
    await api.pets()
    expect(global.fetch).toHaveBeenCalledWith('/api/pets', expect.objectContaining({ method: 'GET' }))

    global.fetch.mockReturnValueOnce(ok([]))
    await api.pets(true)
    expect(global.fetch).toHaveBeenLastCalledWith('/api/pets?include_archived=1', expect.anything())
  })

  it('POSTs a new pet with a JSON body', async () => {
    global.fetch.mockReturnValueOnce(ok({ id: 'nimbus' }, 201))
    const pet = await api.createPet({ name: 'Nimbus', species: 'Cat' })
    expect(pet.id).toBe('nimbus')
    const [, opts] = global.fetch.mock.calls[0]
    expect(opts.method).toBe('POST')
    expect(JSON.parse(opts.body)).toEqual({ name: 'Nimbus', species: 'Cat' })
    expect(opts.headers['content-type']).toBe('application/json')
  })

  it('covers the remaining verbs', async () => {
    global.fetch.mockReturnValue(ok({}))
    await api.health()
    await api.users()
    await api.pet('biscuit')
    await api.updatePet('biscuit', { weight: 4 })
    await api.archivePet('biscuit', { kind: 'rehomed', archived_at: '2026-08-01', memorial_note: 'bye' })
    await api.unarchivePet('biscuit')
    await api.records('biscuit')
    await api.addRecord('biscuit', { title: 'x', occurred_on: '2026-01-01' })
    await api.weights('biscuit')
    await api.addWeight('biscuit', { recorded_on: '2026-01-01', value: 4 })
    await api.costs()
    // reminders (#26)
    await api.reminders('biscuit')
    await api.allReminders()
    await api.addReminder('biscuit', { due_on: '2026-02-01', title: 'Booster' })
    await api.updateReminder(3, { done: true })
    await api.deleteReminder(3)
    // attachments + ingestion (#4)
    await api.recordAttachments(9)
    await api.addAttachment(9, { filename: 'x', media_type: 'image/png', data_base64: 'QQ==' })
    await api.deleteAttachment('att-1')
    await api.ingestion()
    await api.saveIngestion({ model: 'claude-sonnet-5' })
    await api.clearIngestion()
    await api.extract({ media_type: 'application/pdf', data_base64: 'QQ==' })
    const urls = global.fetch.mock.calls.map((c) => c[0])
    expect(urls).toContain('/api/health')
    expect(urls).toContain('/api/pets/biscuit/records')
    expect(urls).toContain('/api/costs/summary')
    expect(urls).toContain('/api/reminders')
    expect(urls).toContain('/api/records/9/attachments')
    expect(urls).toContain('/api/settings/ingestion')
    expect(urls).toContain('/api/extract')
    // the pure URL builder needs no fetch
    expect(api.attachmentUrl('att-1')).toBe('/api/attachments/att-1')
  })

  it('lists models for the stored key (GET) or a pasted key (POST, not saved) (#67)', async () => {
    global.fetch.mockReturnValue(ok([]))
    await api.ingestionModels()
    await api.ingestionModels('sk-ant-pasted')
    const [stored, pasted] = global.fetch.mock.calls
    expect(stored[0]).toBe('/api/settings/ingestion/models')
    expect(stored[1].method).toBe('GET')
    expect(stored[1].body).toBeUndefined()
    expect(pasted[0]).toBe('/api/settings/ingestion/models')
    expect(pasted[1].method).toBe('POST')
    expect(JSON.parse(pasted[1].body)).toEqual({ api_key: 'sk-ant-pasted' })
  })

  it('covers the shared-contacts verbs (#20)', async () => {
    global.fetch.mockReturnValue(ok({}))
    await api.contacts()
    await api.contacts('vet')
    await api.contact('c1')
    await api.createContact({ name: 'Cedar Creek', role: 'vet' })
    await api.updateContact('c1', { phone: '555' })
    await api.deleteContact('c1')
    await api.petContacts('biscuit')
    await api.linkPetContact('biscuit', 'c1')
    await api.unlinkPetContact('biscuit', 'c1')
    await api.parseContactUrl('https://facility.example/policies')
    const calls = global.fetch.mock.calls
    const urls = calls.map((c) => c[0])
    // #42 facility web parser POSTs the URL to /contacts/parse-url
    const parse = calls.find((c) => c[0] === '/api/contacts/parse-url')
    expect(parse[1].method).toBe('POST')
    expect(JSON.parse(parse[1].body)).toEqual({ url: 'https://facility.example/policies' })
    expect(urls).toContain('/api/contacts')
    expect(urls).toContain('/api/contacts?role=vet')
    expect(urls).toContain('/api/contacts/c1')
    expect(urls).toContain('/api/pets/biscuit/contacts')
    expect(urls).toContain('/api/pets/biscuit/contacts/c1')
    // link POSTs { contact_id }
    const link = calls.find((c) => c[0] === '/api/pets/biscuit/contacts' && c[1].method === 'POST')
    expect(JSON.parse(link[1].body)).toEqual({ contact_id: 'c1' })
    // create POSTs the payload; unlink is a DELETE
    const create = calls.find((c) => c[0] === '/api/contacts' && c[1].method === 'POST')
    expect(JSON.parse(create[1].body)).toEqual({ name: 'Cedar Creek', role: 'vet' })
    const unlink = calls.find((c) => c[0] === '/api/pets/biscuit/contacts/c1')
    expect(unlink[1].method).toBe('DELETE')
  })

  it('GETs insurance value per policy, per pet and for the household (#74)', async () => {
    global.fetch.mockReturnValue(ok({}))
    await api.insuranceValue('pol-1')
    await api.petInsuranceValue('lulu')
    await api.householdInsuranceValue()
    expect(global.fetch.mock.calls.map((c) => [c[0], c[1].method])).toEqual([
      ['/api/insurance/pol-1/value', 'GET'],
      ['/api/pets/lulu/insurance/value', 'GET'],
      ['/api/insurance/value', 'GET'],
    ])
  })

  it('covers the insurance verbs and the document URL builder (#8/#29)', async () => {
    global.fetch.mockReturnValue(ok({ id: 'pol-1', updated_at: '2026-07-06T00:00:00' }))
    await api.petInsurance('biscuit')
    await api.createInsurance('biscuit', { provider: 'Lemonade' })
    await api.insurance('pol-1')
    await api.updateInsurance('pol-1', { status: 'expired' })
    await api.deleteInsurance('pol-1')
    await api.setInsuranceDocument('pol-1', { filename: 'p.pdf', media_type: 'application/pdf', data_base64: 'QQ==' })
    await api.deleteInsuranceDocument('pol-1')
    await api.extractInsurance({ filename: 'p.pdf', media_type: 'application/pdf', data_base64: 'QQ==' })
    const calls = global.fetch.mock.calls
    const urls = calls.map((c) => c[0])
    expect(urls).toContain('/api/pets/biscuit/insurance')
    expect(urls).toContain('/api/insurance/pol-1')
    expect(urls).toContain('/api/insurance/pol-1/document')
    expect(urls).toContain('/api/insurance/extract')
    // create POSTs the payload
    const create = calls.find((c) => c[0] === '/api/pets/biscuit/insurance' && c[1].method === 'POST')
    expect(JSON.parse(create[1].body)).toEqual({ provider: 'Lemonade' })
    // document is a PUT; extract is a POST
    const doc = calls.find((c) => c[0] === '/api/insurance/pol-1/document' && c[1].method === 'PUT')
    expect(doc).toBeTruthy()
    // the pure URL builder: cache-busted off updated_at, null without a document
    expect(api.insuranceDocumentUrl({ id: 'pol-1', document_name: 'p.pdf', updated_at: '2026-07-06T00:00:00' }))
      .toBe('/api/insurance/pol-1/document?v=2026-07-06T00%3A00%3A00')
    expect(api.insuranceDocumentUrl({ id: 'pol-1', document_name: null, updated_at: 'x' })).toBe(null)
  })

  it('handles pet photos: PUT, DELETE, and the cache-busting URL', async () => {
    global.fetch.mockReturnValue(ok({ id: 'biscuit', photo_path: 'photos/biscuit/x.jpg' }))
    await api.setPetPhoto('biscuit', { media_type: 'image/jpeg', data_base64: 'QUJD' })
    const [url, opts] = global.fetch.mock.calls[0]
    expect(url).toBe('/api/pets/biscuit/photo')
    expect(opts.method).toBe('PUT')
    expect(JSON.parse(opts.body)).toEqual({ media_type: 'image/jpeg', data_base64: 'QUJD' })

    await api.deletePetPhoto('biscuit')
    expect(global.fetch).toHaveBeenLastCalledWith('/api/pets/biscuit/photo', expect.objectContaining({ method: 'DELETE' }))

    // URL derives from the pet, cache-busted by photo_path; null without a photo
    expect(api.petPhotoUrl({ id: 'biscuit', photo_path: 'photos/biscuit/x.jpg' }))
      .toBe('/api/pets/biscuit/photo?v=photos%2Fbiscuit%2Fx.jpg')
    expect(api.petPhotoUrl({ id: 'biscuit', photo_path: null })).toBe(null)
    expect(api.petPhotoUrl(null)).toBe(null)
  })

  it('reportDiag posts a best-effort report with the triggering file (#41)', async () => {
    global.fetch.mockReturnValue(ok({}, 201))
    const file = new File([new Uint8Array([104, 105])], 'koa.heif', { type: 'image/heic' })
    await api.reportDiag({ context: 'photo-editor:load', message: 'boom' }, file)
    const [url, opts] = global.fetch.mock.calls[0]
    expect(url).toBe('/api/diag')
    expect(opts.method).toBe('POST')
    const body = JSON.parse(opts.body)
    expect(body.context).toBe('photo-editor:load')
    expect(body.url).toBeDefined() // location.href stamped in
    expect(body.file).toEqual({ filename: 'koa.heif', media_type: 'image/heic', data_base64: 'aGk=' })
    // a file report must NOT use keepalive (spec caps keepalive bodies at 64KB,
    // which a real photo blows past → the request would throw and be swallowed)
    expect(opts.keepalive).toBe(false)
  })

  it('reportDiag keepalives small file-less reports (survive page unload)', async () => {
    global.fetch.mockReturnValue(ok({}, 201))
    await api.reportDiag({ context: 'window.error', message: 'boom' })
    const [, opts] = global.fetch.mock.calls[0]
    expect(opts.keepalive).toBe(true)
    expect(JSON.parse(opts.body).file).toBeUndefined()
  })

  it('reportDiag never throws, even offline', async () => {
    global.fetch.mockRejectedValue(new Error('offline'))
    await expect(api.reportDiag({ context: 'x', message: 'y' })).resolves.toBeUndefined()
  })

  it('returns null on 204', async () => {
    global.fetch.mockReturnValueOnce(Promise.resolve({ ok: true, status: 204 }))
    expect(await api.deletePet('x')).toBe(null)
  })

  it('throws the server error message, carrying the status', async () => {
    global.fetch.mockReturnValueOnce(ok({ error: 'name is required' }, 400))
    const err = await api.createPet({}).catch((e) => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err.message).toBe('name is required')
    expect(err.status).toBe(400)
  })

  it('throws status when body is not JSON', async () => {
    global.fetch.mockReturnValueOnce(
      Promise.resolve({ ok: false, status: 500, json: () => Promise.reject(new Error('no json')) }),
    )
    await expect(api.health()).rejects.toThrow('500')
  })
})

describe('api client — ongoing care (#63)', () => {
  beforeEach(() => { global.fetch = vi.fn() })

  it('routes every regimen verb at the right URL', async () => {
    global.fetch.mockReturnValue(ok({ id: 'r1' }))
    await api.petRegimens('pedro')
    expect(global.fetch).toHaveBeenLastCalledWith('/api/pets/pedro/regimens', expect.objectContaining({ method: 'GET' }))

    await api.createRegimen('pedro', { kind: 'preventative', product: 'Simparica Trio', protects: ['flea'] })
    let [url, opts] = global.fetch.mock.calls.at(-1)
    expect(url).toBe('/api/pets/pedro/regimens')
    expect(opts.method).toBe('POST')
    expect(JSON.parse(opts.body).protects).toEqual(['flea'])

    await api.regimen('r1')
    expect(global.fetch).toHaveBeenLastCalledWith('/api/regimens/r1', expect.objectContaining({ method: 'GET' }))

    await api.updateRegimen('r1', { ended_on: null })
    ;[url, opts] = global.fetch.mock.calls.at(-1)
    expect(url).toBe('/api/regimens/r1')
    expect(opts.method).toBe('PATCH')
    // An explicit null must survive JSON encoding — it is what CLEARS the field.
    expect(JSON.parse(opts.body)).toEqual({ ended_on: null })

    await api.deleteRegimen('r1')
    expect(global.fetch).toHaveBeenLastCalledWith('/api/regimens/r1', expect.objectContaining({ method: 'DELETE' }))
  })

  it('sends an explicit null on a pet PATCH so a field can be cleared', async () => {
    global.fetch.mockReturnValue(ok({ id: 'pedro' }))
    await api.updatePet('pedro', { microchip: null, color: null, allergies: [] })
    const [, opts] = global.fetch.mock.calls.at(-1)
    expect(JSON.parse(opts.body)).toEqual({ microchip: null, color: null, allergies: [] })
  })
})
