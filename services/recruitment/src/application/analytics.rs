use chrono::{Duration, NaiveDate};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use recruitment_contracts::Stage;
use sqlx::PgPool;
use uuid::Uuid;

use super::{Clock, FunnelError, PeopleSync};
use crate::{
    domain::{
        DomainError,
        analytics::{self, FunnelStep, SourceStat},
    },
    infrastructure::{analytics as store, vacancies},
};

pub const MIN_DAYS: i64 = 7;
pub const MAX_DAYS: i64 = 365;
pub const DEFAULT_DAYS: i64 = 90;
const WEEKS: usize = 12;

pub struct VacancyLoad {
    pub id: Uuid,
    pub title: String,
    pub recruiter: Option<String>,
    pub active_applications: i64,
    pub open_days: i64,
}

pub struct Analytics {
    pub period_days: i64,
    pub funnel: Vec<FunnelStep>,
    pub worst_stage: Option<Stage>,
    pub median_days_to_hire: Option<f64>,
    pub median_hours_to_first_touch: Option<f64>,
    pub repeat_people: i64,
    pub weekly: Vec<(NaiveDate, i64)>,
    pub sources: Vec<SourceStat>,
    pub vacancies: Vec<VacancyLoad>,
}

#[derive(Clone)]
pub struct AnalyticsService {
    pool: PgPool,
    sync: PeopleSync,
    clock: Clock,
}

impl AnalyticsService {
    pub fn new(pool: PgPool, sync: PeopleSync, clock: Clock) -> Self {
        Self { pool, sync, clock }
    }

    pub async fn report(&self, tenant: TenantId, days: i64) -> Result<Analytics, FunnelError> {
        if !(MIN_DAYS..=MAX_DAYS).contains(&days) {
            return Err(DomainError::invalid("days", "от 7 до 365 суток").into());
        }
        self.sync.sync(tenant).await;
        let now = self.clock.now();
        let since = now - Duration::days(days);
        let weeks_since =
            analytics::week_start(now.date_naive()) - Duration::weeks(WEEKS as i64 - 1);

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let facts = store::facts(&mut tx, since).await?;
        let week_counts = store::week_counts(
            &mut tx,
            weeks_since
                .and_hms_opt(0, 0, 0)
                .unwrap_or_default()
                .and_utc(),
        )
        .await?;
        let repeat_people = store::repeat_people(&mut tx).await?;
        let vacancy_views = vacancies::list(&mut tx, None).await?;
        tx.rollback().await?;

        let today = now.date_naive();
        let (funnel, worst_stage) = analytics::funnel(&facts);
        Ok(Analytics {
            period_days: days,
            funnel,
            worst_stage,
            median_days_to_hire: analytics::days_to_hire(&facts),
            median_hours_to_first_touch: analytics::first_touch_hours(&facts),
            repeat_people,
            weekly: analytics::week_series(&week_counts, today, WEEKS),
            sources: analytics::sources(&facts),
            vacancies: vacancy_views
                .into_iter()
                .map(|v| VacancyLoad {
                    id: v.vacancy.id,
                    open_days: v.vacancy.open_days(today),
                    title: v.vacancy.title,
                    recruiter: v.vacancy.recruiter,
                    active_applications: v.active_applications,
                })
                .collect(),
        })
    }
}
