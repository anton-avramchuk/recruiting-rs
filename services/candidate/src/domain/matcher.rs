use platform_values::{Email, Phone};
use uuid::Uuid;

use super::DomainError;

/// Ключи сопоставления человека - то, что хранится рядом с карточкой.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonKeys {
    pub id: Uuid,
    pub full_name: String,
    pub normalized_name: String,
    pub normalized_email: Option<String>,
    /// Последние десять цифр телефона.
    pub normalized_phone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedBy {
    Email,
    Phone,
    Name,
}

impl MatchedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchedBy::Email => "email",
            MatchedBy::Phone => "phone",
            MatchedBy::Name => "name",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub id: Uuid,
    pub full_name: String,
    pub matched_by: MatchedBy,
}

/// Данные нового человека в тех же нормализованных формах, что и ключи в хранилище.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub normalized_name: String,
    pub normalized_email: Option<String>,
    pub normalized_phone: Option<String>,
}

impl Probe {
    pub fn new(name: &str, email: Option<&str>, phone: Option<&str>) -> Result<Self, DomainError> {
        let normalized_email = match email.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Some(
                Email::parse(raw)
                    .map_err(|_| DomainError::invalid("email", "некорректный email"))?
                    .as_str()
                    .to_string(),
            ),
            None => None,
        };
        let normalized_phone = match phone.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Some(
                Phone::parse(raw)
                    .map_err(|_| DomainError::invalid("phone", "некорректный телефон"))?
                    .significant_digits()
                    .to_string(),
            ),
            None => None,
        };
        Ok(Self {
            normalized_name: name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase(),
            normalized_email,
            normalized_phone,
        })
    }
}

/// Правило «это тот же человек?»: email → телефон → ФИО. Порядок - приоритет: совпадение почты
/// сильнее телефона, телефона - сильнее имени. Совпавшее лишь имя слабее всего, но не
/// отбрасывается: в выгрузках контактов часто нет. Функция чистая: выборку претендентов из
/// хранилища делает вызывающий по тем же трём ключам.
pub fn find_duplicate(pool: &[PersonKeys], probe: &Probe) -> Option<Duplicate> {
    let hit = |p: &PersonKeys, by| Duplicate {
        id: p.id,
        full_name: p.full_name.clone(),
        matched_by: by,
    };
    if let Some(email) = &probe.normalized_email
        && let Some(p) = pool
            .iter()
            .find(|p| p.normalized_email.as_ref() == Some(email))
    {
        return Some(hit(p, MatchedBy::Email));
    }
    if let Some(phone) = &probe.normalized_phone
        && let Some(p) = pool
            .iter()
            .find(|p| p.normalized_phone.as_ref() == Some(phone))
    {
        return Some(hit(p, MatchedBy::Phone));
    }
    if !probe.normalized_name.is_empty()
        && let Some(p) = pool
            .iter()
            .find(|p| p.normalized_name == probe.normalized_name)
    {
        return Some(hit(p, MatchedBy::Name));
    }
    None
}
