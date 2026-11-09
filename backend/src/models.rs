use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use sqlx::types::Json;
use sqlx::FromRow;

/// Deserialize helper that distinguishes an ABSENT JSON key from an explicit
/// `null`, so a PATCH can both leave a field untouched (key omitted) and clear
/// it (key present as `null`). Combined with `#[serde(default)]` on the field:
/// absent → `None` (keep the stored value), `null` → `Some(None)` (clear),
/// value → `Some(Some(v))` (set). Plain `Option<T>` can't tell absent from null,
/// which is why a full-editor PATCH (the record edit modal, #15) needs this.
pub(crate) fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Pet {
    pub id: String,
    pub name: String,
    pub species: String,
    pub breed: Option<String>,
    pub sex: Option<String>,
    pub birth_date: Option<String>,
    pub gotcha_date: Option<String>,
    pub weight: Option<f64>,
    pub weight_unit: Option<String>,
    pub microchip: Option<String>,
    /// Coat colour / markings ("Blue Merle", "Gold") — identity, like breed (#63).
    pub color: Option<String>,
    /// Known allergies as [`LabeledDetail`]: label = allergen, detail = reaction.
    /// NULL when none are recorded, which is also the intake form's "No" (#63).
    pub allergies: Option<Json<Vec<LabeledDetail>>>,
    pub insurer: Option<String>,
    pub insurer_reimburse: Option<String>,
    pub gradient_a: Option<String>,
    pub gradient_b: Option<String>,
    pub photo_path: Option<String>,
    pub archived_at: Option<String>,
    /// How the pet left the active household (#3): `passed` or `rehomed` (see
    /// [`ARCHIVE_KINDS`]). NULL while the pet is active.
    pub archive_kind: Option<String>,
    /// The note shown with an archived pet — a memorial line for a pet that
    /// passed, or where a rehomed pet went. Kept under its original column name.
    pub memorial_note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewPet {
    pub name: String,
    pub species: String,
    pub breed: Option<String>,
    pub sex: Option<String>,
    pub birth_date: Option<String>,
    pub gotcha_date: Option<String>,
    pub weight: Option<f64>,
    pub weight_unit: Option<String>,
    pub microchip: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub allergies: Option<Vec<LabeledDetail>>,
    pub insurer: Option<String>,
    pub insurer_reimburse: Option<String>,
    pub gradient_a: Option<String>,
    pub gradient_b: Option<String>,
    pub created_by: Option<String>,
}

/// Partial profile edit. `name`/`species` are set-or-keep (never null — a pet
/// always has both); every other field uses [`double_option`] so an explicit
/// `null` CLEARS it and an omitted key leaves it, matching record (#15), contact
/// (#20) and insurance (#8) edit semantics. This replaced blanket `COALESCE`,
/// under which a field could be set but never cleared — you could mistype a
/// microchip number and be stuck with it forever. DO NOT go back to COALESCE.
#[derive(Debug, Default, Deserialize)]
pub struct UpdatePet {
    pub name: Option<String>,
    pub species: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub breed: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub sex: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub birth_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub gotcha_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub weight: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub weight_unit: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub microchip: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub color: Option<Option<String>>,
    /// Absent → keep; present → overwrite (an empty list stores NULL).
    #[serde(default, deserialize_with = "double_option")]
    pub allergies: Option<Option<Vec<LabeledDetail>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub insurer: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub insurer_reimburse: Option<Option<String>>,
}

/// The ways a pet can leave the active household (#3). Closed set so Home can
/// put each archived pet in the right section: a pet that `passed` is
/// remembered; a pet that was `rehomed` (sold, adopted out, moved on) is alive
/// and must NEVER get memorial wording.
pub const ARCHIVE_KINDS: &[&str] = &["passed", "rehomed"];

/// A pet's sex (#72). Closed set, mirrored as `SEXES` in `frontend/src/types.ts`,
/// so boarding rules ("no intact females"), intake forms and bulk import can
/// rely on it. Stored plain; the species word people actually use (Gelding,
/// Spayed mare, Tom) is DERIVED for display by `sexLabel` on the client, never
/// stored. NULL means not recorded; `unknown` means the owner doesn't know.
/// Breeding status (stud, dam) is NOT a sex value — it belongs with lineage (#18).
pub const SEXES: &[&str] = &["male", "female", "male_neutered", "female_spayed", "unknown"];

/// A record that looks like a spay or neuter on a pet whose sex is still
/// intact (#72): the profile offers a one-tap update to `sex`. Derived on every
/// read, never stored, so it disappears once the sex is updated or the record
/// is corrected.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SexSuggestion {
    /// The value to set (`female_spayed` or `male_neutered`).
    pub sex: String,
    pub record_id: i64,
    pub record_title: String,
    pub occurred_on: String,
}

