use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use platform_core::tenant::TenantId;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

/// Новый refresh-токен: `<tenant_id>.<случайные 32 байта>`. Тенант внутри токена
/// нужен, чтобы при обмене открыть транзакцию в нужном контексте RLS.
pub fn generate(tenant: TenantId) -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("{}.{}", tenant, URL_SAFE_NO_PAD.encode(bytes))
}

pub fn tenant_of(token: &str) -> Option<TenantId> {
    let (tenant, _) = token.split_once('.')?;
    Uuid::parse_str(tenant).ok().map(TenantId)
}

pub fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn insert(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
    token: &str,
    ttl_secs: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO refresh_tokens (tenant_id, user_id, token_hash, expires_at)
         VALUES ($1, $2, $3, now() + make_interval(secs => $4))",
    )
    .bind(tenant.0)
    .bind(user_id)
    .bind(hash(token))
    .bind(ttl_secs as f64)
    .execute(conn)
    .await?;
    Ok(())
}
