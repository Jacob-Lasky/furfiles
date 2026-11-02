//! Insurance value (#74). Every expected number here is worked by hand from the
//! inputs (shown in the comment beside it), never recomputed by the code under
//! test.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use chrono::NaiveDate;
use serde_json::{json, Value};
use sqlx::types::Json as SqlxJson;

mod common;
use common::send;

use furfiles::import::{claim_key, claim_value, PlannedClaim};
use furfiles::insurance_value::{
    claim_notes, parse_money, pet_value, prorate, sub_limit, PaidVisit, PolicyValue,
};
use furfiles::models::{InsurancePolicy, LabeledDetail};
use furfiles::{api_router, memory_pool, AppState};

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn lemonade_coverages() -> Vec<LabeledDetail> {
    let c = |label: &str, detail: &str| LabeledDetail {
        label: label.into(),
        detail: Some(detail.into()),
    };
    vec![
        c(
            "Accident & Illness Treatment",
            "90% reimbursement for eligible vet treatment and diagnostic costs, up to $50,000/yr, subject to $500 annual deductible",
        ),
        c("Wellness Exam", "Up to $70 total for 1 exam/yr"),
        c("Wellness Testing", "Up to $180/yr (heartworm, FeLV/FIV, internal parasites, blood CBC/Chemistry, fecal tests)"),
        c("Vaccines", "Up to $105 total for 3 vaccines/yr"),
        c("Preventative Care", "Effective 2026-02-10; no deductible applies"),
    ]
}

