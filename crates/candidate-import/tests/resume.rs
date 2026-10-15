use candidate_import::{
    pdf,
    resume::{self, Job},
};

const YEAR: i32 = 2026;

fn sample_text() -> String {
    pdf::extract_text(&std::fs::read("tests/fixtures/demo-rezyume.pdf").unwrap()).unwrap()
}

fn job(start: i32, end: Option<i32>, company: &str, role: &str) -> Job {
    Job {
        start_year: start,
        end_year: end,
        company: company.into(),
        role: role.into(),
        note: String::new(),
    }
}

#[test]
fn the_sample_resume_is_parsed_into_fields() {
    let r = resume::parse(&sample_text(), YEAR);
    assert_eq!(r.name, "Ирина Соколова");
    assert_eq!(r.phone, "+7 916 442-11-07");
    assert_eq!(r.email, "irina@mail.ru");
    assert_eq!(r.city, "Казань");
    assert_eq!(r.salary, Some(210_000));
    assert_eq!(r.experience_years, Some(7));
    assert_eq!(r.skills, ["SQL", "Python", "dbt", "PostgreSQL"]);
    assert_eq!(
        r.jobs,
        [
            job(2022, None, "Ozon", "Ведущий аналитик данных"),
            job(2019, Some(2022), "X5 Tech", "Аналитик"),
            job(2017, Some(2019), "Магнит", "Junior-аналитик"),
        ]
    );
    assert_eq!(r.education.len(), 1);
    assert_eq!(r.education[0].year, Some(2017));
    assert_eq!(
        r.education[0].institution,
        "Казанский федеральный университет"
    );
    assert_eq!(
        r.education[0].specialty,
        "Прикладная математика и информатика"
    );
    assert_eq!(
        r.about,
        "Строила витрины и систему отчетности для розничной сети, вела команду"
    );
}

#[test]
fn nothing_recognizable_leaves_every_field_empty() {
    let r = resume::parse("lorem ipsum\n\nsome text without anything", YEAR);
    assert_eq!(r.name, "");
    assert_eq!(r.phone, "");
    assert_eq!(r.email, "");
    assert_eq!(r.city, "");
    assert_eq!(r.salary, None);
    assert_eq!(r.experience_years, None);
    assert!(r.skills.is_empty() && r.jobs.is_empty() && r.education.is_empty());
    assert!(resume::parse("", YEAR).name.is_empty());
}

#[test]
fn the_name_skips_headings_and_lines_with_digits() {
    let text = "Резюме\nCurriculum Vitae\nТелефон +7 900 111 22 33\nПётр Иванов\nИнженер";
    assert_eq!(resume::parse(text, YEAR).name, "Пётр Иванов");
    assert_eq!(
        resume::parse("John Smith Junior\nx", YEAR).name,
        "John Smith"
    );
    assert_eq!(
        resume::parse("Иван\nПетров Иван Сергеевич", YEAR).name,
        "Петров Иван"
    );
    // имя после четырнадцатой строки не ищется
    let late = format!("{}Анна Орлова", "строка\n".repeat(14));
    assert_eq!(resume::parse(&late, YEAR).name, "");
}

#[test]
fn phone_formats_are_recognized() {
    for (text, expected) in [
        ("тел. 8 (903) 555-66-77", "8 (903) 555-66-77"),
        ("+7(903)5556677", "+7(903)5556677"),
        ("звоните: +7 903 555 66 77.", "+7 903 555 66 77"),
    ] {
        assert_eq!(resume::parse(text, YEAR).phone, expected, "{text}");
    }
    assert_eq!(resume::parse("номер 12345", YEAR).phone, "");
}

#[test]
fn email_is_found_inside_text() {
    let r = resume::parse("писать на (Ivan.Petrov+hr@Mail-box.ru), срочно", YEAR);
    assert_eq!(r.email, "Ivan.Petrov+hr@Mail-box.ru");
    assert_eq!(resume::parse("user@localhost @ nothing", YEAR).email, "");
}

