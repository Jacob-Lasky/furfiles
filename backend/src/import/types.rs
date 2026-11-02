//! The shapes bulk import passes around: what the model returns, the reviewed
//! plan (analyze → human → commit), the commit result, and the household
//! snapshot the model reasons against.

use super::*;

// ---------- what the model returns ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ImportExtraction {
    pub summary: Option<String>,
    pub clinic: Option<ExtractedClinic>,
    pub records: Vec<ExtractedRecord>,
    pub weights: Vec<ExtractedPetWeight>,
    pub reminders: Vec<ExtractedPetReminder>,
    pub completed_reminders: Vec<ExtractedCompletion>,
    pub pet_updates: Vec<ExtractedPetUpdate>,
    pub regimens: Vec<ExtractedRegimen>,
    pub claims: Vec<ExtractedClaim>,
    pub policies: Vec<ExtractedPolicy>,
    pub low_confidence_fields: Vec<String>,
}

/// An insurance claim decision (an "explanation of benefits"): what the insurer
/// paid for one pet's treatment on one date. Not a vet visit of its own — it
/// belongs ON the visit it reimbursed, as the record's covered amount.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedClaim {
    pub pet_id: Option<String>,
    pub claim_number: Option<String>,
    pub policy_number: Option<String>,
    pub insurer: Option<String>,
    pub treatment_date: Option<String>,
    pub items: Vec<ExtractedClaimItem>,
    /// Dollars actually paid out on this claim.
    pub total_paid: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedClaimItem {
    pub description: String,
    pub cost: Option<f64>,
    pub covered: Option<f64>,
    pub note: Option<String>,
}

