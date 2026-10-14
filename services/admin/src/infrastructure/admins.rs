use platform_auth::password;
use sqlx::PgPool;

pub const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    #[error("password must be at least {MIN_PASSWORD_LEN} characters")]
    WeakPassword,
    #[error("invalid email")]
    InvalidEmail,
    #[error("password hashing failed: {0}")]
    Hash(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Создаёт платформенного администратора, если его ещё нет. Существующему пароль не меняет.
/// Возвращает `true`, если администратор создан.
pub async fn seed(pool: &PgPool, email: &str, plain_password: &str) -> Result<bool, SeedError> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(SeedError::InvalidEmail);
    }
    if plain_password.chars().count() < MIN_PASSWORD_LEN {
        return Err(SeedError::WeakPassword);
    }
    let hash = password::hash(plain_password).map_err(|e| SeedError::Hash(e.to_string()))?;
    let res = sqlx::query(
        "INSERT INTO platform_admins (email, password_hash) VALUES ($1, $2)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(email)
    .bind(hash)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}
