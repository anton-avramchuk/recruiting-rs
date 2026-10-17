use chrono::{DateTime, FixedOffset, Utc};
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use recruitment_contracts::Stage;
use sqlx::PgPool;
use uuid::Uuid;

use super::{Clock, FunnelError, PeopleSync};
use crate::{
    domain::{Application, DomainError},
    infrastructure::{
        applications::{
            self as store, ApplicationFilter, ApplicationRow, EventRow, InterviewRow, Sort,
        },
        is_unique_violation, people, vacancies,
    },
};

pub const DEFAULT_PAGE_SIZE: i64 = 50;
pub const MAX_PAGE_SIZE: i64 = 200;
const MAX_BULK: usize = 200;
const MAX_BATCH: usize = 500;
const MAX_INTERVIEW_DAYS: i64 = 31;

pub struct ListQuery {
    pub filter: ApplicationFilter,
    pub sort: Sort,
    pub descending: Option<bool>,
    pub page: i64,
    pub page_size: i64,
}

impl ListQuery {
    pub fn new(filter: ApplicationFilter) -> Self {
        Self {
            filter,
            sort: Sort::Age,
            descending: None,
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

/// Страница откликов грида и признак «данные людей могут отставать».
pub struct GridApplicationPage {
    pub page: grid_contracts::GridPage<ApplicationRow>,
    pub stale: bool,
}

pub struct ApplicationPage {
    pub items: Vec<ApplicationRow>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    /// Данные людей могут отставать: источник был недоступен.
    pub stale: bool,
}

/// Сводка по воронке: этапы в порядке воронки и производные счётчики.
pub struct Overview {
    pub by_stage: Vec<(Stage, i64)>,
    pub new_total: i64,
    pub new_waiting_over_day: i64,
    pub in_work: i64,
    pub overdue: i64,
    pub people: i64,
}

pub struct ApplicationDetail {
    pub application: ApplicationRow,
    pub history: Vec<EventRow>,
    pub person_applications: Vec<ApplicationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkAction {
    Next,
    Reject(Option<String>),
    Letter,
}

impl BulkAction {
    pub fn parse(action: &str, reason: Option<String>) -> Result<Self, DomainError> {
        match action {
            "next" => Ok(BulkAction::Next),
            "reject" => Ok(BulkAction::Reject(reason)),
            "letter" => Ok(BulkAction::Letter),
            _ => Err(DomainError::invalid("action", "next, reject или letter")),
        }
    }
}

/// Итог пакетного создания откликов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchOutcome {
    pub created: i64,
    pub skipped: i64,
    pub missing: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BulkOutcome {
    pub changed: i64,
    pub skipped: i64,
}

fn parse_stage(raw: &str) -> Result<Stage, DomainError> {
    Stage::from_key(raw.trim()).ok_or_else(|| DomainError::invalid("stage", "неизвестный этап"))
}

#[derive(Clone)]
pub struct ApplicationService {
    pool: PgPool,
    sync: PeopleSync,
    clock: Clock,
}

impl ApplicationService {
    pub fn new(pool: PgPool, sync: PeopleSync, clock: Clock) -> Self {
        Self { pool, sync, clock }
    }

    /// Создаёт отклик. Перед проверкой человека проекция догоняет candidate, поэтому только что
    /// созданный кандидат находится сразу.
    pub async fn create(
        &self,
        tenant: TenantId,
        person_id: Uuid,
        vacancy_id: Uuid,
        stage: Option<&str>,
    ) -> Result<ApplicationRow, FunnelError> {
        let stage = stage.map(parse_stage).transpose()?;
        self.sync.sync(tenant).await;
        let now = self.clock.now();

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if !people::exists(&mut tx, person_id).await?
            || !vacancies::exists(&mut tx, vacancy_id).await?
        {
            return Err(FunnelError::NotFound);
        }
        let mut app = Application::create(tenant, person_id, vacancy_id, now);
        if let Some(stage) = stage {
            app.move_to(stage, now);
        }
        if let Err(e) = store::insert(&mut tx, &app).await {
            return Err(if is_unique_violation(&e) {
                FunnelError::Duplicate
            } else {
                e.into()
            });
        }
        store::insert_events(&mut tx, tenant, app.id, &app.take_events()).await?;
        let row = store::row(&mut tx, app.id, now)
            .await?
            .ok_or(FunnelError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn list(
        &self,
        tenant: TenantId,
        query: ListQuery,
    ) -> Result<ApplicationPage, FunnelError> {
        if query.page < 1 || query.page_size < 1 {
            return Err(FunnelError::BadQuery(
                "page and page_size must be positive".into(),
            ));
        }
        let fresh = self.sync.sync(tenant).await;
        let page_size = query.page_size.min(MAX_PAGE_SIZE);
        let offset = (query.page - 1).saturating_mul(page_size);
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let (items, total) = store::rows(
            &mut tx,
            &query.filter,
            self.clock.now(),
            query.sort,
            query.descending,
            page_size,
            offset,
        )
        .await?;
        tx.rollback().await?;
        Ok(ApplicationPage {
            items,
            total,
            page: query.page,
            page_size,
            stale: !fresh,
        })
    }

    /// Страница откликов по запросу грида; без сортировки - давно без движения выше.
    pub async fn list_grid(
        &self,
        tenant: TenantId,
        mut query: grid_contracts::GridQuery,
    ) -> Result<GridApplicationPage, FunnelError> {
        if query.sort.is_empty() {
            query
                .sort
                .push(grid_contracts::SortKey::asc("stage_changed_at"));
        }
        let fresh = self.sync.sync(tenant).await;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let page = store::grid_page(&mut tx, &query, self.clock.now()).await;
        tx.rollback().await?;
        Ok(GridApplicationPage {
            page: page?,
            stale: !fresh,
        })
    }

    pub async fn get(&self, tenant: TenantId, id: Uuid) -> Result<ApplicationDetail, FunnelError> {
        self.sync.sync(tenant).await;
        let now = self.clock.now();
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let application = store::row(&mut tx, id, now)
            .await?
            .ok_or(FunnelError::NotFound)?;
        let history = store::history(&mut tx, id).await?;
        let person_applications = store::person_rows(&mut tx, application.person_id, now).await?;
        tx.rollback().await?;
        Ok(ApplicationDetail {
            application,
            history,
            person_applications,
        })
    }

    /// Люди с откликами на вакансии заказчика: не больше `limit`, признак «людей больше».
    pub async fn people_of_client(
        &self,
        tenant: TenantId,
        client: Uuid,
        limit: i64,
    ) -> Result<(Vec<Uuid>, bool), FunnelError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut ids = store::people_of_client(&mut tx, client, limit + 1).await?;
        tx.rollback().await?;
        let truncated = ids.len() as i64 > limit;
        ids.truncate(limit as usize);
        Ok((ids, truncated))
    }

    /// Сводка по воронке организации.
    pub async fn overview(&self, tenant: TenantId) -> Result<Overview, FunnelError> {
        self.sync.sync(tenant).await;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let tallies = store::stage_tallies(&mut tx, self.clock.now()).await?;
        let people = store::people_count(&mut tx).await?;
        tx.rollback().await?;

        let of = |stage: Stage| tallies.iter().find(|t| t.stage == stage.key());
        let by_stage: Vec<(Stage, i64)> = Stage::ALL
            .into_iter()
            .map(|s| (s, of(s).map_or(0, |t| t.total)))
            .collect();
        Ok(Overview {
            new_total: by_stage[0].1,
            // «новый» просрочен ровно тогда, когда ждёт дольше суток
            new_waiting_over_day: of(Stage::New).map_or(0, |t| t.overdue),
            in_work: by_stage
                .iter()
                .filter(|(s, _)| s.is_active())
                .map(|(_, n)| n)
                .sum(),
            overdue: tallies.iter().map(|t| t.overdue).sum(),
            by_stage,
            people,
        })
    }

    /// Интервью организации за период `[from, to)`; период не длиннее 31 суток.
    pub async fn interviews(
        &self,
        tenant: TenantId,
        from: &str,
        to: &str,
    ) -> Result<Vec<InterviewRow>, FunnelError> {
        let parse = |field: &'static str, raw: &str| {
            DateTime::parse_from_rfc3339(raw.trim())
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| DomainError::invalid(field, "дата и время в формате RFC 3339"))
        };
        let (from, to) = (parse("from", from)?, parse("to", to)?);
        if to <= from {
            return Err(
                DomainError::invalid("to", "конец периода должен быть позже начала").into(),
            );
        }
        if to - from > chrono::Duration::days(MAX_INTERVIEW_DAYS) {
            return Err(DomainError::invalid("to", "период не длиннее 31 суток").into());
        }
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let rows = store::interviews_between(&mut tx, from, to).await?;
        tx.rollback().await?;
        Ok(rows)
    }

    /// Применяет изменение к отклику и сохраняет его вместе с историей.
    async fn change<F>(
        &self,
        tenant: TenantId,
        id: Uuid,
        f: F,
    ) -> Result<ApplicationRow, FunnelError>
    where
        F: FnOnce(&mut Application, DateTime<Utc>) -> Result<(), DomainError>,
    {
        let now = self.clock.now();
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut app = store::load_for_update(&mut tx, id)
            .await?
            .ok_or(FunnelError::NotFound)?;
        f(&mut app, now)?;
        store::save_stage(&mut tx, &app).await?;
        store::insert_events(&mut tx, tenant, app.id, &app.take_events()).await?;
        let row = store::row(&mut tx, id, now)
            .await?
            .ok_or(FunnelError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn move_to(
        &self,
        tenant: TenantId,
        id: Uuid,
        stage: &str,
    ) -> Result<ApplicationRow, FunnelError> {
        let stage = parse_stage(stage)?;
        self.change(tenant, id, |app, now| {
            app.move_to(stage, now);
            Ok(())
        })
        .await
    }

    pub async fn advance(&self, tenant: TenantId, id: Uuid) -> Result<ApplicationRow, FunnelError> {
        self.change(tenant, id, |app, now| app.advance(now)).await
    }

    pub async fn reject(
        &self,
        tenant: TenantId,
        id: Uuid,
        reason: Option<String>,
    ) -> Result<ApplicationRow, FunnelError> {
        self.change(tenant, id, |app, now| {
            app.reject(reason, now);
            Ok(())
        })
        .await
    }

    pub async fn hand_off(
        &self,
        tenant: TenantId,
        id: Uuid,
        system: &str,
    ) -> Result<ApplicationRow, FunnelError> {
        self.change(tenant, id, |app, now| app.hand_off(system, now))
            .await
    }

    /// Назначает интервью. Время приходит со смещением часового пояса пользователя.
    pub async fn schedule_interview(
        &self,
        tenant: TenantId,
        id: Uuid,
        at: &str,
    ) -> Result<ApplicationRow, FunnelError> {
        let at: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(at.trim())
            .map_err(|_| DomainError::invalid("at", "дата и время в формате RFC 3339"))?;
        let scheduled = at.with_timezone(&Utc);
        let offset = at.offset().local_minus_utc();
        let now = self.clock.now();

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut app = store::load_for_update(&mut tx, id)
            .await?
            .ok_or(FunnelError::NotFound)?;
        app.schedule_interview(scheduled, offset, now)?;
        store::insert_interview(&mut tx, tenant, id, scheduled, now).await?;
        store::insert_events(&mut tx, tenant, id, &app.take_events()).await?;
        let row = store::row(&mut tx, id, now)
            .await?
            .ok_or(FunnelError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    /// Создаёт отклики на вакансию для набора людей (импорт). Существующие отклики пропускаются,
    /// людей, которых нет в организации, считает отдельно.
    pub async fn create_batch(
        &self,
        tenant: TenantId,
        vacancy_id: Uuid,
        person_ids: &[Uuid],
        stage: Option<&str>,
        origin: Option<&str>,
    ) -> Result<BatchOutcome, FunnelError> {
        if person_ids.len() > MAX_BATCH {
            return Err(
                DomainError::invalid("person_ids", "не больше 500 кандидатов за раз").into(),
            );
        }
        let stage = stage.map(parse_stage).transpose()?;
        // импортированные люди появляются в проекции по ленте candidate
        self.sync.sync(tenant).await;
        let now = self.clock.now();

        let mut unique: Vec<Uuid> = Vec::with_capacity(person_ids.len());
        for id in person_ids {
            if !unique.contains(id) {
                unique.push(*id);
            }
        }

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if !vacancies::exists(&mut tx, vacancy_id).await? {
            return Err(FunnelError::NotFound);
        }
        let present = store::present_people(&mut tx, &unique).await?;
        let applied = store::applied_people(&mut tx, vacancy_id, &unique).await?;
        let mut outcome = BatchOutcome {
            created: 0,
            skipped: 0,
            missing: 0,
        };
        for person_id in unique {
            if !present.contains(&person_id) {
                outcome.missing += 1;
            } else if applied.contains(&person_id) {
                outcome.skipped += 1;
            } else {
                let mut app = Application::create_from(tenant, person_id, vacancy_id, now, origin);
                if let Some(stage) = stage {
                    app.move_to(stage, now);
                }
                store::insert(&mut tx, &app).await?;
                store::insert_events(&mut tx, tenant, app.id, &app.take_events()).await?;
                outcome.created += 1;
            }
        }
        tx.commit().await?;
        Ok(outcome)
    }

    /// Групповое действие. Чужие и несуществующие идентификаторы игнорируются,
    /// неприменимые отклики пропускаются.
    pub async fn bulk(
        &self,
        tenant: TenantId,
        ids: &[Uuid],
        action: BulkAction,
    ) -> Result<BulkOutcome, FunnelError> {
        if ids.len() > MAX_BULK {
            return Err(DomainError::invalid("ids", "не больше 200 откликов за раз").into());
        }
        let now = self.clock.now();
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let apps = store::load_many_for_update(&mut tx, ids).await?;
        let mut outcome = BulkOutcome {
            changed: 0,
            skipped: 0,
        };
        for mut app in apps {
            let applied = match &action {
                BulkAction::Next => app.advance(now).is_ok(),
                BulkAction::Reject(_) if app.stage == Stage::Rejected => false,
                BulkAction::Reject(reason) => {
                    app.reject(reason.clone(), now);
                    true
                }
                BulkAction::Letter => {
                    app.send_letter(now);
                    true
                }
            };
            if applied {
                store::save_stage(&mut tx, &app).await?;
                store::insert_events(&mut tx, tenant, app.id, &app.take_events()).await?;
                outcome.changed += 1;
            } else {
                outcome.skipped += 1;
            }
        }
        tx.commit().await?;
        Ok(outcome)
    }
}
