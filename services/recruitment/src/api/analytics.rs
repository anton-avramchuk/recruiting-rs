use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use platform_auth::{AuthUser, Permission};
use platform_core::error::{ApiError, ProblemDetails};
use recruitment_contracts::{
    AnalyticsDto, ClientStatDto, FunnelStepDto, SourceStatDto, VacancyLoadDto, WeekCountDto,
};
use serde::Deserialize;
use uuid::Uuid;

use super::{AppState, vacancies::tenant_of};
use crate::application::{DEFAULT_DAYS, FunnelError};

pub fn routes() -> Router<AppState> {
    Router::new().route("/analytics", get(analytics))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct AnalyticsParams {
    /// Период в сутках: от 7 до 365 (по умолчанию 90).
    days: Option<String>,
    /// Только вакансии этого заказчика.
    client_id: Option<String>,
}

#[utoipa::path(get, path = "/analytics", security(("bearer" = [])), params(AnalyticsParams), responses(
    (status = 200, body = AnalyticsDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn analytics(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<AnalyticsParams>,
) -> Result<Json<AnalyticsDto>, ApiError> {
    user.require(Permission::ReadData)?;
    let days = match params.days.as_deref().filter(|d| !d.is_empty()) {
        None => DEFAULT_DAYS,
        Some(raw) => raw
            .parse::<i64>()
            .map_err(|_| FunnelError::BadQuery("days must be a number".into()))?,
    };
    let client = match params.client_id.as_deref().filter(|c| !c.is_empty()) {
        None => None,
        Some(raw) => Some(
            raw.parse::<Uuid>()
                .map_err(|_| FunnelError::BadQuery("client_id must be a uuid".into()))?,
        ),
    };
    let report = state
        .analytics
        .report(tenant_of(&user)?, days, client)
        .await?;
    Ok(Json(AnalyticsDto {
        period_days: report.period_days,
        funnel: report
            .funnel
            .into_iter()
            .map(|s| FunnelStepDto {
                stage: s.stage,
                reached: s.reached,
                conversion: s.conversion,
            })
            .collect(),
        worst_stage: report.worst_stage,
        median_days_to_hire: report.median_days_to_hire,
        median_hours_to_first_touch: report.median_hours_to_first_touch,
        repeat_people: report.repeat_people,
        weekly: report
            .weekly
            .into_iter()
            .map(|(week, count)| WeekCountDto {
                week_start: week.format("%Y-%m-%d").to_string(),
                count,
            })
            .collect(),
        sources: report
            .sources
            .into_iter()
            .map(|s| SourceStatDto {
                source: s.source,
                total: s.total,
                interviews: s.interviews,
                conversion: s.conversion,
            })
            .collect(),
        vacancies: report
            .vacancies
            .into_iter()
            .map(|v| VacancyLoadDto {
                id: v.id,
                title: v.title,
                recruiter: v.recruiter,
                active_applications: v.active_applications,
                open_days: v.open_days,
            })
            .collect(),
        clients: report
            .clients
            .into_iter()
            .map(|c| ClientStatDto {
                client_id: c.client_id,
                name: c.name,
                archived: c.archived,
                vacancies: c.vacancies,
                active_applications: c.active_applications,
                applications: c.applications,
                interviews: c.interviews,
                conversion: c.conversion,
                hired: c.hired,
                median_days_to_hire: c.median_days_to_hire,
            })
            .collect(),
    }))
}
