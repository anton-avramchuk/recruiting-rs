//! DTO сервиса candidate: общие для сервера, BFF и Rust-фронтенда.
//!
//! Даты - строки `YYYY-MM-DD`, моменты времени - RFC 3339: контракты собираются и в wasm,
//! где не нужны ни chrono, ни источник случайности.
use platform_values::double_option;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EmploymentDto {
    pub company: String,
    pub position: String,
    /// `YYYY-MM-DD`.
    pub start: String,
    /// `YYYY-MM-DD`; пусто - текущее место работы.
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EducationDto {
    pub institution: String,
    #[serde(default)]
    pub specialty: Option<String>,
    #[serde(default)]
    pub year_from: Option<i32>,
    #[serde(default)]
    pub year_to: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonDto {
    pub id: Uuid,
    pub full_name: String,
    pub source: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub telegram: Option<String>,
    pub whatsapp: Option<String>,
    pub expected_salary: Option<i64>,
    pub summary: Option<String>,
    pub city: Option<String>,
    pub note: Option<String>,
    pub skills: Vec<String>,
    /// Полных лет: по местам работы, а при их отсутствии - заявленный стаж.
    pub experience_years: i32,
    pub employments: Vec<EmploymentDto>,
    pub educations: Vec<EducationDto>,
    /// RFC 3339.
    pub created_at: String,
}

/// Строка списка: без истории.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonSummaryDto {
    pub id: Uuid,
    pub full_name: String,
    pub source: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub city: Option<String>,
    pub expected_salary: Option<i64>,
    pub skills: Vec<String>,
    pub experience_years: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonPage {
    pub items: Vec<PersonSummaryDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// Состояние человека в ленте изменений (то, что нужно воронке).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonSnapshot {
    pub full_name: String,
    pub source: Option<String>,
    pub city: Option<String>,
    pub expected_salary: Option<i64>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub skills: Vec<String>,
    pub experience_years: i32,
}

/// Запись ленты: последнее состояние человека или признак удаления.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonChange {
    pub seq: i64,
    pub person_id: Uuid,
    pub deleted: bool,
    pub snapshot: Option<PersonSnapshot>,
}

/// Страница ленты; `next` - позиция, с которой читать дальше.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PersonChangesPage {
    pub items: Vec<PersonChange>,
    pub next: i64,
}

/// Найденный «тот же человек» при создании.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DuplicateInfo {
    pub id: Uuid,
    pub full_name: String,
    /// `email`, `phone` или `name`.
    pub matched_by: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreatePersonRequest {
    pub full_name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub telegram: Option<String>,
    #[serde(default)]
    pub whatsapp: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub expected_salary: Option<i64>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    /// Стаж, заявленный при заведении; действует, пока нет мест работы.
    #[serde(default)]
    pub declared_experience_years: Option<i32>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Подтверждение создания, когда сервис уже сообщил о возможном дубле.
    #[serde(default)]
    pub confirm_duplicate: bool,
}

/// Частичное обновление. Для очищаемых полей: нет в теле - не трогать, `null` (или пустая
/// строка) - очистить, значение - заменить.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdatePersonRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub phone: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub email: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub telegram: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub whatsapp: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub source: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<i64>)]
    pub expected_salary: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub summary: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub city: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
}
