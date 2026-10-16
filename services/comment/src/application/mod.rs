//! Сценарии: транзакции в контексте тенанта и права на уровне записи.
use chrono::{DateTime, Utc};
use comment_contracts::TargetType;
use platform_core::tenant::TenantId;
use platform_db::TenantTx;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{Comment, DomainError, NewComment},
    infrastructure::comments::{self, CommentRow},
};

pub const DEFAULT_PAGE: i64 = 50;
pub const MAX_PAGE: i64 = 200;

#[derive(Debug, thiserror::Error)]
pub enum CommentError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("not found")]
    NotFound,
    /// Чужой комментарий: править или удалять нельзя.
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    BadQuery(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Страница ленты и курсор следующей.
pub struct Page {
    pub items: Vec<CommentRow>,
    pub next: Option<String>,
}

/// Курсор: микросекунды времени создания и идентификатор последней записи.
pub fn encode_cursor(row: &CommentRow) -> String {
    format!("{}.{}", row.created_at.timestamp_micros(), row.id)
}

pub fn decode_cursor(raw: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let (micros, id) = raw.split_once('.')?;
    Some((
        DateTime::from_timestamp_micros(micros.parse().ok()?)?,
        id.parse().ok()?,
    ))
}

#[derive(Clone)]
pub struct CommentService {
    pool: PgPool,
}

impl CommentService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        tenant: TenantId,
        author_id: Uuid,
        author_name: Option<&str>,
        new: NewComment,
    ) -> Result<CommentRow, CommentError> {
        let comment = Comment::create(tenant, author_id, author_name, new)?;
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        comments::insert(&mut tx, &comment).await?;
        let row = comments::get(&mut tx, comment.id)
            .await?
            .ok_or(CommentError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn list(
        &self,
        tenant: TenantId,
        target_type: &str,
        target_id: Uuid,
        after: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Page, CommentError> {
        let target = TargetType::from_key(target_type.trim()).ok_or_else(|| {
            CommentError::BadQuery("target_type must be client, vacancy or person".into())
        })?;
        let after = after
            .map(|raw| {
                decode_cursor(raw).ok_or_else(|| CommentError::BadQuery("bad cursor".into()))
            })
            .transpose()?;
        let limit = limit.unwrap_or(DEFAULT_PAGE);
        if limit < 1 {
            return Err(CommentError::BadQuery("limit must be positive".into()));
        }
        let limit = limit.min(MAX_PAGE);
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut rows = comments::list(&mut tx, target, target_id, after, limit + 1).await?;
        tx.rollback().await?;
        let has_more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next = has_more.then(|| rows.last().map(encode_cursor)).flatten();
        Ok(Page { items: rows, next })
    }

    pub async fn edit(
        &self,
        tenant: TenantId,
        actor: Uuid,
        id: Uuid,
        text: &str,
    ) -> Result<CommentRow, CommentError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let mut comment = comments::get(&mut tx, id)
            .await?
            .ok_or(CommentError::NotFound)?
            .into_domain();
        if !comment.can_edit(actor) {
            return Err(CommentError::Forbidden);
        }
        comment.edit(text)?;
        comments::update_body(&mut tx, id, &comment.body).await?;
        let row = comments::get(&mut tx, id)
            .await?
            .ok_or(CommentError::NotFound)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn delete(
        &self,
        tenant: TenantId,
        actor: Uuid,
        is_owner: bool,
        id: Uuid,
    ) -> Result<(), CommentError> {
        let mut tx = TenantTx::begin(&self.pool, tenant).await?;
        let comment = comments::get(&mut tx, id)
            .await?
            .ok_or(CommentError::NotFound)?
            .into_domain();
        if !comment.can_delete(actor, is_owner) {
            return Err(CommentError::Forbidden);
        }
        comments::delete(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_roundtrips_and_garbage_is_rejected() {
        let at = DateTime::from_timestamp_micros(1_700_000_000_123_456).unwrap();
        let id = Uuid::new_v4();
        let raw = format!("{}.{id}", at.timestamp_micros());
        assert_eq!(decode_cursor(&raw), Some((at, id)));
        for bad in [
            "",
            "x",
            "1.2",
            "abc.00000000-0000-0000-0000-000000000000",
            "1700",
        ] {
            assert_eq!(decode_cursor(bad), None, "{bad}");
        }
    }
}
