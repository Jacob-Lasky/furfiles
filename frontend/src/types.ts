// The FurFiles API contract, mirrored from the Rust backend's serialized types
// (backend/src/models.rs + ingestion.rs). This is the single source of truth for
// what the REST API returns and accepts on the frontend; keep it in step with
// the Rust structs — a field renamed there is a bug detectable here at compile
// time (the whole point of #51). JSON keys are snake_case: the backend does not
// rename fields, so the wire names match the Rust field names verbatim.

export interface User {
  id: string
  name: string
  color: string
  created_at: string
}

export interface Pet {
  id: string
  name: string
  species: string
  breed: string | null
  /** One of SEXES (#72), or null when not recorded. Display it with `sexLabel`. */
  sex: Sex | null
  birth_date: string | null
  gotcha_date: string | null
  weight: number | null
  weight_unit: string | null
  microchip: string | null
  /** Coat colour / markings ("Blue Merle", "Gold") — identity, like breed (#63). */
  color: string | null
  /** Known allergies: label = allergen, detail = reaction. null when none are
   *  recorded, which is also a vet intake form's "No" (#63). */
  allergies: LabeledDetail[] | null
  insurer: string | null
  insurer_reimburse: string | null
  gradient_a: string | null
  gradient_b: string | null
  photo_path: string | null
  archived_at: string | null
  /** How the pet left the active household (#3); null while active. */
  archive_kind: ArchiveKind | null
  /** Shown with an archived pet: a memorial line, or where a rehomed pet went. */
  memorial_note: string | null
  created_by: string | null
  created_at: string
  updated_at: string
}

/** A pet's sex (#72). Mirrors `SEXES` in backend/src/models.rs — keep the two in
 *  step. Stored plain; the species word people use (Gelding, Spayed mare, Tom)
 *  comes from `sexLabel` in format.ts and is never stored. Breeding status (stud,
 *  dam) is not a sex value; that belongs with lineage (#18). */
export const SEXES = ['male', 'female', 'male_neutered', 'female_spayed', 'unknown'] as const
export type Sex = (typeof SEXES)[number]

/** A spay/neuter record on a pet whose sex still says intact (#72), derived on
 *  every read: the profile offers a one-tap update to `sex`. */
export interface SexSuggestion {
  sex: Sex
  record_id: number
  record_title: string
  occurred_on: string
}

/** Mirrors `ARCHIVE_KINDS` in backend/src/models.rs. `passed` is remembered;
 *  `rehomed` (sold, adopted out, moved on) is alive and never gets memorial
 *  wording. */
export type ArchiveKind = 'passed' | 'rehomed'

/** POST /pets/:id/archive — archive a pet, or edit an existing archive. */
export interface ArchivePetInput {
  kind: ArchiveKind
  archived_at: string | null
  memorial_note: string | null
}

/** One itemized charge on a record's cost breakdown. `amount_cents` is the line
 *  total in cents (money is never a float); null if unknown. */
export interface LineItem {
  description: string
  amount_cents: number | null
}

export interface MedicalRecord {
  id: number
  pet_id: string
  occurred_on: string
  ends_on: string | null
  title: string
  notes: string | null
  category: string
  cost_cents: number | null
  covered_cents: number | null
  source: string
  /** The shared contact (clinic/vet) this record happened at (#20), or null. */
  contact_id: string | null
  extra: Record<string, string> | null
  line_items: LineItem[] | null
  created_by: string | null
  created_at: string
}

/** A generic labeled free-text detail (label + optional detail). Reused for a
 *  contact's requirements (#42) AND its hours (#42 follow-up) — same shape, one
 *  type, mirrors `LabeledDetail` in models.rs. */
export interface LabeledDetail {
  label: string
  detail: string | null
}

/** A shared contact — vet, emergency vet, specialist, groomer, or (via #42)
 *  daycare/boarding facility — entered once and linked to many pets (#20). */
