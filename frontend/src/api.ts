// Thin client over the FurFiles REST API. The UI consumes ONLY this public API
// (API-first rule): anything here a script or test can call the same way.
import { readDataUrl } from './format'
import type {
  User, Pet, PetSummary, PetDetail, MedicalRecord, WeightEntry,
  Reminder, ReminderWithPet, Attachment, CostsSummary, IngestionSettings, ProviderModel,
  ExtractResponse, NewPetInput, UpdatePetInput, RecordInput, WeightInput,
  ReminderInput, ReminderUpdateInput, AttachmentInput, PetPhotoInput,
  Household, HouseholdPortrait, HouseholdPortraitInput,
  IngestionInput, ExtractInput, DiagReportInput, ArchivePetInput,
  Contact, NewContactInput, UpdateContactInput, ParseUrlResponse,
  InsurancePolicy, NewInsuranceInput, UpdateInsuranceInput,
  InsuranceDocumentInput, InsuranceExtractResponse,
  PolicyValue, PetInsuranceValue, InsuranceValueSummary,
  Regimen, NewRegimenInput, UpdateRegimenInput,
  ImportPlan, ImportAnalyzeInput, ImportCommitInput, ImportResult, WeightUpdateInput,
  DisplaySettings, DisplaySettingsInput,
} from './types'

type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'

/** A non-2xx answer: the server's `{error}` message, plus the HTTP status so a
 *  caller can tell "your input was refused" (4xx) from "try again" (5xx). */
