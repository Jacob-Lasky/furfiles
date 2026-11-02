// Pure formatting/derivation helpers, unit-tested (frontend coverage).
import { SEXES } from './types'
import type { LineItem, LabeledDetail, ContactRole, Sex } from './types'

/** Cents to a display string like "$420" or "$4.20". */
export function money(cents: number | null | undefined): string {
  if (cents == null) return '—'
  const dollars = cents / 100
  return Number.isInteger(dollars)
    ? `$${dollars.toLocaleString()}`
    : `$${dollars.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

/** Today as an ISO date `YYYY-MM-DD`, for defaulting a new record's date. */
export function todayISO(now: Date = new Date()): string {
  const y = now.getFullYear()
  const m = String(now.getMonth() + 1).padStart(2, '0')
  const d = String(now.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

/** Human file size like "48 B" / "812 KB" / "4.2 MB", for attachment chips. */
export function fileSize(bytes: number | null | undefined): string {
  if (bytes == null) return ''
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** True for image MIME types — rendered as a thumbnail vs a generic file chip. */
export function isImage(mediaType: string | null | undefined): boolean {
  return typeof mediaType === 'string' && mediaType.startsWith('image/')
}

/** True for PDFs — rendered inline in the read-only record view (#44) via an
 *  <iframe> of the attachment URL (the backend serves it `inline` with the right
 *  Content-Type), rather than a link that opens a new tab. */
export function isPdf(mediaType: string | null | undefined): boolean {
  return mediaType === 'application/pdf'
}

/** Read a File/Blob as a data URL ("data:<mime>;base64,<data>"). The one place
 *  the FileReader dance lives — the attachment picker and the photo editor both
 *  build on it (one keeps the base64 tail, the other feeds an <img> src). */
export function readDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result))
    r.onerror = () => reject(r.error || new Error('read failed'))
    r.readAsDataURL(file)
  })
}

/**
 * Render a safe subset of Markdown to HTML for notes (#4). SECURITY: the source
 * (user-typed OR LLM-extracted from an uploaded document) is NOT trusted, so we
 * HTML-escape first and only ever emit a fixed set of tags. Links are restricted
 * to http(s), so an injected `javascript:` URI can never become an anchor.
 * Supports: #/##/### headings, **bold**, *italic*, `code`, [text](http(s)://…),
 * `-`/`*` and `1.` lists, paragraphs.
 */
export function renderMarkdown(src: string | null | undefined): string {
  if (!src) return ''
  // Quotes too: a link URL is emitted inside href="…", and an unescaped `"` in
  // it (`[x](https://a/"onmouseover="…")`) would close the attribute and add an
  // event handler. Found by an outside review of #23, which renders
  // model-written notes; every notes view shares this renderer.
  const esc = (s: string) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')
  const inline = (s: string) =>
    esc(s)
      .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
      .replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>')
      .replace(/`([^`]+)`/g, '<code>$1</code>')
      .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>')
  const out: string[] = []
  let list: 'ul' | 'ol' | null = null
  const closeList = () => { if (list) { out.push(`</${list}>`); list = null } }
  for (const raw of src.replace(/\r\n/g, '\n').split('\n')) {
    const line = raw.trimEnd()
    let m: RegExpMatchArray | null
    if ((m = line.match(/^(#{1,3})\s+(.*)$/))) {
      closeList(); const n = m[1].length + 2; out.push(`<h${n}>${inline(m[2])}</h${n}>`)
    } else if ((m = line.match(/^\s*[-*]\s+(.*)$/))) {
      if (list !== 'ul') { closeList(); out.push('<ul>'); list = 'ul' }
      out.push(`<li>${inline(m[1])}</li>`)
    } else if ((m = line.match(/^\s*\d+\.\s+(.*)$/))) {
      if (list !== 'ol') { closeList(); out.push('<ol>'); list = 'ol' }
      out.push(`<li>${inline(m[1])}</li>`)
    } else if (line.trim() === '') {
      closeList()
    } else {
      closeList(); out.push(`<p>${inline(line)}</p>`)
    }
  }
  closeList()
  return out.join('')
}

// ---- record-form field <-> API conversions (#15) ----
// Shared by the add-record and edit-record sheets so cents/dollars and the
// editable line-item / detail rows convert the same way in both. The editable
// `amount` is always a dollars string; the API stores integer cents.

/** An editable cost-breakdown row: description + a dollars string. */
export interface LineItemRow { desc: string; amount: string }
/** An editable custom-detail row: a label and its value, both free text. */
export interface ExtraRow { key: string; val: string }
/** The shared record-form fields (#15), owned by the add/edit sheets and mutated
 *  in place by RecordForm. `cost` is an editable dollars string. */
export interface RecordFormFields {
  occurred_on: string
  ends_on: string
  title: string
  category: string
  cost: string
  notes: string
  /** The linked clinic/vet contact id (#20), '' for none. */
  contact_id: string
}

/** Dollars string → integer cents, or null when blank/NaN. */
export function centsFromDollars(str: string | null | undefined): number | null {
  if (str === '' || str == null) return null
  const n = Math.round(parseFloat(str) * 100)
  return Number.isNaN(n) ? null : n
}

/** Integer cents → an editable dollars string ('' for null). */
export function dollarsFromCents(cents: number | null | undefined): string {
  return cents == null ? '' : String(cents / 100)
}

/** Editable [{desc, amount}] rows → API [{description, amount_cents}]; drops
 *  rows with a blank description. */
export function itemsToLineItems(items: LineItemRow[] | null | undefined): LineItem[] {
  const out: LineItem[] = []
  for (const it of items || []) {
    const d = (it.desc || '').trim()
    if (!d) continue
    out.push({ description: d, amount_cents: centsFromDollars(it.amount) })
  }
  return out
}

/** API line_items → editable [{desc, amount}] rows. */
export function lineItemsToItems(lineItems: LineItem[] | null | undefined): LineItemRow[] {
  return (lineItems || []).map((li) => ({
    desc: li.description || '', amount: dollarsFromCents(li.amount_cents),
  }))
}

/** Editable [{key, val}] rows → API extra object (last write wins on a dup
 *  label), or undefined when empty so the column stays NULL. */
export function rowsToExtra(rows: ExtraRow[] | null | undefined): Record<string, string> | undefined {
  const out: Record<string, string> = {}
  for (const r of rows || []) {
    const k = (r.key || '').trim()
    const v = (r.val || '').trim()
    if (k && v) out[k] = v
  }
  return Object.keys(out).length ? out : undefined
}

/** API extra object → editable [{key, val}] rows. */
export function extraToRows(extra: Record<string, string> | null | undefined): ExtraRow[] {
  return Object.entries(extra || {}).map(([key, val]) => ({ key, val: String(val) }))
}

/** An editable {label, detail} row, both free text. Shared by every sheet that
 *  edits a LabeledDetail list — a contact's requirements/hours (#42) and a
 *  policy's coverages/exclusions (#8/#29) — so the round-trip lives in one place. */
export interface LabeledRow { label: string; detail: string }

/** API LabeledDetail[] → editable rows (null detail → ''). */
export function labeledToRows(items: LabeledDetail[] | null | undefined): LabeledRow[] {
  return (items || []).map((r) => ({ label: r.label, detail: r.detail || '' }))
}

/** Editable rows → API LabeledDetail[]; drops blank-label rows, empty detail → null. */
export function rowsToLabeled(rows: LabeledRow[]): LabeledDetail[] {
  return rows
    .filter((r) => r.label.trim())
    .map((r) => ({ label: r.label.trim(), detail: r.detail.trim() || null }))
}

/** Append parsed items to `target` that aren't already present (case-insensitive
 *  on label) — used to merge AI-extracted rows without clobbering user input. */
export function mergeLabeledRows(target: LabeledRow[], items: LabeledDetail[] | null | undefined): void {
  const have = new Set(target.map((r) => r.label.trim().toLowerCase()))
  for (const it of items || []) {
    const label = (it.label || '').trim()
    if (label && !have.has(label.toLowerCase())) {
      target.push({ label, detail: it.detail || '' })
      have.add(label.toLowerCase())
    }
  }
}

/** Human label for an insurance policy kind (#8/#29). Single source so the sheet
 *  and the pet profile can't drift on the wording. */
export function insuranceKindLabel(kind: string | null | undefined): string {
  return kind === 'wellness' ? 'Wellness plan' : 'Insurance'
}

// ---- Ongoing care (#63) ----

/** Display label for a care kind. Unknown kinds fall through unchanged rather
 *  than being hidden, so a server that grows a kind still renders something. */
export function regimenKindLabel(kind: string | null | undefined): string {
  switch (kind) {
    case 'food': return 'Food'
    case 'preventative': return 'Preventative'
    case 'medication': return 'Medication'
    case 'supplement': return 'Supplement'
    default: return kind || 'Care'
  }
}

/** Glyph for a care kind, matching the timeline's milestone-icon convention. */
export function regimenKindIcon(kind: string | null | undefined): string {
  switch (kind) {
    case 'food': return '🍽'
    case 'preventative': return '🛡'
    case 'supplement': return '🌿'
    default: return '💊'
  }
}

/** Human label for a protection tag ("heartworm" → "Heartworm"). */
export function protectionLabel(p: string): string {
  return p === 'worm' ? 'Worms' : p ? p[0].toUpperCase() + p.slice(1) : p
}

/** "1 cup · 2x/day" — the dose line under a care entry's product name. Empty
 *  when neither is recorded, so the caller can skip the row entirely. */
export function regimenDose(r: { amount: string | null; frequency: string | null }): string {
  return [r.amount, r.frequency].filter((x) => x && x.trim()).join(' · ')
}

/** Is the pet still on this? `ended_on: null` is the "current" test a vet intake
 *  form asks about — there is deliberately no separate boolean to keep in step. */
export function isCurrentCare(r: { ended_on: string | null }): boolean {
  return !r.ended_on
}

/** The current preventatives covering `protection` ("heartworm"). This is what
 *  answers "Is your pet current on Heartworm Prevention? Brand: ___". */
export function currentProtection<T extends { ended_on: string | null; protects: string[] | null }>(
  regimens: T[] | null | undefined,
  protection: string,
): T[] {
  return (regimens || []).filter((r) => isCurrentCare(r) && (r.protects || []).includes(protection))
}

/** Suffix for a premium interval: '/yr', '/mo', or '' when unknown. */
export function premiumIntervalSuffix(interval: string | null | undefined): string {
  return interval === 'month' ? '/mo' : interval === 'year' ? '/yr' : ''
}

/** Whole days between two ISO dates (`YYYY-MM-DD`), parsed by parts as local
 * midnight so there's no timezone drift. Positive = `iso` is in the future. */
function daysUntil(iso: string, from: string = todayISO()): number {
  const [ay, am, ad] = iso.split('-').map(Number)
  const [by, bm, bd] = from.split('-').map(Number)
  const a = Date.UTC(ay, am - 1, ad)
  const b = Date.UTC(by, bm - 1, bd)
  return Math.round((a - b) / 86_400_000)
}

/** True when a reminder's due date is in the past (a fact about the task, not a
 * grade of the owner — drives a gentle "overdue" accent, not an alarm). */
export function isOverdue(iso: string | null | undefined, from: string = todayISO()): boolean {
  return !!iso && daysUntil(iso, from) < 0
}

/** Neutral, non-evaluative phrasing for when a reminder is due — the "what's
 * next" voice that replaced the care score (#35). E.g. "overdue", "due today",
 * "due in 3 wks", "due in 4 mo". */
export function dueLabel(iso: string | null | undefined, from: string = todayISO()): string {
  if (!iso) return ''
  const d = daysUntil(iso, from)
  if (d < 0) return 'overdue'
  if (d === 0) return 'due today'
  if (d === 1) return 'due tomorrow'
  if (d < 7) return `due in ${d} days`
  if (d < 28) {
    const w = Math.round(d / 7)
    return `due in ${w} wk${w === 1 ? '' : 's'}`
  }
  if (d < 365) {
    const m = Math.round(d / 30)
    return `due in ${m} mo`
  }
  const y = Math.round(d / 365)
  return `due in ${y} yr${y === 1 ? '' : 's'}`
}

/** A pet's age from an ISO birth date, calendar-based: months under a year,
 *  "1 yr" / "1½ yr" under two, whole years after. Null when unknown or future. */
export function ageFrom(birthDate: string | null | undefined, now: Date = new Date()): string | null {
  const months = ageInMonths(birthDate, now)
  if (months == null) return null
  // Young animals change fast, so their age reads in finer units (Jake,
  // 2026-10-11): months under a year, half years until two, whole years after.
  if (months < 1) return '<1 mo'
  if (months < 12) return `${months} mo`
  if (months < 24) return months < 18 ? '1 yr' : '1½ yr'
  return `${Math.floor(months / 12)} yr`
}

/** Whole months since an ISO birth date, or null when unknown / in the future.
 *  Shared by `ageFrom` and `lifeStage` so the two can never disagree. */
function ageInMonths(birthDate: string | null | undefined, now: Date): number | null {
  if (!birthDate) return null
  // Parse the ISO date by parts: `new Date('2025-11-04')` is UTC midnight, which
  // is the previous evening west of Greenwich and made a birthday tick a day late.
  const [y, m, d] = birthDate.split('-').map(Number)
  if (!y || !m || !d) return null
  let months = (now.getFullYear() - y) * 12 + (now.getMonth() + 1 - m)
  if (now.getDate() < d) months--
  return months < 0 ? null : months
}

// ---- sex and life stage (#72, #73) ----

// What people actually call each stored sex, per species (#72). Horses get the
// horse words automatically ("neutered male horse" reads wrong; everyone says
// gelding). Everyone else gets plain words, and the household's "Use breeder
// terms" setting swaps an INTACT dog or cat to Dog / Bitch or Tom / Queen.
// Typed against `Sex`, so a value added to SEXES without wording here is a
// compile error.
const SEX_WORDS: Record<Sex, string> = {
  male: 'Male', female: 'Female', male_neutered: 'Neutered male', female_spayed: 'Spayed female', unknown: 'Unknown',
}
const HORSE_SEX_WORDS: Record<Sex, string> = {
  male: 'Stallion', female: 'Mare', male_neutered: 'Gelding', female_spayed: 'Spayed mare', unknown: 'Unknown',
}
const BREEDER_WORDS: Record<string, Partial<Record<Sex, string>>> = {
  dog: { male: 'Dog', female: 'Bitch' },
  cat: { male: 'Tom', female: 'Queen' },
}

function isSex(v: string | null | undefined): v is Sex {
  return (SEXES as readonly string[]).includes(v as string)
}

/** Display label for a pet's sex — the ONE place the wording lives (#72).
 *  '' when not recorded; an unrecognised legacy string is shown as typed. */
export function sexLabel(sex: string | null | undefined, species: string | null | undefined, breederTerms = false): string {
  if (!sex) return ''
  if (!isSex(sex)) return sex
  const track = speciesTrack(species)
  if (track === 'horse') return HORSE_SEX_WORDS[sex]
  return (breederTerms && BREEDER_WORDS[track]?.[sex]) || SEX_WORDS[sex]
}

/** The sex picker's options for a species, worded the way `sexLabel` shows them. */
export function sexOptions(species: string | null | undefined, breederTerms = false): { value: Sex; label: string }[] {
  return SEXES.map((value) => ({ value, label: sexLabel(value, species, breederTerms) }))
}

// Life-stage bands (#73), in months of age. Cited, not invented:
// - Dogs: AAHA 2019 Canine Life Stage Guidelines
//   (https://www.aaha.org/resources/life-stage-canine-2019/canine-life-stage-definitions/):
//   puppy until rapid growth stops (~6–9 mo by size), young adult until
//   physical/social maturity (3–4 y), mature adult, then senior for the last 25%
//   of estimated lifespan. FurFiles has no size field, so it takes the UPPER end
//   of each AAHA range (9 mo, 4 y) — calling a still-growing giant-breed pup an
//   adult is the worse mistake — and a 12-year lifespan, the median longevity of
//   owned dogs in England (O'Neill et al. 2013, VetCompass, Vet J 198:638), which
//   puts senior at 9 y. A breed-size field would refine all three (#19).
// - Cats: 2021 AAHA/AAFP Feline Life Stage Guidelines (https://catvets.com/life-stage):
//   kitten birth–1 y, young adult 1–6 y, mature adult 7–10 y, senior over 10.
// - Horses: foal in the first year and yearling in the second (The Jockey Club
//   Principal Rules); colt (intact male) / filly (female) under 4, the common
//   usage (racing extends it through age 4); then the adult sex word; senior from
//   20 (Pugh, AAEP 2003 Horseman's Day, https://thehorse.com/152078/aaep-2003-geriatric-nutrition/;
//   some studies say 15, the later and more common cut-off is used here).
const DOG_STAGES: [number, string][] = [[9, 'Puppy'], [48, 'Young adult'], [108, 'Mature adult']]
const CAT_STAGES: [number, string][] = [[12, 'Kitten'], [84, 'Young adult'], [120, 'Mature adult']]

/** A pet's life stage from species, birth date and sex (#73), or null when the
 *  species has no cited bands or the birth date is unknown. Pure and derived:
 *  nothing is stored, so it moves on by itself as the pet ages. */
export function lifeStage(
  species: string | null | undefined,
  birthDate: string | null | undefined,
  sex: string | null | undefined,
  now: Date = new Date(),
): string | null {
  const months = ageInMonths(birthDate, now)
  if (months == null) return null
  const track = speciesTrack(species)
  if (track === 'dog' || track === 'cat') {
    const band = (track === 'dog' ? DOG_STAGES : CAT_STAGES).find(([upTo]) => months < upTo)
    return band ? band[1] : 'Senior'
  }
  if (track !== 'horse') return null
  if (months < 12) return 'Foal'
  if (months < 24) return 'Yearling'
  if (months >= 240) return 'Senior'
  if (months < 48) {
    if (sex === 'male') return 'Colt'
    if (sex === 'female' || sex === 'female_spayed') return 'Filly'
    if (sex === 'male_neutered') return 'Gelding'
    return 'Young horse'
  }
  return isSex(sex) && sex !== 'unknown' ? HORSE_SEX_WORDS[sex] : 'Adult'
}

/** The profile hero's sex and life-stage tags (#72, #73), each null when it
 *  shouldn't show. Horse words overlap: an adult horse's stage IS its sex word
 *  (Gelding), so the stage is dropped; and a young intact horse is a Colt or
 *  Filly, never a "Stallion" or "Mare", so the sex tag gives way to the stage.
 *  `unknown` is no more a fact than blank, so it shows nothing. */
export function sexAndStageTags(
  pet: { species: string; birth_date: string | null; sex: string | null; archived_at?: string | null },
  breederTerms = false,
  now: Date = new Date(),
): { sex: string | null; stage: string | null } {
  const sex = pet.sex && pet.sex !== 'unknown' ? sexLabel(pet.sex, pet.species, breederTerms) : null
  const stage = petLifeStage(pet, now)
  if (stage === 'Colt' || stage === 'Filly') {
    return { sex: pet.sex === 'female_spayed' ? sex : null, stage }
  }
  return { sex, stage: stage && stage !== sex ? stage : null }
}

/** `lifeStage` for a pet record. An archived pet gets none: a pet that passed
 *  would otherwise keep "ageing" into Senior after its death, and a rehomed pet's
 *  age is no longer this household's to track. */
export function petLifeStage(
  pet: { species: string; birth_date: string | null; sex: string | null; archived_at?: string | null },
  now: Date = new Date(),
): string | null {
  return pet.archived_at ? null : lifeStage(pet.species, pet.birth_date, pet.sex, now)
}

/** ISO date to "Aug 2024" for gotcha day / soft date display. Null-safe. */
export function monthYear(date: string | null | undefined): string | null {
  if (!date) return null
  const d = new Date(date)
  if (Number.isNaN(d.getTime())) return null
  return d.toLocaleDateString(undefined, { month: 'short', year: 'numeric', timeZone: 'UTC' })
}

/** ISO date to "Aug 1, 2026", parsed by parts so no timezone can shift the day.
 *  Null-safe; returns null for anything that isn't a real YYYY-MM-DD prefix. */
export function longDate(date: string | null | undefined): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(date || '')
  if (!m) return null
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]))
  if (Number.isNaN(d.getTime()) || d.getUTCDate() !== +m[3]) return null
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric', timeZone: 'UTC' })
}

/** True when an archived pet was rehomed (alive) rather than passed. A legacy
 *  archive with no kind is a memorial — that was the only meaning before #3. */
export function isRehomed(pet: { archived_at?: string | null; archive_kind?: string | null } | null | undefined): boolean {
  return !!pet && !!pet.archived_at && pet.archive_kind === 'rehomed'
}

/** The heading an archived pet's page and card carry (#3). Memorial wording only
 *  for a pet that passed; a rehomed pet is alive. */
export function archiveHeading(pet: { archived_at?: string | null; archive_kind?: string | null }): string {
  return isRehomed(pet) ? 'Moved on' : 'In loving memory'
}

// Single source of truth for cost categories AND their colors, shared by the
// pet timeline, the calendar, the costs donut, and the add-record picker so a
// category reads the same everywhere. Insertion order is the display order.
const CATEGORY_COLORS: Record<string, string> = {
  Appointments: 'var(--brand)',
  Prescriptions: '#F3B44C',
  Food: '#57C4CC',
  Boarding: 'var(--good)',
  Grooming: '#9B8CE0',
}
/** Ordered category list for pickers — derived so it can never drift from the
 *  color map. Default a new record to CATEGORIES[0] ("Appointments"). */
export const CATEGORIES: string[] = Object.keys(CATEGORY_COLORS)
const CATEGORY_FALLBACK = ['#57C4CC', '#9B8CE0', 'var(--good)', '#F3B44C', 'var(--ink-3)']

/** Stable color for a cost category. Unknown/absent categories get a
 *  deterministic fallback keyed off `i` so a legend stays visually distinct. */
export function categoryColor(category: string | null | undefined, i = 0): string {
  return (category && CATEGORY_COLORS[category]) || CATEGORY_FALLBACK[i % CATEGORY_FALLBACK.length]
}

// The one canonical paw glyph, shared by Paw.vue (logo / chrome) and
// SpeciesMark.vue (the 'paw' fallback) so there is a single source of truth for
// the shape. 24x24 viewBox, filled with currentColor.
export const PAW_PATH = 'M6.5 11.2c1.2 0 2.1-1.5 2.1-3.3S7.7 4.6 6.5 4.6 4.4 6.1 4.4 7.9s.9 3.3 2.1 3.3zM17.5 11.2c1.2 0 2.1-1.5 2.1-3.3s-.9-3.3-2.1-3.3-2.1 1.5-2.1 3.3.9 3.3 2.1 3.3zM9.9 8c1.05 0 1.9-1.36 1.9-3S10.95 2 9.9 2 8 3.36 8 5s.85 3 1.9 3zM14.1 8c1.05 0 1.9-1.36 1.9-3s-.85-3-1.9-3-1.9 1.36-1.9 3 .85 3 1.9 3zM12 12c-2.4 0-4.3 2.1-5.2 3.9-.5 1-1.5 1.9-1.5 3.1 0 1.6 1.3 2.6 2.9 2.3 1.3-.24 2.5-.8 3.8-.8s2.5.56 3.8.8c1.6.3 2.9-.7 2.9-2.3 0-1.2-1-2.1-1.5-3.1C16.3 14.1 14.4 12 12 12z'

// Species-aware placeholder track for the portrait watermark. The gradients in
// design.md cover dog/cat/horse/bird/fish, so the track set matches — anything
// else falls back to the generic paw. Free-text `species` is matched loosely
// (lowercase, contains). Order matters: the *fish checks run before dog/cat so
// "catfish"/"dogfish" resolve to a fish, not a paw of the wrong animal.
const SPECIES_TRACKS: [string, string[]][] = [
  ['fish', ['fish', 'goldfish', 'betta', 'guppy', 'koi', 'tetra', 'cichlid']],
  ['horse', ['horse', 'pony', 'equine', 'mare', 'gelding', 'stallion', 'colt', 'filly', 'foal']],
  ['bird', ['bird', 'parrot', 'parakeet', 'budgie', 'cockatiel', 'canary', 'finch', 'macaw', 'cockatoo']],
  ['dog', ['dog', 'puppy', 'pup', 'canine', 'hound']],
  ['cat', ['cat', 'kitten', 'kitty', 'feline']],
]

/** Normalize a free-text species to a portrait track key: one of
 *  'dog' | 'cat' | 'horse' | 'bird' | 'fish', else 'paw' for anything
 *  unrecognized. Single source of truth for the placeholder-track choice. */
export function speciesTrack(species: string | null | undefined): string {
  const s = String(species || '').toLowerCase()
  for (const [track, needles] of SPECIES_TRACKS) {
    if (needles.some((n) => s.includes(n))) return track
  }
  return 'paw'
}

// ---- shared contacts (#20) ----

/** Display label for a contact role. Single source of truth for the wording,
 *  shared by the contacts page, the care-team section, and the contact sheet so
 *  a role reads the same everywhere. Typed as `Record<ContactRole, string>`, so
 *  adding a role to `CONTACT_ROLES` without a label here (or vice versa) is a
 *  compile error — the two can't drift. */
const CONTACT_ROLE_LABELS: Record<ContactRole, string> = {
  vet: 'Vet',
  emergency: 'Emergency vet',
  specialist: 'Specialist',
  groomer: 'Groomer',
  boarding: 'Daycare & boarding',
  other: 'Other',
}
export function contactRoleLabel(role: string | null | undefined): string {
  return (role && CONTACT_ROLE_LABELS[role as ContactRole]) || 'Contact'
}

// ---- pet milestones (#32) ----
// Birthday and gotcha day are derived from the pet's own fields — no schema.
// The timeline shows the ONE-TIME actual dates; the calendar shows the RECURRING
// yearly anniversary. Both live here so the two surfaces stay in sync.

/** The two anniversary-bearing fields; the loose shapes below accept both real
 *  pets and the partial fixtures the milestone tests pass. */
type AnniversaryField = 'birth_date' | 'gotcha_date'
interface MilestonePet {
  name: string
  birth_date?: string | null
  gotcha_date?: string | null
}
interface AnniversaryPet extends MilestonePet {
  id: string
  archived_at?: string | null
}
export interface Milestone { date: string; subkind: string; title: string }
export interface MonthMilestone { day: number; petId: string; petName: string; subkind: string; title: string }
export interface UpcomingMilestone { subkind: string; date: string; petId: string; petName: string; title: string }

// [pet field, subkind, anniversary verb]. Single source for the calendar month
// grid AND the agenda so the two can't drift.
const ANNIVERSARY_FIELDS: [AnniversaryField, string, string][] = [
  ['birth_date', 'birthday', 'birthday'],
  ['gotcha_date', 'gotcha', 'gotcha day'],
]

/** The glyph for a milestone subkind, shared by every surface that renders one. */
export function milestoneIcon(subkind: string | null | undefined): string {
  return subkind === 'birthday' ? '🎂' : '🏡'
}

/** One-time life milestones for a pet's timeline: born (birth_date) and home
 *  (gotcha_date). Returns [{ date, subkind, title }], skipping unknown dates. */
export function petMilestones(pet: MilestonePet | null | undefined): Milestone[] {
  const out: Milestone[] = []
  if (!pet) return out
  if (pet.birth_date) out.push({ date: pet.birth_date, subkind: 'birthday', title: `${pet.name} was born` })
  if (pet.gotcha_date) out.push({ date: pet.gotcha_date, subkind: 'gotcha', title: `${pet.name} came home` })
  return out
}

/** The next yearly recurrence of an ISO date's MM-DD on/after `from` (ISO), as
 *  ISO. Feb 29 clamps to the last day of Feb in a non-leap year. Null-safe. */
export function nextAnniversary(iso: string | null | undefined, from: string = todayISO()): string | null {
  if (!iso) return null
  const [, mm, dd] = String(iso).split('-').map(Number)
  if (!mm || !dd) return null
  const fromYear = Number(from.split('-')[0])
  for (const y of [fromYear, fromYear + 1]) {
    const last = new Date(y, mm, 0).getDate() // days in month mm (1-based)
    const day = Math.min(dd, last)
    const cand = `${y}-${String(mm).padStart(2, '0')}-${String(day).padStart(2, '0')}`
    if (cand >= from) return cand
  }
  return null
}

/** Recurring milestone anniversaries that land in the given year/month (0-based),
 *  for the calendar month grid. ACTIVE pets only — an archived pet (passed OR
 *  rehomed, #3) doesn't get "upcoming birthday" chips. Returns [{ day, petId, petName, subkind, title }];
 *  the day is clamped to the month's length (Feb 29 → 28 in a non-leap year). */
export function monthMilestones(pets: AnniversaryPet[] | null | undefined, year: number, month: number): MonthMilestone[] {
  const out: MonthMilestone[] = []
  const daysInMonth = new Date(year, month + 1, 0).getDate()
  for (const p of pets || []) {
    if (!p || p.archived_at) continue
    for (const [field, subkind, verb] of ANNIVERSARY_FIELDS) {
      const iso = p[field]
      if (!iso) continue
      const [, mm, dd] = String(iso).split('-').map(Number)
      if (!mm || !dd || mm - 1 !== month) continue
      out.push({ day: Math.min(dd, daysInMonth), petId: p.id, petName: p.name, subkind, title: `${p.name}’s ${verb}` })
    }
  }
  return out
}

/** The NEXT upcoming occurrence of each active pet's milestones on/after `from`,
 *  for the calendar agenda (#32). Same field set and phrasing as
 *  [`monthMilestones`]; archived pets (passed or rehomed) are excluded. Returns
 *  [{ subkind, date, petId, petName, title }]. */
export function upcomingMilestones(pets: AnniversaryPet[] | null | undefined, from: string = todayISO()): UpcomingMilestone[] {
  const out: UpcomingMilestone[] = []
  for (const p of pets || []) {
    if (!p || p.archived_at) continue
    for (const [field, subkind, verb] of ANNIVERSARY_FIELDS) {
      if (!p[field]) continue
      const date = nextAnniversary(p[field], from)
      if (!date) continue
      out.push({ subkind, date, petId: p.id, petName: p.name, title: `${p.name}’s ${verb}` })
    }
  }
  return out
}

/** Names as prose: "Pedro", "Pedro and Lulu", "Pedro, Lulu and Kipper". */
export function andList(names: string[]): string {
  if (names.length <= 1) return names[0] || ''
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/** A pet's two portrait-gradient stops, with the warm brand fallback. Shared by
 *  the CSS gradient below and the family-portrait canvas (#75). */
export function petColors(pet: { gradient_a?: string | null; gradient_b?: string | null } | null | undefined): [string, string] {
  return [(pet && pet.gradient_a) || '#F2624C', (pet && pet.gradient_b) || '#DB4E38']
}

/** CSS gradient string from a pet's two stops, with a warm fallback. */
export function petGradient(pet: { gradient_a?: string | null; gradient_b?: string | null } | null | undefined): string {
  const [a, b] = petColors(pet)
  return `linear-gradient(140deg, ${a} 0%, ${b} 100%)`
}

/** The shape of a rendered weight trend: an SVG path, its endpoint, and the box. */
export interface WeightTrend { line: string; last: number[]; w: number; h: number }

/** SVG path (smooth Catmull-Rom) through [x,y] points, for trend charts. */
export function smoothPath(pts: number[][] | null | undefined): string {
  if (!pts || pts.length < 2) return ''
  let d = `M ${pts[0][0].toFixed(1)},${pts[0][1].toFixed(1)}`
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] || pts[i]
    const p1 = pts[i]
    const p2 = pts[i + 1]
    const p3 = pts[i + 2] || p2
    const c1x = p1[0] + (p2[0] - p0[0]) / 6
    const c1y = p1[1] + (p2[1] - p0[1]) / 6
    const c2x = p2[0] - (p3[0] - p1[0]) / 6
    const c2y = p2[1] - (p3[1] - p1[1]) / 6
    d += ` C ${c1x.toFixed(1)},${c1y.toFixed(1)} ${c2x.toFixed(1)},${c2y.toFixed(1)} ${p2[0].toFixed(1)},${p2[1].toFixed(1)}`
  }
  return d
}

/** Map weight entries to an SVG path + endpoint within a w×h box. */
export function weightTrend(weights: { value: number }[] | null | undefined, w = 300, h = 120): WeightTrend | null {
  if (!weights || weights.length < 2) return null
  const vals = weights.map((e) => e.value)
  const min = Math.min(...vals)
  const max = Math.max(...vals)
  const rng = max - min || 1
  const pad = 16
  const pts = weights.map((e, i) => [
    (i / (weights.length - 1)) * (w - pad * 2) + pad,
    h - 14 - ((e.value - min) / rng) * (h - 40),
  ])
  return { line: smoothPath(pts), last: pts[pts.length - 1], w, h }
}