export interface Contact {
  id: string
  name: string
  role: string
  doctor: string | null
  phone: string | null
  email: string | null
  address: string | null
  website: string | null
  notes: string | null
  /** Facility requirements (#42): a checklist, or null when there are none. */
  requirements: LabeledDetail[] | null
  /** Hours (#42 follow-up): open hours + drop-off/pick-up windows, or null. */
  hours: LabeledDetail[] | null
  created_by: string | null
  created_at: string
  updated_at: string
}

/** The contact roles the backend accepts; drives the role picker. Mirrors
 *  `CONTACT_ROLES` in handlers.rs — keep the two in step. */
export const CONTACT_ROLES = [
  'vet', 'emergency', 'specialist', 'groomer', 'boarding', 'other',
] as const
export type ContactRole = (typeof CONTACT_ROLES)[number]

/** Plan kinds the backend accepts (`INSURANCE_KINDS` in models.rs). `insurance`
 *  is a reimbursement policy; `wellness` a vet preventative-care membership. */
export const INSURANCE_KINDS = ['insurance', 'wellness'] as const
export type InsuranceKind = (typeof INSURANCE_KINDS)[number]
/** Policy lifecycle states (`INSURANCE_STATUSES` in models.rs). */
export const INSURANCE_STATUSES = ['active', 'expired', 'cancelled'] as const
export type InsuranceStatus = (typeof INSURANCE_STATUSES)[number]

/** A pet's insurance policy or wellness plan (#8/#29). Money is INTEGER cents.
 *  `coverages`/`exclusions` reuse LabeledDetail. A policy embeds ONE document;
 *  its internal path never leaves the server — the client learns a document
 *  exists from `document_name`/`document_media_type`. Mirrors `InsurancePolicy`
 *  in models.rs. */
export interface InsurancePolicy {
  id: string
  pet_id: string
  kind: string
  provider: string
  plan_name: string | null
  policy_number: string | null
  status: string
  start_date: string | null
  end_date: string | null
  premium_cents: number | null
  premium_interval: string | null
  reimburse_percent: number | null
  deductible_cents: number | null
  annual_limit_cents: number | null
  notes: string | null
  coverages: LabeledDetail[] | null
  exclusions: LabeledDetail[] | null
  document_name: string | null
  document_media_type: string | null
  document_size_bytes: number | null
  created_by: string | null
  created_at: string
  updated_at: string
}

/** A file attached to a record. Bytes live on disk; this is the metadata. */
export interface Attachment {
  id: string
  record_id: number
  filename: string
  media_type: string
  size_bytes: number
  source: string
  /** Hex SHA-256 of the bytes (#23); null only for a legacy row whose file was missing. */
  sha256: string | null
  created_at: string
}

/** A record together with its attachments (the pet-profile timeline shape).
 *  `contact` is the resolved clinic/vet when `contact_id` is set (#20). */
export interface RecordDetail extends MedicalRecord {
  attachments: Attachment[]
  contact: Contact | null
}

/** A record that may or may not carry its attachments: the timeline delivers
 *  RecordDetail, the calendar delivers a bare MedicalRecord (attachments are
 *  fetched on demand). The record sheet accepts either. */
export type RecordLike = MedicalRecord & { attachments?: Attachment[] }

export interface WeightEntry {
  id: number
  pet_id: string
  recorded_on: string
  value: number
  unit: string
}

export interface Reminder {
  id: number
  pet_id: string
  due_on: string
  title: string
  kind: string | null
  done: boolean
  source: string
  created_at: string
}

/** A reminder joined with its pet's name, for the cross-pet calendar. */
export interface ReminderWithPet {
  id: number
  pet_id: string
  pet_name: string
  due_on: string
  title: string
  kind: string | null
  done: boolean
}

/** Pet plus its soonest upcoming reminder ("what's next"), for list views
 * (GET /pets). The care score was removed (#35) — it graded the owner; this is
 * a neutral, actionable fact instead. Null when the pet is all caught up. */
export interface PetSummary extends Pet {
  next_reminder: Reminder | null
}

