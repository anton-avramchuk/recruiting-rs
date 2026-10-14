//! Домен кандидата: чистая логика без БД и HTTP.
use candidate::domain::{
    ContactsInput, DomainError, Education, Employment, MatchedBy, NewPerson, Person, PersonKeys,
    Probe, Telegram, WhatsApp, experience_years, find_duplicate, normalize_skills,
};
use chrono::NaiveDate;
use platform_core::tenant::TenantId;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn job(start: &str, end: Option<&str>) -> Employment {
    Employment::new("Acme", "Dev", d(start), end.map(d), None).unwrap()
}

fn person(name: &str) -> Person {
    Person::create(
        TenantId::new(),
        NewPerson {
            full_name: name.into(),
            ..NewPerson::default()
        },
    )
    .unwrap()
}

// ---------- создание и ФИО ----------

#[test]
fn full_name_collapses_whitespace() {
    assert_eq!(person("  Иван   Петров ").full_name, "Иван Петров");
}

#[test]
fn empty_full_name_is_rejected() {
    for bad in ["", "   "] {
        let err = Person::create(
            TenantId::new(),
            NewPerson {
                full_name: bad.into(),
                ..NewPerson::default()
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                DomainError::Invalid {
                    field: "full_name",
                    ..
                }
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn minimal_person_has_no_contacts_and_zero_experience() {
    let p = person("Иван");
    assert!(p.phone.is_none() && p.email.is_none());
    assert_eq!(p.experience_years(d("2026-01-01")), 0);
}

// ---------- контакты ----------

#[test]
fn contacts_are_normalized_on_creation() {
    let p = Person::create(
        TenantId::new(),
        NewPerson {
            full_name: "Иван".into(),
            contacts: ContactsInput {
                phone: Some("8 (916) 123-45-67".into()),
                email: Some(" Ivan@Mail.RU ".into()),
                telegram: Some("t.me/Ivanov".into()),
                whatsapp: Some("+7 916 123 45 67".into()),
            },
            ..NewPerson::default()
        },
    )
    .unwrap();
    assert_eq!(p.phone.unwrap().as_str(), "+79161234567");
    assert_eq!(p.email.unwrap().as_str(), "ivan@mail.ru");
    assert_eq!(p.telegram.unwrap().as_str(), "ivanov");
    assert_eq!(p.whatsapp.unwrap().as_str(), "+79161234567");
}

#[test]
fn telegram_accepts_links_and_handles() {
    for raw in [
        "t.me/Ivanov",
        "https://t.me/ivanov",
        "@Ivanov",
        "ivanov",
        " IVANOV ",
    ] {
        assert_eq!(Telegram::parse(raw).unwrap().as_str(), "ivanov", "{raw}");
    }
    for bad in ["", "abc", "has space", "bad-dash", &"x".repeat(33)] {
        assert!(Telegram::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn whatsapp_is_a_phone_number() {
    assert_eq!(
        WhatsApp::parse("8 916 123 45 67").unwrap().as_str(),
        "+79161234567"
    );
    assert!(WhatsApp::parse("nope").is_err());
}

#[test]
fn invalid_contacts_name_the_failing_field() {
    for (field, contacts) in [
        (
            "email",
            ContactsInput {
                email: Some("not-an-email".into()),
                ..Default::default()
            },
        ),
        (
            "phone",
            ContactsInput {
                phone: Some("12345".into()),
                ..Default::default()
            },
        ),
        (
            "telegram",
            ContactsInput {
                telegram: Some("a".into()),
                ..Default::default()
            },
        ),
        (
            "whatsapp",
            ContactsInput {
                whatsapp: Some("xx".into()),
                ..Default::default()
            },
        ),
    ] {
        let err = Person::create(
            TenantId::new(),
            NewPerson {
                full_name: "Иван".into(),
                contacts,
                ..NewPerson::default()
            },
        )
        .unwrap_err();
        assert!(
            matches!(&err, DomainError::Invalid { field: f, .. } if *f == field),
            "{field}: {err:?}"
        );
    }
}

#[test]
fn empty_contact_means_no_contact() {
    let mut p = person("Иван");
    p.change_contacts(ContactsInput {
        phone: Some("+7 916 123 45 67".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(p.phone.is_some());
    p.change_contacts(ContactsInput {
        phone: Some("   ".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(p.phone.is_none(), "blank clears the contact");
}

#[test]
fn failed_contact_change_leaves_the_person_untouched() {
    let mut p = person("Иван");
    p.change_contacts(ContactsInput {
        email: Some("a@b.io".into()),
        ..Default::default()
    })
    .unwrap();
    let before = p.clone();
    let err = p
        .change_contacts(ContactsInput {
            email: Some("c@d.io".into()),
            phone: Some("12".into()),
            ..Default::default()
        })
        .unwrap_err();
    assert!(matches!(err, DomainError::Invalid { field: "phone", .. }));
    assert_eq!(p, before);
}

// ---------- профиль ----------

#[test]
fn skills_are_trimmed_deduplicated_ignoring_case() {
    let skills = normalize_skills([
        "Rust",
        " rust ",
        "SQL",
        "",
        "  ",
        "sql",
        "Machine   Learning",
    ])
    .unwrap();
    assert_eq!(skills, ["Rust", "SQL", "Machine Learning"]);
}

#[test]
fn salary_and_declared_experience_must_be_sane() {
    let bad_salary = Person::create(
        TenantId::new(),
        NewPerson {
            full_name: "И".into(),
            expected_salary: Some(-1),
            ..NewPerson::default()
        },
    );
    assert!(matches!(
        bad_salary,
        Err(DomainError::Invalid {
            field: "expected_salary",
            ..
        })
    ));
    let bad_exp = Person::create(
        TenantId::new(),
        NewPerson {
            full_name: "И".into(),
            declared_experience_years: Some(120),
            ..NewPerson::default()
        },
    );
    assert!(matches!(
        bad_exp,
        Err(DomainError::Invalid {
            field: "declared_experience_years",
            ..
        })
    ));
}

// ---------- места работы, образование, стаж ----------

#[test]
fn employment_validates_its_period_and_text() {
    assert!(Employment::new("Acme", "Dev", d("2020-01-01"), Some(d("2019-12-31")), None).is_err());
    assert!(Employment::new("", "Dev", d("2020-01-01"), None, None).is_err());
    assert!(Employment::new("Acme", " ", d("2020-01-01"), None, None).is_err());
    assert!(Employment::new("Acme", "Dev", d("2020-01-01"), Some(d("2020-01-01")), None).is_ok());
}

#[test]
fn education_validates_years() {
    assert!(Education::new("MSU", None, Some(2015), Some(2010)).is_err());
    assert!(Education::new("", None, None, None).is_err());
    assert!(Education::new("MSU", Some("CS".into()), Some(2010), Some(2015)).is_ok());
}

#[test]
fn experience_counts_whole_years_over_all_periods() {
    // 2 года 7 месяцев + 1 год 8 месяцев = 4 года 3 месяца
    let jobs = [
        job("2020-01-01", Some("2022-08-01")),
        job("2023-01-01", Some("2024-09-01")),
    ];
    assert_eq!(experience_years(&jobs, d("2026-01-01")), 4);
}

#[test]
fn exactly_one_year_is_one_year() {
    assert_eq!(
        experience_years(&[job("2021-01-01", Some("2022-01-01"))], d("2026-01-01")),
        1
    );
    assert_eq!(
        experience_years(&[job("2021-01-01", Some("2021-12-31"))], d("2026-01-01")),
        0
    );
}

#[test]
fn overlapping_periods_are_counted_once() {
    let jobs = [
        job("2020-01-01", Some("2022-01-01")),
        job("2021-01-01", Some("2023-01-01")),
    ];
    assert_eq!(experience_years(&jobs, d("2026-01-01")), 3);
    // вложенный период ничего не добавляет
    let nested = [
        job("2020-01-01", Some("2024-01-01")),
        job("2021-01-01", Some("2022-01-01")),
    ];
    assert_eq!(experience_years(&nested, d("2026-01-01")), 4);
}

#[test]
fn the_current_job_runs_until_today() {
    assert_eq!(
        experience_years(&[job("2020-03-01", None)], d("2026-02-28")),
        5
    );
    assert_eq!(
        experience_years(&[job("2020-03-01", None)], d("2026-03-01")),
        6
    );
}

#[test]
fn declared_experience_applies_only_without_history() {
    let mut p = Person::create(
        TenantId::new(),
        NewPerson {
            full_name: "Иван".into(),
            declared_experience_years: Some(5),
            ..NewPerson::default()
        },
    )
    .unwrap();
    assert_eq!(p.experience_years(d("2026-01-01")), 5);

    p.replace_employments(vec![job("2024-01-01", Some("2025-01-01"))]);
    assert_eq!(
        p.experience_years(d("2026-01-01")),
        1,
        "history wins over the declared value"
    );

    p.replace_employments(vec![]);
    assert_eq!(
        p.experience_years(d("2026-01-01")),
        5,
        "back to the declared value"
    );
}

// ---------- ключи и поиск дубля ----------

fn keys(id: u128, name: &str, email: Option<&str>, phone: Option<&str>) -> PersonKeys {
    PersonKeys {
        id: uuid::Uuid::from_u128(id),
        full_name: name.into(),
        normalized_name: name.to_lowercase(),
        normalized_email: email.map(Into::into),
        normalized_phone: phone.map(Into::into),
    }
}

fn probe(name: &str, email: Option<&str>, phone: Option<&str>) -> Probe {
    Probe::new(name, email, phone).unwrap()
}

#[test]
fn person_keys_follow_the_normalized_values() {
    let p = Person::create(
        TenantId::new(),
        NewPerson {
            full_name: " Иван  Петров".into(),
            contacts: ContactsInput {
                phone: Some("8 (916) 123-45-67".into()),
                email: Some("IVAN@mail.ru".into()),
                ..Default::default()
            },
            ..NewPerson::default()
        },
    )
    .unwrap();
    let k = p.keys();
    assert_eq!(k.normalized_name, "иван петров");
    assert_eq!(k.normalized_email.as_deref(), Some("ivan@mail.ru"));
    assert_eq!(k.normalized_phone.as_deref(), Some("9161234567"));
}

#[test]
fn duplicate_by_email_is_the_strongest_signal() {
    let pool = [
        keys(1, "Другой Человек", Some("x@y.io"), Some("9161234567")),
        keys(2, "Иван Петров", Some("ivan@mail.ru"), None),
    ];
    let hit = find_duplicate(
        &pool,
        &probe("Кто-то", Some("IVAN@mail.ru"), Some("8 916 123 45 67")),
    )
    .unwrap();
    assert_eq!(
        (hit.id, hit.matched_by),
        (uuid::Uuid::from_u128(2), MatchedBy::Email)
    );
}

#[test]
fn phone_beats_name() {
    let pool = [
        keys(1, "Иван Петров", None, None),
        keys(2, "Пётр Иванов", None, Some("9161234567")),
    ];
    let hit = find_duplicate(&pool, &probe("Иван Петров", None, Some("+7 916 123-45-67"))).unwrap();
    assert_eq!(
        (hit.id, hit.matched_by),
        (uuid::Uuid::from_u128(2), MatchedBy::Phone)
    );
}

#[test]
fn name_only_match_is_the_weakest_but_still_found() {
    let pool = [keys(1, "Иван Петров", Some("a@b.io"), None)];
    let hit = find_duplicate(&pool, &probe("  иван   ПЕТРОВ ", None, None)).unwrap();
    assert_eq!(hit.matched_by, MatchedBy::Name);
}

#[test]
fn no_match_gives_none() {
    let pool = [keys(1, "Иван Петров", Some("a@b.io"), Some("9161234567"))];
    assert!(
        find_duplicate(
            &pool,
            &probe("Анна Сидорова", Some("z@z.io"), Some("+7 999 000 11 22"))
        )
        .is_none()
    );
    assert!(find_duplicate(&[], &probe("Иван", None, None)).is_none());
}

#[test]
fn invalid_probe_contacts_are_reported() {
    assert!(Probe::new("Иван", Some("bad"), None).is_err());
    assert!(Probe::new("Иван", None, Some("12")).is_err());
}
