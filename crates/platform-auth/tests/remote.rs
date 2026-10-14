//! Проверка токенов по ключу, опубликованному другим сервисом (JWKS).
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use platform_auth::{Audience, AuthError, Claims, KeyPair, Role, TokenSigner, TokenVerifier, jwks};
use platform_core::tenant::TenantId;
use serde_json::Value;
use uuid::Uuid;

const INTERVAL: Duration = Duration::from_millis(150);

#[derive(Clone)]
struct Identity {
    key: Arc<Mutex<KeyPair>>,
    fetches: Arc<AtomicUsize>,
    down: Arc<AtomicBool>,
}

impl Identity {
    fn new() -> Self {
        Self {
            key: Arc::new(Mutex::new(KeyPair::generate("k1"))),
            fetches: Arc::default(),
            down: Arc::default(),
        }
    }

    fn rotate(&self) {
        *self.key.lock().unwrap() = KeyPair::generate("k2");
    }

    fn token(&self, aud: Audience, ttl: i64) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            Some(TenantId::new()),
            vec![Role::Owner],
            aud,
            ttl,
        );
        TokenSigner::new(&self.key.lock().unwrap())
            .sign(&claims)
            .unwrap()
    }

    async fn serve(&self) -> String {
        async fn handler(State(i): State<Identity>) -> Result<Json<Value>, StatusCode> {
            i.fetches.fetch_add(1, Ordering::SeqCst);
            if i.down.load(Ordering::SeqCst) {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            Ok(Json(jwks::to_jwks(&i.key.lock().unwrap().public_key())))
        }
        let app = Router::new()
            .route("/.well-known/jwks.json", get(handler))
            .with_state(self.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/.well-known/jwks.json")
    }
}

fn verifier(url: &str) -> TokenVerifier {
    TokenVerifier::remote(url, Audience::Tenant, INTERVAL)
}

#[tokio::test]
async fn accepts_tokens_signed_by_the_published_key() {
    let id = Identity::new();
    let v = verifier(&id.serve().await);
    let claims = v
        .verify_async(&id.token(Audience::Tenant, 600))
        .await
        .unwrap();
    assert_eq!(claims.roles, vec![Role::Owner]);
    // ключ загружен один раз и переиспользуется
    v.verify_async(&id.token(Audience::Tenant, 600))
        .await
        .unwrap();
    assert_eq!(id.fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn survives_a_key_rotation_without_restart() {
    let id = Identity::new();
    let v = verifier(&id.serve().await);
    v.verify_async(&id.token(Audience::Tenant, 600))
        .await
        .unwrap();

    id.rotate();
    tokio::time::sleep(INTERVAL + Duration::from_millis(50)).await;
    let claims = v.verify_async(&id.token(Audience::Tenant, 600)).await;
    assert!(claims.is_ok(), "new key is picked up: {claims:?}");
    assert_eq!(id.fetches.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn foreign_signatures_cannot_cause_a_refetch_storm() {
    let id = Identity::new();
    let v = verifier(&id.serve().await);
    v.verify_async(&id.token(Audience::Tenant, 600))
        .await
        .unwrap();

    let stranger = TokenSigner::new(&KeyPair::generate("evil"))
        .sign(&Claims::new(
            Uuid::new_v4(),
            None,
            vec![Role::Owner],
            Audience::Tenant,
            600,
        ))
        .unwrap();
    for _ in 0..25 {
        assert!(v.verify_async(&stranger).await.is_err());
    }
    assert!(
        id.fetches.load(Ordering::SeqCst) <= 2,
        "at most one extra fetch inside the interval, got {}",
        id.fetches.load(Ordering::SeqCst)
    );
}

#[tokio::test]
async fn unreachable_identity_rejects_requests_and_recovers_later() {
    let id = Identity::new();
    id.down.store(true, Ordering::SeqCst);
    let v = verifier(&id.serve().await);
    let token = id.token(Audience::Tenant, 600);

    assert!(
        v.verify_async(&token).await.is_err(),
        "no key yet -> reject"
    );

    id.down.store(false, Ordering::SeqCst);
    tokio::time::sleep(INTERVAL + Duration::from_millis(50)).await;
    assert!(
        v.verify_async(&token).await.is_ok(),
        "key picked up after recovery"
    );
}

#[tokio::test]
async fn wrong_audience_and_expired_tokens_do_not_trigger_refetches() {
    let id = Identity::new();
    let v = verifier(&id.serve().await);
    v.verify_async(&id.token(Audience::Tenant, 600))
        .await
        .unwrap();
    tokio::time::sleep(INTERVAL + Duration::from_millis(50)).await;

    assert!(matches!(
        v.verify_async(&id.token(Audience::Platform, 600)).await,
        Err(AuthError::WrongAudience)
    ));
    assert!(
        v.verify_async(&id.token(Audience::Tenant, -300))
            .await
            .is_err()
    );
    assert_eq!(id.fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fixed_key_verifier_still_works_through_the_async_api() {
    let kp = KeyPair::generate("k");
    let v = TokenVerifier::new(&kp.public_key(), Audience::Tenant);
    let t = TokenSigner::new(&kp)
        .sign(&Claims::new(
            Uuid::new_v4(),
            None,
            vec![],
            Audience::Tenant,
            600,
        ))
        .unwrap();
    assert!(v.verify_async(&t).await.is_ok());
}