/** Full profile payload for the pet detail view (GET /pets/:id). */
export interface PetDetail extends Pet {
  /** The soonest not-done reminder ("what's next"), derived from `reminders`. */
  next_reminder: Reminder | null
  records: RecordDetail[]
  weights: WeightEntry[]
  reminders: Reminder[]
  /** The pet's care team: the shared contacts linked to it (#20). */
  contacts: Contact[]
  /** Insurance policies + wellness plans on this pet (#8/#29). */
  insurance_policies: InsurancePolicy[]
  /** Standing care: food, preventatives, medications, supplements (#63).
   *  Current entries (no `ended_on`) first. */
  regimens: Regimen[]
  /** A spay/neuter record while the sex is still intact (#72), or null. */
  sex_suggestion: SexSuggestion | null
}

export interface CategoryTotal {
  category: string
  cost_cents: number
}

export interface PetCostRow {
  pet_id: string
  name: string
  insurer: string | null
  cost_cents: number
  covered_cents: number
}

export interface CostsSummary {
  total_cents: number
  covered_cents: number
  by_category: CategoryTotal[]
  by_pet: PetCostRow[]
}

/** Safe ingestion-settings view. The API key is write-only server-side; only
 *  its presence is reported (`has_key`). */
export interface IngestionSettings {
  enabled: boolean
  provider: string
  model: string
  has_key: boolean
}

/** One model the key can call, for the Settings picker (#67). Mirrors
 *  `ProviderModel` in ingestion.rs; the list arrives newest first. */
export interface ProviderModel {
  id: string
  display_name: string
  created_at: string | null
}

/** Household display preferences (GET/PUT /settings/display, #72). */
export interface DisplaySettings {
  /** Breeder terms for intact dogs and cats: Dog / Bitch, Tom / Queen. */
  breeder_terms: boolean
}

// ---- extraction (#4/#26) ----

export interface ExtractedLineItem {
  description: string
  /** Dollars as the model reads it; converted to cents on save. */
  amount: number | null
}

export interface ExtractedWeight {
  value: number | null
  unit: string | null
}

export interface ExtractedReminder {
  due_on: string | null
  title: string
}

export interface ExtractedFields {
  occurred_on: string | null
  title: string | null
  category: string | null
  clinic: string | null
  cost: number | null
  notes: string | null
  suggested_pet_name: string | null
  weight: ExtractedWeight | null
  reminders: ExtractedReminder[]
  line_items: ExtractedLineItem[]
  extra_fields: Record<string, string>
  low_confidence_fields: string[]
}

export interface ExtractResponse {
  fields: ExtractedFields
  provider: string
  model: string
}

/** Care kinds the backend accepts (`REGIMEN_KINDS` in models.rs) — keep in step.
 *  Ordered the way the profile shows them. */
export const REGIMEN_KINDS = ['food', 'preventative', 'medication', 'supplement'] as const
export type RegimenKind = (typeof REGIMEN_KINDS)[number]

/** What a preventative protects against (`PROTECTIONS` in models.rs). A closed
 *  set so "is this pet current on heartworm prevention?" has one answer instead
 *  of depending on how the brand name was typed. */
export const PROTECTIONS = ['flea', 'tick', 'heartworm', 'worm'] as const
export type Protection = (typeof PROTECTIONS)[number]

/** What a pet is ongoingly ON: its food, preventatives, medications and
 *  supplements (#63). One entity rather than flat diet/flea/tick/heartworm
 *  columns — a vet form asks for food and drugs in the same "name, amount,
 *  frequency" shape, and one product (Simparica Trio) covers several
 *  protections at once. `ended_on: null` means the pet is still on it. */
export interface Regimen {
  id: string
  pet_id: string
  kind: string
  product: string
  /** Dose or serving: "1 cup", "1 pill". */
  amount: string | null
  /** "2x/day", "monthly". */
  frequency: string | null
  /** Protections a preventative covers; null for food/medication/supplement. */
  protects: string[] | null
  started_on: string | null
  /** null = still on it. */
  ended_on: string | null
  notes: string | null
  created_by: string | null
  created_at: string
  updated_at: string
}

