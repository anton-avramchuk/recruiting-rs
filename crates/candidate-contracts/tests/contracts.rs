use candidate_contracts::{
    CreatePersonRequest, DuplicateInfo, EmploymentDto, PersonPage, UpdatePersonRequest,
};
use serde_json::json;

#[test]
fn update_distinguishes_absent_null_and_value() {
    let absent: UpdatePersonRequest = serde_json::from_value(json!({})).unwrap();
    assert_eq!(absent.phone, None, "field not sent -> untouched");

    let cleared: UpdatePersonRequest = serde_json::from_value(json!({"phone": null})).unwrap();
    assert_eq!(cleared.phone, Some(None), "null -> clear");

    let set: UpdatePersonRequest =
        serde_json::from_value(json!({"phone": "+7 916 123 45 67"})).unwrap();
    assert_eq!(set.phone, Some(Some("+7 916 123 45 67".to_string())));

    let salary: UpdatePersonRequest =
        serde_json::from_value(json!({"expected_salary": null})).unwrap();
    assert_eq!(salary.expected_salary, Some(None));
    let salary: UpdatePersonRequest =
        serde_json::from_value(json!({"expected_salary": 250000})).unwrap();
    assert_eq!(salary.expected_salary, Some(Some(250000)));
}

#[test]
fn update_does_not_serialize_untouched_fields() {
    let req = UpdatePersonRequest {
        email: Some(None),
        ..UpdatePersonRequest::default()
    };
    assert_eq!(serde_json::to_value(&req).unwrap(), json!({"email": null}));
}

#[test]
fn create_has_sensible_defaults() {
    let req: CreatePersonRequest =
        serde_json::from_value(json!({"full_name": "Иван Петров"})).unwrap();
    assert_eq!(req.full_name, "Иван Петров");
    assert!(req.skills.is_empty());
    assert!(!req.confirm_duplicate);
    assert_eq!(req.phone, None);
}

#[test]
fn page_and_duplicate_round_trip() {
    let page = PersonPage {
        items: vec![],
        total: 0,
        page: 1,
        page_size: 20,
    };
    let back: PersonPage = serde_json::from_str(&serde_json::to_string(&page).unwrap()).unwrap();
    assert_eq!(back, page);

    let dup = DuplicateInfo {
        id: uuid::Uuid::nil(),
        full_name: "Иван".into(),
        matched_by: "email".into(),
    };
    assert_eq!(serde_json::to_value(&dup).unwrap()["matched_by"], "email");
}

#[test]
fn employment_dates_are_plain_iso_strings() {
    let e: EmploymentDto = serde_json::from_value(
        json!({"company": "Acme", "position": "Dev", "start": "2020-01-15"}),
    )
    .unwrap();
    assert_eq!(e.end, None);
    assert_eq!(e.start, "2020-01-15");
}
