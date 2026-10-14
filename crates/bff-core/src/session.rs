use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};

/// Серверная сессия. Токены никогда не покидают сервер: браузер получает только id сессии.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix-время истечения access-токена (секунды).
    pub access_expires_at: i64,
    pub csrf_token: String,
    /// Несекретные данные входа для интерфейса (email, тенант). Пароль сюда не попадает.
    #[serde(default)]
    pub meta: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
#[error("session store error: {0}")]
pub struct StoreError(pub String);

impl From<redis::RedisError> for StoreError {
    fn from(e: redis::RedisError) -> Self {
        StoreError(e.to_string())
    }
}

/// 32 случайных байта в base64url.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, session: &Session, ttl: Duration) -> Result<String, StoreError>;
    async fn get(&self, id: &str) -> Result<Option<Session>, StoreError>;
    /// Обновляет существующую сессию; удалённую или истёкшую не воскрешает.
    async fn save(&self, id: &str, session: &Session, ttl: Duration) -> Result<(), StoreError>;
    async fn delete(&self, id: &str) -> Result<(), StoreError>;
}

/// Хранилище в памяти процесса: для тестов и локальной разработки без Redis.
#[derive(Default)]
pub struct MemorySessionStore {
    inner: Mutex<HashMap<String, (Instant, Session)>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn create(&self, session: &Session, ttl: Duration) -> Result<String, StoreError> {
        let id = random_token();
        self.inner
            .lock()
            .expect("store lock")
            .insert(id.clone(), (Instant::now() + ttl, session.clone()));
        Ok(id)
    }

    async fn get(&self, id: &str) -> Result<Option<Session>, StoreError> {
        let mut map = self.inner.lock().expect("store lock");
        match map.get(id) {
            Some((expires, s)) if *expires > Instant::now() => Ok(Some(s.clone())),
            Some(_) => {
                map.remove(id);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn save(&self, id: &str, session: &Session, ttl: Duration) -> Result<(), StoreError> {
        let mut map = self.inner.lock().expect("store lock");
        if let Some(entry) = map.get_mut(id).filter(|(e, _)| *e > Instant::now()) {
            *entry = (Instant::now() + ttl, session.clone());
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.inner.lock().expect("store lock").remove(id);
        Ok(())
    }
}

/// Хранилище в Redis. `prefix` разделяет ключи слоёв (crm/admin) в одном Redis.
#[derive(Clone)]
pub struct RedisSessionStore {
    conn: ConnectionManager,
    prefix: String,
}

impl RedisSessionStore {
    pub async fn connect(url: &str, prefix: &str) -> Result<Self, StoreError> {
        let client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self {
            conn,
            prefix: prefix.to_string(),
        })
    }

    fn key(&self, id: &str) -> String {
        format!("{}{id}", self.prefix)
    }
}

fn ttl_millis(ttl: Duration) -> u64 {
    ttl.as_millis().max(1) as u64
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn create(&self, session: &Session, ttl: Duration) -> Result<String, StoreError> {
        let id = random_token();
        let json = serde_json::to_string(session).map_err(|e| StoreError(e.to_string()))?;
        let mut conn = self.conn.clone();
        let _: () = redis::cmd("SET")
            .arg(self.key(&id))
            .arg(json)
            .arg("PX")
            .arg(ttl_millis(ttl))
            .query_async(&mut conn)
            .await?;
        Ok(id)
    }

    async fn get(&self, id: &str) -> Result<Option<Session>, StoreError> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn.get(self.key(id)).await?;
        raw.map(|json| serde_json::from_str(&json).map_err(|e| StoreError(e.to_string())))
            .transpose()
    }

    async fn save(&self, id: &str, session: &Session, ttl: Duration) -> Result<(), StoreError> {
        let json = serde_json::to_string(session).map_err(|e| StoreError(e.to_string()))?;
        let mut conn = self.conn.clone();
        // XX: только если ключ существует
        let _: Option<String> = redis::cmd("SET")
            .arg(self.key(id))
            .arg(json)
            .arg("PX")
            .arg(ttl_millis(ttl))
            .arg("XX")
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), StoreError> {
        let mut conn = self.conn.clone();
        let _: () = conn.del(self.key(id)).await?;
        Ok(())
    }
}