#[test]
fn salary_sits_next_to_the_ruble_in_any_form() {
    for (text, expected) in [
        ("Ожидания: 210000 руб", 210_000),
        ("зарплата 250 000 ₽", 250_000),
        ("хочу 180\u{a0}000 р.", 180_000),
        ("300000 RUB", 300_000),
    ] {
        assert_eq!(resume::parse(text, YEAR).salary, Some(expected), "{text}");
    }
    assert_eq!(resume::parse("опыт 7 лет, 15 руб", YEAR).salary, None);
}

#[test]
fn experience_is_taken_from_the_text_or_computed_from_jobs() {
    assert_eq!(
        resume::parse("Опыт работы 7 лет", YEAR).experience_years,
        Some(7)
    );
    assert_eq!(
        resume::parse("Опыт: более 12 лет в ИТ", YEAR).experience_years,
        Some(12)
    );
    assert_eq!(resume::parse("5 лет опыта", YEAR).experience_years, Some(5));
    assert_eq!(resume::parse("1 год опыт", YEAR).experience_years, Some(1));
    // не указан: считаем от самого раннего места работы
    let r = resume::parse(
        "2018 - 2020, Acme, Dev\n2020 - наст. время, Beta, Lead",
        YEAR,
    );
    assert_eq!(r.experience_years, Some(8));
}

#[test]
fn current_job_variants_have_no_end_year() {
    for end in [
        "наст. время",
        "настоящее время",
        "по настоящее время",
        "present",
        "now",
        "сейчас",
    ] {
        let r = resume::parse(&format!("2021 — {end}, Acme, Dev"), YEAR);
        assert_eq!(r.jobs, [job(2021, None, "Acme", "Dev")], "{end}");
    }
}

#[test]
fn a_period_alone_on_the_line_takes_the_next_line_as_the_company() {
    let r = resume::parse(
        "2019 – 2021\nЯндекс, Разработчик, Go и Kafka\n2021 — 2023\nАвито — Тимлид",
        YEAR,
    );
    assert_eq!(r.jobs.len(), 2);
    assert_eq!(r.jobs[0].company, "Яндекс");
    assert_eq!(r.jobs[0].role, "Разработчик");
    assert_eq!(r.jobs[0].note, "Go и Kafka");
    assert_eq!(r.jobs[1], job(2021, Some(2023), "Авито", "Тимлид"));
}

#[test]
fn education_lines_are_not_mistaken_for_jobs() {
    let r = resume::parse(
        "2012 — 2017, МФТИ университет, Прикладная математика\n2018 - 2020, Acme, Dev",
        YEAR,
    );
    assert_eq!(r.jobs, [job(2018, Some(2020), "Acme", "Dev")]);
    assert_eq!(r.education.len(), 1);
    assert_eq!(r.education[0].year, Some(2017));
    assert_eq!(r.education[0].institution, "МФТИ университет");
}

#[test]
fn skills_come_in_text_order_without_false_hits() {
    let r = resume::parse(
        "Опыт с Docker, потом python и SQL; Google, Gopher и Django не считаются; Go, 1С",
        YEAR,
    );
    assert_eq!(r.skills, ["Docker", "Python", "SQL", "Go", "1С"]);
}

#[test]
fn cities_are_recognized_longest_name_first() {
    assert_eq!(
        resume::parse("живу в санкт-петербурге? нет: Санкт-Петербург", YEAR).city,
        "Санкт-Петербург"
    );
    assert_eq!(resume::parse("г. Самара", YEAR).city, "Самара");
}

#[test]
fn limits_keep_the_result_small() {
    let many_jobs: String = (2000..2016)
        .map(|y| format!("{y} - {}, Co{y}, Dev\n", y + 1))
        .collect();
    assert_eq!(resume::parse(&many_jobs, YEAR).jobs.len(), 6);
    let many_edu = "Университет А\nУниверситет Б\nУниверситет В\nУниверситет Г\n";
    assert_eq!(resume::parse(many_edu, YEAR).education.len(), 3);
}
