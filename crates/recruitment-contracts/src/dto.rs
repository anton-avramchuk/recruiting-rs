use platform_values::double_option;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::Stage;

// ---------- вакансии ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VacancyDto {
    pub id: Uuid,
    pub title: String,
    pub department: Option<String>,
    pub city: Option<String>,
    /// План найма.
    pub openings: i32,
    /// `high`, `normal` или `low`.
    pub priority: String,
    pub hiring_manager: Option<String>,
    pub recruiter: Option<String>,
    pub required_skills: Vec<String>,
    /// `YYYY-MM-DD`.
    pub opened_on: String,
    /// Дней с открытия.
    pub open_days: i64,
    /// Отклики не на этапах «Выход» и «Отказ».
    pub active_applications: i64,
    pub total_applications: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateVacancyRequest {
    pub title: String,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub openings: Option<i32>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub hiring_manager: Option<String>,
    #[serde(default)]
    pub recruiter: Option<String>,
    #[serde(default)]
    pub required_skills: Vec<String>,
    /// Сколько дней вакансия уже открыта.
    #[serde(default)]
    pub open_days: Option<i32>,
}

/// Частичное обновление: нет в теле - не трогать, `null` или пустая строка - очистить.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateVacancyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub department: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub city: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openings: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub hiring_manager: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub recruiter: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_skills: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_days: Option<i32>,
}

// ---------- отклики ----------

/// Отклик со сведениями о человеке и вакансии (строка списка и доски).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApplicationDto {
    pub id: Uuid,
    pub person_id: Uuid,
    pub vacancy_id: Uuid,
    pub vacancy_title: String,
    pub full_name: String,
    pub source: Option<String>,
    pub city: Option<String>,
    pub expected_salary: Option<i64>,
    pub stage: Stage,
    /// Полных часов на текущем этапе.
    pub hours_on_stage: i64,
    pub overdue: bool,
    /// Процент совпадения навыков с требованиями вакансии; `null`, если требований нет.
    pub match_percent: Option<u8>,
    /// Сколько всего откликов у этого человека.
    pub person_applications: i64,
    /// RFC 3339.
    pub stage_changed_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApplicationPage {
    pub items: Vec<ApplicationDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EventDto {
    /// `created`, `stage`, `rejected`, `letter`, `interview` или `handoff`.
    pub kind: String,
    pub text: String,
    /// RFC 3339.
    pub at: String,
}

/// Отклик с историей и остальными откликами того же человека.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApplicationDetailDto {
    pub application: ApplicationDto,
    /// Новые записи выше.
    pub history: Vec<EventDto>,
    /// Все отклики человека, включая текущий.
    pub person_applications: Vec<ApplicationDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateApplicationRequest {
    pub person_id: Uuid,
    pub vacancy_id: Uuid,
    /// Начальный этап (по умолчанию `new`).
    #[serde(default)]
    pub stage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MoveRequest {
    pub stage: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RejectRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InterviewRequest {
    /// RFC 3339 со смещением часового пояса пользователя.
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HandoffRequest {
    /// Одна из `HR_SYSTEMS`.
    pub system: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BulkRequest {
    /// `next`, `reject` или `letter`.
    pub action: String,
    pub ids: Vec<Uuid>,
    /// Причина отказа для `reject`.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BulkResult {
    pub changed: i64,
    pub skipped: i64,
}

// ---------- сводка и интервью ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StageCount {
    pub stage: Stage,
    pub count: i64,
}

/// Сводка по воронке организации.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OverviewDto {
    /// Все восемь этапов в порядке воронки, нули включены.
    pub by_stage: Vec<StageCount>,
    pub new_total: i64,
    /// Новые отклики, ждущие дольше суток.
    pub new_waiting_over_day: i64,
    /// Отклики от «Новый» до «Оффер».
    pub in_work: i64,
    /// Просрочено по SLA.
    pub overdue: i64,
    /// Людей в базе организации.
    pub people: i64,
}

/// Назначенное интервью.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InterviewItemDto {
    pub id: Uuid,
    pub application_id: Uuid,
    /// RFC 3339.
    pub scheduled_at: String,
    pub full_name: String,
    pub vacancy_title: String,
    pub hiring_manager: Option<String>,
    pub stage: Stage,
}

// ---------- пакетное создание откликов ----------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BatchApplicationsRequest {
    pub vacancy_id: Uuid,
    pub person_ids: Vec<Uuid>,
    /// Начальный этап (по умолчанию `new`).
    #[serde(default)]
    pub stage: Option<String>,
    /// Откуда пришли отклики, например имя файла импорта: попадёт в историю.
    #[serde(default)]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BatchApplicationsResponse {
    pub created: i64,
    /// Отклик на эту вакансию уже был.
    pub skipped: i64,
    /// Кандидата нет в организации.
    pub missing: i64,
}

// ---------- аналитика ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FunnelStepDto {
    pub stage: Stage,
    /// Сколько откликов дошло до этапа.
    pub reached: i64,
    /// Доля от дошедших до предыдущего этапа, проценты (для первого этапа 100).
    pub conversion: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WeekCountDto {
    /// Понедельник недели, `YYYY-MM-DD` (UTC).
    pub week_start: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SourceStatDto {
    pub source: String,
    pub total: i64,
    /// Дошли до «Интервью HR» и дальше.
    pub interviews: i64,
    pub conversion: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VacancyLoadDto {
    pub id: Uuid,
    pub title: String,
    pub recruiter: Option<String>,
    pub active_applications: i64,
    pub open_days: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AnalyticsDto {
    pub period_days: i64,
    pub funnel: Vec<FunnelStepDto>,
    /// Этап с наибольшей потерей (минимальная конверсия среди этапов после первого).
    pub worst_stage: Option<Stage>,
    pub median_days_to_hire: Option<f64>,
    pub median_hours_to_first_touch: Option<f64>,
    /// Людей с более чем одним откликом.
    pub repeat_people: i64,
    pub weekly: Vec<WeekCountDto>,
    pub sources: Vec<SourceStatDto>,
    pub vacancies: Vec<VacancyLoadDto>,
}
