//! Pet sex as a closed set (#72): validation, reading free text onto the set,
//! and spotting a record that shows a spay or neuter.
//!
//! The stored values are [`SEXES`]. How they READ (Gelding, Spayed female,
//! Tom, …) is a display concern owned by `sexLabel` in `frontend/src/format.ts`,
//! not stored here, so one fact is never typed three ways again.

use crate::error::{AppError, AppResult};
use crate::handlers::blank_to_none;
use crate::models::{MedicalRecord, SexSuggestion, SEXES};

/// Validate a client-sent sex. Blank or absent is `None` (not recorded), the
/// same as every other optional field; anything outside [`SEXES`] is a 400.
pub fn validate_sex(v: Option<String>) -> AppResult<Option<String>> {
    let Some(s) = blank_to_none(v) else {
        return Ok(None);
    };
    if SEXES.contains(&s.as_str()) {
        Ok(Some(s))
    } else {
        Err(AppError::BadRequest(format!(
            "unknown sex '{s}' (expected one of: {})",
            SEXES.join(", ")
        )))
    }
}

/// Lower-cased words of a free-text value, split on anything that isn't a
/// letter or digit ("Male · neutered" → `["male", "neutered"]`, "F/S" →
/// `["f", "s"]`).
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Two words next to each other ("F/S", "M N").
fn adjacent(w: &[String], a: &str, b: &str) -> bool {
    w.windows(2).any(|p| p[0] == a && p[1] == b)
}

fn any(w: &[String], set: &[&str]) -> bool {
    w.iter().any(|x| set.contains(&x.as_str()))
}

// The procedure words, shared by the free-text reader (a "Spayed" sex) and the
// record detector (a "Spay" visit) so the two can't disagree on what a spay is.
#[rustfmt::skip]
const SPAY_WORDS: &[&str] = &["spay", "spayed", "spaying", "ovariohysterectomy", "ovariectomy", "ohe", "ovh"];
#[rustfmt::skip]
const CASTRATION_WORDS: &[&str] = &["castrate", "castrated", "castration", "gelding", "gelded", "geld", "orchiectomy", "orchidectomy"];
const NEUTER_WORDS: &[&str] = &["neuter", "neutered", "neutering"];
/// Clinic shorthand for a sterilised sex: female spayed, male neutered or
/// castrated (either word order).
const SPAYED_SHORT: &[&str] = &["fs", "sf"];
const NEUTERED_SHORT: &[&str] = &["mn", "nm", "mc", "cm"];
/// Words that describe a sterilised SEX but are not a procedure on a record
/// ("Altered male", "desexed"). Kept out of [`sterilisation_in`]: a record
/// titled "Altered diet" is not a neuter.
const NEUTERED_STATUS: &[&str] = &["altered", "desexed"];
/// Any of these makes a sex text describe an INTACT animal, whatever else it
/// says: "Female, not spayed", "Intact female, spay planned", "Entire male".
#[rustfmt::skip]
const INTACT_WORDS: &[&str] = &["intact", "entire", "not", "no", "non", "planned", "scheduled", "pending", "due"];
#[rustfmt::skip]
const FEMALE_WORDS: &[&str] = &["female", "f", "fi", "mare", "queen", "bitch", "filly", "hen", "doe", "jill", "jenny"];
#[rustfmt::skip]
const MALE_WORDS: &[&str] = &["male", "m", "mi", "stallion", "tom", "colt", "stud", "buck", "boar", "jack", "rooster", "cock", "entire"];

/// Every single word [`sex_from_text`] reacts to. Public for the migration
/// parity test, which feeds each one through both the Rust and the SQL mapping.
pub fn vocabulary() -> Vec<&'static str> {
    [
        SPAY_WORDS,
        CASTRATION_WORDS,
        NEUTER_WORDS,
        SPAYED_SHORT,
        NEUTERED_SHORT,
        NEUTERED_STATUS,
        INTACT_WORDS,
        FEMALE_WORDS,
        MALE_WORDS,
    ]
    .concat()
}

