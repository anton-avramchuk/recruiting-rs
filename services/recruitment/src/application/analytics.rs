use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use recruitment_contracts::Stage;
use sqlx::PgPool;
use uuid::Uuid;

use super::{ClientsSync, Clock, FunnelError, PeopleSync};
use crate::{
    domain::{
        DomainError,
        analytics::{self, ClientInfo, ClientStat, FunnelStep, SourceStat, VacancyRef},
    },
    infrastructure::{analytics as store, clients as client_store, vacancies},
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
    /// Заказчики организации; фильтр по заказчику их не сужает.
    pub clients: Vec<ClientStat>,
}

#[derive(Clone)]
pub struct AnalyticsService {
    pool: PgPool,
    sync: PeopleSync,
    clients_sync: ClientsSync,
    clock: Clock,
}

impl AnalyticsService {
    pub fn new(pool: PgPool, sync: PeopleSync, clients_sync: ClientsSync, clock: Clock) -> Self {
        Self {
            pool,
            sync,
            clients_sync,
            clock,
        }
    }

    /// Метрики за `days` суток; `client` сужает их до вакансий одного заказчика.
    pub async fn report(
        &self,
        tenant: TenantId,
        days: i64,
        client: Option<Uuid>,
    ) -> Result<Analytics, FunnelError> {
        if !(MIN_DAYS..=MAX_DAYS).contains(&days) {
            return Err(DomainError::invalid("days", "от 7 до 365 суток").into());
        }
        self.sync.sync(tenant).await;
        self.clients_sync.sync(tenant).await;
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
            client,
        )
        .await?;
        let repeat_people = store::repeat_people(&mut tx, client).await?;
        let all_vacancies = vacancies::list(&mut tx, None).await?;
        let client_rows = client_store::all(&mut tx).await?;
        tx.rollback().await?;

        // таблица заказчиков считается по всем данным, остальные блоки - по выбранному заказчику
        let info: HashMap<Uuid, ClientInfo> = client_rows
            .into_iter()
            .map(|(id, name, archived)| (id, ClientInfo { name, archived }))
            .collect();
        let refs: Vec<VacancyRef> = all_vacancies
            .iter()
            .map(|v| VacancyRef {
                client_id: v.vacancy.client_id,
                active_applications: v.active_applications,
            })
            .collect();
        let clients = analytics::clients(&facts, &refs, &info);
        let facts: Vec<_> = match client {
            Some(id) => facts
                .into_iter()
                .filter(|f| f.client_id == Some(id))
                .collect(),
            None => facts,
        };
        let vacancy_views: Vec<_> = all_vacancies
            .into_iter()
            .filter(|v| client.is_none_or(|id| v.vacancy.client_id == Some(id)))
            .collect();

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
            clients,
        })
    }
}