export interface NewRegimenInput {
  kind?: string
  product?: string
  amount?: string | null
  frequency?: string | null
  protects?: string[] | null
  started_on?: string | null
  ended_on?: string | null
  notes?: string | null
  created_by?: string | null
}
/** PATCH /regimens/:rid (`UpdateRegimen` in models.rs) — kind/product are
 *  set-or-keep, the rest null-vs-absent. `created_by` is NOT accepted on an
 *  edit, so it is absent here rather than inherited from the create shape. */
export interface UpdateRegimenInput {
  kind?: string
  product?: string
  amount?: string | null
  frequency?: string | null
  protects?: string[] | null
  started_on?: string | null
  ended_on?: string | null
  notes?: string | null
}

// ---- request payloads ----
// Fields are optional here on purpose. Most of these also back PATCH endpoints
// that are genuinely partial (send only what changed), and for the POST paths
// the backend is the single validator of what's required — mirroring those
// rules here would fork a contract the server already owns and enforces (400 on
// a missing field). The client's job is to get the field NAMES right, which
// these types do enforce.

/** PATCH /pets/:id (`UpdatePet` in models.rs). Every key except name/species is
 *  null-vs-absent on the server: OMIT a key to keep the stored value, send
 *  `null` to CLEAR it. */
export interface UpdatePetInput {
  name?: string
  species?: string
  breed?: string | null
  sex?: Sex | null
  birth_date?: string | null
  gotcha_date?: string | null
  weight?: number | null
  weight_unit?: string | null
  microchip?: string | null
  color?: string | null
  allergies?: LabeledDetail[] | null
  insurer?: string | null
  insurer_reimburse?: string | null
}

/** POST /pets (`NewPet` in models.rs). A superset of the update shape: the
 *  portrait gradient and `created_by` are settable only at creation, so they
 *  live here and NOT on [`UpdatePetInput`] — the server ignores them on PATCH. */
export interface NewPetInput extends UpdatePetInput {
  gradient_a?: string | null
  gradient_b?: string | null
  created_by?: string | null
}

export interface RecordInput {
  occurred_on?: string
  ends_on?: string | null
  title?: string
  category?: string
  cost_cents?: number | null
  covered_cents?: number | null
  source?: string
  notes?: string | null
  /** Set-or-clear the clinic/vet link (#20): omit to keep, null to unlink. */
  contact_id?: string | null
  extra?: Record<string, string> | null
  line_items?: LineItem[]
  created_by?: string | null
}

/** Create/replace a contact (#20). The backend validates name + role. */
export interface NewContactInput {
  name: string
  role?: string
  doctor?: string | null
  phone?: string | null
  email?: string | null
  address?: string | null
  website?: string | null
  notes?: string | null
  /** Facility requirements (#42); omit to keep, null/[] to clear. */
  requirements?: LabeledDetail[] | null
  /** Contact hours (#42 follow-up); omit to keep, null/[] to clear. */
  hours?: LabeledDetail[] | null
}
/** Partial contact edit; an omitted key keeps the value, null clears it. */
export type UpdateContactInput = Partial<NewContactInput>

// ---- facility web parser (#42) ----

/** Fields the LLM proposes from a facility's website (mirrors `FacilityFields`
 *  in ingestion.rs). The sheet pre-fills from these; nothing is saved until the
 *  user confirms. */
export interface FacilityFields {
  name: string | null
  /** One of CONTACT_ROLES ('boarding' covers daycare), or null if unclear. */
  role: string | null
  /** The contact person: the vet at a clinic, else the named owner/manager. */
  doctor: string | null
  phone: string | null
  email: string | null
  address: string | null
  notes: string | null
  requirements: LabeledDetail[]
  hours: LabeledDetail[]
}

export interface ParseUrlResponse {
  fields: FacilityFields
  provider: string
  model: string
}

// ---- insurance / wellness-plan extraction (#8/#29) ----

/** Fields the LLM proposes from an insurance/plan document (mirrors
 *  `InsuranceFields` in ingestion.rs). Money amounts are DOLLARS as the model
 *  reads them; the sheet converts to cents on save, like a record's cost. The
 *  sheet pre-fills from these; nothing is saved until the user confirms. */
