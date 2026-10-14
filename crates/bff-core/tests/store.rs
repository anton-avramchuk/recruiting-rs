//! Контракт хранилища сессий: одинаковое поведение для памяти и Redis.
use std::time::Duration;

use bff_core::{MemorySessionStore, RedisSessionStore, Session, SessionStore};

fn session(tag: &str) -> Session {
    Session {
        access_token: format!("access-{tag}"),
        refresh_token: Some(format!("refresh-{tag}")),
        access_expires_at: 1_900_000_000,
        csrf_token: format!("csrf-{tag}"),
    }
}

async fn contract(store: &dyn SessionStore) {
    let ttl = Duration::from_secs(60);

    // create -> get
    let id = store.create(&session("a"), ttl).await.unwrap();
    assert!(id.len() >= 32, "session id must be long and random");
    assert_eq!(store.get(&id).await.unwrap(), Some(session("a")));

    // идентификаторы уникальны
    let id2 = store.create(&session("a"), ttl).await.unwrap();
    assert_ne!(id, id2);

    // save перезаписывает
    store.save(&id, &session("b"), ttl).await.unwrap();
    assert_eq!(store.get(&id).await.unwrap(), Some(session("b")));
    assert_eq!(store.get(&id2).await.unwrap(), Some(session("a")));

    // delete
    store.delete(&id).await.unwrap();
    assert_eq!(store.get(&id).await.unwrap(), None);
    store.delete(&id).await.unwrap(); // идемпотентно

    // неизвестный id
    assert_eq!(store.get("nope").await.unwrap(), None);

    // save не воскрешает удалённую сессию
    store.save(&id, &session("c"), ttl).await.unwrap_or(());
    assert_eq!(store.get(&id).await.unwrap(), None);
}

async fn expiry(store: &dyn SessionStore) {
    let id = store
        .create(&session("e"), Duration::from_millis(300))
        .await
        .unwrap();
    assert!(store.get(&id).await.unwrap().is_some());
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        store.get(&id).await.unwrap(),
        None,
        "expired session is gone"
    );
}

fn redis_url() -> String {
    std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into())
}

#[tokio::test]
async fn memory_store_satisfies_the_contract() {
    let store = MemorySessionStore::new();
    contract(&store).await;
    expiry(&store).await;
}

#[tokio::test]
async fn redis_store_satisfies_the_contract() {
    let prefix = format!("test:{}:", uuid::Uuid::new_v4().simple());
    let store = RedisSessionStore::connect(&redis_url(), &prefix)
        .await
        .expect("redis is up (docker compose up -d)");
    contract(&store).await;
    expiry(&store).await;
}

#[tokio::test]
async fn redis_prefixes_isolate_sessions() {
    let url = redis_url();
    let a = RedisSessionStore::connect(
        &url,
        &format!("test:{}:crm:", uuid::Uuid::new_v4().simple()),
    )
    .await
    .unwrap();
    let b = RedisSessionStore::connect(
        &url,
        &format!("test:{}:adm:", uuid::Uuid::new_v4().simple()),
    )
    .await
    .unwrap();
    let id = a
        .create(&session("x"), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(a.get(&id).await.unwrap().is_some());
    assert_eq!(
        b.get(&id).await.unwrap(),
        None,
        "other layer cannot see the session"
    );
}
