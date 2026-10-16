//! Метрики аналитики: чистые функции над фактами об откликах.
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use recruitment::domain::analytics::{
    AppFact, ClientInfo, NO_CLIENT, UNKNOWN_CLIENT, VacancyRef, clients, days_to_hire,
    first_touch_hours, funnel, median, sources, week_series, week_start,
};
use recruitment_contracts::Stage;
use std::collections::HashMap;
use uuid::Uuid;

fn t(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, hour, 0, 0).unwrap()
}

fn fact(stage: Stage, reached: usize) -> AppFact {
    AppFact {
        created_at: t(1, 10),
        stage,
        stage_changed_at: t(1, 10),
        source: None,
        client_id: None,
        reached,
        first_touch: None,
    }
}

fn counts(facts: &[AppFact]) -> Vec<i64> {
    funnel(facts).0.iter().map(|s| s.reached).collect()
}

fn conversions(facts: &[AppFact]) -> Vec<i64> {
    funnel(facts).0.iter().map(|s| s.conversion).collect()
}

// ---------- воронка ----------

#[test]
fn the_funnel_has_seven_steps_in_order() {
    let (steps, worst) = funnel(&[]);
    let stages: Vec<Stage> = steps.iter().map(|s| s.stage).collect();
    assert_eq!(
        stages,
        [
            Stage::New,
            Stage::Screening,
            Stage::Hr,
            Stage::Tech,
            Stage::Final,
            Stage::Offer,
            Stage::Hired
        ]
    );
    assert!(steps.iter().all(|s| s.reached == 0 && s.conversion == 0));
    assert_eq!(worst, None, "no applications, no worst stage");
}

#[test]
fn an_application_counts_on_every_stage_it_reached() {
    // дошёл до «Тех. интервью» (4), затем отказ
    let facts = [fact(Stage::Rejected, 4)];
    assert_eq!(counts(&facts), [1, 1, 1, 1, 0, 0, 0]);
}

#[test]
fn the_current_stage_counts_even_without_events() {
    let facts = [fact(Stage::Final, 0)];
    assert_eq!(counts(&facts), [1, 1, 1, 1, 1, 0, 0]);
}

