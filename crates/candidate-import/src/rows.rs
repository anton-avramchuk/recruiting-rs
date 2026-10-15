//! Строки таблицы и разобранное резюме -> строки импорта для API.
use candidate_contracts::{EducationDto, EmploymentDto, ImportRow};

use crate::{
    columns::Field,
    resume::{self, Education, Job, ParsedResume},
};

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn optional(s: &str) -> Option<String> {
    Some(clean(s)).filter(|s| !s.is_empty())
}

/// Число из текста: «280 000 ₽» -> 280000; без цифр — `None`.
fn amount(s: &str) -> Option<i64> {
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 12 {
        None
    } else {
        digits.parse().ok()
    }
}

/// Первое целое число в тексте («5 лет», «5,5» -> 5).
fn first_number(s: &str) -> Option<i32> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok().filter(|n| (0..=80).contains(n))
}

fn skills(s: &str) -> Vec<String> {
    s.split([',', ';', '\n'])
        .map(clean)
        .filter(|s| !s.is_empty())
        .collect()
}

fn employment(job: &Job) -> Option<EmploymentDto> {
    // и компания, и должность обязательны: если названо одно, оно заменяет второе
    let company = if job.company.is_empty() {
        &job.role
    } else {
        &job.company
    };
    let position = if job.role.is_empty() {
        &job.company
    } else {
        &job.role
    };
    if company.is_empty() || position.is_empty() {
        return None;
    }
    Some(EmploymentDto {
        company: company.clone(),
        position: position.clone(),
        start: format!("{:04}-01-01", job.start_year),
        end: job.end_year.map(|y| format!("{y:04}-12-31")),
        description: Some(job.note.clone()).filter(|n| !n.is_empty()),
    })
}

fn education(e: &Education) -> Option<EducationDto> {
    if e.institution.is_empty() {
        return None;
    }
    Some(EducationDto {
        institution: e.institution.clone(),
        specialty: Some(e.specialty.clone()).filter(|s| !s.is_empty()),
        year_from: None,
        year_to: e.year,
    })
}

fn lines(cell: &str) -> impl Iterator<Item = &str> {
    cell.split(['\n', ';'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
}

/// Одна строка таблицы по сопоставлению колонок.
pub fn build(map: &[Field], cells: &[String], default_source: &str) -> ImportRow {
    let mut row = ImportRow::default();
    let mut role = String::new();
    let mut about = String::new();
    let mut leftover_jobs = Vec::new();

    for (field, cell) in map.iter().zip(cells) {
        let cell = cell.trim();
        match field {
            Field::Skip => {}
            Field::Name => row.full_name = clean(cell),
            Field::Phone => row.phone = optional(cell),
            Field::Email => row.email = optional(cell),
            Field::City => row.city = optional(cell),
            Field::Source => row.source = optional(cell),
            Field::Role => role = clean(cell),
            Field::About => about = clean(cell),
            Field::Salary => row.expected_salary = amount(cell),
            Field::Experience => row.declared_experience_years = first_number(cell),
            Field::Skills => row.skills.extend(skills(cell)),
            Field::Jobs => {
                for line in lines(cell) {
                    match resume::job_from_text(line).as_ref().and_then(employment) {
                        Some(job) => row.employments.push(job),
                        None => leftover_jobs.push(line.to_string()),
                    }
                }
            }
            Field::Education => {
                for line in lines(cell) {
                    if let Some(e) = education(&resume::education_from_text(line)) {
                        row.educations.push(e);
                    }
                }
            }
        }
    }

    row.summary = match (role.is_empty(), about.is_empty()) {
        (false, false) => Some(format!("{role}. {about}")),
        (false, true) => Some(role),
        (true, false) => Some(about),
        (true, true) => None,
    };
    if !leftover_jobs.is_empty() {
        row.note = Some(format!("Опыт работы: {}", leftover_jobs.join("; ")));
    }
    if row.source.is_none() {
        row.source = optional(default_source);
    }
    row
}

/// Строки данных таблицы (без заголовка) с номерами строк в файле (заголовок — строка 1).
/// Полностью пустые строки пропускаются; строка без имени сохраняется, чтобы сервер отклонил её
/// с понятной причиной.
pub fn build_all(
    grid: &[Vec<String>],
    map: &[Field],
    default_source: &str,
) -> Vec<(usize, ImportRow)> {
    grid.iter()
        .enumerate()
        .skip(1)
        .filter(|(_, cells)| cells.iter().any(|c| !c.trim().is_empty()))
        .map(|(i, cells)| (i + 1, build(map, cells, default_source)))
        .collect()
}

/// Строка импорта из разобранного резюме.
pub fn from_resume(r: &ParsedResume, default_source: &str) -> ImportRow {
    ImportRow {
        full_name: clean(&r.name),
        phone: optional(&r.phone),
        email: optional(&r.email),
        city: optional(&r.city),
        source: optional(default_source),
        expected_salary: r.salary.and_then(|s| i64::try_from(s).ok()),
        declared_experience_years: r.experience_years.filter(|n| (0..=80).contains(n)),
        skills: r.skills.clone(),
        summary: optional(&r.about),
        note: None,
        employments: r.jobs.iter().filter_map(employment).collect(),
        educations: r.education.iter().filter_map(education).collect(),
    }
}

/// Места работы построчно: «2022 — наст. время — Ozon — Аналитик» (так их правят в форме резюме).
pub fn jobs_text(jobs: &[Job]) -> String {
    jobs.iter()
        .map(|j| {
            let end = j
                .end_year
                .map_or("наст. время".to_string(), |y| y.to_string());
            let mut parts = vec![format!("{} — {end}", j.start_year)];
            parts.extend(
                [&j.company, &j.role, &j.note]
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .cloned(),
            );
            parts.join(" — ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Образование построчно: «2017 — Университет — Специальность».
pub fn education_text(items: &[Education]) -> String {
    items
        .iter()
        .map(|e| {
            let mut parts = Vec::new();
            if let Some(y) = e.year {
                parts.push(y.to_string());
            }
            parts.extend(
                [&e.institution, &e.specialty]
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .cloned(),
            );
            parts.join(" — ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