/// Archive a pet, or edit an existing archive (#3, #37). An OMITTED field keeps
/// its current value — so a script fixing a typo in the note can't silently
/// turn a rehomed (living) pet into a memorial or reset the date to today —
/// and only falls back to a default on a first archive.
#[derive(Debug, Deserialize)]
pub struct ArchivePet {
    /// `passed` or `rehomed`. Omitted: keep the current kind, or `passed` (the
    /// original meaning of archiving) on a first archive. Blank is rejected.
    #[serde(default)]
    pub kind: Option<String>,
    /// Omitted: keep. `null` / blank: clear.
    #[serde(default, deserialize_with = "double_option")]
    pub memorial_note: Option<Option<String>>,
    /// The date it happened (YYYY-MM-DD). Omitted: keep the current date, or
    /// today on a first archive. `null` / blank: today. A real date is preferred
    /// so the record is honest.
    #[serde(default, deserialize_with = "double_option")]
    pub archived_at: Option<Option<String>>,
}

/// One itemized charge on a record's cost breakdown (#4). `amount_cents` is the
/// line total in cents (money is never a float); NULL if the amount is unknown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineItem {
    pub description: String,
    pub amount_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct MedicalRecord {
    pub id: i64,
    pub pet_id: String,
    pub occurred_on: String,
    /// Optional end of a date range (#15): a multi-day treatment, a boarding
    /// stay. NULL for a single-day event.
    pub ends_on: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub category: String,
    pub cost_cents: Option<i64>,
    pub covered_cents: Option<i64>,
    pub source: String,
    /// The shared contact (clinic/vet) this record happened at (#20). NULL when
    /// the record isn't tied to a contact. `RecordDetail` resolves it to the full
    /// [`Contact`] for display.
    pub contact_id: Option<String>,
    /// Free-form label -> value details (#4): anything an LLM captured from a
    /// document beyond the fixed fields, or custom fields a user added by hand.
    /// Stored as a JSON object in the `extra` TEXT column; NULL when empty.
    pub extra: Option<Json<BTreeMap<String, String>>>,
    /// Itemized cost breakdown; JSON array in the `line_items` TEXT column, NULL
    /// when there's no breakdown. `cost_cents` remains the record total.
    pub line_items: Option<Json<Vec<LineItem>>>,
    pub created_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewRecord {
    pub occurred_on: String,
    #[serde(default)]
    pub ends_on: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
    pub cost_cents: Option<i64>,
    pub covered_cents: Option<i64>,
    #[serde(default = "default_source")]
    pub source: String,
    /// Optional shared contact (clinic/vet) this record happened at (#20).
    #[serde(default)]
    pub contact_id: Option<String>,
    /// Optional custom/extra label -> value details (see [`MedicalRecord::extra`]).
    #[serde(default)]
    pub extra: Option<BTreeMap<String, String>>,
    /// Optional itemized cost breakdown (see [`MedicalRecord::line_items`]).
    #[serde(default)]
    pub line_items: Option<Vec<LineItem>>,
    pub created_by: Option<String>,
}
/// Record categories the UI can colour, in picker order; the first is the
/// default. The model's tool schemas take their enum from here. The frontend's
/// `CATEGORY_COLORS` in `format.ts` mirrors this list (a second language, so a
/// second copy; keep them in step).
pub const RECORD_CATEGORIES: &[&str] = &["Appointments", "Prescriptions", "Food", "Boarding", "Grooming"];

fn default_category() -> String {
    RECORD_CATEGORIES[0].to_string()
}
fn default_source() -> String {
    "manual".to_string()
}

/// Partial update for a record (#15). Every field is optional: an omitted key
/// leaves the stored value untouched. The nullable fields use [`double_option`]
/// so an explicit `null` clears them (the edit modal blanking a cost or the
/// notes), while omitting the key keeps them — a distinction plain `Option`
/// can't make. `title`/`occurred_on`/`category` are never null (set-or-keep).
#[derive(Debug, Default, Deserialize)]
pub struct UpdateRecord {
    pub occurred_on: Option<String>,
    pub title: Option<String>,
    pub category: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub ends_on: Option<Option<String>>,
    /// Set-or-clear the record's clinic/vet link (#20): omit to keep, `null` to
    /// unlink, an id to point it at that contact.
    #[serde(default, deserialize_with = "double_option")]
    pub contact_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cost_cents: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub covered_cents: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub extra: Option<Option<BTreeMap<String, String>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub line_items: Option<Option<Vec<LineItem>>>,
}

/// A medical record together with its attachments, for the pet-profile timeline.
/// `contact` is the resolved clinic/vet (#20) when `record.contact_id` is set, so
/// the timeline can show the clinic name without a second lookup.
#[derive(Debug, Serialize)]
pub struct RecordDetail {
    #[serde(flatten)]
    pub record: MedicalRecord,
    pub attachments: Vec<Attachment>,
    pub contact: Option<Contact>,
}

/// A file attached to a record (#4). The bytes live on disk; this is metadata.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Attachment {
    pub id: String,
    pub record_id: i64,
    pub filename: String,
    pub media_type: String,
    pub size_bytes: i64,
    #[serde(skip)]
    pub stored_path: String,
    pub source: String,
    /// Hex SHA-256 of the bytes (#23): how bulk import recognises a document
    /// that's already in the library under any filename. NULL only for a legacy
    /// row whose file was missing when the startup backfill ran.
    pub sha256: Option<String>,
    pub created_at: String,
}

