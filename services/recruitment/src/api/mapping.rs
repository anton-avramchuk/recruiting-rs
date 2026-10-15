//! Преобразование строк хранилища в DTO.
use chrono::{NaiveDate, Utc};
use recruitment_contracts::{ApplicationDto, EventDto, Stage, VacancyDto};

use crate::infrastructure::{
    applications::{ApplicationRow, EventRow},
    vacancies::VacancyView,
};

pub fn vacancy_to_dto(view: &VacancyView, today: NaiveDate) -> VacancyDto {
    let v = &view.vacancy;
    VacancyDto {
        id: v.id,
        title: v.title.clone(),
        department: v.department.clone(),
        city: v.city.clone(),
        openings: v.openings,
        priority: v.priority.key().to_string(),
        hiring_manager: v.hiring_manager.clone(),
        recruiter: v.recruiter.clone(),
        required_skills: v.required_skills.clone(),
        opened_on: v.opened_on.format("%Y-%m-%d").to_string(),
        open_days: v.open_days(today),
        active_applications: view.active_applications,
        total_applications: view.total_applications,
    }
}

pub fn application_to_dto(row: &ApplicationRow) -> ApplicationDto {
    let stage = Stage::from_key(&row.stage).unwrap_or(Stage::New);
    ApplicationDto {
        id: row.id,
        person_id: row.person_id,
        vacancy_id: row.vacancy_id,
        vacancy_title: row.vacancy_title.clone(),
        full_name: row.full_name.clone(),
        source: row.source.clone(),
        city: row.city.clone(),
        expected_salary: row.expected_salary,
        stage,
        hours_on_stage: row.hours_on_stage,
        overdue: stage.overdue(row.hours_on_stage),
        match_percent: row.match_percent.map(|m| m.clamp(0, 100) as u8),
        person_applications: row.person_applications,
        stage_changed_at: row.stage_changed_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
    }
}

pub fn event_to_dto(row: &EventRow) -> EventDto {
    EventDto {
        kind: row.kind.clone(),
        text: row.text.clone(),
        at: row.at.with_timezone(&Utc).to_rfc3339(),
    }
}
