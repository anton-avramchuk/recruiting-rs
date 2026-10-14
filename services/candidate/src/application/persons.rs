use chrono::NaiveDate;
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;

use uuid::Uuid;

use crate::{
    domain::{
        ContactsInput, DomainError, Duplicate, Education, Employment, NewPerson, Person, Probe,
        ProfilePatch, find_duplicate,
    },
    infrastructure::persons::{self as store, ListFilter, PersonSummary, StoreError},
};

/// Правка контактов: `None` - не трогать, `Some(None)` - очистить, `Some(Some(v))` - заменить.
#[derive(Debug, Clone, Default)]
pub struct ContactsPatch {
    pub phone: Option<Option<String>>,
    pub email: Option<Option<String>>,
    pub telegram: Option<Option<String>>,
    pub whatsapp: Option<Option<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdatePerson {
    pub profile: ProfilePatch,
    pub contacts: ContactsPatch,
}

pub const DEFAULT_PAGE_SIZE: i64 = 20;
pub const MAX_PAGE_SIZE: i64 = 100;

pub struct ListQuery {
    pub filter: ListFilter,
    pub page: i64,
    pub page_size: i64,
}

pub struct Page {
    pub items: Vec<PersonSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum PersonError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Найден «тот же человек», создание не подтверждено.
    #[error("possible duplicate of {}", .0.full_name)]
    Duplicate(Duplicate),
    #[error("person not found")]
    NotFound,
    #[error("{0}")]
    BadQuery(String),
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

    pub async fn get(&self, tenant: TenantId, id: Uuid) -> Result<Person, PersonError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let person = store::get(&mut tx, id).await?;
        tx.rollback().await?;
        person.ok_or(PersonError::NotFound)
    }

    pub async fn list(&self, tenant: TenantId, query: ListQuery) -> Result<Page, PersonError> {
        if query.page < 1 || query.page_size < 1 {
            return Err(PersonError::BadQuery(
                "page and page_size must be positive".into(),
            ));
        }
        let page_size = query.page_size.min(MAX_PAGE_SIZE);
        let offset = (query.page - 1).saturating_mul(page_size);
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let (items, total) = store::list(&mut tx, &query.filter, page_size, offset).await?;
        tx.rollback().await?;
        Ok(Page {
            items,
            total,
            page: query.page,
            page_size,
        })
    }

    /// Частичное обновление: изменения применяются к копии и сохраняются только целиком.
    pub async fn update(
        &self,
        tenant: TenantId,
        id: Uuid,
        changes: UpdatePerson,
    ) -> Result<Person, PersonError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut person = store::get(&mut tx, id)
            .await?
            .ok_or(PersonError::NotFound)?;

        let mut contacts: ContactsInput = person.contacts_input();
        let patch = changes.contacts;
        if let Some(v) = patch.phone {
            contacts.phone = v;
        }
        if let Some(v) = patch.email {
            contacts.email = v;
        }
        if let Some(v) = patch.telegram {
            contacts.telegram = v;
        }
        if let Some(v) = patch.whatsapp {
            contacts.whatsapp = v;
        }
        // сначала всё проверяется на копии: при ошибке карточка остаётся прежней
        let mut candidate = person.clone();
        candidate.apply_profile(changes.profile)?;
        candidate.change_contacts(contacts)?;
        person = candidate;

        store::update(&mut tx, &person, person.experience_years(self.today())).await?;
        tx.commit().await?;
        Ok(person)
    }

    /// Заменяет места работы и пересчитывает стаж в одной транзакции.
    pub async fn replace_employments(
        &self,
        tenant: TenantId,
        id: Uuid,
        employments: Vec<Employment>,
    ) -> Result<Person, PersonError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut person = store::get(&mut tx, id)
            .await?
            .ok_or(PersonError::NotFound)?;
        person.replace_employments(employments);
        let years = person.experience_years(self.today());
        store::replace_employments(&mut tx, tenant, id, person.employments(), years).await?;
        tx.commit().await?;
        Ok(person)
    }

    pub async fn replace_educations(
        &self,
        tenant: TenantId,
        id: Uuid,
        educations: Vec<Education>,
    ) -> Result<Person, PersonError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut person = store::get(&mut tx, id)
            .await?
            .ok_or(PersonError::NotFound)?;
        person.replace_educations(educations);
        store::replace_educations(&mut tx, tenant, id, person.educations()).await?;
        tx.commit().await?;
        Ok(person)
    }

    /// Необратимое удаление вместе с историей.
    pub async fn delete(&self, tenant: TenantId, id: Uuid) -> Result<(), PersonError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        if !store::delete(&mut tx, id).await? {
            return Err(PersonError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }
}