/// Pet portrait upload (#21). The client crops/rotates/zooms on a canvas and
/// sends the finished square image base64-encoded — same wire shape as an
/// attachment, minus the record scoping. Only image types are accepted; the
/// bytes share the attachments storage volume but the reference lives on the
/// pet's reserved `photo_path`, not in the attachments table (one per pet,
/// stable `/pets/:id/photo` URL, no schema reshape).
#[derive(Debug, Deserialize)]
pub struct NewPetPhoto {
    pub media_type: String,
    pub data_base64: String,
}

/// Upload payload: the file is sent base64-encoded, consistent with the rest of
/// the JSON API and with the extraction path (which already has the bytes).
#[derive(Debug, Deserialize)]
pub struct NewAttachment {
    pub filename: String,
    pub media_type: String,
    pub data_base64: String,
    #[serde(default = "default_att_source")]
    pub source: String,
}
fn default_att_source() -> String {
    "upload".to_string()
}

// ---------- Diagnostics capture (#41) ----------

/// A client-submitted diagnostic report. The frontend posts this (best-effort)
/// when it catches an error, optionally attaching the file it was working on —
/// e.g. the exact HEIC a phone browser failed to convert — so the failure can be
/// reproduced. Stored in an EPHEMERAL dir (wiped on container restart), never on
/// the `/data` volume.
#[derive(Debug, Deserialize)]
pub struct NewDiag {
    pub context: String,
    pub message: String,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub extra: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub file: Option<DiagFile>,
}

/// The attached file being diagnosed, base64-encoded (same wire shape as an
/// upload). The bytes are what make a client-side failure reproducible.
#[derive(Debug, Deserialize)]
pub struct DiagFile {
    pub filename: String,
    pub media_type: String,
    pub data_base64: String,
}

