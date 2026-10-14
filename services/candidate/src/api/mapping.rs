//! Преобразование агрегата в DTO и обратно.
use candidate_contracts::{EducationDto, EmploymentDto, PersonDto};
use chrono::NaiveDate;

use crate::domain::Person;

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