/// Read free text ("Male · neutered", "Neutered Male", "MN", "FS", "Gelding",
/// "Spayed") onto [`SEXES`]. `None` only for blank input; text that names no
/// sex is `unknown`. Rules, in order:
/// 0. an intact word anywhere (intact, entire, not, no, planned, due…) rules
///    out 1 and 2: "Female, not spayed" is an intact female;
/// 1. any spay word, "FS"/"SF", or a neuter word next to a female word
///    ("Neutered female", British usage) → `female_spayed`;
/// 2. any neuter/castration/gelding/altered word or "MN"/"MC" → `male_neutered`
///    (a bare "Neutered" is assumed male, as it almost always means);
/// 3. a female word (female, F, mare, queen, bitch, filly…) → `female`, checked
///    before male because "female" contains "male" in looser matchers;
/// 4. a male word (male, M, stallion, tom, colt, stud…) → `male`;
/// 5. anything else → `unknown`.
///
/// `migrations/0016_pet_sex_closed_set.sql` applied these same rules, in SQL,
/// to the free text stored before #72, and
/// `tests/api.rs::sex_migration_matches_the_rust_mapping` proves they agree.
/// That migration is FROZEN once applied (sqlx checksums it and refuses to
/// start if it changes), so NEVER edit it to follow a change here: today this
/// function only reads document wording for bulk import. If you teach it a new
/// word, the parity test fails; list the word in that test's `ADDED_AFTER_0016`
/// so it is skipped, and leave the migration alone.
pub fn sex_from_text(text: &str) -> Option<&'static str> {
    if text.trim().is_empty() {
        return None;
    }
    let w = words(text);
    let intact = any(&w, INTACT_WORDS);
    let neutered = !intact
        && (any(&w, NEUTER_WORDS)
            || any(&w, CASTRATION_WORDS)
            || any(&w, NEUTERED_SHORT)
            || any(&w, NEUTERED_STATUS)
            || adjacent(&w, "m", "n")
            || adjacent(&w, "m", "c"));
    let female = any(&w, FEMALE_WORDS);
    Some(
        if !intact
            && (any(&w, SPAY_WORDS)
                || any(&w, SPAYED_SHORT)
                || adjacent(&w, "f", "s")
                || adjacent(&w, "s", "f")
                || (neutered && female))
        {
            "female_spayed"
        } else if neutered {
            "male_neutered"
        } else if female || adjacent(&w, "f", "i") {
            "female"
        } else if any(&w, MALE_WORDS) || adjacent(&w, "m", "i") {
            "male"
        } else {
            "unknown"
        },
    )
}

/// Which way a sterilisation procedure points. A spay is only ever done to a
/// female and a castration only to a male; "neuter" is used for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sterilisation {
    Spay,
    Castration,
    Either,
}

/// Words that make a record ABOUT a sterilisation without one being performed:
/// a quote, a consult, a recommendation, a booking, a cancellation.
const NOT_PERFORMED: &[&str] = &[
    "estimate",
    "quote",
    "consult",
    "consultation",
    "recommend",
    "recommended",
    "recommendation",
    "discuss",
    "discussed",
    "discussion",
    "declined",
    "deferred",
    "scheduled",
    "schedule",
    "booked",
    "booking",
    "planned",
    "consent",
    "deposit",
    "cancelled",
    "canceled",
    "cancel",
    "postponed",
    "rescheduled",
    "waitlist",
    "due",
    "not",
    "no",
];

/// Does this text (a record title, or a billed line item) show a spay or
/// neuter? Word matches only, from the same lists [`sex_from_text`] reads:
/// spay, ovariohysterectomy, OHE/OVH, ovariectomy (spay); castration, gelding,
/// orchiectomy (castration); neuter (either).
/// "Pre-spay bloodwork", "Spay consult", "Spay scheduled", "Spay cancelled"
/// do not count — the procedure hasn't happened. A "post-spay recheck" does.
///
/// Callers pass a record's TITLE and LINE ITEMS, never its notes: an exam note
/// routinely describes the patient as "5yo FS" or says "discussed spay at 6
/// months", and neither is the procedure.
pub fn sterilisation_in(text: &str) -> Option<Sterilisation> {
    let w = words(text);
    if not_performed(&w) {
        return None;
    }
    let mut found: Option<Sterilisation> = None;
    for (i, x) in w.iter().enumerate() {
        if i > 0 && w[i - 1] == "pre" {
            continue;
        }
        let x = x.as_str();
        let k = if SPAY_WORDS.contains(&x) {
            Sterilisation::Spay
        } else if CASTRATION_WORDS.contains(&x) {
            Sterilisation::Castration
        } else if NEUTER_WORDS.contains(&x) {
            Sterilisation::Either
        } else {
            continue;
        };
        // A specific word beats the ambiguous "neuter".
        if found.is_none() || found == Some(Sterilisation::Either) {
            found = Some(k);
        }
    }
    found
}