/// A stored diagnostic report. Server-enriched with the app version and the
/// request's user-agent so a "broke on my phone" report is actually debuggable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagReport {
    pub id: String,
    pub received_at: String,
    pub app_version: String,
    pub user_agent: Option<String>,
    pub context: String,
    pub message: String,
    pub stack: Option<String>,
    pub url: Option<String>,
    pub extra: Option<BTreeMap<String, String>>,
    pub file: Option<DiagFileMeta>,
}

/// Metadata for a report's attached file. The bytes live beside `report.json` in
/// the report's dir under a fixed name, so no internal path needs storing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagFileMeta {
    pub filename: String,
    pub media_type: String,
    pub size_bytes: i64,
}

// ---------- LLM ingestion settings (#4) ----------

/// Safe settings view. The API key is WRITE-ONLY: it is never returned, only its
/// presence is reported via `has_key`.
#[derive(Debug, Serialize)]
pub struct IngestionSettingsView {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub has_key: bool,
}

/// Partial update. Any omitted field is left unchanged. For `api_key`, an
/// absent/null/empty value keeps the existing key and a non-empty string
/// replaces it (encrypted at rest); use DELETE to clear the key entirely.
/// `Debug` is hand-written so the key can never reach a log via `{:?}`.
#[derive(Default, Deserialize)]
pub struct IngestionSettingsUpdate {
    pub enabled: Option<bool>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

impl std::fmt::Debug for IngestionSettingsUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestionSettingsUpdate")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// `POST /settings/ingestion/models` (#67): list models for a key the person has
/// pasted but not saved, so the picker fills before Save. The key is used for
/// that one call and never stored or returned. No `Debug`, so it can't be logged.
#[derive(Deserialize)]
pub struct ModelsForKeyRequest {
    pub api_key: String,
}

/// Household display preferences (#72). Wording only — nothing stored about a
/// pet changes with these.
#[derive(Debug, Serialize)]
pub struct DisplaySettings {
    /// Use breeder terms for intact dogs and cats: Dog / Bitch, Tom / Queen,
    /// instead of Male / Female. Off by default.
    pub breeder_terms: bool,
}

/// Partial update; an omitted field is left unchanged.
#[derive(Debug, Deserialize)]
pub struct DisplaySettingsUpdate {
    pub breeder_terms: Option<bool>,
}

/// Extraction request: a single document to read.
#[derive(Debug, Deserialize)]
pub struct ExtractRequest {
    #[serde(default)]
    pub filename: Option<String>,
    pub media_type: String,
    pub data_base64: String,
}

/// Extraction response: structured fields for the user to confirm. Nothing is
/// saved — the frontend pre-fills the record modal from this.
#[derive(Debug, Serialize)]
pub struct ExtractResponse {
    pub fields: crate::ingestion::ExtractedFields,
    pub provider: String,
    pub model: String,
}

/// Preview/debug request (#4 troubleshooting, #5 scriptability): send a document
/// to the model and get the RAW response back. With `prompt` set, runs free-form
/// (no forced tool) so you can see how the model naturally reads the file;
/// without it, runs the real extraction tool and returns the raw tool output.
#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub media_type: String,
    pub data_base64: String,
    #[serde(default)]
    pub prompt: Option<String>,
}

// ---------- Reminders / calendar events (#26) ----------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Reminder {
    pub id: i64,
    pub pet_id: String,
    pub due_on: String,
    pub title: String,
    pub kind: Option<String>,
    pub done: bool,
    pub source: String,
    pub created_at: String,
}

/// A reminder with its pet's name, for the cross-pet calendar list.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ReminderWithPet {
    pub id: i64,
    pub pet_id: String,
    pub pet_name: String,
    pub due_on: String,
    pub title: String,
    pub kind: Option<String>,
    pub done: bool,
}

