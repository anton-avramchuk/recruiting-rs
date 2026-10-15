//! Этапы воронки и правила, общие для сервера и интерфейса.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Просрочка нового отклика, часов.
const NEW_SLA_HOURS: i64 = 24;
/// Просрочка отклика на остальных активных этапах, часов (5 суток).
const ACTIVE_SLA_HOURS: i64 = 5 * 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum Stage {
    #[serde(rename = "new")]
    New,
    #[serde(rename = "screening")]
    Screening,
    #[serde(rename = "hr")]
    Hr,
    #[serde(rename = "tech")]
    Tech,
    #[serde(rename = "final")]
    Final,
    #[serde(rename = "offer")]
    Offer,
    #[serde(rename = "hired")]
    Hired,
    #[serde(rename = "rejected")]
    Rejected,
}

impl Stage {
    /// Все этапы в порядке воронки.
    pub const ALL: [Stage; 8] = [
        Stage::New,
        Stage::Screening,
        Stage::Hr,
        Stage::Tech,
        Stage::Final,
        Stage::Offer,
        Stage::Hired,
        Stage::Rejected,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Stage::New => "new",
            Stage::Screening => "screening",
            Stage::Hr => "hr",
            Stage::Tech => "tech",
            Stage::Final => "final",
            Stage::Offer => "offer",
            Stage::Hired => "hired",
            Stage::Rejected => "rejected",
        }
    }

    pub fn from_key(key: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| s.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Stage::New => "Новый",
            Stage::Screening => "Скрининг",
            Stage::Hr => "Интервью HR",
            Stage::Tech => "Тех. интервью",
            Stage::Final => "Финал",
            Stage::Offer => "Оффер",
            Stage::Hired => "Выход",
            Stage::Rejected => "Отказ",
        }
    }

    /// Номер этапа в воронке, с единицы.
    pub fn number(self) -> u8 {
        Stage::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8 + 1
    }

    /// Следующий этап по порядку; у «Выход» и «Отказ» его нет.
    pub fn next(self) -> Option<Stage> {
        match self {
            Stage::Hired | Stage::Rejected => None,
            other => Stage::ALL.get(other.number() as usize).copied(),
        }
    }

    /// «Новый» … «Оффер».
    pub fn is_active(self) -> bool {
        !matches!(self, Stage::Hired | Stage::Rejected)
    }

    /// Просрочен ли отклик, пробывший на этапе столько часов.
    pub fn overdue(self, hours_on_stage: i64) -> bool {
        match self {
            Stage::New => hours_on_stage > NEW_SLA_HOURS,
            Stage::Hired | Stage::Rejected => false,
            _ => hours_on_stage > ACTIVE_SLA_HOURS,
        }
    }
}

/// Доля требуемых навыков, которые есть у человека, в процентах (регистр не важен).
/// Без требований совпадение не определено.
pub fn match_percent(required: &[String], skills: &[String]) -> Option<u8> {
    let mut wanted: Vec<String> = Vec::new();
    for r in required {
        let r = r.trim().to_lowercase();
        if !r.is_empty() && !wanted.contains(&r) {
            wanted.push(r);
        }
    }
    if wanted.is_empty() {
        return None;
    }
    let have: Vec<String> = skills.iter().map(|s| s.trim().to_lowercase()).collect();
    let hit = wanted.iter().filter(|w| have.contains(w)).count();
    let total = wanted.len();
    Some(((100 * hit + total / 2) / total) as u8)
}