/// The sterilised value a procedure moves this sex to, or `None` when it
/// doesn't apply: already sterilised, or a procedure that can't fit the
/// recorded sex (a spay on a male). An unrecorded/unknown sex takes the
/// procedure's own direction, but a bare "neuter" can't say which.
pub fn sterilised_form(current: Option<&str>, procedure: Sterilisation) -> Option<&'static str> {
    use Sterilisation::*;
    match (current.unwrap_or("unknown"), procedure) {
        ("male", Castration | Either) => Some("male_neutered"),
        ("female", Spay | Either) => Some("female_spayed"),
        ("unknown", Spay) => Some("female_spayed"),
        ("unknown", Castration) => Some("male_neutered"),
        _ => None,
    }
}

/// Is `to` what sterilising a pet recorded as `current` would make it? True for
/// male → male_neutered and female → female_spayed, and for either from an
/// unrecorded/unknown sex.
pub fn sterilises(current: Option<&str>, to: &str) -> bool {
    matches!(
        (current.unwrap_or("unknown"), to),
        ("male" | "unknown", "male_neutered") | ("female" | "unknown", "female_spayed")
    )
}

fn not_performed(w: &[String]) -> bool {
    any(w, NOT_PERFORMED)
}

/// The procedure a record shows, from its title first, then its billed lines.
/// The TITLE speaks for the whole record: a "Spay estimate" whose lines price
/// an OHE is still only an estimate.
pub fn record_sterilisation<'a>(
    title: &str,
    line_items: impl IntoIterator<Item = &'a str>,
) -> Option<Sterilisation> {
    if not_performed(&words(title)) {
        return None;
    }
    sterilisation_in(title).or_else(|| line_items.into_iter().find_map(sterilisation_in))
}