#[test]
fn a_new_application_reaches_only_the_first_stage() {
    assert_eq!(counts(&[fact(Stage::New, 0)]), [1, 0, 0, 0, 0, 0, 0]);
    // отказ без других событий: всё равно был «Новым»
    assert_eq!(counts(&[fact(Stage::Rejected, 0)]), [1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn jumping_over_stages_counts_the_skipped_ones() {
    let facts = [fact(Stage::Offer, 6)];
    assert_eq!(counts(&facts), [1, 1, 1, 1, 1, 1, 0]);
}

#[test]
fn conversion_is_relative_to_the_previous_stage_and_the_worst_drop_is_named() {
    let mut facts = Vec::new();
    for _ in 0..4 {
        facts.push(fact(Stage::Rejected, 1)); // остановились на «Новый»
    }
    for _ in 0..3 {
        facts.push(fact(Stage::Rejected, 2)); // остановились на «Скрининг»
    }
    for _ in 0..3 {
        facts.push(fact(Stage::Hired, 7));
    }
    // новый 10, скрининг 6, интервью HR 3, дальше без потерь
    assert_eq!(counts(&facts), [10, 6, 3, 3, 3, 3, 3]);
    assert_eq!(conversions(&facts), [100, 60, 50, 100, 100, 100, 100]);
    assert_eq!(funnel(&facts).1, Some(Stage::Hr));
}

#[test]
fn no_loss_means_no_worst_stage_and_ties_name_the_earlier_stage() {
    let facts = [fact(Stage::Hired, 7), fact(Stage::Hired, 7)];
    assert_eq!(funnel(&facts).1, None);

    // 4 -> 3 (75%) -> 2 (67%) -> 1 (50%) -> 1: минимум на «Тех. интервью»
    let steps = [
        fact(Stage::Rejected, 1),
        fact(Stage::Rejected, 2),
        fact(Stage::Rejected, 3),
        fact(Stage::Hired, 7),
    ];
    assert_eq!(funnel(&steps).1, Some(Stage::Tech));

    // 2 -> 0: потеря на втором этапе (0%), дальше 0 из 0 потерей не считается
    let equal = [fact(Stage::Rejected, 1), fact(Stage::Rejected, 1)];
    assert_eq!(funnel(&equal).1, Some(Stage::Screening));
}

#[test]
fn conversion_is_rounded_to_whole_percent() {
    let mut facts = vec![fact(Stage::Rejected, 1)];
    facts.extend([fact(Stage::Hired, 7), fact(Stage::Hired, 7)]);
    // 2 из 3 = 66,7% -> 67
    assert_eq!(conversions(&facts)[1], 67);
}

// ---------- медианы ----------

#[test]
fn the_median_of_odd_even_and_empty_lists() {
    assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), Some(2.5));
    assert_eq!(median(vec![5.0]), Some(5.0));
    assert_eq!(median(vec![]), None);
}

#[test]
fn time_to_hire_is_measured_for_hired_applications_only() {
    let hired = |days: i64| AppFact {
        stage_changed_at: t(1, 10) + Duration::days(days),
        ..fact(Stage::Hired, 7)
    };
    let rejected = AppFact {
        stage_changed_at: t(1, 10) + Duration::days(999),
        ..fact(Stage::Rejected, 3)
    };
    assert_eq!(
        days_to_hire(&[hired(10), hired(40), hired(20), rejected]),
        Some(20.0)
    );
    assert_eq!(days_to_hire(&[fact(Stage::New, 0)]), None);
    assert_eq!(days_to_hire(&[]), None);
}

#[test]
fn first_touch_ignores_applications_nobody_touched() {
    let touched = |hours: i64| AppFact {
        first_touch: Some(t(1, 10) + Duration::hours(hours)),
        ..fact(Stage::Screening, 2)
    };
    let untouched = fact(Stage::New, 0);
    assert_eq!(
        first_touch_hours(&[touched(2), touched(4), touched(30), untouched]),
        Some(4.0)
    );
    assert_eq!(first_touch_hours(&[fact(Stage::New, 0)]), None);
}

// ---------- источники ----------

fn with_source(source: Option<&str>, stage: Stage, reached: usize) -> AppFact {
    AppFact {
        source: source.map(str::to_string),
        ..fact(stage, reached)
    }
}

#[test]
fn sources_are_sorted_by_conversion_then_volume_then_name() {
    let facts = [
        with_source(Some("hh.ru"), Stage::Hired, 7),
        with_source(Some("hh.ru"), Stage::Hired, 7),
        with_source(Some("hh.ru"), Stage::Rejected, 1),
        with_source(Some("hh.ru"), Stage::New, 0),
        with_source(Some("Реферал"), Stage::Hr, 3),
        with_source(Some("Реферал"), Stage::Offer, 6),
        with_source(None, Stage::New, 0),
        with_source(Some("  "), Stage::Rejected, 2),
        with_source(Some("Хабр"), Stage::Hr, 3),
    ];
    let s = sources(&facts);
    let names: Vec<&str> = s.iter().map(|r| r.source.as_str()).collect();
    assert_eq!(names, ["Реферал", "Хабр", "hh.ru", "Не указан"]);
    assert_eq!((s[0].total, s[0].interviews, s[0].conversion), (2, 2, 100));
    assert_eq!((s[2].total, s[2].interviews, s[2].conversion), (4, 2, 50));
    assert_eq!(
        (s[3].total, s[3].interviews, s[3].conversion),
        (2, 0, 0),
        "blank and missing sources are merged"
    );
}

#[test]
fn interview_reached_means_hr_or_further() {
    let s = sources(&[
        with_source(Some("a"), Stage::Rejected, 2),
        with_source(Some("a"), Stage::Rejected, 3),
    ]);
    assert_eq!(s[0].interviews, 1);
}

// ---------- недели ----------

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

#[test]
fn weeks_start_on_monday() {
    assert_eq!(week_start(d(3, 4)), d(3, 2), "Wednesday -> Monday");
    assert_eq!(week_start(d(3, 2)), d(3, 2));
    assert_eq!(
        week_start(d(3, 8)),
        d(3, 2),
        "Sunday belongs to the week begun on Monday"
    );
}

#[test]
fn the_series_has_twelve_weeks_ending_with_the_current_one() {
    let counts = [
        (d(3, 2), 5),
        (d(2, 16), 3),
        (NaiveDate::from_ymd_opt(2025, 10, 1).unwrap(), 99),
    ];
    let series = week_series(&counts, d(3, 4), 12);
    assert_eq!(series.len(), 12);
    assert_eq!(series.last().unwrap(), &(d(3, 2), 5));
    assert_eq!(series[0].0, d(3, 2) - Duration::weeks(11));
    assert!(
        series
            .windows(2)
            .all(|w| w[1].0 - w[0].0 == Duration::days(7))
    );
    let total: i64 = series.iter().map(|(_, n)| *n).sum();
    assert_eq!(total, 8, "weeks outside the window are dropped");
    assert_eq!(series.iter().find(|(w, _)| *w == d(2, 16)).unwrap().1, 3);
    assert_eq!(series.iter().filter(|(_, n)| *n == 0).count(), 10);
}

// ---------- заказчики ----------

fn of_client(client: Option<Uuid>, stage: Stage, reached: usize) -> AppFact {
    AppFact {
        client_id: client,
        ..fact(stage, reached)
    }
}

fn hired_after(client: Option<Uuid>, days: i64) -> AppFact {
    AppFact {
        client_id: client,
        stage: Stage::Hired,
        reached: 7,
        created_at: t(1, 10),
        stage_changed_at: t(1, 10) + Duration::days(days),
        ..fact(Stage::Hired, 7)
    }
}

fn vacancy(client: Option<Uuid>, active: i64) -> VacancyRef {
    VacancyRef {
        client_id: client,
        active_applications: active,
    }
}

fn names(pairs: &[(Uuid, &str, bool)]) -> HashMap<Uuid, ClientInfo> {
    pairs
        .iter()
        .map(|(id, name, archived)| {
            (
                *id,
                ClientInfo {
                    name: (*name).into(),
                    archived: *archived,
                },
            )
        })
        .collect()
}

#[test]
fn clients_are_compared_by_applications_interviews_hires_and_time_to_hire() {
    let (acme, globex) = (Uuid::new_v4(), Uuid::new_v4());
    let facts = [
        of_client(Some(acme), Stage::New, 1),
        of_client(Some(acme), Stage::Rejected, 3),
        of_client(Some(acme), Stage::Rejected, 4),
        hired_after(Some(acme), 20),
        of_client(Some(globex), Stage::New, 1),
        of_client(Some(globex), Stage::Screening, 2),
    ];
    let vacancies = [
        vacancy(Some(acme), 2),
        vacancy(Some(acme), 1),
        vacancy(Some(globex), 2),
    ];
    let out = clients(
        &facts,
        &vacancies,
        &names(&[(acme, "Acme", false), (globex, "Globex", false)]),
    );

    assert_eq!(out.len(), 2);
    let a = &out[0];
    assert_eq!((a.name.as_str(), a.client_id), ("Acme", Some(acme)));
    assert_eq!((a.vacancies, a.active_applications), (2, 3));
    assert_eq!((a.applications, a.interviews, a.conversion), (4, 3, 75));
    assert_eq!((a.hired, a.median_days_to_hire), (1, Some(20.0)));
    let g = &out[1];
    assert_eq!((g.applications, g.interviews, g.conversion), (2, 0, 0));
    assert_eq!((g.hired, g.median_days_to_hire), (0, None));
}

#[test]
fn old_vacancies_form_a_row_without_a_client_that_goes_last_on_ties() {
    let acme = Uuid::new_v4();
    let facts = [
        of_client(Some(acme), Stage::New, 1),
        of_client(None, Stage::New, 1),
    ];
    let out = clients(
        &facts,
        &[vacancy(None, 1), vacancy(Some(acme), 1)],
        &names(&[(acme, "Zeta", false)]),
    );
    assert_eq!(out[0].name, "Zeta");
    assert_eq!((out[1].name.as_str(), out[1].client_id), (NO_CLIENT, None));
}

#[test]
fn equal_applications_are_ordered_by_name() {
    let (b, a) = (Uuid::new_v4(), Uuid::new_v4());
    let out = clients(
        &[],
        &[vacancy(Some(b), 0), vacancy(Some(a), 0)],
        &names(&[(b, "Бета", false), (a, "Альфа", false)]),
    );
    let order: Vec<_> = out.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(order, ["Альфа", "Бета"]);
}

#[test]
fn a_client_with_vacancies_but_no_applications_shows_zeros_and_no_median() {
    let acme = Uuid::new_v4();
    let out = clients(
        &[],
        &[vacancy(Some(acme), 0)],
        &names(&[(acme, "Acme", true)]),
    );
    assert_eq!(out.len(), 1);
    let c = &out[0];
    assert_eq!(
        (
            c.vacancies,
            c.applications,
            c.interviews,
            c.conversion,
            c.hired
        ),
        (1, 0, 0, 0, 0)
    );
    assert_eq!(c.median_days_to_hire, None);
    assert!(c.archived);
}

#[test]
fn a_client_without_vacancies_is_not_listed_and_an_unknown_one_gets_a_label() {
    let (listed, ghost) = (Uuid::new_v4(), Uuid::new_v4());
    let out = clients(
        &[],
        &[vacancy(Some(ghost), 0)],
        &names(&[(listed, "Без вакансий", false)]),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, UNKNOWN_CLIENT);
}

#[test]
fn no_vacancies_means_no_rows() {
    assert!(clients(&[], &[], &HashMap::new()).is_empty());
}
