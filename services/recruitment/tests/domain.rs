use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use platform_core::tenant::TenantId;
use recruitment::domain::{Application, DomainError, EventKind, NewVacancy, Vacancy, VacancyPatch};
use recruitment_contracts::{Priority, Stage};
use uuid::Uuid;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 10, 12, 0, 0).unwrap()
}

fn vacancy(new: NewVacancy) -> Vacancy {
    Vacancy::create(TenantId::new(), new, today()).unwrap()
}

fn titled(title: &str) -> NewVacancy {
    NewVacancy {
        title: title.into(),
        client_id: Some(Uuid::new_v4()),
        ..NewVacancy::default()
    }
}

fn field(err: DomainError) -> &'static str {
    match err {
        DomainError::Invalid { field, .. } => field,
        other => panic!("not a validation error: {other}"),
    }
}

// ---------- вакансия ----------

#[test]
fn a_vacancy_needs_a_title_and_a_client() {
    let v = vacancy(titled("  Senior   Go-разработчик "));
    assert_eq!(v.title, "Senior Go-разработчик");
    assert_eq!(v.openings, 1);
    assert_eq!(v.priority, Priority::Normal);
    assert_eq!(v.open_days(today()), 0);
    assert!(v.required_skills.is_empty());
}

#[test]
fn a_new_vacancy_without_a_client_is_rejected() {
    let err = Vacancy::create(
        TenantId::new(),
        NewVacancy {
            client_id: None,
            ..titled("A")
        },
        today(),
    )
    .unwrap_err();
    assert_eq!(field(err), "client_id");
}

#[test]
fn a_vacancy_keeps_its_client_and_contact() {
    let (client, contact) = (Uuid::new_v4(), Uuid::new_v4());
    let v = vacancy(NewVacancy {
        client_id: Some(client),
        contact_id: Some(contact),
        ..titled("A")
    });
    assert_eq!((v.client_id, v.contact_id), (Some(client), Some(contact)));
}

#[test]
fn switching_the_client_resets_the_contact_unless_a_new_one_is_given() {
    let (first, second, contact) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mut v = vacancy(NewVacancy {
        client_id: Some(first),
        contact_id: Some(contact),
        ..titled("A")
    });
    v.apply(
        VacancyPatch {
            client_id: Some(Some(second)),
            ..VacancyPatch::default()
        },
        today(),
    )
    .unwrap();
    assert_eq!((v.client_id, v.contact_id), (Some(second), None));

    let new_contact = Uuid::new_v4();
    v.apply(
        VacancyPatch {
            client_id: Some(Some(first)),
            contact_id: Some(Some(new_contact)),
            ..VacancyPatch::default()
        },
        today(),
    )
    .unwrap();
    assert_eq!(
        (v.client_id, v.contact_id),
        (Some(first), Some(new_contact))
    );
}

#[test]
fn the_same_client_keeps_the_contact() {
    let (client, contact) = (Uuid::new_v4(), Uuid::new_v4());
    let mut v = vacancy(NewVacancy {
        client_id: Some(client),
        contact_id: Some(contact),
        ..titled("A")
    });
    v.apply(
        VacancyPatch {
            client_id: Some(Some(client)),
            ..VacancyPatch::default()
        },
        today(),
    )
    .unwrap();
    assert_eq!(v.contact_id, Some(contact));
}

#[test]
fn the_client_cannot_be_removed() {
    let mut v = vacancy(titled("A"));
    let before = v.clone();
    let err = v
        .apply(
            VacancyPatch {
                client_id: Some(None),
                ..VacancyPatch::default()
            },
            today(),
        )
        .unwrap_err();
    assert_eq!(field(err), "client_id");
    assert_eq!(v, before);
}

#[test]
fn a_contact_needs_a_client() {
    let mut legacy = Vacancy::restore(
        Uuid::new_v4(),
        TenantId::new(),
        "Старая".into(),
        None,
        None,
        None,
        1,
        "normal",
        None,
        vec![],
        today(),
    )
    .unwrap();
    let err = legacy
        .apply(
            VacancyPatch {
                contact_id: Some(Some(Uuid::new_v4())),
                ..VacancyPatch::default()
            },
            today(),
        )
        .unwrap_err();
    assert_eq!(field(err), "contact_id");
    // а назначить заказчика старой вакансии можно
    legacy
        .apply(
            VacancyPatch {
                client_id: Some(Some(Uuid::new_v4())),
                ..VacancyPatch::default()
            },
            today(),
        )
        .unwrap();
    assert!(legacy.client_id.is_some());
}

#[test]
fn empty_title_is_rejected() {
    let err = Vacancy::create(TenantId::new(), titled("   "), today()).unwrap_err();
    assert_eq!(field(err), "title");
}