/// The standing "Update to Spayed female?" suggestion (#72): the NEWEST record
/// showing a spay or neuter that fits the pet's recorded sex, while that sex is
/// still intact (or unrecorded/unknown and the procedure says which way).
/// `records` are newest first, as the pet detail loads them.
pub fn suggestion(sex: Option<&str>, records: &[MedicalRecord]) -> Option<SexSuggestion> {
    records.iter().find_map(|r| {
        let items = r
            .line_items
            .iter()
            .flat_map(|li| li.0.iter().map(|i| i.description.as_str()));
        let to = sterilised_form(sex, record_sterilisation(&r.title, items)?)?;
        Some(SexSuggestion {
            sex: to.to_string(),
            record_id: r.id,
            record_title: r.title.clone(),
            occurred_on: r.occurred_on.clone(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_text_maps_onto_the_closed_set() {
        // Free-text values seen in a household before the closed set (#72).
        assert_eq!(sex_from_text("Male · neutered"), Some("male_neutered"));
        assert_eq!(sex_from_text("Neutered male"), Some("male_neutered"));
        assert_eq!(sex_from_text("Female"), Some("female"));
        assert_eq!(sex_from_text("Gelding"), Some("male_neutered"));
        // Document wording the import model may hand back.
        for (t, want) in [
            ("Neutered Male", "male_neutered"),
            ("MN", "male_neutered"),
            ("M/N", "male_neutered"),
            ("Male Castrated", "male_neutered"),
            ("FS", "female_spayed"),
            ("F/S", "female_spayed"),
            ("SF", "female_spayed"),
            ("Spayed", "female_spayed"),
            ("Spayed female", "female_spayed"),
            ("Female · spayed", "female_spayed"),
            ("Neutered female", "female_spayed"),
            ("Spayed mare", "female_spayed"),
            ("ovariohysterectomy", "female_spayed"),
            ("Neutered", "male_neutered"),
            ("Female · intact", "female"),
            ("Intact female", "female"),
            ("F", "female"),
            ("Mare", "female"),
            ("Queen", "female"),
            ("Bitch", "female"),
            ("Filly", "female"),
            ("Male", "male"),
            ("Intact male", "male"),
            ("M", "male"),
            ("Stallion", "male"),
            ("Tom", "male"),
            ("Colt", "male"),
            ("male_neutered", "male_neutered"),
            ("female_spayed", "female_spayed"),
            ("unknown", "unknown"),
            ("Kitten", "unknown"),
            ("?", "unknown"),
            // An intact word wins over a procedure word.
            ("Female, not spayed", "female"),
            ("Intact female, spay planned", "female"),
            ("Male, not neutered", "male"),
            ("Entire male", "male"),
            ("No spay", "unknown"),
            ("Altered male", "male_neutered"),
            ("Altered female", "female_spayed"),
            ("Desexed", "male_neutered"),
            ("Female\u{a0}neutered", "female_spayed"),
        ] {
            assert_eq!(sex_from_text(t), Some(want), "{t}");
        }
        assert_eq!(sex_from_text("   "), None);
        assert_eq!(sex_from_text(""), None);
    }

    #[test]
    fn validate_accepts_only_the_set() {
        assert_eq!(
            validate_sex(Some("female_spayed".into()))
                .unwrap()
                .as_deref(),
            Some("female_spayed")
        );
        assert_eq!(
            validate_sex(Some(" male ".into())).unwrap().as_deref(),
            Some("male")
        );
        assert_eq!(validate_sex(Some("".into())).unwrap(), None);
        assert_eq!(validate_sex(None).unwrap(), None);
        assert!(validate_sex(Some("Neutered male".into())).is_err());
        assert!(validate_sex(Some("MALE".into())).is_err());
    }

    #[test]
    fn spots_a_performed_sterilisation() {
        use Sterilisation::*;
        assert_eq!(sterilisation_in("Spay"), Some(Spay));
        assert_eq!(sterilisation_in("OHE + microchip"), Some(Spay));
        assert_eq!(sterilisation_in("Canine neuter 20-40 lb"), Some(Either));
        assert_eq!(sterilisation_in("Castration"), Some(Castration));
        assert_eq!(sterilisation_in("Gelding"), Some(Castration));
        assert_eq!(sterilisation_in("Neuter (castration)"), Some(Castration));
        assert_eq!(sterilisation_in("Post-spay recheck"), Some(Spay));
        assert_eq!(sterilisation_in("Pre-spay bloodwork"), None);
        assert_eq!(sterilisation_in("Spay estimate"), None);
        assert_eq!(sterilisation_in("Neuter consult"), None);
        assert_eq!(sterilisation_in("Annual wellness exam"), None);
        for planned in [
            "Spay scheduled",
            "Spay consent",
            "Spay cancelled",
            "Neuter booked",
            "Spay due",
            "No spay",
        ] {
            assert_eq!(sterilisation_in(planned), None, "{planned}");
        }
        assert_eq!(
            sterilisation_in("Altered diet"),
            None,
            "a status word is not a procedure"
        );
        assert_eq!(
            record_sterilisation("Spay estimate", ["OHE 20-40 lb"]),
            None,
            "the title speaks for the record"
        );
        assert_eq!(sterilisation_in("Spaghetti incident"), None);
        assert_eq!(
            record_sterilisation("Surgery", ["Anesthesia", "Feline spay"]),
            Some(Spay)
        );
        assert_eq!(record_sterilisation("Surgery", ["Anesthesia"]), None);
    }

    #[test]
    fn sterilises_only_points_one_way() {
        assert!(sterilises(Some("female"), "female_spayed"));
        assert!(sterilises(Some("male"), "male_neutered"));
        assert!(sterilises(None, "female_spayed"));
        assert!(sterilises(Some("unknown"), "male_neutered"));
        assert!(!sterilises(Some("male"), "female_spayed"));
        assert!(!sterilises(Some("female_spayed"), "female_spayed"));
        assert!(!sterilises(Some("female"), "female"));
    }

    #[test]
    fn sterilised_form_follows_the_recorded_sex() {
        use Sterilisation::*;
        assert_eq!(sterilised_form(Some("female"), Spay), Some("female_spayed"));
        assert_eq!(
            sterilised_form(Some("female"), Either),
            Some("female_spayed")
        );
        assert_eq!(sterilised_form(Some("female"), Castration), None);
        assert_eq!(sterilised_form(Some("male"), Either), Some("male_neutered"));
        assert_eq!(
            sterilised_form(Some("male"), Castration),
            Some("male_neutered")
        );
        assert_eq!(sterilised_form(Some("male"), Spay), None);
        assert_eq!(sterilised_form(None, Spay), Some("female_spayed"));
        assert_eq!(
            sterilised_form(Some("unknown"), Castration),
            Some("male_neutered")
        );
        assert_eq!(sterilised_form(None, Either), None);
        assert_eq!(sterilised_form(Some("female_spayed"), Spay), None);
        assert_eq!(sterilised_form(Some("male_neutered"), Either), None);
    }
}