export interface InsuranceFields {
  kind: string | null
  provider: string | null
  plan_name: string | null
  policy_number: string | null
  start_date: string | null
  end_date: string | null
  premium: number | null
  premium_interval: string | null
  reimburse_percent: number | null
  deductible: number | null
  annual_limit: number | null
  notes: string | null
  coverages: LabeledDetail[]
  exclusions: LabeledDetail[]
  low_confidence_fields: string[]
}

export interface InsuranceExtractResponse {
  fields: InsuranceFields
  provider: string
  model: string
}

/** Create an insurance policy (#8/#29). The backend validates provider/kind/status. */
export interface NewInsuranceInput {
  kind?: string
  provider: string
  plan_name?: string | null
  policy_number?: string | null
  status?: string
  start_date?: string | null
  end_date?: string | null
  premium_cents?: number | null
  premium_interval?: string | null
  reimburse_percent?: number | null
  deductible_cents?: number | null
  annual_limit_cents?: number | null
  notes?: string | null
  coverages?: LabeledDetail[] | null
  exclusions?: LabeledDetail[] | null
}
/** Partial policy edit; an omitted key keeps the value, null clears it. */
export type UpdateInsuranceInput = Partial<NewInsuranceInput>

/** Attach/replace a policy's document. */
export interface InsuranceDocumentInput {
  filename: string
  media_type: string
  data_base64: string
}

// ---- insurance value (#74): mirrors backend/src/insurance_value.rs ----

/** Where a term stands on `as_of`. `unknown` = no start date recorded. */
export type PolicyPhase = 'upcoming' | 'current' | 'ended' | 'unknown'

/** A visit payout counted toward a term. `claim` = its "Insurance claim …"
 *  note names this policy; `term` = its date falls inside the term. */
export interface InsurancePayout {
  record_id: number
  occurred_on: string
  title: string
  amount_cents: number
  matched_by: 'claim' | 'term'
}

/** Accident & illness break-even: `bill_cents = deductible + premium / rate`,
 *  rounded up. `bill_cents` is null (and `reachable` false) when no single
 *  bill can return the premium (0%, or an annual limit below it). */
export interface BreakEven {
  bill_cents: number | null
  premium_cents: number
  deductible_cents: number
  reimburse_percent: number
  annual_limit_cents: number | null
  reachable: boolean
}

export interface SubLimit {
  label: string
  limit_cents: number
  /** Known only while nothing has been paid back this term (then = limit). */
  unused_cents: number | null
}

/** Preventative sub-limits from the coverages. Payouts are recorded per visit,
 *  not per benefit, so only the lower bound `unused_at_least_cents` is exact. */
export interface Preventative {
  allowance_cents: number
  unused_at_least_cents: number
  limits: SubLimit[]
}

/** One policy term, valued (GET /insurance/:pid/value). */
export interface PolicyValue {
  policy_id: string
  pet_id: string
  kind: string
  provider: string
  plan_name: string | null
  policy_number: string | null
  status: string
  start_date: string | null
  /** Recorded end, or start + 1 year for an active policy (see `assumed`). */
  end_date: string | null
  phase: PolicyPhase
  annual_premium_cents: number | null
  /** The whole term, prorated by day to its real end. */
  term_premium_cents: number | null
  /** Paid in up to and including `as_of`. */
  paid_in_cents: number | null
  paid_back_cents: number
  /** paid back − paid in; negative = more paid in than back. */
  net_cents: number | null
  payouts: InsurancePayout[]
  break_even: BreakEven | null
  preventative: Preventative | null
  /** Inputs a number needed but the policy lacks, by name ("deductible"…). */
  missing: string[]
  /** Defaults leaned on ("end date", "premium per year"). */
  assumed: string[]
}

/** Every term of one pet (GET /pets/:id/insurance/value). */
export interface PetInsuranceValue {
  pet_id: string
  name: string
  as_of: string
  /** Current term first, then upcoming, then most recent start. */
  policies: PolicyValue[]
  /** Totals over the INSURANCE terms. A wellness plan pays at the counter, so
   *  its fee is summed apart in `wellness_paid_in_cents`. */
  paid_in_cents: number
  paid_back_cents: number
  net_cents: number
  /** Some insurance term lacks what paid-in needs, so the sum is short. */
  paid_in_incomplete: boolean
  /** Wellness-plan fees paid in to date. */
  wellness_paid_in_cents: number
  /** Payouts no term or claim note covers. */
  unmatched_paid_back_cents: number
}