export class ApiError extends Error {
  readonly status: number
  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

async function req<T>(method: Method, path: string, body?: unknown): Promise<T> {
  const opts: RequestInit = { method, headers: {} }
  if (body !== undefined) {
    ;(opts.headers as Record<string, string>)['content-type'] = 'application/json'
    opts.body = JSON.stringify(body)
  }
  const res = await fetch(`/api${path}`, opts)
  if (!res.ok) {
    let msg = `${res.status}`
    try {
      const j = await res.json()
      if (j && j.error) msg = j.error
    } catch (_) { /* non-JSON error body */ }
    throw new ApiError(msg, res.status)
  }
  if (res.status === 204) return null as T
  return res.json() as Promise<T>
}

export const api = {
  health: () => req<unknown>('GET', '/health'),
  users: () => req<User[]>('GET', '/users'),
  pets: (includeArchived = false) =>
    req<PetSummary[]>('GET', `/pets${includeArchived ? '?include_archived=1' : ''}`),
  pet: (id: string) => req<PetDetail>('GET', `/pets/${id}`),
  createPet: (data: NewPetInput) => req<Pet>('POST', '/pets', data),
  updatePet: (id: string, data: UpdatePetInput) => req<Pet>('PATCH', `/pets/${id}`, data),
  // Archive (#3): `passed` or `rehomed`. Re-posting edits the archive;
  // unarchive brings the pet back to the active household.
  archivePet: (id: string, data: ArchivePetInput) => req<Pet>('POST', `/pets/${id}/archive`, data),
  unarchivePet: (id: string) => req<Pet>('POST', `/pets/${id}/unarchive`),
  deletePet: (id: string) => req<void>('DELETE', `/pets/${id}`),

  // Portrait photo (#21): one per pet, stored server-side, served by a stable
  // per-pet URL. `photo_path` changes on every upload, so key the cache-buster
  // off it — a replaced photo gets a fresh URL, the gradient fallback gets null.
  setPetPhoto: (id: string, data: PetPhotoInput) => req<Pet>('PUT', `/pets/${id}/photo`, data),
  deletePetPhoto: (id: string) => req<void>('DELETE', `/pets/${id}/photo`),
  petPhotoUrl: (pet: Pick<Pet, 'id' | 'photo_path'> | null | undefined) =>
    pet && pet.photo_path ? `/api/pets/${pet.id}/photo?v=${encodeURIComponent(pet.photo_path)}` : null,

  // Family portrait (#75): one per household, uploaded or generated in the
  // browser from the active pets' photos. Same cache-busting idea as pet photos.
  household: () => req<Household>('GET', '/household'),
  setHouseholdPortrait: (data: HouseholdPortraitInput) => req<Household>('PUT', '/household/portrait', data),
  deleteHouseholdPortrait: () => req<Household>('DELETE', '/household/portrait'),
  householdPortraitUrl: (p: Pick<HouseholdPortrait, 'version'> | null | undefined) =>
    p ? `/api/household/portrait?v=${encodeURIComponent(p.version)}` : null,
  records: (id: string) => req<MedicalRecord[]>('GET', `/pets/${id}/records`),
  addRecord: (id: string, data: RecordInput) => req<MedicalRecord>('POST', `/pets/${id}/records`, data),
  updateRecord: (id: string, rid: number, data: RecordInput) =>
    req<MedicalRecord>('PATCH', `/pets/${id}/records/${rid}`, data),
  deleteRecord: (id: string, rid: number) => req<void>('DELETE', `/pets/${id}/records/${rid}`),
  weights: (id: string) => req<WeightEntry[]>('GET', `/pets/${id}/weights`),
  addWeight: (id: string, data: WeightInput) => req<WeightEntry>('POST', `/pets/${id}/weights`, data),
  updateWeight: (id: string, wid: number, data: WeightUpdateInput) =>
    req<WeightEntry>('PATCH', `/pets/${id}/weights/${wid}`, data),
  deleteWeight: (id: string, wid: number) => req<void>('DELETE', `/pets/${id}/weights/${wid}`),
  costs: () => req<CostsSummary>('GET', '/costs/summary'),

  // Reminders / calendar events (#26)
  reminders: (id: string) => req<Reminder[]>('GET', `/pets/${id}/reminders`),
  allReminders: () => req<ReminderWithPet[]>('GET', '/reminders'),
  addReminder: (id: string, data: ReminderInput) => req<Reminder>('POST', `/pets/${id}/reminders`, data),
  updateReminder: (rid: number, data: ReminderUpdateInput) => req<Reminder>('PATCH', `/reminders/${rid}`, data),
  deleteReminder: (rid: number) => req<void>('DELETE', `/reminders/${rid}`),

  // Attachments (#4): files live on the server; upload/list/delete by record.
  recordAttachments: (rid: number) => req<Attachment[]>('GET', `/records/${rid}/attachments`),
  addAttachment: (rid: number, data: AttachmentInput) => req<Attachment>('POST', `/records/${rid}/attachments`, data),
  deleteAttachment: (aid: string) => req<void>('DELETE', `/attachments/${aid}`),
  // Direct URL for <img>/<a> — the browser streams the bytes from the API.
  attachmentUrl: (aid: string) => `/api/attachments/${aid}`,

  // Shared contacts (#20): the care-team entity, and per-pet links. Editing a
  // contact updates it everywhere it's shown — one source of truth for a clinic.
  contacts: (role?: string) =>
    req<Contact[]>('GET', `/contacts${role ? `?role=${encodeURIComponent(role)}` : ''}`),
  contact: (cid: string) => req<Contact>('GET', `/contacts/${cid}`),
  createContact: (data: NewContactInput) => req<Contact>('POST', '/contacts', data),
  // Facility web parser (#42): fetch a URL server-side → proposed fields +
  // requirements. Never writes; the sheet pre-fills from the response.
  parseContactUrl: (url: string) => req<ParseUrlResponse>('POST', '/contacts/parse-url', { url }),
  updateContact: (cid: string, data: UpdateContactInput) => req<Contact>('PATCH', `/contacts/${cid}`, data),
  deleteContact: (cid: string) => req<void>('DELETE', `/contacts/${cid}`),
  petContacts: (id: string) => req<Contact[]>('GET', `/pets/${id}/contacts`),
  // Link returns the pet's full (updated) care team; unlink is a 204.
  linkPetContact: (id: string, contact_id: string) =>
    req<Contact[]>('POST', `/pets/${id}/contacts`, { contact_id }),
  unlinkPetContact: (id: string, cid: string) => req<void>('DELETE', `/pets/${id}/contacts/${cid}`),

  // Ongoing care (#63): food, preventatives, medications, supplements. Also
  // carried on the pet detail payload, so these are for scripts and refreshes.
  petRegimens: (id: string) => req<Regimen[]>('GET', `/pets/${id}/regimens`),
  createRegimen: (id: string, data: NewRegimenInput) =>
    req<Regimen>('POST', `/pets/${id}/regimens`, data),
  regimen: (rid: string) => req<Regimen>('GET', `/regimens/${rid}`),
  updateRegimen: (rid: string, data: UpdateRegimenInput) =>
    req<Regimen>('PATCH', `/regimens/${rid}`, data),
  deleteRegimen: (rid: string) => req<void>('DELETE', `/regimens/${rid}`),

  // Insurance & wellness plans (#8/#29): pet-level policies with an embedded
  // document. Coverage numbers are cents; the sheet converts on save.
  petInsurance: (id: string) => req<InsurancePolicy[]>('GET', `/pets/${id}/insurance`),
  createInsurance: (id: string, data: NewInsuranceInput) =>
    req<InsurancePolicy>('POST', `/pets/${id}/insurance`, data),
  insurance: (pid: string) => req<InsurancePolicy>('GET', `/insurance/${pid}`),
  updateInsurance: (pid: string, data: UpdateInsuranceInput) =>
    req<InsurancePolicy>('PATCH', `/insurance/${pid}`, data),
  deleteInsurance: (pid: string) => req<void>('DELETE', `/insurance/${pid}`),
  // Document is set/removed by its own endpoint (returns the updated policy).
  setInsuranceDocument: (pid: string, data: InsuranceDocumentInput) =>
    req<InsurancePolicy>('PUT', `/insurance/${pid}/document`, data),
  deleteInsuranceDocument: (pid: string) =>
    req<InsurancePolicy>('DELETE', `/insurance/${pid}/document`),
  // Stable per-policy document URL for <iframe>/<img>/<a>. Cache-bust off
  // `updated_at` (bumped on every document write); null when there's no document.
  insuranceDocumentUrl: (p: Pick<InsurancePolicy, 'id' | 'document_name' | 'updated_at'>) =>
    p.document_name ? `/api/insurance/${p.id}/document?v=${encodeURIComponent(p.updated_at)}` : null,
  // Propose fields from an insurance/plan document. Never writes; sheet pre-fills.
  extractInsurance: (data: InsuranceDocumentInput) =>
    req<InsuranceExtractResponse>('POST', '/insurance/extract', data),
  // Insurance value (#74): paid in vs paid back per term, and the break-even.
  insuranceValue: (pid: string) => req<PolicyValue>('GET', `/insurance/${pid}/value`),
  petInsuranceValue: (id: string) => req<PetInsuranceValue>('GET', `/pets/${id}/insurance/value`),
  householdInsuranceValue: () => req<InsuranceValueSummary>('GET', '/insurance/value'),

  // LLM document ingestion (#4). The API key is write-only server-side.
  ingestion: () => req<IngestionSettings>('GET', '/settings/ingestion'),
  saveIngestion: (data: IngestionInput) => req<IngestionSettings>('PUT', '/settings/ingestion', data),
  clearIngestion: () => req<IngestionSettings>('DELETE', '/settings/ingestion'),
  // #67: the provider's model list. With no argument it uses the STORED key;
  // with a pasted key it lists for that key without saving it.
  ingestionModels: (apiKey?: string) =>
    apiKey === undefined
      ? req<ProviderModel[]>('GET', '/settings/ingestion/models')
      : req<ProviderModel[]>('POST', '/settings/ingestion/models', { api_key: apiKey }),
  // Display preferences (#72): breeder terms for intact dogs and cats.
  displaySettings: () => req<DisplaySettings>('GET', '/settings/display'),
  saveDisplaySettings: (data: DisplaySettingsInput) => req<DisplaySettings>('PUT', '/settings/display', data),
  extract: (data: ExtractInput) => req<ExtractResponse>('POST', '/extract', data),

  // Bulk import (#23): one document → a plan of records + updates across ANY
  // pets (analyze never writes), then commit the reviewed plan with the file.
  importAnalyze: (data: ImportAnalyzeInput) => req<ImportPlan>('POST', '/import/analyze', data),
  importCommit: (data: ImportCommitInput) => req<ImportResult>('POST', '/import/commit', data),

  // Diagnostics (#41): best-effort client error report, optionally with the file
  // that triggered it (e.g. the exact HEIC a phone browser can't convert), so the
  // failure can be pulled and reproduced. NEVER throws — diagnostics must not
  // spawn new errors — and does not go through `req` (which throws on non-2xx).
  reportDiag: async (report: DiagReportInput, file?: File | Blob | null): Promise<void> => {
    try {
      const body: Record<string, unknown> = { url: location.href, ...report }
      if (file) {
        const dataUrl = await readDataUrl(file)
        body.file = {
          filename: (file as File).name || 'upload',
          media_type: file.type || 'application/octet-stream',
          data_base64: dataUrl.split(',')[1] || '',
        }
      }
      // keepalive lets a report survive page-unload, but the Fetch spec caps a
      // keepalive body at 64KB — a captured file (e.g. a ~1MB HEIC) blows past
      // that and the request throws. So only keepalive the small, file-less
      // reports; file reports are sent normally (the page is alive anyway).
      await fetch('/api/diag', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
        keepalive: !file,
      })
    } catch (_) { /* best-effort: never let diagnostics surface an error */ }
  },
}