#[derive(Debug, Deserialize)]
pub struct NewReminder {
    pub due_on: String,
    pub title: String,
    pub kind: Option<String>,
    #[serde(default = "default_reminder_source")]
    pub source: String,
}
fn default_reminder_source() -> String {
    "manual".to_string()
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateReminder {
    pub done: Option<bool>,
    pub due_on: Option<String>,
    pub title: Option<String>,
}

// ---------- Shared contacts (#20) ----------

/// A generic labeled free-text detail: a short `label` + optional `detail`. Reused
/// for both a facility's requirements (#42 — "Vaccinations": "Rabies, DHPP…") and
/// its hours (#42 follow-up — "Drop-off": "6–10am"), since both are the same
/// label→value shape. One type, so the two can't drift.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LabeledDetail {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
}

/// A shared contact — a vet clinic, emergency vet, groomer, or (via #42) a
/// daycare/boarding facility — entered once and linked to many pets. `role` keeps
/// the entity generic so new contact types don't need a new table.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub role: String,
    pub doctor: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub website: Option<String>,
    pub notes: Option<String>,
    /// Facility requirements (#42): a JSON array of {label, detail}; NULL when
    /// there are none. Populated by the web parser or by hand.
    pub requirements: Option<Json<Vec<LabeledDetail>>>,
    /// Hours (#42 follow-up): open hours + drop-off/pick-up windows as
    /// {label, detail}; NULL when unknown. Applies to any contact, vets included.
    pub hours: Option<Json<Vec<LabeledDetail>>>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewContact {
    pub name: String,
    #[serde(default = "default_contact_role")]
    pub role: String,
    pub doctor: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub website: Option<String>,
    pub notes: Option<String>,
    /// Optional facility requirements (#42).
    #[serde(default)]
    pub requirements: Option<Vec<LabeledDetail>>,
    /// Optional hours (#42 follow-up).
    #[serde(default)]
    pub hours: Option<Vec<LabeledDetail>>,
    pub created_by: Option<String>,
}
fn default_contact_role() -> String {
    "vet".to_string()
}

/// Partial update. `name`/`role` are set-or-keep (never null); the rest use
/// [`double_option`] so an explicit `null` clears a field and an omitted key
/// leaves it — the same editor semantics as record edit (#15).
#[derive(Debug, Default, Deserialize)]
pub struct UpdateContact {
    pub name: Option<String>,
    pub role: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub doctor: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub email: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub address: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub website: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub requirements: Option<Option<Vec<LabeledDetail>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub hours: Option<Option<Vec<LabeledDetail>>>,
}

/// Body for linking an existing contact to a pet (POST /pets/:id/contacts).
#[derive(Debug, Deserialize)]
pub struct LinkContact {
    pub contact_id: String,
}

/// Request to parse a facility's website (#42): the LLM fetches the URL and
/// proposes contact fields + a requirements checklist. Nothing is written — the
/// frontend pre-fills the contact sheet (human-in-the-loop, like #4/#26).
#[derive(Debug, Deserialize)]
pub struct ParseUrlRequest {
    pub url: String,
}

/// The proposed facility, for the user to confirm before it's saved.
#[derive(Debug, Serialize)]
pub struct ParseUrlResponse {
    pub fields: crate::ingestion::FacilityFields,
    pub provider: String,
    pub model: String,
}

// ---------- Insurance & wellness plans (#8 layer 1 + #29 insurance slice) ----------

/// Plan kinds. `insurance` is true pet insurance (reimbursement %, deductible,
/// annual limit); `wellness` is a vet's preventative-care membership (copay +
/// monthly fee, no reimbursement). Kept closed so the UI can label each correctly.
pub const INSURANCE_KINDS: &[&str] = &["insurance", "wellness"];
/// Policy lifecycle states, closed set.
pub const INSURANCE_STATUSES: &[&str] = &["active", "expired", "cancelled"];

