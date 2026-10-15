use chrono::{Duration, NaiveDate};
use platform_core::tenant::TenantId;
use platform_values::{SkillsError, normalize_skills};
use recruitment_contracts::Priority;
use uuid::Uuid;

use super::DomainError;

fn collapse(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn optional(raw: Option<String>) -> Option<String> {
    raw.map(|s| collapse(&s)).filter(|s| !s.is_empty())
}

fn title(raw: &str) -> Result<String, DomainError> {
    let t = collapse(raw);
    if t.is_empty() {
        Err(DomainError::invalid("title", "не может быть пустым"))
    } else {
        Ok(t)
    }
}

fn openings(value: i32) -> Result<i32, DomainError> {
    if value < 1 {
        Err(DomainError::invalid("openings", "не меньше 1"))
    } else {
        Ok(value)
    }
}

fn priority(raw: &str) -> Result<Priority, DomainError> {
    Priority::from_key(raw.trim())
        .ok_or_else(|| DomainError::invalid("priority", "high, normal или low"))
}

fn skills(items: Vec<String>) -> Result<Vec<String>, DomainError> {
    normalize_skills(items).map_err(|e| match e {
        SkillsError::TooLong => {
            DomainError::invalid("required_skills", "навык длиннее 50 символов")
        }
        SkillsError::TooMany => DomainError::invalid("required_skills", "не больше 50 навыков"),
    })
}

fn opened_on(open_days: i32, today: NaiveDate) -> Result<NaiveDate, DomainError> {
    if open_days < 0 {
        return Err(DomainError::invalid(
            "open_days",
            "не может быть отрицательным",
        ));
    }
    Ok(today - Duration::days(i64::from(open_days)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewVacancy {
    pub title: String,
    pub department: Option<String>,
    pub city: Option<String>,
    pub openings: Option<i32>,
    pub priority: Option<String>,
    pub hiring_manager: Option<String>,
    pub recruiter: Option<String>,
    pub required_skills: Vec<String>,
    /// Сколько дней вакансия уже открыта.
    pub open_days: Option<i32>,
}

/// Частичное обновление. Для очищаемых полей: `None` - не трогать, `Some(None)` или
/// пустая строка - очистить, `Some(Some(v))` - заменить.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VacancyPatch {
    pub title: Option<String>,
    pub department: Option<Option<String>>,
    pub city: Option<Option<String>>,
    pub openings: Option<i32>,
    pub priority: Option<String>,
    pub hiring_manager: Option<Option<String>>,
    pub recruiter: Option<Option<String>>,
    pub required_skills: Option<Vec<String>>,
    pub open_days: Option<i32>,
}

/// Агрегат «вакансия».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vacancy {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub title: String,
    pub department: Option<String>,
    pub city: Option<String>,
    pub openings: i32,
    pub priority: Priority,
    pub hiring_manager: Option<String>,
    pub recruiter: Option<String>,
    pub required_skills: Vec<String>,
    pub opened_on: NaiveDate,
}

impl Vacancy {
    pub fn create(
        tenant_id: TenantId,
        new: NewVacancy,
        today: NaiveDate,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            title: title(&new.title)?,
            department: optional(new.department),
            city: optional(new.city),
            openings: openings(new.openings.unwrap_or(1))?,
            priority: match new.priority {
                Some(p) => priority(&p)?,
                None => Priority::Normal,
            },
            hiring_manager: optional(new.hiring_manager),
            recruiter: optional(new.recruiter),
            required_skills: skills(new.required_skills)?,
            opened_on: opened_on(new.open_days.unwrap_or(0), today)?,
        })
    }

    /// Восстановление из хранилища.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        tenant_id: TenantId,
        title: String,
        department: Option<String>,
        city: Option<String>,
        openings: i32,
        priority: &str,
        hiring_manager: Option<String>,
        recruiter: Option<String>,
        required_skills: Vec<String>,
        opened_on: NaiveDate,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            tenant_id,
            title,
            department,
            city,
            openings,
            priority: self::priority(priority)?,
            hiring_manager,
            recruiter,
            required_skills,
            opened_on,
        })
    }

    /// Сколько дней прошло с открытия.
    pub fn open_days(&self, today: NaiveDate) -> i64 {
        (today - self.opened_on).num_days().max(0)
    }

    /// Применяет изменения целиком или не применяет вовсе.
    pub fn apply(&mut self, patch: VacancyPatch, today: NaiveDate) -> Result<(), DomainError> {
        let mut next = self.clone();
        if let Some(t) = patch.title {
            next.title = title(&t)?;
        }
        if let Some(v) = patch.department {
            next.department = optional(v);
        }
        if let Some(v) = patch.city {
            next.city = optional(v);
        }
        if let Some(n) = patch.openings {
            next.openings = openings(n)?;
        }
        if let Some(p) = patch.priority {
            next.priority = priority(&p)?;
        }
        if let Some(v) = patch.hiring_manager {
            next.hiring_manager = optional(v);
        }
        if let Some(v) = patch.recruiter {
            next.recruiter = optional(v);
        }
        if let Some(s) = patch.required_skills {
            next.required_skills = skills(s)?;
        }
        if let Some(d) = patch.open_days {
            next.opened_on = opened_on(d, today)?;
        }
        *self = next;
        Ok(())
    }
}