#[test]
fn openings_below_one_are_rejected() {
    let err = Vacancy::create(
        TenantId::new(),
        NewVacancy {
            openings: Some(0),
            ..titled("A")
        },
        today(),
    )
    .unwrap_err();
    assert_eq!(field(err), "openings");
}

#[test]
fn unknown_priority_is_rejected() {
    let err = Vacancy::create(
        TenantId::new(),
        NewVacancy {
            priority: Some("urgent".into()),
            ..titled("A")
        },
        today(),
    )
    .unwrap_err();
    assert_eq!(field(err), "priority");
}

#[test]
fn opened_days_ago_sets_the_opening_date_backwards() {
    let v = vacancy(NewVacancy {
        open_days: Some(52),
        ..titled("A")
    });
    assert_eq!(v.open_days(today()), 52);
    assert_eq!(v.open_days(today() + Duration::days(1)), 53);
}

#[test]
fn negative_open_days_are_rejected() {
    let err = Vacancy::create(
        TenantId::new(),
        NewVacancy {
            open_days: Some(-1),
            ..titled("A")
        },
        today(),
    )
    .unwrap_err();
    assert_eq!(field(err), "open_days");
}

#[test]
fn required_skills_are_normalized_like_candidate_skills() {
    let v = vacancy(NewVacancy {
        required_skills: vec![" Go ".into(), "go".into(), "".into(), "K8s".into()],
        ..titled("A")
    });
    assert_eq!(v.required_skills, ["Go", "K8s"]);
}

#[test]
fn patch_changes_only_given_fields_and_clears_optional_ones() {
    let mut v = vacancy(NewVacancy {
        city: Some("Казань".into()),
        recruiter: Some("Мария".into()),
        ..titled("A")
    });
    v.apply(
        VacancyPatch {
            openings: Some(3),
            priority: Some("high".into()),
            recruiter: Some(None),
            ..VacancyPatch::default()
        },
        today(),
    )
    .unwrap();
    assert_eq!(v.openings, 3);
    assert_eq!(v.priority, Priority::High);
    assert_eq!(v.recruiter, None);
    assert_eq!(v.city.as_deref(), Some("Казань"));
    assert_eq!(v.title, "A");
}

#[test]
fn failed_patch_changes_nothing() {
    let mut v = vacancy(titled("A"));
    let before = v.clone();
    let err = v
        .apply(
            VacancyPatch {
                title: Some("Новое".into()),
                openings: Some(0),
                ..VacancyPatch::default()
            },
            today(),
        )
        .unwrap_err();
    assert_eq!(field(err), "openings");
    assert_eq!(v, before);
}

// ---------- отклик ----------

fn app() -> Application {
    Application::create(TenantId::new(), Uuid::new_v4(), Uuid::new_v4(), now())
}

fn kinds(app: &mut Application) -> Vec<(EventKind, String)> {
    app.take_events()
        .into_iter()
        .map(|e| (e.kind, e.text))
        .collect()
}

#[test]
fn a_new_application_starts_at_new_with_a_created_event() {
    let mut a = app();
    assert_eq!(a.stage, Stage::New);
    assert_eq!(a.stage_changed_at, now());
    assert_eq!(
        kinds(&mut a),
        [(EventKind::Created, "Получен отклик".into())]
    );
    assert!(a.take_events().is_empty(), "events are taken once");
}

#[test]
fn moving_resets_the_timer_and_records_the_stage() {
    let mut a = app();
    a.take_events();
    let later = now() + Duration::hours(3);
    assert!(a.move_to(Stage::Screening, later));
    assert_eq!(a.stage, Stage::Screening);
    assert_eq!(a.stage_changed_at, later);
    assert_eq!(
        kinds(&mut a),
        [(EventKind::Stage, "Этап → Скрининг".into())]
    );
}

#[test]
fn moving_to_the_same_stage_changes_nothing() {
    let mut a = app();
    a.take_events();
    assert!(!a.move_to(Stage::New, now() + Duration::hours(3)));
    assert_eq!(a.stage_changed_at, now());
    assert!(a.take_events().is_empty());
}

#[test]
fn rejected_can_return_to_an_active_stage() {
    let mut a = app();
    a.reject(None, now());
    a.move_to(Stage::Hr, now());
    assert_eq!(a.stage, Stage::Hr);
}

#[test]
fn advance_goes_one_stage_forward() {
    let mut a = app();
    a.move_to(Stage::Tech, now());
    a.advance(now()).unwrap();
    assert_eq!(a.stage, Stage::Final);
    a.move_to(Stage::Offer, now());
    a.advance(now()).unwrap();
    assert_eq!(a.stage, Stage::Hired);
}

