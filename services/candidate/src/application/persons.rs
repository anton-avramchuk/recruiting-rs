use chrono::NaiveDate;
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;

use crate::{
    domain::{DomainError, Duplicate, NewPerson, Person, Probe, find_duplicate},
    infrastructure::persons::{self as store, StoreError},
};

#[derive(Debug, thiserror::Error)]
pub enum PersonError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Найден «тот же человек», создание не подтверждено.
    #[error("possible duplicate of {}", .0.full_name)]
    Duplicate(Duplicate),
    #[error("person not found")]
    NotFound,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Источник «сегодня»: фиксируется в тестах, чтобы стаж по текущему месту был предсказуем.
#[derive(Clone, Copy)]
pub enum Clock {
    System,
    Fixed(NaiveDate),
}

impl Clock {
    pub fn today(self) -> NaiveDate {
        match self {
            Clock::System => chrono::Utc::now().date_naive(),
            Clock::Fixed(date) => date,
        }
    }
}

#[derive(Clone)]
pub struct PersonService {
    pool: PgPool,
    clock: Clock,
}

impl PersonService {
    pub fn new(pool: PgPool, clock: Clock) -> Self {
        Self { pool, clock }
    }

    pub fn today(&self) -> NaiveDate {
        self.clock.today()
    }

    /// Создаёт кандидата. Если в организации уже есть «тот же человек» (email → телефон → ФИО)
    /// и создание не подтверждено, возвращает `Duplicate`.
    pub async fn create(
        &self,
        tenant: TenantId,
        new: NewPerson,
        confirm_duplicate: bool,
    ) -> Result<Person, PersonError> {
        let person = Person::create(tenant, new)?;

        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if !confirm_duplicate {
            let keys = person.keys();
            let probe = Probe {
                normalized_name: keys.normalized_name,
                normalized_email: keys.normalized_email,
                normalized_phone: keys.normalized_phone,
            };
            let pool = store::find_candidates(&mut tx, &probe).await?;
            if let Some(duplicate) = find_duplicate(&pool, &probe) {
                return Err(PersonError::Duplicate(duplicate));
            }
        }
        store::insert(&mut tx, &person, person.experience_years(self.today())).await?;
        tx.commit().await?;
        Ok(person)
    }
}