/// A pet's insurance policy or wellness plan (#8/#29). Money is INTEGER cents,
/// matching `medical_records.cost_cents`. `coverages`/`exclusions` reuse
/// [`LabeledDetail`] as JSON, NULL when empty. `document_path` is the internal
/// path to the embedded policy file and is `#[serde(skip)]` — never leaked; the
/// client learns a document exists from `document_name`/`document_media_type`.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct InsurancePolicy {
    pub id: String,
    pub pet_id: String,
    pub kind: String,
    pub provider: String,
    pub plan_name: Option<String>,
    pub policy_number: Option<String>,
    pub status: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub premium_cents: Option<i64>,
    pub premium_interval: Option<String>,
    pub reimburse_percent: Option<i64>,
    pub deductible_cents: Option<i64>,
    pub annual_limit_cents: Option<i64>,
    pub notes: Option<String>,
    pub coverages: Option<Json<Vec<LabeledDetail>>>,
    pub exclusions: Option<Json<Vec<LabeledDetail>>>,
    #[serde(skip)]
    pub document_path: Option<String>,
    pub document_name: Option<String>,
    pub document_media_type: Option<String>,
    pub document_size_bytes: Option<i64>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewInsurancePolicy {
    #[serde(default = "default_insurance_kind")]
    pub kind: String,
    pub provider: String,
    pub plan_name: Option<String>,
    pub policy_number: Option<String>,
    #[serde(default = "default_insurance_status")]
    pub status: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub premium_cents: Option<i64>,
    pub premium_interval: Option<String>,
    pub reimburse_percent: Option<i64>,
    pub deductible_cents: Option<i64>,
    pub annual_limit_cents: Option<i64>,
    pub notes: Option<String>,
    #[serde(default)]
    pub coverages: Option<Vec<LabeledDetail>>,
    #[serde(default)]
    pub exclusions: Option<Vec<LabeledDetail>>,
    pub created_by: Option<String>,
}
fn default_insurance_kind() -> String {
    "insurance".to_string()
}
fn default_insurance_status() -> String {
    "active".to_string()
}

/// Partial update. `kind`/`provider`/`status` are set-or-keep (never null); the
/// rest use [`double_option`] so an explicit `null` clears and an omitted key
/// leaves the stored value — the same editor semantics as record/contact edit.
/// The embedded document is managed by its own endpoint, not here.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateInsurancePolicy {
    pub kind: Option<String>,
    pub provider: Option<String>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub plan_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub policy_number: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub start_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub end_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub premium_cents: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub premium_interval: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub reimburse_percent: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub deductible_cents: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub annual_limit_cents: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub coverages: Option<Option<Vec<LabeledDetail>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub exclusions: Option<Option<Vec<LabeledDetail>>>,
}

/// Body for attaching/replacing a policy's document (PUT /insurance/:pid/document).
#[derive(Debug, Deserialize)]
pub struct NewInsuranceDocument {
    pub filename: String,
    pub media_type: String,
    pub data_base64: String,
}

/// Request to extract fields from an insurance/plan document (POST /insurance/extract).
/// Mirrors [`ExtractRequest`]; never writes — the frontend pre-fills the sheet.
#[derive(Debug, Deserialize)]
pub struct InsuranceExtractRequest {
    #[serde(default)]
    pub filename: Option<String>,
    pub media_type: String,
    pub data_base64: String,
}

/// Proposed insurance fields for the user to confirm before saving.
#[derive(Debug, Serialize)]
pub struct InsuranceExtractResponse {
    pub fields: crate::ingestion::InsuranceFields,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WeightEntry {
    pub id: i64,
    pub pet_id: String,
    pub recorded_on: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Deserialize)]
pub struct NewWeight {
    pub recorded_on: String,
    pub value: f64,
    #[serde(default = "default_unit")]
    pub unit: String,
}
fn default_unit() -> String {
    "kg".to_string()
}

/// Partial weight correction (#23): omitted keys keep their stored value.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateWeight {
    pub recorded_on: Option<String>,
    pub value: Option<f64>,
    pub unit: Option<String>,
}


// ---------- Ongoing care: food, preventatives, medications, supplements (#63) ----------

/// What a pet is ongoingly ON. One entity rather than flat `diet` /
/// `flea_prevention` / `tick_prevention` / `heartworm_prevention` columns,
/// because a vet intake form asks for food and drugs in the SAME shape
/// ("name, amount, frequency") and because ONE product routinely covers several
/// protections at once — Simparica Trio is flea AND tick AND heartworm in a
/// single monthly pill. See `0013_pet_care.sql`; DO NOT flatten this back out.
pub const REGIMEN_KINDS: &[&str] = &["food", "preventative", "medication", "supplement"];