/** Pets with at least one policy (GET /insurance/value). */
export interface InsuranceValueSummary {
  as_of: string
  pets: PetInsuranceValue[]
}

export interface WeightInput {
  recorded_on?: string
  value?: number
  unit?: string
}

export interface ReminderInput {
  due_on?: string
  title?: string
  kind?: string | null
  source?: string
}
export interface ReminderUpdateInput {
  done?: boolean
  due_on?: string
  title?: string
}

export interface AttachmentInput {
  filename: string
  media_type: string
  data_base64: string
  source?: string
}

export interface PetPhotoInput {
  media_type: string
  data_base64: string
}

/** Where the family portrait came from (#75). Mirrors `PORTRAIT_SOURCES` in
 *  backend/src/household.rs. */
export type PortraitSource = 'uploaded' | 'generated'

/** The household's family portrait (GET /household). The bytes are served at
 *  the stable `/api/household/portrait`; `version` changes on every replace. */
export interface HouseholdPortrait {
  source: PortraitSource
  /** Sorted ids of the pets a generated portrait was composed from; [] when uploaded. */
  pet_ids: string[]
  version: string
  updated_at: string
  /** A generated portrait no longer matches the active pets (one was added,
   *  archived, rehomed, brought back or deleted). Always false when uploaded. */
  roster_changed: boolean
}

export interface Household {
  portrait: HouseholdPortrait | null
}

/** PUT /household/portrait. `pet_ids` is required for `generated` and must be
 *  empty for `uploaded` (the default). */
export interface HouseholdPortraitInput extends PetPhotoInput {
  source?: PortraitSource
  pet_ids?: string[]
}

export interface IngestionInput {
  enabled?: boolean
  provider?: string
  model?: string
  api_key?: string
}

/** PUT /settings/display — an omitted field is left unchanged. */
export interface DisplaySettingsInput {
  breeder_terms?: boolean
}

export interface ExtractInput {
  filename?: string
  media_type: string
  data_base64: string
}

/** A best-effort client error report (#41), optionally with the file that
 *  triggered it. `url`/`file` are filled in by the api layer. */
export interface DiagReportInput {
  context: string
  message: string
  stack?: string | null
  url?: string
  extra?: Record<string, string>
}

// ---------- Bulk import (#23) ----------
// Mirrors `backend/src/import.rs`. A plan round-trips: `/import/analyze`
// returns it, the screen toggles `selected` / edits fields, `/import/commit`
// takes it back with the file. Each item's `selected` starts as the server's
// recommendation (duplicates and overwrites start off).

/** A record that already carries this exact file (matched by SHA-256). */
export interface FileDuplicate {
  attachment_id: string
  record_id: number
  pet_id: string
  pet_name: string
  title: string
  occurred_on: string
  filename: string
}

export interface PlannedClinic {
  selected: boolean
  /** An existing contact this clinic IS; null → commit creates a new contact. */
  contact_id: string | null
  matched_name: string | null
  name: string
  role: string
  phone: string | null
  email: string | null
  address: string | null
  website: string | null
}

export interface RecordRef {
  record_id: number
  title: string
  occurred_on: string
  reason: string
}

export interface PlannedRecord {
  selected: boolean
  /** null when the model couldn't tell whose record it is — the user picks. */
  pet_id: string | null
  pet_name_on_document: string | null
  occurred_on: string
  ends_on: string | null
  title: string
  category: string
  cost_cents: number | null
  line_items: LineItem[]
  notes: string | null
  extra: Record<string, string>
  duplicate_of: RecordRef | null
}

export interface PlannedLink { selected: boolean; pet_id: string }

/** A saved visit this document duplicates that has no clinic yet. */
export interface PlannedClinicLink {
  selected: boolean
  record_id: number
  pet_id: string
  title: string
  occurred_on: string
}

