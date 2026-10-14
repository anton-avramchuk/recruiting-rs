use chrono::NaiveDate;
use platform_core::tenant::TenantId;
use platform_values::{Email, Phone};
use uuid::Uuid;

use super::{DomainError, Education, Employment, PersonKeys, Telegram, WhatsApp, history};

const MAX_SKILLS: usize = 50;
const MAX_SKILL_LEN: usize = 50;
const MAX_DECLARED_EXPERIENCE: i32 = 80;

fn collapse(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Навыки без пробелов по краям и повторов (регистр не различается, остаётся первое написание).
pub fn normalize_skills<I, S>(skills: I) -> Result<Vec<String>, DomainError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for raw in skills {
        let skill = collapse(raw.as_ref());
        if skill.is_empty() {
            continue;
        }
        if skill.chars().count() > MAX_SKILL_LEN {
            return Err(DomainError::invalid("skills", "навык длиннее 50 символов"));
        }
        if !out.iter().any(|s| s.to_lowercase() == skill.to_lowercase()) {
            out.push(skill);
        }
    }
    if out.len() > MAX_SKILLS {
        return Err(DomainError::invalid("skills", "не больше 50 навыков"));
    }
    Ok(out)
}

fn optional(raw: Option<String>) -> Option<String> {
    raw.map(|s| collapse(&s)).filter(|s| !s.is_empty())
}

/// Контакты как ввёл пользователь. Пусто или `None` - контакта нет.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactsInput {
    pub phone: Option<String>,
    pub email: Option<String>,
    pub telegram: Option<String>,
    pub whatsapp: Option<String>,
}

struct Contacts {
    phone: Option<Phone>,
    email: Option<Email>,
    telegram: Option<Telegram>,
    whatsapp: Option<WhatsApp>,
}

fn filled(raw: &Option<String>) -> Option<&str> {
    raw.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl ContactsInput {
    fn parse(&self) -> Result<Contacts, DomainError> {
        Ok(Contacts {
            phone: filled(&self.phone)
                .map(|v| {
                    Phone::parse(v)
                        .map_err(|_| DomainError::invalid("phone", "некорректный телефон"))
                })
                .transpose()?,
            email: filled(&self.email)
                .map(|v| {
                    Email::parse(v).map_err(|_| DomainError::invalid("email", "некорректный email"))
                })
                .transpose()?,
            telegram: filled(&self.telegram).map(Telegram::parse).transpose()?,
            whatsapp: filled(&self.whatsapp).map(WhatsApp::parse).transpose()?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewPerson {
    pub full_name: String,
    pub contacts: ContactsInput,
    pub source: Option<String>,
    pub expected_salary: Option<i64>,
    pub summary: Option<String>,
    pub skills: Vec<String>,
    pub declared_experience_years: Option<i32>,
}

/// Агрегат «кандидат». Существует сам по себе, независимо от вакансий.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub full_name: String,
    pub source: Option<String>,
    pub phone: Option<Phone>,
    pub email: Option<Email>,
    pub telegram: Option<Telegram>,
    pub whatsapp: Option<WhatsApp>,
    pub expected_salary: Option<i64>,
    pub summary: Option<String>,
    pub skills: Vec<String>,
    pub declared_experience_years: i32,
    employments: Vec<Employment>,
    educations: Vec<Education>,
}

fn full_name(raw: &str) -> Result<String, DomainError> {
    let name = collapse(raw);
    if name.is_empty() {
        Err(DomainError::invalid("full_name", "не может быть пустым"))
    } else {
        Ok(name)
    }
}

fn salary(value: Option<i64>) -> Result<Option<i64>, DomainError> {
    match value {
        Some(v) if v < 0 => Err(DomainError::invalid(
            "expected_salary",
            "не может быть отрицательной",
        )),
        other => Ok(other),
    }
}

impl Person {
    pub fn create(tenant_id: TenantId, new: NewPerson) -> Result<Self, DomainError> {
        let full_name = full_name(&new.full_name)?;
        let contacts = new.contacts.parse()?;
        let declared = new.declared_experience_years.unwrap_or(0);
        if !(0..=MAX_DECLARED_EXPERIENCE).contains(&declared) {
            return Err(DomainError::invalid(
                "declared_experience_years",
                "от 0 до 80 лет",
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            full_name,
            source: optional(new.source),
            phone: contacts.phone,
            email: contacts.email,
            telegram: contacts.telegram,
            whatsapp: contacts.whatsapp,
            expected_salary: salary(new.expected_salary)?,
            summary: new
                .summary
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            skills: normalize_skills(&new.skills)?,
            declared_experience_years: declared,
            employments: Vec::new(),
            educations: Vec::new(),
        })
    }

    /// Заменяет все четыре контакта целиком (`None`/пусто - контакта нет). При ошибке
    /// карточка не меняется.
    pub fn change_contacts(&mut self, input: ContactsInput) -> Result<(), DomainError> {
        let contacts = input.parse()?;
        self.phone = contacts.phone;
        self.email = contacts.email;
        self.telegram = contacts.telegram;
        self.whatsapp = contacts.whatsapp;
        Ok(())
    }

    pub fn employments(&self) -> &[Employment] {
        &self.employments
    }

    pub fn educations(&self) -> &[Education] {
        &self.educations
    }

    pub fn replace_employments(&mut self, employments: Vec<Employment>) {
        self.employments = employments;
    }

    pub fn replace_educations(&mut self, educations: Vec<Education>) {
        self.educations = educations;
    }

    /// По местам работы; пока их нет - заявленный при заведении.
    pub fn experience_years(&self, today: NaiveDate) -> i32 {
        if self.employments.is_empty() {
            self.declared_experience_years
        } else {
            history::experience_years(&self.employments, today)
        }
    }

    /// Ключи сопоставления: пересчитываются из текущих значений при каждой записи.
    pub fn keys(&self) -> PersonKeys {
        PersonKeys {
            id: self.id,
            full_name: self.full_name.clone(),
            normalized_name: self.full_name.to_lowercase(),
            normalized_email: self.email.as_ref().map(|e| e.as_str().to_string()),
            normalized_phone: self
                .phone
                .as_ref()
                .map(|p| p.significant_digits().to_string()),
        }
    }
}
