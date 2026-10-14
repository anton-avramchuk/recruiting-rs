mod common;

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use common::{FakeDirectory, setup_with};
use identity::{
    application::tenants::{TenantDirectory, TenantStatus},
    infrastructure::directory::{CachedDirectory, HttpTenantDirectory},
};
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "svc-secret";

#[derive(Clone, Default)]
struct MockAdmin {
    tenants: Arc<Mutex<HashMap<String, (Uuid, String)>>>,
    calls: Arc<AtomicUsize>,
    broken: Arc<AtomicBool>,
}

impl MockAdmin {
    fn add(&self, slug: &str, status: &str) -> TenantId {
        let id = Uuid::new_v4();
        self.tenants
            .lock()
            .unwrap()
            .insert(slug.into(), (id, status.into()));
        TenantId(id)
    }
    fn set_status(&self, slug: &str, status: &str) {
        self.tenants.lock().unwrap().get_mut(slug).unwrap().1 = status.into();
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    async fn serve(&self) -> String {
        async fn by_slug(
            State(m): State<MockAdmin>,
            h: HeaderMap,
            Path(slug): Path<String>,
        ) -> Result<Json<Value>, StatusCode> {
            guard(&m, &h)?;
            let t = m.tenants.lock().unwrap();
            t.get(&slug)
                .map(|(id, st)| Json(json!({"id": id, "slug": slug, "status": st})))
                .ok_or(StatusCode::NOT_FOUND)
        }
        async fn by_id(
            State(m): State<MockAdmin>,
            h: HeaderMap,
            Path(id): Path<Uuid>,
        ) -> Result<Json<Value>, StatusCode> {
            guard(&m, &h)?;
            let t = m.tenants.lock().unwrap();
            t.iter()
                .find(|(_, (tid, _))| *tid == id)
                .map(|(slug, (id, st))| Json(json!({"id": id, "slug": slug, "status": st})))
                .ok_or(StatusCode::NOT_FOUND)
        }
        fn guard(m: &MockAdmin, h: &HeaderMap) -> Result<(), StatusCode> {
            m.calls.fetch_add(1, Ordering::SeqCst);
            if h.get("authorization").and_then(|v| v.to_str().ok())
                != Some(&format!("Bearer {TOKEN}"))
            {
                return Err(StatusCode::UNAUTHORIZED);
            }
            if m.broken.load(Ordering::SeqCst) {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Ok(())
        }
        let app = Router::new()
            .route("/internal/tenants/by-slug/{slug}", get(by_slug))
            .route("/internal/tenants/{id}", get(by_id))
            .with_state(self.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }
}

async fn directory(admin: &MockAdmin, ttl: Duration) -> CachedDirectory<HttpTenantDirectory> {
    CachedDirectory::new(HttpTenantDirectory::new(admin.serve().await, TOKEN), ttl)
}

#[tokio::test]
async fn resolves_known_and_unknown_tenants() {
    let admin = MockAdmin::default();
    let id = admin.add("acme", "active");
    admin.add("sleepy", "suspended");
    admin.add("half", "provisioning");
    let dir = directory(&admin, Duration::from_secs(30)).await;

    let t = dir.resolve("acme").await.unwrap().unwrap();
    assert_eq!((t.id, t.status), (id, TenantStatus::Active));
    assert_eq!(
        dir.resolve("sleepy").await.unwrap().unwrap().status,
        TenantStatus::Suspended
    );
    assert!(dir.resolve("nope").await.unwrap().is_none());
    // тенант, чьё создание не завершено, для identity не существует
    assert!(dir.resolve("half").await.unwrap().is_none());
    assert_eq!(dir.status(id).await.unwrap(), Some(TenantStatus::Active));
}

#[tokio::test]
async fn upstream_failure_is_an_error_not_a_default() {
    let admin = MockAdmin::default();
    admin.add("acme", "active");
    admin.broken.store(true, Ordering::SeqCst);
    let dir = directory(&admin, Duration::from_secs(30)).await;
    assert!(dir.resolve("acme").await.is_err());
}

#[tokio::test]
async fn wrong_service_token_is_an_error() {
    let admin = MockAdmin::default();
    admin.add("acme", "active");
    let dir = CachedDirectory::new(
        HttpTenantDirectory::new(admin.serve().await, "wrong"),
        Duration::from_secs(30),
    );
    assert!(dir.resolve("acme").await.is_err());
}

#[tokio::test]
async fn answers_are_cached_for_the_ttl_only() {
    let admin = MockAdmin::default();
    admin.add("acme", "active");
    let dir = directory(&admin, Duration::from_millis(150)).await;

    dir.resolve("acme").await.unwrap();
    dir.resolve("acme").await.unwrap();
    assert_eq!(admin.calls(), 1, "second call must be served from cache");

    tokio::time::sleep(Duration::from_millis(200)).await;
    dir.resolve("acme").await.unwrap();
    assert_eq!(admin.calls(), 2, "entry expires after ttl");
}

#[tokio::test]
async fn suspension_blocks_login_and_refresh_once_the_cache_expires() {
    let admin = MockAdmin::default();
    let tid = admin.add("acme", "active");
    let dir = directory(&admin, Duration::from_millis(150)).await;
    let ctx = setup_with(Arc::new(dir), FakeDirectory::default()).await;
    ctx.seed_user(tid, "anna@acme.io", "s3cret-pass", &["owner"])
        .await;
    let login = json!({"tenant":"acme","email":"anna@acme.io","password":"s3cret-pass"});

    let (s, tokens) = ctx.post("/auth/login", login.clone()).await;
    assert_eq!(s, 200);

    admin.set_status("acme", "suspended");
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(ctx.post("/auth/login", login.clone()).await.0, 403);
    let refresh = json!({"refresh_token": tokens["refresh_token"]});
    assert_eq!(ctx.post("/auth/refresh", refresh.clone()).await.0, 403);

    // активация возвращает доступ (токен остался действительным: приостановка не отзывает его)
    admin.set_status("acme", "active");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(ctx.post("/auth/login", login).await.0, 200);
    assert_eq!(ctx.post("/auth/refresh", refresh).await.0, 200);
    ctx.close().await;
}