export interface PlannedWeight {
  selected: boolean
  pet_id: string
  recorded_on: string
  value: number
  unit: string
  /** Set when this corrects a weight already logged on that date. */
  existing_id: number | null
  existing_value: number | null
  existing_unit: string | null
  /** The document didn't print when it was measured; starts unticked. */
  date_is_inferred: boolean
}

export interface PlannedReminder {
  selected: boolean
  pet_id: string
  due_on: string
  title: string
  duplicate_of: number | null
  /** Another open reminder this pet has the same day under a different title. */
  same_day: string | null
}

export interface PlannedCompletion {
  selected: boolean
  reminder_id: number
  pet_id: string
  title: string
  due_on: string
  reason: string | null
}

export interface PlannedPetUpdate {
  selected: boolean
  pet_id: string
  field: string
  current: string | null
  value: string
  reason: string | null
}

export interface PlannedRegimen {
  selected: boolean
  pet_id: string
  kind: string
  product: string
  amount: string | null
  frequency: string | null
  protects: string[]
  started_on: string | null
  ended_on: string | null
  notes: string | null
  duplicate_of: string | null
  /** A current entry for the same product at another dose; saving ends it. */
  replaces: string | null
}

export interface ClaimItem {
  description: string
  cost_cents: number | null
  covered_cents: number | null
  note: string | null
}

/** An insurance payout: recorded on the visit it reimbursed (`record_id`), or
 *  as a visit of its own when none is saved. */
export interface PlannedClaim {
  selected: boolean
  pet_id: string | null
  claim_number: string
  policy_number: string | null
  insurer: string | null
  treatment_date: string
  items: ClaimItem[]
  paid_cents: number
  record_id: number | null
  record_title: string | null
  /** All of the pet's saved visits that day, when there's more than one. */
  visit_options: VisitOption[]
  already_recorded: boolean
}

export interface VisitOption {
  record_id: number
  title: string
  cost_cents: number | null
}

export interface PlannedPolicy {
  selected: boolean
  pet_id: string | null
  kind: string
  provider: string
  plan_name: string | null
  policy_number: string | null
  start_date: string | null
  end_date: string | null
  premium_cents: number | null
  premium_interval: string | null
  reimburse_percent: number | null
  deductible_cents: number | null
  annual_limit_cents: number | null
  notes: string | null
  coverages: LabeledDetail[]
  exclusions: LabeledDetail[]
  existing_id: string | null
}

/** An existing policy the new one replaces; saving marks it expired. */
export interface PlannedPolicyEnd {
  selected: boolean
  policy_id: string
  pet_id: string
  provider: string
  policy_number: string | null
  end_date: string
  /** Index into `policies` of the replacing policy; only ended alongside it. */
  replaced_by: number
  seen_end_date: string | null
}

export interface ImportPlan {
  sha256: string
  file_duplicates: FileDuplicate[]
  summary: string | null
  clinic: PlannedClinic | null
  records: PlannedRecord[]
  care_team: PlannedLink[]
  clinic_links: PlannedClinicLink[]
  weights: PlannedWeight[]
  reminders: PlannedReminder[]
  completed_reminders: PlannedCompletion[]
  pet_updates: PlannedPetUpdate[]
  regimens: PlannedRegimen[]
  claims: PlannedClaim[]
  policies: PlannedPolicy[]
  policy_ends: PlannedPolicyEnd[]
  low_confidence_fields: string[]
  provider: string
  model: string
}

export interface ImportAnalyzeInput {
  filename?: string
  media_type: string
  data_base64: string
}

export interface ImportCommitInput {
  filename: string
  media_type: string
  data_base64: string
  plan: ImportPlan
}

export interface ImportResult {
  records: MedicalRecord[]
  contact: Contact | null
  care_team_links: number
  clinic_links: number
  weights_added: number
  weights_corrected: number
  reminders_added: number
  reminders_completed: number
  pet_updates: number
  regimens_added: number
  regimens_ended: number
  claims_recorded: number
  policies_added: number
  policies_ended: number
  skipped_existing: number
}

export interface WeightUpdateInput {
  recorded_on?: string
  value?: number
  unit?: string
}