/// What a preventative protects against. Closed set so the "is your pet current
/// on heartworm prevention?" question has a single, queryable answer instead of
/// depending on how the brand name was typed.
pub const PROTECTIONS: &[&str] = &["flea", "tick", "heartworm", "worm"];

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Regimen {
    pub id: String,
    pub pet_id: String,
    pub kind: String,
    pub product: String,
    /// Dose or serving: "1 cup", "1 pill", "45+ lbs".
    pub amount: Option<String>,
    /// "2x/day", "monthly".
    pub frequency: Option<String>,
    /// For a preventative, the protections it covers, as JSON `["flea","tick"]`.
    /// NULL for food/medication/supplement.
    pub protects: Option<Json<Vec<String>>>,
    pub started_on: Option<String>,
    /// NULL means the pet is STILL on it. This is the "current" test the intake
    /// form asks about — not a boolean flag that has to be kept in step.
    pub ended_on: Option<String>,
    pub notes: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewRegimen {
    #[serde(default = "default_regimen_kind")]
    pub kind: String,
    pub product: String,
    pub amount: Option<String>,
    pub frequency: Option<String>,
    #[serde(default)]
    pub protects: Option<Vec<String>>,
    pub started_on: Option<String>,
    pub ended_on: Option<String>,
    pub notes: Option<String>,
    pub created_by: Option<String>,
}
fn default_regimen_kind() -> String {
    "medication".to_string()
}

/// Partial update. `kind`/`product` are set-or-keep (never null); the rest use
/// [`double_option`] — an explicit `null` clears, an omitted key keeps. Clearing
/// `ended_on` is how you put a pet back ON something you stopped by mistake.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateRegimen {
    pub kind: Option<String>,
    pub product: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub amount: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub frequency: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub protects: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub started_on: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub ended_on: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
}

/// Pet plus its soonest upcoming reminder ("what's next"), for list views. The
/// former composite "care score" was removed (#35) — it graded the owner rather
/// than informing them; `next_reminder` is a neutral, actionable fact instead.
#[derive(Debug, Serialize)]
pub struct PetSummary {
    #[serde(flatten)]
    pub pet: Pet,
    /// The soonest not-done reminder for this pet (overdue sorts first), or
    /// `null` when the pet is all caught up.
    pub next_reminder: Option<Reminder>,
}

/// Full profile payload for the pet detail view. Records carry their attachments.
#[derive(Debug, Serialize)]
pub struct PetDetail {
    #[serde(flatten)]
    pub pet: Pet,
    /// The soonest not-done reminder ("what's next"), derived from `reminders`
    /// below. Replaces the removed care score (#35).
    pub next_reminder: Option<Reminder>,
    pub records: Vec<RecordDetail>,
    pub weights: Vec<WeightEntry>,
    pub reminders: Vec<Reminder>,
    /// The pet's care team: the shared contacts linked to it (#20).
    pub contacts: Vec<Contact>,
    /// Insurance policies and wellness plans on this pet (#8/#29).
    pub insurance_policies: Vec<InsurancePolicy>,
    /// Standing care: the food, preventatives, medications and supplements this
    /// pet is (or was) on (#63). Current entries first.
    pub regimens: Vec<Regimen>,
    /// A spay/neuter record while the sex still says intact (#72), or null.
    pub sex_suggestion: Option<SexSuggestion>,
}

#[derive(Debug, Serialize)]
pub struct CategoryTotal {
    pub category: String,
    pub cost_cents: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PetCostRow {
    pub pet_id: String,
    pub name: String,
    pub insurer: Option<String>,
    pub cost_cents: i64,
    pub covered_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct CostsSummary {
    pub total_cents: i64,
    pub covered_cents: i64,
    pub by_category: Vec<CategoryTotal>,
    pub by_pet: Vec<PetCostRow>,
}
