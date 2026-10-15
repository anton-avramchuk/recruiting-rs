//! Слияние найденного человека с данными импорта: заполняются только пустые поля.
use candidate::domain::{ContactsInput, Education, Employment, NewPerson, Person};
use chrono::NaiveDate;
use platform_core::tenant::TenantId;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn person(new: NewPerson) -> Person {
    Person::create(TenantId::new(), new).unwrap()
}

fn named(name: &str) -> NewPerson {
    NewPerson {
        full_name: name.into(),
        ..NewPerson::default()
    }
}

fn job(company: &str) -> Employment {
    Employment::new(company, "Dev", d("2020-01-01"), None, None).unwrap()
}

fn school(name: &str) -> Education {
    Education::new(name, None, None, Some(2019)).unwrap()
}

#[test]
fn empty_fields_are_filled_and_the_change_is_reported() {
    let mut existing = person(named("Иван Петров"));
    let incoming = person(NewPerson {
        contacts: ContactsInput {
            phone: Some("+7 916 123 45 67".into()),
            email: Some("ivan@mail.ru".into()),
            ..ContactsInput::default()
        },
        city: Some("Казань".into()),
        source: Some("hh.ru".into()),
        expected_salary: Some(200_000),
        summary: Some("кратко".into()),
        note: Some("заметка".into()),
        declared_experience_years: Some(5),
        ..named("Иван Петров")
    });
    assert!(existing.merge_missing(&incoming));
    assert_eq!(existing.phone.as_ref().unwrap().as_str(), "+79161234567");
    assert_eq!(existing.email.as_ref().unwrap().as_str(), "ivan@mail.ru");
    assert_eq!(existing.city.as_deref(), Some("Казань"));
    assert_eq!(existing.source.as_deref(), Some("hh.ru"));
    assert_eq!(existing.expected_salary, Some(200_000));
    assert_eq!(existing.summary.as_deref(), Some("кратко"));
    assert_eq!(existing.note.as_deref(), Some("заметка"));
    assert_eq!(existing.declared_experience_years, 5);
}

#[test]
fn existing_values_are_never_overwritten() {
    let mut existing = person(NewPerson {
        contacts: ContactsInput {
            phone: Some("+7 903 000 00 00".into()),
            email: Some("old@mail.ru".into()),
            ..ContactsInput::default()
        },
        city: Some("Москва".into()),
        source: Some("Реферал".into()),
        expected_salary: Some(100_000),
        summary: Some("старое".into()),
        note: Some("старая заметка".into()),
        declared_experience_years: Some(3),
        ..named("Иван Петров")
    });
    let before = existing.clone();
    let incoming = person(NewPerson {
        contacts: ContactsInput {
            phone: Some("+7 916 123 45 67".into()),
            email: Some("new@mail.ru".into()),
            ..ContactsInput::default()
        },
        city: Some("Казань".into()),
        source: Some("hh.ru".into()),
        expected_salary: Some(900_000),
        summary: Some("новое".into()),
        note: Some("новая заметка".into()),
        declared_experience_years: Some(9),
        ..named("Иван Петров")
    });
    assert!(!existing.merge_missing(&incoming), "nothing to fill");
    assert_eq!(existing, before);
}

#[test]
fn skills_are_united_without_repeats_ignoring_case() {
    let mut existing = person(NewPerson {
        skills: vec!["Rust".into(), "SQL".into()],
        ..named("Иван")
    });
    let incoming = person(NewPerson {
        skills: vec!["sql".into(), "Docker".into()],
        ..named("Иван")
    });
    assert!(existing.merge_missing(&incoming));
    assert_eq!(existing.skills, ["Rust", "SQL", "Docker"]);

    let same = person(NewPerson {
        skills: vec!["RUST".into()],
        ..named("Иван")
    });
    assert!(!existing.merge_missing(&same));
}

#[test]
fn history_is_added_only_when_the_person_has_none() {
    let mut empty = person(named("Иван"));
    let mut incoming = person(named("Иван"));
    incoming.replace_employments(vec![job("Acme")]);
    incoming.replace_educations(vec![school("МГУ")]);
    assert!(empty.merge_missing(&incoming));
    assert_eq!(empty.employments().len(), 1);
    assert_eq!(empty.educations().len(), 1);

    let mut known = person(named("Иван"));
    known.replace_employments(vec![job("Beta")]);
    known.replace_educations(vec![school("МФТИ")]);
    assert!(!known.merge_missing(&incoming));
    assert_eq!(known.employments()[0].company, "Beta");
    assert_eq!(known.educations()[0].institution, "МФТИ");
}

#[test]
fn declared_experience_fills_only_a_zero() {
    let mut zero = person(named("Иван"));
    let incoming = person(NewPerson {
        declared_experience_years: Some(4),
        ..named("Иван")
    });
    assert!(zero.merge_missing(&incoming));
    assert_eq!(zero.declared_experience_years, 4);
    let mut set = person(NewPerson {
        declared_experience_years: Some(2),
        ..named("Иван")
    });
    assert!(!set.merge_missing(&incoming));
    assert_eq!(set.declared_experience_years, 2);
}

#[test]
fn the_name_and_identity_never_change() {
    let mut existing = person(named("Иван Петров"));
    let id = existing.id;
    let incoming = person(named("ПЕТРОВ ИВАН"));
    existing.merge_missing(&incoming);
    assert_eq!(existing.full_name, "Иван Петров");
    assert_eq!(existing.id, id);
}
