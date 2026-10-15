use recruitment_contracts::{Stage, match_percent};

#[test]
fn there_are_eight_stages_in_funnel_order() {
    let keys: Vec<&str> = Stage::ALL.iter().map(|s| s.key()).collect();
    assert_eq!(
        keys,
        [
            "new",
            "screening",
            "hr",
            "tech",
            "final",
            "offer",
            "hired",
            "rejected"
        ]
    );
    assert_eq!(Stage::ALL[0].label(), "Новый");
    assert_eq!(Stage::ALL[7].label(), "Отказ");
    assert_eq!(Stage::Tech.number(), 4);
}

#[test]
fn keys_round_trip_and_unknown_is_rejected() {
    for stage in Stage::ALL {
        assert_eq!(Stage::from_key(stage.key()), Some(stage));
    }
    assert_eq!(Stage::from_key("done"), None);
    assert_eq!(Stage::from_key(""), None);
}

#[test]
fn serde_uses_the_same_keys() {
    assert_eq!(serde_json::to_string(&Stage::Hr).unwrap(), "\"hr\"");
    assert_eq!(
        serde_json::from_str::<Stage>("\"offer\"").unwrap(),
        Stage::Offer
    );
    assert!(serde_json::from_str::<Stage>("\"nope\"").is_err());
}

#[test]
fn next_follows_the_order_and_stops_at_the_end() {
    assert_eq!(Stage::New.next(), Some(Stage::Screening));
    assert_eq!(Stage::Tech.next(), Some(Stage::Final));
    assert_eq!(Stage::Offer.next(), Some(Stage::Hired));
    assert_eq!(Stage::Hired.next(), None);
    assert_eq!(Stage::Rejected.next(), None);
}

#[test]
fn only_stages_between_new_and_offer_are_active() {
    assert!(Stage::New.is_active());
    assert!(Stage::Offer.is_active());
    assert!(!Stage::Hired.is_active());
    assert!(!Stage::Rejected.is_active());
}

#[test]
fn new_is_overdue_after_a_day() {
    assert!(!Stage::New.overdue(24));
    assert!(Stage::New.overdue(25));
    assert!(Stage::New.overdue(30));
}

#[test]
fn other_active_stages_are_overdue_after_five_days() {
    assert!(!Stage::Tech.overdue(4 * 24));
    assert!(!Stage::Tech.overdue(5 * 24));
    assert!(Stage::Tech.overdue(5 * 24 + 1));
    assert!(Stage::Screening.overdue(200));
}

#[test]
fn closed_stages_are_never_overdue() {
    assert!(!Stage::Rejected.overdue(24 * 30));
    assert!(!Stage::Hired.overdue(24 * 30));
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn match_is_the_share_of_required_skills_the_person_has() {
    let required = s(&["Go", "K8s", "PostgreSQL"]);
    assert_eq!(match_percent(&required, &s(&["go", "k8S"])), Some(67));
    assert_eq!(
        match_percent(&required, &s(&["Go", "K8s", "PostgreSQL", "Rust"])),
        Some(100)
    );
    assert_eq!(match_percent(&required, &s(&["Java"])), Some(0));
}

#[test]
fn match_is_undefined_without_required_skills() {
    assert_eq!(match_percent(&[], &s(&["Go"])), None);
}

#[test]
fn duplicates_in_requirements_count_once() {
    assert_eq!(match_percent(&s(&["Go", "go"]), &s(&["Go"])), Some(100));
}

#[test]
fn sla_thresholds_are_public_and_match_the_overdue_rule() {
    use recruitment_contracts::{ACTIVE_SLA_HOURS, NEW_SLA_HOURS};
    assert_eq!(NEW_SLA_HOURS, 24);
    assert_eq!(ACTIVE_SLA_HOURS, 120);
    assert!(!Stage::New.overdue(NEW_SLA_HOURS));
    assert!(Stage::New.overdue(NEW_SLA_HOURS + 1));
    assert!(!Stage::Final.overdue(ACTIVE_SLA_HOURS));
    assert!(Stage::Final.overdue(ACTIVE_SLA_HOURS + 1));
}
