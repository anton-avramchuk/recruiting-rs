//! Преобразование агрегата в DTO и обратно.
use candidate_contracts::{EducationDto, EmploymentDto, PersonDto, PersonSummaryDto};
use chrono::NaiveDate;

use crate::{
    domain::{DomainError, Education, Employment, Person},
    infrastructure::persons::PersonSummary,
};

const DATE_FORMAT: &str = "%Y-%m-%d";

pub fn person_to_dto(person: &Person, today: NaiveDate) -> PersonDto {
    PersonDto {
        id: person.id,
        full_name: person.full_name.clone(),
        source: person.source.clone(),
        phone: person.phone.as_ref().map(|p| p.as_str().to_string()),
        email: person.email.as_ref().map(|e| e.as_str().to_string()),
        telegram: person.telegram.as_ref().map(|t| t.as_str().to_string()),
        whatsapp: person.whatsapp.as_ref().map(|w| w.as_str().to_string()),
        expected_salary: person.expected_salary,
        summary: person.summary.clone(),
        skills: person.skills.clone(),
        experience_years: person.experience_years(today),
        employments: person
            .employments()
            .iter()
            .map(|e| EmploymentDto {
                company: e.company.clone(),
                position: e.position.clone(),
                start: e.start.format(DATE_FORMAT).to_string(),
                end: e.end.map(|d| d.format(DATE_FORMAT).to_string()),
                description: e.description.clone(),
            })
            .collect(),
        educations: person
            .educations()
            .iter()
            .map(|e| EducationDto {
                institution: e.institution.clone(),
                specialty: e.specialty.clone(),
                year_from: e.year_from,
                year_to: e.year_to,
            })
            .collect(),
        created_at: person.created_at.to_rfc3339(),
    }
}

pub fn summary_to_dto(s: PersonSummary) -> PersonSummaryDto {
    PersonSummaryDto {
        id: s.id,
        full_name: s.full_name,
        source: s.source,
        phone: s.phone,
        email: s.email,
        expected_salary: s.expected_salary,
        skills: s.skills,
        experience_years: s.experience_years,
        created_at: s.created_at.to_rfc3339(),
    }
}

fn parse_date(field: &'static str, raw: &str) -> Result<NaiveDate, DomainError> {
    NaiveDate::parse_from_str(raw.trim(), DATE_FORMAT)
        .map_err(|_| DomainError::invalid(field, "дата в формате ГГГГ-ММ-ДД"))
}

pub fn employments_from_dto(items: Vec<EmploymentDto>) -> Result<Vec<Employment>, DomainError> {
    items
        .into_iter()
        .map(|e| {
            let start = parse_date("start", &e.start)?;
            let end = e
                .end
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| parse_date("end", s))
                .transpose()?;
            Employment::new(&e.company, &e.position, start, end, e.description)
        })
        .collect()
}

pub fn educations_from_dto(items: Vec<EducationDto>) -> Result<Vec<Education>, DomainError> {
    items
        .into_iter()
        .map(|e| Education::new(&e.institution, e.specialty, e.year_from, e.year_to))
        .collect()
}
