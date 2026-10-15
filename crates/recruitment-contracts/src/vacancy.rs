use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Приоритет вакансии: влияет на порядок в списке.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum Priority {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "low")]
    Low,
}

impl Priority {
    pub fn key(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Normal => "normal",
            Priority::Low => "low",
        }
    }

    pub fn from_key(key: &str) -> Option<Priority> {
        [Priority::High, Priority::Normal, Priority::Low]
            .into_iter()
            .find(|p| p.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "Высокий",
            Priority::Normal => "Обычный",
            Priority::Low => "Низкий",
        }
    }
}

/// Кадровые системы, в которые «передаётся» вышедший кандидат (передача имитируется).
pub const HR_SYSTEMS: [&str; 4] = ["1С:ЗУП", "PeopleForce", "Odoo HR", "Внешний API"];