/// An insurance policy or wellness plan document, for one pet. Same fields the
/// single-document insurance extractor (#8/#29) returns, plus whose it is.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedPolicy {
    pub pet_id: Option<String>,
    #[serde(flatten)]
    pub fields: ingestion::InsuranceFields,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedClinic {
    pub existing_contact_id: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub website: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedRecord {
    pub pet_id: Option<String>,
    pub pet_name_on_document: Option<String>,
    pub occurred_on: Option<String>,
    pub ends_on: Option<String>,
    pub title: Option<String>,
    pub category: Option<String>,
    /// Dollars, as the model reads them.
    pub cost: Option<f64>,
    pub line_items: Vec<ExtractedLineItem>,
    pub notes: Option<String>,
    pub extra_fields: BTreeMap<String, String>,
    pub duplicate_of_record_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedPetWeight {
    pub pet_id: Option<String>,
    pub recorded_on: Option<String>,
    pub value: Option<f64>,
    pub unit: Option<String>,
    /// The document didn't print a measurement date, so the model guessed one
    /// (live: a vaccination summary's undated "14.5 lb" got its print date).
    pub date_is_inferred: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedPetReminder {
    pub pet_id: Option<String>,
    pub due_on: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedCompletion {
    pub reminder_id: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedPetUpdate {
    pub pet_id: Option<String>,
    pub field: Option<String>,
    pub value: Option<String>,
    pub reason: Option<String>,
    /// For a `sex` update: the document shows the spay or neuter being
    /// PERFORMED (a surgery invoice, a discharge note), not merely the status
    /// printed in a header. Such an update is ticked even though it overwrites
    /// (#72).
    pub sterilisation_performed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtractedRegimen {
    pub pet_id: Option<String>,
    pub kind: Option<String>,
    pub product: Option<String>,
    pub amount: Option<String>,
    pub frequency: Option<String>,
    pub protects: Vec<String>,
    pub started_on: Option<String>,
    pub ended_on: Option<String>,
    pub notes: Option<String>,
    pub duplicate_of_regimen_id: Option<String>,
}

// ---------- the reviewed plan (analyze → human → commit) ----------

/// The import plan for ONE document. Round-trips: `analyze` returns it, the
/// client toggles `selected` / edits fields, `commit` takes it back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ImportPlan {
    pub sha256: String,
    /// Records that already carry this exact file. Non-empty means the document
    /// is already in FurFiles; every record is then deselected.
    pub file_duplicates: Vec<FileDuplicate>,
    pub summary: Option<String>,
    pub clinic: Option<PlannedClinic>,
    pub records: Vec<PlannedRecord>,
    /// Pets whose care team should gain the document's clinic.
    pub care_team: Vec<PlannedLink>,
    /// Saved visits this document duplicates that have no clinic yet: linking
    /// them is how re-uploading an old invoice teaches FurFiles who the vet was.
    pub clinic_links: Vec<PlannedClinicLink>,
    pub weights: Vec<PlannedWeight>,
    pub reminders: Vec<PlannedReminder>,
    pub completed_reminders: Vec<PlannedCompletion>,
    pub pet_updates: Vec<PlannedPetUpdate>,
    pub regimens: Vec<PlannedRegimen>,
    /// Insurance payouts to record against visits (or as a visit of their own).
    pub claims: Vec<PlannedClaim>,
    /// New insurance policies / wellness plans.
    pub policies: Vec<PlannedPolicy>,
    /// Existing policies a new one replaces, to mark ended.
    pub policy_ends: Vec<PlannedPolicyEnd>,
    pub low_confidence_fields: Vec<String>,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, sqlx::FromRow)]
#[serde(default)]
pub struct FileDuplicate {
    pub attachment_id: String,
    pub record_id: i64,
    pub pet_id: String,
    pub pet_name: String,
    pub title: String,
    pub occurred_on: String,
    pub filename: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedClinic {
    pub selected: bool,
    /// An existing contact this clinic IS. None → commit creates a new contact.
    pub contact_id: Option<String>,
    /// The matched contact's stored name, for "AMC of Springfield (already saved)".
    pub matched_name: Option<String>,
    pub name: String,
    #[serde(default = "default_role")]
    pub role: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub website: Option<String>,
}
pub(crate) fn default_role() -> String {
    "vet".into()
}

/// An existing record a proposed one duplicates, and why we think so.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RecordRef {
    pub record_id: i64,
    pub title: String,
    pub occurred_on: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedRecord {
    pub selected: bool,
    /// None when the model couldn't tell whose record it is; commit refuses a
    /// selected record without one, so the screen makes the user pick.
    pub pet_id: Option<String>,
    pub pet_name_on_document: Option<String>,
    pub occurred_on: String,
    pub ends_on: Option<String>,
    pub title: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub cost_cents: Option<i64>,
    pub line_items: Vec<LineItem>,
    pub notes: Option<String>,
    pub extra: BTreeMap<String, String>,
    pub duplicate_of: Option<RecordRef>,
}
pub(crate) fn default_category() -> String {
    CATEGORIES[0].into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedClinicLink {
    pub selected: bool,
    pub record_id: i64,
    pub pet_id: String,
    pub title: String,
    pub occurred_on: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedLink {
    pub selected: bool,
    pub pet_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedWeight {
    pub selected: bool,
    pub pet_id: String,
    pub recorded_on: String,
    pub value: f64,
    pub unit: String,
    /// A weight already logged for this pet on this date with a DIFFERENT value;
    /// committing a selected entry corrects it instead of adding a second point.
    pub existing_id: Option<i64>,
    pub existing_value: Option<f64>,
    /// The unit of the point being corrected, so a 50 lb → 50 kg edit made after
    /// analysis is caught as stale.
    pub existing_unit: Option<String>,
    /// The measurement date was guessed, so the entry starts unticked.
    pub date_is_inferred: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedReminder {
    pub selected: bool,
    pub pet_id: String,
    pub due_on: String,
    pub title: String,
    pub duplicate_of: Option<i64>,
    /// Another open reminder this pet already has on the SAME day, under a
    /// different title. Equivalence by meaning is the model's job ("DA2PP" vs
    /// "DHPP"); when it misses one, this puts the likely twin in front of the
    /// person ticking the box instead of silently adding a second.
    pub same_day: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedCompletion {
    pub selected: bool,
    pub reminder_id: i64,
    pub pet_id: String,
    pub title: String,
    pub due_on: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedPetUpdate {
    pub selected: bool,
    pub pet_id: String,
    pub field: String,
    pub current: Option<String>,
    pub value: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedRegimen {
    pub selected: bool,
    pub pet_id: String,
    pub kind: String,
    pub product: String,
    pub amount: Option<String>,
    pub frequency: Option<String>,
    pub protects: Vec<String>,
    pub started_on: Option<String>,
    pub ended_on: Option<String>,
    pub notes: Option<String>,
    pub duplicate_of: Option<String>,
    /// A current entry for the same product with a DIFFERENT dose or frequency:
    /// saving this one ends that one (a dose change, not a second product).
    pub replaces: Option<String>,
}

/// One claim item as the insurer itemised it. Money in cents.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClaimItem {
    pub description: String,
    pub cost_cents: Option<i64>,
    pub covered_cents: Option<i64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedClaim {
    pub selected: bool,
    pub pet_id: Option<String>,
    pub claim_number: String,
    pub policy_number: Option<String>,
    pub insurer: Option<String>,
    pub treatment_date: String,
    pub items: Vec<ClaimItem>,
    pub paid_cents: i64,
    /// The visit this claim reimbursed (same pet, same treatment date). None →
    /// commit records the claim as a visit of its own.
    pub record_id: Option<i64>,
    pub record_title: Option<String>,
    /// Every saved visit of this pet on the treatment date, when there's more
    /// than one, so the person can point the claim at the right one.
    pub visit_options: Vec<VisitOption>,
    /// This claim number is already recorded on that pet's records.
    pub already_recorded: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VisitOption {
    pub record_id: i64,
    pub title: String,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedPolicy {
    pub selected: bool,
    pub pet_id: Option<String>,
    pub kind: String,
    pub provider: String,
    pub plan_name: Option<String>,
    pub policy_number: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub premium_cents: Option<i64>,
    pub premium_interval: Option<String>,
    pub reimburse_percent: Option<i64>,
    pub deductible_cents: Option<i64>,
    pub annual_limit_cents: Option<i64>,
    pub notes: Option<String>,
    pub coverages: Vec<LabeledDetail>,
    pub exclusions: Vec<LabeledDetail>,
    /// A saved policy with this number for this pet: nothing new to add.
    pub existing_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannedPolicyEnd {
    pub selected: bool,
    pub policy_id: String,
    pub pet_id: String,
    pub provider: String,
    pub policy_number: Option<String>,
    /// The day the replacing policy starts.
    pub end_date: String,
    /// Index into `policies` of the policy that replaces this one: commit only
    /// ends this one if that one is saved (or already was).
    pub replaced_by: usize,
    /// The end date this policy had at analysis, so commit never overwrites an
    /// end someone set since.
    pub seen_end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportAnalyzeRequest {
    #[serde(default)]
    pub filename: Option<String>,
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportCommitRequest {
    pub filename: String,
    pub media_type: String,
    pub data_base64: String,
    pub plan: ImportPlan,
}

/// What a commit wrote, for the screen's "Saved" summary and for scripts.
#[derive(Debug, Default, Serialize)]
pub struct ImportResult {
    pub records: Vec<MedicalRecord>,
    pub contact: Option<Contact>,
    pub care_team_links: usize,
    pub clinic_links: usize,
    pub weights_added: usize,
    pub weights_corrected: usize,
    pub reminders_added: usize,
    pub reminders_completed: usize,
    pub pet_updates: usize,
    pub regimens_added: usize,
    pub regimens_ended: usize,
    pub claims_recorded: usize,
    pub policies_added: usize,
    pub policies_ended: usize,
    /// Items a retried commit found already saved and skipped.
    pub skipped_existing: usize,
}

// ---------- the household snapshot the model reasons against ----------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RecordBrief {
    pub id: i64,
    pub pet_id: String,
    pub occurred_on: String,
    pub title: String,
    pub cost_cents: Option<i64>,
    pub clinic: Option<String>,
    /// The record's custom fields; insurance claims are recorded here under
    /// [`claim_key`], which is how a claim is known to be already recorded.
    pub extra: Option<sqlx::types::Json<BTreeMap<String, String>>>,
}

#[derive(Debug, Clone, Default)]
pub struct Household {
    pub pets: Vec<Pet>,
    pub contacts: Vec<Contact>,
    /// (pet_id, contact_id) care-team links.
    pub care_team: Vec<(String, String)>,
    pub records: Vec<RecordBrief>,
    pub open_reminders: Vec<Reminder>,
    pub weights: Vec<WeightEntry>,
    pub current_regimens: Vec<Regimen>,
    pub policies: Vec<InsurancePolicy>,
    pub known_fields: Vec<String>,
}
