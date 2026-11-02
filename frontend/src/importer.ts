// Pure helpers behind the bulk-import screen (#23). Kept out of the components
// so the rules (what counts as importable, what a save will write, what blocks
// a save) are unit-tested directly rather than through the DOM.
import { sexLabel } from './format'
import type { ImportPlan, ImportResult, PetSummary, PlannedPolicyEnd, PlannedClaim, PlannedPolicy } from './types'

/** One dropped file on the import page, from pick to saved. `data` is the
 *  base64 body, cleared once saved so a big drop doesn't sit in memory. */
export interface ImportEntry {
  id: number
  name: string
  media_type: string
  size: number
  data: string
  status: 'queued' | 'converting' | 'analyzing' | 'ready' | 'saving' | 'saved' | 'error' | 'unsupported'
  plan: ImportPlan | null
  result: ImportResult | null
  error: string
}

/** What the backend's `/import/analyze` accepts (mirrors `ingestion::ALLOWED_MEDIA`). */
export const IMPORTABLE_TYPES = ['application/pdf', 'image/jpeg', 'image/png', 'image/webp', 'image/gif']

const BY_EXT: Record<string, string> = {
  pdf: 'application/pdf', jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png',
  webp: 'image/webp', gif: 'image/gif', heic: 'image/heic', heif: 'image/heif',
}

/** A file's media type, falling back to its extension: some pickers (and every
 *  drag from a desktop file manager on Linux) hand over an empty `type`, and a
 *  vet portal's ".PDF" download is still a PDF. */
export function mediaTypeOf(file: { name: string; type: string }): string {
  if (file.type) return file.type
  const ext = file.name.toLowerCase().split('.').pop() || ''
  return BY_EXT[ext] || 'application/octet-stream'
}

export function importable(mediaType: string): boolean {
  return IMPORTABLE_TYPES.includes(mediaType)
}

/** Why a file can't be imported, in words the owner can act on. (An iPhone HEIC is
 *  not refused: `documentFile` converts it to a JPEG before it reaches `importable`, #71.) */
export function unsupportedReason(_mediaType: string): string {
  return 'Only PDFs and photos (JPEG, PNG, WebP, GIF, HEIC) can be imported.'
}

export interface PlanCounts {
  records: number
  updates: number
}

/** How many records and other updates a save of this plan would write. */
export function planCounts(plan: ImportPlan): PlanCounts {
  const on = <T extends { selected: boolean }>(xs: T[]) => xs.filter((x) => x.selected).length
  return {
    records: on(plan.records),
    updates:
      (plan.clinic && plan.clinic.selected && !plan.clinic.contact_id ? 1 : 0) +
      on(plan.care_team) + on(plan.clinic_links) + on(plan.weights) + on(plan.reminders) +
      on(plan.completed_reminders) + on(plan.pet_updates) + on(plan.regimens) +
      on(plan.claims) + on(plan.policies) +
      plan.policy_ends.filter((e) => e.selected && endApplies(plan, e)).length,
  }
}

/** Mirrors the server's `replacement_kept`: an old policy is ended only
 *  alongside its replacement, for the same pet, being saved or already saved. */
export function endApplies(plan: ImportPlan, e: PlannedPolicyEnd): boolean {
  const p = plan.policies[e.replaced_by]
  return !!p && p.pet_id === e.pet_id && (p.selected || p.existing_id != null)
}

export function hasWork(plan: ImportPlan): boolean {
  const c = planCounts(plan)
  return c.records + c.updates > 0
}

/** Selected records nobody has said whose they are — a save is blocked until each has a pet. */
export function recordsNeedingPet(plan: ImportPlan): number {
  const need = <T extends { selected: boolean; pet_id: string | null }>(xs: T[]) =>
    xs.filter((x) => x.selected && !x.pet_id).length
  return need(plan.records) + need(plan.claims) + need(plan.policies)
}