/// Lulu's Lemonade policy shape: $911.36/yr, 90%, $500 deductible, $50k limit.
fn policy(
    id: &str,
    number: &str,
    start: &str,
    end: Option<&str>,
    status: &str,
    premium: i64,
) -> InsurancePolicy {
    InsurancePolicy {
        id: id.into(),
        pet_id: "lulu".into(),
        kind: "insurance".into(),
        provider: "Lemonade".into(),
        plan_name: None,
        policy_number: Some(number.into()),
        status: status.into(),
        start_date: Some(start.into()),
        end_date: end.map(Into::into),
        premium_cents: Some(premium),
        premium_interval: Some("year".into()),
        reimburse_percent: Some(90),
        deductible_cents: Some(50000),
        annual_limit_cents: Some(5000000),
        notes: None,
        coverages: Some(SqlxJson(lemonade_coverages())),
        exclusions: None,
        document_path: None,
        document_name: None,
        document_media_type: None,
        document_size_bytes: None,
        created_by: None,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn visit(id: i64, on: &str, covered: i64, claim: Option<(&str, &str)>) -> PaidVisit {
    PaidVisit {
        id,
        pet_id: "lulu".into(),
        occurred_on: on.into(),
        title: format!("visit {id}"),
        covered_cents: covered,
        extra: claim.map(|(number, value)| {
            SqlxJson(BTreeMap::from([(
                format!("Insurance claim {number}"),
                value.to_string(),
            )]))
        }),
    }
}

fn only(p: InsurancePolicy, visits: &[PaidVisit], as_of: &str) -> PolicyValue {
    pet_value("lulu", "Lulu", &[p], visits, d(as_of))
        .policies
        .remove(0)
}

// ---------- pure parsing ----------

/// Producer/consumer contract: whatever import writes for a claim, insurance
/// value must read back as (payout, policy). Built from import's own writers,
/// not a hand-typed copy of the format.
#[test]
fn claim_notes_read_back_exactly_what_import_writes() {
    let claim = PlannedClaim {
        claim_number: "demo-clm-0952".into(),
        policy_number: Some("DEMO-POL-0401".into()),
        paid_cents: 17712,
        ..Default::default()
    };
    let extra = BTreeMap::from([
        (claim_key(&claim.claim_number), claim_value(&claim)),
        ("Diagnosis".to_string(), "$5 · policy X".to_string()), // not a claim key
    ]);
    assert_eq!(
        claim_notes(&extra),
        vec![(17712, "DEMO-POL-0401".to_string())]
    );
    // A claim recorded without a policy number names no policy.
    let bare = PlannedClaim {
        policy_number: None,
        ..claim.clone()
    };
    let extra = BTreeMap::from([(claim_key(&bare.claim_number), claim_value(&bare))]);
    assert_eq!(claim_notes(&extra), vec![]);
}

#[test]
fn parse_money_reads_the_first_dollar_amount_as_cents() {
    assert_eq!(
        parse_money("$177.12 paid · policy DEMO-POL-0401"),
        Some(17712)
    );
    assert_eq!(parse_money("Up to $1,050.50 a year"), Some(105050));
    assert_eq!(parse_money("up to $70."), Some(7000));
    assert_eq!(parse_money("$4.5"), Some(450));
    assert_eq!(parse_money("no money here"), None);
    assert_eq!(parse_money("$ abc"), None);
}

#[test]
fn sub_limit_finds_preventative_caps_and_skips_everything_else() {
    let c = |label: &str, detail: Option<&str>| LabeledDetail {
        label: label.into(),
        detail: detail.map(Into::into),
    };
    assert_eq!(
        sub_limit(&c("Wellness Exam", Some("Up to $70 total for 1 exam/yr"))),
        Some(7000)
    );
    assert_eq!(
        sub_limit(&c("Vaccines", Some("$105/yr for 3 vaccines"))),
        Some(10500)
    );
    assert_eq!(
        sub_limit(&c("Dental cleaning", Some("$150 per year"))),
        Some(15000)
    );
    // The accident & illness cap is not a preventative allowance.
    assert_eq!(
        sub_limit(&c("Accident & Illness", Some("up to $50,000/yr"))),
        None
    );
    assert_eq!(sub_limit(&c("Annual limit", Some("Up to $5,000"))), None);
    assert_eq!(sub_limit(&c("Deductible", Some("$500/yr"))), None);
    // A wellness membership's copay is a price, not a cap.
    assert_eq!(sub_limit(&c("Office visits", Some("Up to $0 copay"))), None);
    // A dollar figure that isn't a yearly cap, or none at all.
    assert_eq!(sub_limit(&c("Spay/neuter", Some("$200 discount"))), None);
    assert_eq!(
        sub_limit(&c(
            "Preventative Care",
            Some("Effective 2026-02-10; no deductible applies")
        )),
        None
    );
    assert_eq!(sub_limit(&c("Vaccines", None)), None);
}

#[test]
fn prorate_charges_whole_years_in_full_and_the_rest_by_day() {
    // A one-year term is exactly the annual premium.
    assert_eq!(prorate(91136, d("2025-09-14"), d("2026-09-14")), 91136);
    // Also across a Feb 29 (2027-09-14 → 2028-09-14 is 366 days).
    assert_eq!(prorate(91136, d("2027-09-14"), d("2028-09-14")), 91136);
    // 2026-02-10 → 2026-09-14 = 216 days of a 365-day year:
    // 91136 × 216 / 365 = 53932.54 → 53933.
    assert_eq!(prorate(91136, d("2026-02-10"), d("2026-09-14")), 53933);
    // Two years and 10 days: 2 × 1000 + 1000 × 10 / 365 (27.4) → 2027.
    assert_eq!(prorate(1000, d("2024-01-01"), d("2026-01-11")), 2027);
    assert_eq!(prorate(91136, d("2026-09-14"), d("2026-09-14")), 0);
    assert_eq!(prorate(91136, d("2026-09-14"), d("2026-01-01")), 0);
}

// ---------- worked case: Lulu, 2025-26 and 2026-27 ----------

#[test]
fn lulu_old_and_current_terms_with_claims_inside_outside_and_by_note() {
    let old = policy(
        "old",
        "DEMO-POL-0401",
        "2025-09-14",
        Some("2026-09-14"),
        "expired",
        91136,
    );
    let cur = policy(
        "cur",
        "DEMO-POL-0778",
        "2026-09-14",
        Some("2027-09-14"),
        "active",
        75033,
    );
    let visits = [
        visit(
            1,
            "2026-02-12",
            5000,
            Some(("DEMO-CLM-0818", "$50.00 paid · policy DEMO-POL-0401")),
        ),
        visit(2, "2026-05-20", 17700, None), // no note: inside the old term by date
        visit(
            3,
            "2026-09-04",
            17712,
            Some(("DEMO-CLM-0952", "$177.12 paid · policy DEMO-POL-0401")),
        ),
        visit(
            4,
            "2026-09-18",
            18000,
            Some(("DEMO-CLM-0392", "$180.00 paid · policy DEMO-POL-0778")),
        ),
        visit(5, "2025-06-01", 8000, None), // before any recorded term
    ];
    let v = pet_value("lulu", "Lulu", &[old, cur], &visits, d("2026-10-11"));

    // Current term first.
    assert_eq!(v.policies[0].policy_id, "cur");
    assert_eq!(v.policies[1].policy_id, "old");

    let o = &v.policies[1];
    assert_eq!(o.phase, "ended");
    assert_eq!(o.term_premium_cents, Some(91136));
    assert_eq!(o.paid_in_cents, Some(91136)); // the term is over: all of it
    assert_eq!(o.paid_back_cents, 40412); // 5000 + 17700 + 17712
    assert_eq!(o.net_cents, Some(-50724)); // 40412 − 91136
    let by: Vec<(i64, &str)> = o
        .payouts
        .iter()
        .map(|p| (p.record_id, p.matched_by))
        .collect();
    assert_eq!(by, vec![(1, "claim"), (2, "term"), (3, "claim")]);
    // 50000 + ceil(91136 × 100 / 90 = 101262.2) = 50000 + 101263.
    let be = o.break_even.as_ref().unwrap();
    assert_eq!(be.bill_cents, Some(151263));
    assert!(be.reachable);
    assert_eq!(
        (be.premium_cents, be.deductible_cents, be.reimburse_percent),
        (91136, 50000, 90)
    );
    let prev = o.preventative.as_ref().unwrap();
    assert_eq!(prev.allowance_cents, 35500); // 7000 + 18000 + 10500
    assert_eq!(prev.unused_at_least_cents, 0); // paid back 40412 > 35500
    assert_eq!(
        prev.limits
            .iter()
            .map(|l| l.label.as_str())
            .collect::<Vec<_>>(),
        ["Wellness Exam", "Wellness Testing", "Vaccines"]
    );
    assert!(prev.limits.iter().all(|l| l.unused_cents.is_none()));
    assert!(o.missing.is_empty() && o.assumed.is_empty());

    let c = &v.policies[0];
    assert_eq!(c.phase, "current");
    assert_eq!(c.term_premium_cents, Some(75033));
    // 2026-09-14 → 2026-10-11 inclusive = 28 days: 75033 × 28 / 365 = 5755.96 → 5756.
    assert_eq!(c.paid_in_cents, Some(5756));
    assert_eq!(c.paid_back_cents, 18000);
    assert_eq!(c.net_cents, Some(12244));
    // 50000 + 75033 × 100 / 90 (= 83370 exactly).
    assert_eq!(c.break_even.as_ref().unwrap().bill_cents, Some(133370));
    assert_eq!(
        c.preventative.as_ref().unwrap().unused_at_least_cents,
        17500
    ); // 35500 − 18000

    assert_eq!(v.paid_in_cents, 96892); // 91136 + 5756
    assert_eq!(v.paid_back_cents, 58412); // 40412 + 18000
    assert_eq!(v.net_cents, -38480);
    assert!(!v.paid_in_incomplete);
    assert_eq!(v.unmatched_paid_back_cents, 8000); // visit 5
    assert_eq!(v.wellness_paid_in_cents, 0);
}

// ---------- edge cases ----------

#[test]
fn a_policy_replaced_early_is_prorated_to_its_real_end() {
    let p = policy(
        "p",
        "DEMO-POL-0401",
        "2026-02-10",
        Some("2026-09-14"),
        "expired",
        91136,
    );
    let v = only(p, &[], "2026-10-11");
    assert_eq!(v.term_premium_cents, Some(53933)); // 216/365 of 91136
    assert_eq!(v.paid_in_cents, Some(53933));
    // 50000 + ceil(53933 × 100 / 90 = 59925.6) = 50000 + 59926.
    assert_eq!(v.break_even.unwrap().bill_cents, Some(109926));
}

#[test]
fn a_missing_reimbursement_or_deductible_is_named_not_invented() {
    let mut p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", 60000);
    p.reimburse_percent = None;
    let v = only(p.clone(), &[], "2026-12-31");
    assert!(v.break_even.is_none());
    assert_eq!(v.missing, vec!["reimbursement %"]);
    assert_eq!(v.paid_in_cents, Some(60000)); // the rest still adds up
    p.deductible_cents = None;
    assert_eq!(
        only(p, &[], "2026-12-31").missing,
        vec!["reimbursement %", "deductible"]
    );
}

#[test]
fn a_missing_premium_or_start_blanks_paid_in_and_says_why() {
    let mut p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", 0);
    p.premium_cents = None;
    let v = only(p, &[visit(1, "2026-03-01", 4000, None)], "2026-06-01");
    assert_eq!(v.paid_in_cents, None);
    assert_eq!(v.net_cents, None);
    assert!(v.break_even.is_none());
    assert_eq!(v.missing, vec!["premium"]);
    assert_eq!(v.paid_back_cents, 4000);

    let mut p = policy("p", "X", "2026-01-01", None, "active", 60000);
    p.start_date = None;
    let v = only(p, &[visit(1, "2026-03-01", 4000, None)], "2026-06-01");
    assert_eq!(v.phase, "unknown");
    assert_eq!(v.missing, vec!["start date"]);
    assert_eq!(v.paid_back_cents, 0); // no term to place the payout in
}

#[test]
fn a_zero_premium_breaks_even_at_the_deductible() {
    let p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", 0);
    let v = only(p, &[visit(1, "2026-03-01", 4000, None)], "2026-06-01");
    assert_eq!(v.paid_in_cents, Some(0));
    assert_eq!(v.term_premium_cents, Some(0));
    assert_eq!(v.net_cents, Some(4000));
    assert_eq!(v.break_even.unwrap().bill_cents, Some(50000));
}

#[test]
fn a_claim_dated_outside_the_term_is_not_counted_unless_its_note_names_the_policy() {
    let p = policy(
        "p",
        "DEMO-POL-0401",
        "2025-09-14",
        Some("2026-09-14"),
        "expired",
        91136,
    );
    // The renewal day belongs to the next term, not this one.
    let visits = [
        visit(1, "2026-09-14", 6000, None),
        visit(2, "2025-09-13", 2500, None),
    ];
    let v = pet_value(
        "lulu",
        "Lulu",
        std::slice::from_ref(&p),
        &visits,
        d("2026-10-11"),
    );
    assert_eq!(v.policies[0].paid_back_cents, 0);
    assert_eq!(v.unmatched_paid_back_cents, 8500);
    // The insurer's own claim note outranks the calendar.
    let named = [visit(
        1,
        "2026-09-20",
        6000,
        Some(("LPC1", "$60.00 paid · policy demo-pol-0401")),
    )];
    let v = only(p, &named, "2026-10-11");
    assert_eq!(v.paid_back_cents, 6000);
    assert_eq!(v.payouts[0].matched_by, "claim");
}

#[test]
fn a_claim_note_never_counts_more_than_the_visit_paid_and_the_rest_goes_by_date() {
    let p = policy("p", "A", "2026-01-01", Some("2027-01-01"), "active", 60000);
    // Note says $90 but the visit records $60 covered: clamp to 60.
    let v = only(
        p.clone(),
        &[visit(
            1,
            "2026-02-01",
            6000,
            Some(("C1", "$90.00 paid · policy A")),
        )],
        "2026-06-01",
    );
    assert_eq!(v.paid_back_cents, 6000);
    // Note says $20 of a $60 payout: 20 by note, the other 40 by date.
    let v = only(
        p,
        &[visit(
            1,
            "2026-02-01",
            6000,
            Some(("C1", "$20.00 paid · policy A")),
        )],
        "2026-06-01",
    );
    let parts: Vec<(i64, &str)> = v
        .payouts
        .iter()
        .map(|x| (x.amount_cents, x.matched_by))
        .collect();
    assert_eq!(parts, vec![(2000, "claim"), (4000, "term")]);
}

#[test]
fn overlapping_terms_never_count_a_payout_twice() {
    let a = policy("a", "A", "2026-01-01", Some("2027-01-01"), "active", 60000);
    let b = policy("b", "B", "2026-06-01", Some("2027-06-01"), "active", 60000);
    let v = pet_value(
        "lulu",
        "Lulu",
        &[a, b],
        &[visit(1, "2026-07-01", 5000, None)],
        d("2026-08-01"),
    );
    let back: Vec<(&str, i64)> = v
        .policies
        .iter()
        .map(|p| (p.policy_id.as_str(), p.paid_back_cents))
        .collect();
    // Both current; b started later, so it holds the un-noted payout.
    assert!(back.contains(&("b", 5000)) && back.contains(&("a", 0)));
    assert_eq!(v.paid_back_cents, 5000);
}

#[test]
fn a_monthly_premium_is_annualised_and_prorated_by_day() {
    let mut p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", 4500);
    p.premium_interval = Some("month".into());
    let v = only(p, &[], "2026-03-31");
    assert_eq!(v.annual_premium_cents, Some(54000)); // 4500 × 12
    assert_eq!(v.term_premium_cents, Some(54000));
    // Jan 1 → Mar 31 inclusive = 90 days: 54000 × 90 / 365 = 13315.07 → 13315.
    assert_eq!(v.paid_in_cents, Some(13315));
}

#[test]
fn an_unknown_interval_is_missing_and_no_interval_is_assumed_yearly() {
    let mut p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", 4500);
    p.premium_interval = Some("quarter".into());
    let v = only(p.clone(), &[], "2026-03-31");
    assert_eq!(v.paid_in_cents, None);
    assert_eq!(v.missing, vec!["premium interval"]);
    p.premium_interval = None;
    let v = only(p, &[], "2027-03-31");
    assert_eq!(v.paid_in_cents, Some(4500));
    assert_eq!(v.assumed, vec!["premium per year"]);
}

#[test]
fn an_active_policy_without_an_end_runs_a_year_and_an_ended_one_is_unknown() {
    let p = policy("p", "X", "2026-01-01", None, "active", 36500);
    let v = only(p.clone(), &[], "2026-01-10");
    assert_eq!(v.end_date.as_deref(), Some("2027-01-01"));
    assert_eq!(v.assumed, vec!["end date"]);
    assert_eq!(v.paid_in_cents, Some(1000)); // 10 days of 36500/365
    let mut ended = p;
    ended.status = "cancelled".into();
    let pv = pet_value("lulu", "Lulu", &[ended], &[], d("2026-06-01"));
    assert_eq!(pv.policies[0].missing, vec!["end date"]);
    assert_eq!(pv.policies[0].paid_in_cents, None);
    assert!(pv.paid_in_incomplete);
}

#[test]
fn an_upcoming_policy_has_paid_nothing_in_yet() {
    let p = policy("p", "X", "2026-12-01", Some("2027-12-01"), "active", 60000);
    let v = only(p, &[], "2026-10-11");
    assert_eq!(v.phase, "upcoming");
    assert_eq!(v.paid_in_cents, Some(0));
    assert_eq!(v.term_premium_cents, Some(60000));
}

#[test]
fn break_even_is_unreachable_past_the_annual_limit_or_at_zero_percent() {
    let mut p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", 91136);
    p.annual_limit_cents = Some(50000);
    let be = only(p.clone(), &[], "2026-06-01").break_even.unwrap();
    assert!(!be.reachable);
    assert_eq!(be.bill_cents, None);
    p.annual_limit_cents = None;
    p.reimburse_percent = Some(0);
    assert_eq!(
        only(p, &[], "2026-06-01").break_even.unwrap().bill_cents,
        None
    );
}

#[test]
fn untouched_sub_limits_are_all_still_available() {
    let p = policy("p", "X", "2026-09-14", Some("2027-09-14"), "active", 75033);
    let prev = only(p, &[], "2026-10-11").preventative.unwrap();
    assert_eq!(prev.unused_at_least_cents, 35500);
    let left: Vec<Option<i64>> = prev.limits.iter().map(|l| l.unused_cents).collect();
    assert_eq!(left, vec![Some(7000), Some(18000), Some(10500)]);
}

#[test]
fn a_wellness_plan_shows_its_fee_and_only_claims_that_name_it() {
    let mut w = policy(
        "w",
        "WP-1",
        "2026-01-01",
        Some("2027-01-01"),
        "active",
        3995,
    );
    w.kind = "wellness".into();
    w.premium_interval = Some("month".into());
    w.reimburse_percent = None;
    w.deductible_cents = None;
    w.coverages = Some(SqlxJson(vec![LabeledDetail {
        label: "Office visits".into(),
        detail: Some("$0 copay".into()),
    }]));
    let v = pet_value(
        "lulu",
        "Lulu",
        &[w],
        &[
            visit(1, "2026-03-01", 5000, None),
            visit(
                2,
                "2026-04-01",
                3000,
                Some(("C9", "$30.00 paid · policy WP-1")),
            ),
        ],
        d("2026-12-31"),
    );
    let p = &v.policies[0];
    assert_eq!(p.paid_in_cents, Some(47940)); // 3995 × 12, the full year
    assert_eq!(p.paid_back_cents, 3000); // only the payout its note names
    assert!(p.break_even.is_none());
    assert!(
        p.missing.is_empty(),
        "a wellness plan has no reimbursement to miss"
    );
    assert!(p.preventative.is_none());
    assert_eq!(v.unmatched_paid_back_cents, 5000);
    // The pet's totals are the insurance terms; the fee is counted apart.
    assert_eq!((v.paid_in_cents, v.paid_back_cents, v.net_cents), (0, 0, 0));
    assert_eq!(v.wellness_paid_in_cents, 47940);
    assert!(!v.paid_in_incomplete);
}

#[test]
fn coverage_caps_that_are_not_preventative_care_are_not_allowance() {
    let c = |label: &str, detail: &str| LabeledDetail {
        label: label.into(),
        detail: Some(detail.into()),
    };
    assert_eq!(sub_limit(&c("Hospitalization", "Up to $5,000/yr")), None);
    assert_eq!(
        sub_limit(&c("Wellness annual limit", "Up to $250 a year")),
        Some(25000)
    );
    assert_eq!(
        sub_limit(&c("Routine care", "up to $1,050.50")),
        Some(105050)
    );
}

#[test]
fn visits_after_as_of_have_not_happened_yet() {
    let p = policy("p", "A", "2026-01-01", Some("2027-01-01"), "active", 36500);
    let visits = [
        visit(1, "2026-03-01", 4000, None),
        visit(
            2,
            "2026-08-01",
            9000,
            Some(("C2", "$90.00 paid · policy A")),
        ),
    ];
    let v = pet_value("lulu", "Lulu", &[p], &visits, d("2026-06-30"));
    assert_eq!(v.policies[0].paid_back_cents, 4000);
    assert_eq!(v.policies[0].paid_in_cents, Some(18100)); // Jan 1 → Jun 30 = 181 days × 100
    assert_eq!(v.unmatched_paid_back_cents, 0);
}

#[test]
fn a_renewal_under_the_same_number_keeps_each_claim_in_its_own_term() {
    let old = policy(
        "old",
        "SAME-1",
        "2025-09-14",
        Some("2026-09-14"),
        "expired",
        91136,
    );
    let new = policy(
        "new",
        "SAME-1",
        "2026-09-14",
        Some("2027-09-14"),
        "active",
        75033,
    );
    let note = Some(("C", "$50.00 paid · policy SAME-1"));
    let visits = [
        visit(1, "2026-02-12", 5000, note),
        visit(2, "2026-10-01", 5000, note),
    ];
    let v = pet_value("lulu", "Lulu", &[new, old], &visits, d("2026-10-11"));
    let back: Vec<(&str, i64)> = v
        .policies
        .iter()
        .map(|p| (p.policy_id.as_str(), p.paid_back_cents))
        .collect();
    assert_eq!(back, vec![("new", 5000), ("old", 5000)]);
}

#[test]
fn a_claim_naming_a_policy_the_pet_lacks_is_never_credited_to_another() {
    let b = policy("b", "B", "2026-01-01", Some("2027-01-01"), "active", 60000);
    let visits = [visit(
        1,
        "2026-03-01",
        7000,
        Some(("C1", "$50.00 paid · policy A-UNKNOWN")),
    )];
    let v = pet_value("lulu", "Lulu", &[b], &visits, d("2026-06-01"));
    // $50 named policy A (not recorded): unmatched. The other $20 isn't
    // explained by a note, so it falls to B by date.
    assert_eq!(v.unmatched_paid_back_cents, 5000);
    assert_eq!(v.policies[0].paid_back_cents, 2000);
}

#[test]
fn a_wellness_plan_cannot_see_its_allowance_being_used() {
    let mut w = policy(
        "w",
        "WP-1",
        "2026-01-01",
        Some("2027-01-01"),
        "active",
        3995,
    );
    w.kind = "wellness".into();
    let prev = only(w, &[], "2026-06-01").preventative.unwrap();
    assert_eq!(prev.allowance_cents, 35500);
    assert_eq!(prev.unused_at_least_cents, 0);
    assert!(prev.limits.iter().all(|l| l.unused_cents.is_none()));
}

#[test]
fn a_negative_or_enormous_premium_never_panics_or_goes_negative() {
    let mut p = policy("p", "X", "2026-01-01", Some("2027-01-01"), "active", -91136);
    let v = only(p.clone(), &[], "2026-06-01");
    assert_eq!(v.missing, vec!["premium"]);
    assert_eq!(v.paid_in_cents, None);
    p.premium_cents = Some(i64::MAX / 2);
    p.premium_interval = Some("month".into());
    let v = only(p, &[visit(1, "2026-03-01", i64::MAX, None)], "2026-12-31");
    assert_eq!(v.annual_premium_cents, Some(i64::MAX)); // saturated, not wrapped
    assert_eq!(v.paid_in_cents, Some(i64::MAX));
    assert_eq!(v.paid_back_cents, i64::MAX);
    assert_eq!(v.break_even.unwrap().bill_cents, None); // limit < premium
}

// ---------- through the REST API ----------

async fn setup() -> axum::Router {
    let app = api_router(AppState::for_test(memory_pool().await));
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Lulu","species":"Dog"})),
    )
    .await;
    send(
        &app,
        "POST",
        "/pets",
        Some(json!({"name":"Pedro","species":"Dog"})),
    )
    .await;
    app
}

async fn add_policy(app: &axum::Router, body: Value) -> String {
    let (s, p) = send(app, "POST", "/pets/lulu/insurance", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    p["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn value_endpoints_report_lulus_terms() {
    let app = setup().await;
    let old = add_policy(
        &app,
        json!({"provider":"Lemonade","policy_number":"DEMO-POL-0401","status":"expired",
               "start_date":"2025-09-14","end_date":"2026-09-14","premium_cents":91136,
               "premium_interval":"year","reimburse_percent":90,"deductible_cents":50000,
               "annual_limit_cents":5000000,
               "coverages":[{"label":"Vaccines","detail":"Up to $105 total for 3 vaccines/yr"}]}),
    )
    .await;
    let cur = add_policy(
        &app,
        json!({"provider":"Lemonade","policy_number":"DEMO-POL-0778","status":"active",
               "start_date":"2026-09-14","end_date":"2027-09-14","premium_cents":75033,
               "premium_interval":"year","reimburse_percent":90,"deductible_cents":50000}),
    )
    .await;
    for (on, covered, extra) in [
        (
            "2026-02-12",
            5000,
            json!({"Insurance claim DEMO-CLM-0818":"$50.00 paid · policy DEMO-POL-0401"}),
        ),
        ("2026-05-20", 17700, json!({})),
        (
            "2026-09-04",
            17712,
            json!({"Insurance claim DEMO-CLM-0952":"$177.12 paid · policy DEMO-POL-0401"}),
        ),
        ("2026-09-18", 18000, json!({})),
    ] {
        let (s, _) = send(
            &app,
            "POST",
            "/pets/lulu/records",
            Some(json!({"occurred_on":on,"title":"Visit","cost_cents":covered * 2,"covered_cents":covered,"extra":extra})),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    // A visit with no payout contributes nothing.
    send(
        &app,
        "POST",
        "/pets/lulu/records",
        Some(json!({"occurred_on":"2026-03-01","title":"Nails","cost_cents":2000})),
    )
    .await;

    let (s, h) = send(&app, "GET", "/insurance/value?as_of=2026-10-11", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["as_of"], "2026-10-11");
    let pets = h["pets"].as_array().unwrap();
    assert_eq!(pets.len(), 1, "Pedro has no policy, so no row");
    assert_eq!(pets[0]["name"], "Lulu");
    assert_eq!(pets[0]["paid_in_cents"], 96892);
    assert_eq!(pets[0]["paid_back_cents"], 58412);

    let (_, pet) = send(
        &app,
        "GET",
        "/pets/lulu/insurance/value?as_of=2026-10-11",
        None,
    )
    .await;
    assert_eq!(pet["policies"][0]["policy_id"], cur.as_str());
    assert_eq!(pet["policies"][1]["paid_back_cents"], 40412);

    let (s, one) = send(
        &app,
        "GET",
        &format!("/insurance/{old}/value?as_of=2026-10-11"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(one["paid_in_cents"], 91136);
    assert_eq!(one["net_cents"], -50724);
    assert_eq!(one["break_even"]["bill_cents"], 151263);
    assert_eq!(one["preventative"]["allowance_cents"], 10500);
    assert_eq!(one["payouts"].as_array().unwrap().len(), 3);
    assert_eq!(one["phase"], "ended");

    // No `as_of`: valued today, still a 200.
    let (s, _) = send(&app, "GET", &format!("/insurance/{cur}/value"), None).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn policy_amounts_are_validated_on_write() {
    let app = setup().await;
    for bad in [
        json!({"provider":"L","premium_cents":-1}),
        json!({"provider":"L","deductible_cents":-50000}),
        json!({"provider":"L","annual_limit_cents":-1}),
        json!({"provider":"L","reimburse_percent":150}),
        json!({"provider":"L","reimburse_percent":-10}),
    ] {
        let (s, e) = send(&app, "POST", "/pets/lulu/insurance", Some(bad.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad} → {e}");
    }
    let id = add_policy(
        &app,
        json!({"provider":"L","reimburse_percent":100,"premium_cents":0}),
    )
    .await;
    let (s, _) = send(
        &app,
        "PATCH",
        &format!("/insurance/{id}"),
        Some(json!({"reimburse_percent":101})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, p) = send(
        &app,
        "PATCH",
        &format!("/insurance/{id}"),
        Some(json!({"reimburse_percent":80})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p["reimburse_percent"], 80);
}

#[tokio::test]
async fn value_endpoints_validate_and_404() {
    let app = setup().await;
    let (s, _) = send(&app, "GET", "/insurance/value?as_of=2026-02-31", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&app, "GET", "/insurance/nope/value", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = send(&app, "GET", "/pets/nobody/insurance/value", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // A pet with no policies: an empty, honest answer.
    let (s, v) = send(&app, "GET", "/pets/pedro/insurance/value", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["policies"], json!([]));
    assert_eq!(v["paid_in_cents"], 0);
    let (_, h) = send(&app, "GET", "/insurance/value", None).await;
    assert_eq!(h["pets"], json!([]));
}
