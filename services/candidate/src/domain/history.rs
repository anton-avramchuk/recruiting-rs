use chrono::{Datelike, NaiveDate};

use super::DomainError;

fn text(field: &'static str, raw: &str) -> Result<String, DomainError> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        Err(DomainError::invalid(field, "не может быть пустым"))
    } else {
        Ok(collapsed)
    }
}

fn optional_text(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Место работы. Без даты окончания - текущее.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Employment {
    pub company: String,
    pub position: String,
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    pub description: Option<String>,
}

impl Employment {
    pub fn new(
        company: &str,
        position: &str,
        start: NaiveDate,
        end: Option<NaiveDate>,
        description: Option<String>,
    ) -> Result<Self, DomainError> {
        if end.is_some_and(|e| e < start) {
            return Err(DomainError::invalid(
                "employments",
                "дата окончания раньше даты начала",
            ));
        }
        Ok(Self {
            company: text("employments", company)?,
            position: text("employments", position)?,
            start,
            end,
            description: optional_text(description),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Education {
    pub institution: String,
    pub specialty: Option<String>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
}

impl Education {
    pub fn new(
        institution: &str,
        specialty: Option<String>,
        year_from: Option<i32>,
        year_to: Option<i32>,
    ) -> Result<Self, DomainError> {
        if let (Some(from), Some(to)) = (year_from, year_to)
            && to < from
        {
            return Err(DomainError::invalid(
                "educations",
                "год окончания раньше года начала",
            ));
        }
        Ok(Self {
            institution: text("educations", institution)?,
            specialty: optional_text(specialty),
            year_from,
            year_to,
        })
    }
}

/// Полных месяцев между датами (`from <= to`).
fn months_between(from: NaiveDate, to: NaiveDate) -> i64 {
    let months = (i64::from(to.year()) - i64::from(from.year())) * 12
        + (i64::from(to.month()) - i64::from(from.month()));
    (months - i64::from(to.day() < from.day())).max(0)
}

/// Стаж в полных годах по объединению периодов: пересечения учитываются один раз,
/// текущее место работы длится до `today`.
pub fn experience_years(employments: &[Employment], today: NaiveDate) -> i32 {
    let mut periods: Vec<(NaiveDate, NaiveDate)> = employments
        .iter()
        .map(|e| (e.start, e.end.unwrap_or(today).max(e.start)))
        .collect();
    periods.sort();

    let mut merged: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    for (start, end) in periods {
        match merged.last_mut() {
            Some((_, last_end)) if start <= *last_end => *last_end = (*last_end).max(end),
            _ => merged.push((start, end)),
        }
    }
    let months: i64 = merged.iter().map(|(s, e)| months_between(*s, *e)).sum();
    i32::try_from(months / 12).unwrap_or(i32::MAX)
}