function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`
}

/** "Save 3 records + 4 updates" — says exactly what the button will write. */
export function saveLabel(plan: ImportPlan): string {
  const { records, updates } = planCounts(plan)
  const parts: string[] = []
  if (records) parts.push(plural(records, 'record'))
  if (updates) parts.push(plural(updates, 'update'))
  return parts.length ? `Save ${parts.join(' + ')}` : 'Nothing selected'
}

/** One line describing what a commit wrote. */
export function resultSummary(r: ImportResult): string {
  const parts: string[] = []
  if (r.records.length) parts.push(plural(r.records.length, 'record'))
  if (r.contact) parts.push('a new contact')
  if (r.care_team_links) parts.push(plural(r.care_team_links, 'care-team link'))
  if (r.clinic_links) parts.push(`${plural(r.clinic_links, 'visit')} linked to the clinic`)
  const weights = r.weights_added + r.weights_corrected
  if (weights) parts.push(plural(weights, 'weight'))
  if (r.reminders_added) parts.push(plural(r.reminders_added, 'reminder'))
  if (r.reminders_completed) parts.push(`${plural(r.reminders_completed, 'reminder')} marked done`)
  if (r.pet_updates) parts.push(plural(r.pet_updates, 'profile update'))
  if (r.regimens_added) parts.push(plural(r.regimens_added, 'care item'))
  if (r.claims_recorded) parts.push(plural(r.claims_recorded, 'insurance claim'))
  if (r.policies_added) parts.push(plural(r.policies_added, 'policy', 'policies'))
  if (r.policies_ended) parts.push(`${plural(r.policies_ended, 'old policy', 'old policies')} ended`)
  return parts.length ? `Saved ${parts.join(', ')}.` : 'Nothing to save.'
}

const FIELD_LABELS: Record<string, string> = {
  name: 'Name', breed: 'Breed', sex: 'Sex', color: 'Color', microchip: 'Microchip', birth_date: 'Birthday',
}
export function petFieldLabel(field: string): string {
  return FIELD_LABELS[field] || field
}

/** A proposed profile value as the profile will show it: sex in the pet's own
 *  species words (#72), everything else as written. */
export function petFieldValue(field: string, value: string | null, species: string | null | undefined, breederTerms = false): string {
  if (value == null) return ''
  return field === 'sex' ? sexLabel(value, species, breederTerms) : value
}

export function petName(pets: PetSummary[], id: string | null | undefined): string {
  if (!id) return 'Unassigned'
  const p = pets.find((x) => x.id === id)
  return p ? p.name : id
}

/** "Sep 4, 2026" from an ISO date, parsed by parts so no timezone shifts it a day. */
export function shortDate(iso: string | null | undefined): string {
  if (!iso) return ''
  const [y, m, d] = iso.split('-').map(Number)
  if (!y || !m || !d) return iso
  return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString(undefined, {
    month: 'short', day: 'numeric', year: 'numeric', timeZone: 'UTC',
  })
}

/** Run `worker` over `items` with at most `limit` in flight. Analysis of a long
 *  history report takes a minute or more; two at a time keeps a 20-file drop
 *  moving without hammering the provider's rate limit. */
export async function runLimited<T>(items: T[], limit: number, worker: (item: T) => Promise<void>): Promise<void> {
  let next = 0
  const lane = async () => {
    while (next < items.length) {
      const item = items[next++]
      await worker(item)
    }
  }
  await Promise.all(Array.from({ length: Math.max(1, Math.min(limit, items.length)) }, lane))
}

/** A claim moved to another pet: its matched visit and "already recorded" were
 *  about the old pet, so they no longer apply (the server re-checks on save). */
export function reassignClaim(c: PlannedClaim): void {
  c.record_id = null
  c.record_title = null
  c.visit_options = []
  c.already_recorded = false
  c.selected = true
}

/** A policy moved to another pet: tick the end of THAT pet's replaced policy
 *  (if the plan has one) and untick the others. */
export function reassignPolicy(plan: ImportPlan, index: number, p: PlannedPolicy): void {
  for (const e of plan.policy_ends) {
    if (e.replaced_by === index) e.selected = e.pet_id === p.pet_id
  }
}