#[test]
fn advance_is_a_conflict_at_the_end_of_the_funnel() {
    for end in [Stage::Hired, Stage::Rejected] {
        let mut a = app();
        a.move_to(end, now());
        a.take_events();
        assert!(matches!(a.advance(now()), Err(DomainError::Conflict(_))));
        assert_eq!(a.stage, end);
        assert!(a.take_events().is_empty());
    }
}

#[test]
fn rejection_records_reason_and_a_letter() {
    let mut a = app();
    a.take_events();
    a.reject(Some(" Закрыли вакансию другим кандидатом ".into()), now());
    assert_eq!(a.stage, Stage::Rejected);
    assert_eq!(
        kinds(&mut a),
        [
            (
                EventKind::Rejected,
                "Отказ: Закрыли вакансию другим кандидатом".into()
            ),
            (EventKind::Letter, "Письмо с отказом отправлено".into()),
        ]
    );
}

#[test]
fn rejection_without_a_reason_is_plain() {
    let mut a = app();
    a.take_events();
    a.reject(Some("  ".into()), now());
    assert_eq!(kinds(&mut a)[0], (EventKind::Rejected, "Отказ".into()));
}

#[test]
fn interview_in_the_past_is_invalid() {
    let mut a = app();
    let err = a
        .schedule_interview(now() - Duration::minutes(1), 0, now())
        .unwrap_err();
    assert_eq!(field(err), "at");
}

#[test]
fn interview_is_recorded_with_an_invitation() {
    let mut a = app();
    a.take_events();
    let at = Utc.with_ymd_and_hms(2026, 1, 11, 15, 0, 0).unwrap();
    a.schedule_interview(at, 0, now()).unwrap();
    assert_eq!(
        kinds(&mut a),
        [
            (
                EventKind::Interview,
                "Интервью назначено на 11.01.2026 15:00".into()
            ),
            (
                EventKind::Letter,
                "Приглашение на интервью отправлено".into()
            ),
        ]
    );
}

#[test]
fn interview_time_is_shown_in_the_offset_given_by_the_user() {
    let mut a = app();
    a.take_events();
    let at = Utc.with_ymd_and_hms(2026, 1, 11, 12, 0, 0).unwrap();
    // +3 часа от UTC (в секундах)
    a.schedule_interview(at, 3 * 3600, now()).unwrap();
    assert_eq!(kinds(&mut a)[0].1, "Интервью назначено на 11.01.2026 15:00");
}

#[test]
fn handoff_needs_the_hired_stage_and_a_known_system() {
    let mut a = app();
    assert!(matches!(
        a.hand_off("1С:ЗУП", now()),
        Err(DomainError::Conflict(_))
    ));
    a.move_to(Stage::Hired, now());
    assert_eq!(field(a.hand_off("Excel", now()).unwrap_err()), "system");
    a.take_events();
    a.hand_off("1С:ЗУП", now()).unwrap();
    assert_eq!(
        kinds(&mut a),
        [(EventKind::Handoff, "Карточка передана в 1С:ЗУП".into())]
    );
}

#[test]
fn a_letter_is_just_a_history_entry() {
    let mut a = app();
    a.take_events();
    a.send_letter(now());
    assert_eq!(a.stage, Stage::New);
    assert_eq!(
        kinds(&mut a),
        [(EventKind::Letter, "Письмо кандидату отправлено".into())]
    );
}

// ---------- этап в событиях ----------

fn stages_of(app: &mut Application) -> Vec<(EventKind, Option<Stage>)> {
    app.take_events()
        .into_iter()
        .map(|e| (e.kind, e.stage))
        .collect()
}

#[test]
fn stage_events_carry_the_stage() {
    let mut a = app();
    assert_eq!(stages_of(&mut a), [(EventKind::Created, None)]);
    a.move_to(Stage::Tech, now());
    assert_eq!(stages_of(&mut a), [(EventKind::Stage, Some(Stage::Tech))]);
}

#[test]
fn a_rejection_event_carries_the_rejected_stage_and_its_letter_does_not() {
    let mut a = app();
    a.take_events();
    a.reject(Some("мимо".into()), now());
    assert_eq!(
        stages_of(&mut a),
        [
            (EventKind::Rejected, Some(Stage::Rejected)),
            (EventKind::Letter, None)
        ]
    );
}

#[test]
fn other_events_have_no_stage() {
    let mut a = app();
    a.take_events();
    a.send_letter(now());
    a.schedule_interview(now() + Duration::days(1), 0, now())
        .unwrap();
    assert!(stages_of(&mut a).iter().all(|(_, stage)| stage.is_none()));
}
