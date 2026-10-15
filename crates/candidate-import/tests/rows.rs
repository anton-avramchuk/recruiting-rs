use candidate_contracts::{EducationDto, EmploymentDto};
use candidate_import::{
    columns::{self, Field},
    delimited, pdf, resume, rows, xlsx,
};

const SOURCE: &str = "Импорт из файла";
const YEAR: i32 = 2026;

fn csv_grid() -> Vec<Vec<String>> {
    delimited::parse(&std::fs::read_to_string("tests/fixtures/demo-kandidaty.csv").unwrap())
        .unwrap()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---------- сопоставление колонок ----------

#[test]
fn headers_of_the_csv_sample_are_matched() {
    let grid = csv_grid();
    assert_eq!(
        columns::guess_all(&grid[0]),
        [
            Field::Name,
            Field::Phone,
            Field::Email,
            Field::City,
            Field::Role,
            Field::Salary,
            Field::Skills,
            Field::Jobs,
            Field::Education,
        ]
    );
}

#[test]
fn headers_of_the_xlsx_sample_are_matched() {
    let grid =
        xlsx::read_grid(&std::fs::read("tests/fixtures/demo-kandidaty.xlsx").unwrap()).unwrap();
    assert_eq!(
        columns::guess_all(&grid[0]),
        [
            Field::Name,
            Field::Phone,
            Field::Email,
            Field::City,
            Field::Role,
            Field::Salary,
            Field::Skills,
        ]
    );
}

#[test]
fn headers_are_matched_by_keywords_in_any_case() {
    for (header, field) in [
        ("ФИО", Field::Name),
        ("Кандидат", Field::Name),
        ("Full NAME", Field::Name),
        ("Мобильный", Field::Phone),
        ("E-mail", Field::Email),
        ("Регион", Field::City),
        ("Желаемая позиция", Field::Role),
        ("Оклад", Field::Salary),
        ("Стаж", Field::Experience),
        ("Ключевые навыки", Field::Skills),
        ("Места работы", Field::Jobs),
        ("Вуз", Field::Education),
        ("Откуда", Field::Source),
        ("Заметки", Field::About),
        ("Что-то своё", Field::Skip),
    ] {
        assert_eq!(columns::guess(header, 5), field, "{header}");
    }
}

#[test]
fn the_first_column_is_the_name_when_nothing_matches() {
    assert_eq!(columns::guess("Колонка 1", 0), Field::Name);
    assert_eq!(columns::guess("Колонка 2", 1), Field::Skip);
    let map = columns::guess_all(&strings(&["A", "B"]));
    assert_eq!(map, [Field::Name, Field::Skip]);
}

#[test]
fn a_name_column_is_required_for_import() {
    assert!(columns::has_name(&[Field::Skip, Field::Name]));
    assert!(!columns::has_name(&[Field::Skip, Field::Phone]));
    assert!(!columns::has_name(&[]));
}

#[test]
fn fields_have_keys_and_labels() {
    for f in Field::ALL {
        assert!(!f.label().is_empty());
        assert_eq!(Field::from_key(f.key()), Some(f));
    }
    assert_eq!(Field::from_key("nope"), None);
}

// ---------- строки ----------

#[test]
fn the_csv_sample_becomes_import_rows() {
    let grid = csv_grid();
    let map = columns::guess_all(&grid[0]);
    let built = rows::build_all(&grid, &map, SOURCE);
    assert_eq!(built.len(), 2);
    assert_eq!(
        built[0].0, 2,
        "numbers are spreadsheet rows: the header is row 1"
    );

    let r = &built[0].1;
    assert_eq!(r.full_name, "Пётр Гусев");
    assert_eq!(r.phone.as_deref(), Some("+7 903 111-22-33"));
    assert_eq!(r.email.as_deref(), Some("petr@mail.ru"));
    assert_eq!(r.city.as_deref(), Some("Москва"));
    assert_eq!(r.expected_salary, Some(280_000));
    assert_eq!(r.skills, ["Go", "K8s", "PostgreSQL"]);
    assert_eq!(r.source.as_deref(), Some(SOURCE));
    assert_eq!(r.summary.as_deref(), Some("Go-разработчик"));
    assert_eq!(
        r.employments,
        [EmploymentDto {
            company: "Яндекс".into(),
            position: "Разработчик".into(),
            start: "2021-01-01".into(),
            end: None,
            description: None,
        }]
    );
    assert_eq!(
        r.educations,
        [EducationDto {
            institution: "МФТИ".into(),
            specialty: Some("Прикладная математика".into()),
            year_from: None,
            year_to: Some(2019),
        }]
    );
    let second = &built[1].1;
    assert_eq!(second.employments[0].end.as_deref(), Some("2026-12-31"));
    assert_eq!(second.city.as_deref(), Some("Санкт-Петербург"));
}

#[test]
fn numbers_are_cleaned() {
    let map = [Field::Name, Field::Salary, Field::Experience];
    let row = |salary: &str, exp: &str| rows::build(&map, &strings(&["Иван", salary, exp]), SOURCE);
    assert_eq!(row("280 000 ₽", "5 лет").expected_salary, Some(280_000));
    assert_eq!(row("280 000 ₽", "5 лет").declared_experience_years, Some(5));
    assert_eq!(row("250000", "5,5").declared_experience_years, Some(5));
    assert_eq!(row("по договорённости", "").expected_salary, None);
    assert_eq!(row("", "много").declared_experience_years, None);
    assert_eq!(row("0", "0").declared_experience_years, Some(0));
}

#[test]
fn skills_split_on_commas_semicolons_and_newlines() {
    let map = [Field::Name, Field::Skills];
    let r = rows::build(
        &map,
        &strings(&["Иван", "Go, SQL;Docker\nRust,,  "]),
        SOURCE,
    );
    assert_eq!(r.skills, ["Go", "SQL", "Docker", "Rust"]);
}

#[test]
fn several_jobs_in_one_cell_are_split_by_lines() {
    let map = [Field::Name, Field::Jobs];
    let cell = "2019 — 2021 — Авито — Разработчик\n2021 — наст. время — Ozon — Тимлид";
    let r = rows::build(&map, &strings(&["Иван", cell]), SOURCE);
    assert_eq!(r.employments.len(), 2);
    assert_eq!(r.employments[1].company, "Ozon");
    assert_eq!(r.employments[1].end, None);
}

#[test]
fn jobs_without_a_period_are_kept_in_the_note() {
    let map = [Field::Name, Field::Jobs];
    let r = rows::build(
        &map,
        &strings(&["Иван", "работал в банке и в школе"]),
        SOURCE,
    );
    assert!(r.employments.is_empty());
    assert_eq!(
        r.note.as_deref(),
        Some("Опыт работы: работал в банке и в школе")
    );
}

#[test]
fn education_without_a_year_has_no_year() {
    let map = [Field::Name, Field::Education];
    let r = rows::build(&map, &strings(&["Иван", "МГУ — Экономика"]), SOURCE);
    assert_eq!(r.educations[0].institution, "МГУ");
    assert_eq!(r.educations[0].specialty.as_deref(), Some("Экономика"));
    assert_eq!(r.educations[0].year_to, None);
}

#[test]
fn role_and_comment_make_the_summary() {
    let map = [Field::Name, Field::Role, Field::About];
    let both = rows::build(
        &map,
        &strings(&["Иван", "Аналитик", "Хороший специалист"]),
        SOURCE,
    );
    assert_eq!(
        both.summary.as_deref(),
        Some("Аналитик. Хороший специалист")
    );
    let only_comment = rows::build(&map, &strings(&["Иван", "", "Хороший специалист"]), SOURCE);
    assert_eq!(only_comment.summary.as_deref(), Some("Хороший специалист"));
    let none = rows::build(&map, &strings(&["Иван", "", ""]), SOURCE);
    assert_eq!(none.summary, None);
}

#[test]
fn source_from_the_file_wins_over_the_default() {
    let map = [Field::Name, Field::Source];
    let own = rows::build(&map, &strings(&["Иван", "hh.ru"]), SOURCE);
    assert_eq!(own.source.as_deref(), Some("hh.ru"));
    let default = rows::build(&map, &strings(&["Иван", ""]), SOURCE);
    assert_eq!(default.source.as_deref(), Some(SOURCE));
}

#[test]
fn empty_rows_are_skipped_but_a_row_without_a_name_is_kept_for_rejection() {
    let grid = vec![
        strings(&["Имя", "Телефон"]),
        strings(&["", ""]),
        strings(&["", "+7 900 000 00 00"]),
        strings(&["Анна", ""]),
    ];
    let map = columns::guess_all(&grid[0]);
    let built = rows::build_all(&grid, &map, SOURCE);
    assert_eq!(built.len(), 2);
    assert_eq!(built[0].0, 3);
    assert_eq!(built[0].1.full_name, "");
    assert_eq!(built[1].0, 4);
}

#[test]
fn the_resume_becomes_an_import_row() {
    let text =
        pdf::extract_text(&std::fs::read("tests/fixtures/demo-rezyume.pdf").unwrap()).unwrap();
    let parsed = resume::parse(&text, YEAR);
    let r = rows::from_resume(&parsed, SOURCE);
    assert_eq!(r.full_name, "Ирина Соколова");
    assert_eq!(r.phone.as_deref(), Some("+7 916 442-11-07"));
    assert_eq!(r.email.as_deref(), Some("irina@mail.ru"));
    assert_eq!(r.city.as_deref(), Some("Казань"));
    assert_eq!(r.expected_salary, Some(210_000));
    assert_eq!(r.declared_experience_years, Some(7));
    assert_eq!(r.skills, ["SQL", "Python", "dbt", "PostgreSQL"]);
    assert_eq!(r.employments.len(), 3);
    assert_eq!(r.employments[0].start, "2022-01-01");
    assert_eq!(r.employments[0].end, None);
    assert_eq!(r.employments[1].end.as_deref(), Some("2022-12-31"));
    assert_eq!(r.educations[0].year_to, Some(2017));
    assert!(r.summary.as_deref().unwrap().starts_with("Строила витрины"));
    assert_eq!(r.source.as_deref(), Some(SOURCE));
}

// ---------- текстовый вид для формы резюме ----------

#[test]
fn jobs_and_education_round_trip_through_their_text_form() {
    let text =
        pdf::extract_text(&std::fs::read("tests/fixtures/demo-rezyume.pdf").unwrap()).unwrap();
    let parsed = resume::parse(&text, YEAR);
    let jobs = rows::jobs_text(&parsed.jobs);
    assert_eq!(
        jobs,
        "2022 — наст. время — Ozon — Ведущий аналитик данных\n2019 — 2022 — X5 Tech — Аналитик\n2017 — 2019 — Магнит — Junior-аналитик"
    );
    let edu = rows::education_text(&parsed.education);
    assert_eq!(
        edu,
        "2017 — Казанский федеральный университет — Прикладная математика и информатика"
    );
    // текст из формы даёт те же записи, что и разобранное резюме
    let map = [Field::Name, Field::Jobs, Field::Education];
    let from_form = rows::build(&map, &strings(&["Ирина", &jobs, &edu]), SOURCE);
    let direct = rows::from_resume(&parsed, SOURCE);
    assert_eq!(from_form.employments, direct.employments);
    assert_eq!(from_form.educations, direct.educations);
}

#[test]
fn empty_lists_have_an_empty_text_form() {
    assert_eq!(rows::jobs_text(&[]), "");
    assert_eq!(rows::education_text(&[]), "");
}
