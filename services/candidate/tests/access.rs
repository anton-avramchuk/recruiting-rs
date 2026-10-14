//! Аутентификация и права на всех маршрутах кандидатов, изоляция организаций, проверка токенов по JWKS.
mod common;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use candidate::{
    MIGRATOR,
    api::{self, AppState},
};
use common::{App, setup};
use platform_auth::{Audience, Claims, KeyPair, Role, TokenSigner, TokenVerifier, jwks};
use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

/// (метод, путь, тело, нужное право)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Needs {
    Read,
    Write,
    Delete,
}

fn endpoints(id: &str) -> Vec<(&'static str, String, Option<Value>, Needs)> {
    vec![
        ("GET", "/persons".into(), None, Needs::Read),
        ("GET", format!("/persons/{id}"), None, Needs::Read),
        (
            "POST",
            "/persons".into(),
            Some(json!({"full_name": "Новый Человек", "confirm_duplicate": true})),
            Needs::Write,
        ),
        (
            "PATCH",
            format!("/persons/{id}"),
            Some(json!({"source": "x"})),
            Needs::Write,
        ),
        (
            "PUT",
            format!("/persons/{id}/employments"),
            Some(json!([])),
            Needs::Write,
        ),
        (
            "PUT",
            format!("/persons/{id}/educations"),
            Some(json!([])),
            Needs::Write,
        ),
        ("DELETE", format!("/persons/{id}"), None, Needs::Delete),
    ]
}

async fn seeded() -> (App, TenantId, String) {
    let app = setup().await;
    let tenant = TenantId::new();
    let owner = app.token(tenant, &[Role::Owner]);
    let (s, p) = app
        .send(
            "POST",
            "/persons",
            Some(&owner),
            Some(json!({"full_name": "Иван Петров"})),
        )
        .await;
    assert_eq!(s, 201);
    (app, tenant, p["id"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn every_route_requires_a_token() {
    let (app, _tenant, id) = seeded().await;
    for (method, path, body, _) in endpoints(&id) {
        let (s, _) = app.send(method, &path, None, body).await;
        assert_eq!(s, 401, "{method} {path}");
    }
    let (s, _) = app.send("GET", "/persons", Some("garbage"), None).await;
    assert_eq!(s, 401);
    app.close().await;
}

#[tokio::test]
async fn permissions_follow_the_roles() {
    let (app, tenant, id) = seeded().await;
    for (role, allowed) in [
        (Role::Viewer, vec![Needs::Read]),
        (Role::Recruiter, vec![Needs::Read, Needs::Write]),
    ] {
        let token = app.token(tenant, &[role]);
        for (method, path, body, needs) in endpoints(&id) {
            let (s, _) = app.send(method, &path, Some(&token), body).await;
            if allowed.contains(&needs) {
                assert!(s < 300, "{role:?} {method} {path} -> {s}");
            } else {
                assert_eq!(s, 403, "{role:?} {method} {path}");
            }
        }
    }
    // владельцу доступно всё, в том числе удаление (последним: оно убирает кандидата)
    let owner = app.token(tenant, &[Role::Owner]);
    for (method, path, body, _) in endpoints(&id) {
        let (s, _) = app.send(method, &path, Some(&owner), body).await;
        assert!(s < 300, "owner {method} {path} -> {s}");
    }
    app.close().await;
}

#[tokio::test]
async fn platform_admin_tokens_do_not_open_tenant_data() {
    let (app, _tenant, id) = seeded().await;
    let token = app.platform_token();
    for (method, path, body, _) in endpoints(&id) {
        let (s, _) = app.send(method, &path, Some(&token), body).await;
        assert_eq!(s, 403, "{method} {path}");
    }
    app.close().await;
}

#[tokio::test]
async fn a_tenant_token_without_a_tenant_is_refused() {
    let (app, _tenant, id) = seeded().await;
    let token = app.tenantless_token();
    for (method, path, body, _) in endpoints(&id) {
        let (s, _) = app.send(method, &path, Some(&token), body).await;
        assert_eq!(s, 403, "{method} {path}");
    }
    app.close().await;
}

#[tokio::test]
async fn another_organization_sees_nothing_on_any_route() {
    let (app, tenant, id) = seeded().await;
    let stranger = app.token(TenantId::new(), &[Role::Owner]);
    for (method, path, body, _) in endpoints(&id) {
        let (s, resp) = app.send(method, &path, Some(&stranger), body).await;
        match (method, path.as_str()) {
            ("GET", "/persons") => {
                assert_eq!((s, resp["total"].as_i64()), (200, Some(0)), "list is empty");
            }
            ("POST", "/persons") => assert_eq!(s, 201, "creating in own organization works"),
            _ => assert_eq!(s, 404, "{method} {path}"),
        }
    }
    // кандидат исходной организации цел, а чужое создание не просочилось
    let owner = app.token(tenant, &[Role::Owner]);
    let (_, page) = app.send("GET", "/persons", Some(&owner), None).await;
    assert_eq!(page["total"], 1);
    app.close().await;
}

// ---------- проверка токенов по ключу identity (JWKS) ----------

#[derive(Clone)]
struct Identity {
    key: Arc<Mutex<KeyPair>>,
    down: Arc<AtomicBool>,
}

impl Identity {
    async fn serve(&self) -> String {
        async fn handler(State(i): State<Identity>) -> Result<Json<Value>, StatusCode> {
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

    fn token(&self, tenant: TenantId) -> String {
        let claims = Claims::new(
            Uuid::new_v4(),
            Some(tenant),
            vec![Role::Owner],
            Audience::Tenant,
            600,
        );
        TokenSigner::new(&self.key.lock().unwrap())
            .sign(&claims)
            .unwrap()
    }
}

async fn call(router: &Router, token: &str) -> u16 {
    let req = axum::http::Request::get("/persons")
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    router.clone().oneshot(req).await.unwrap().status().as_u16()
}

#[tokio::test]
async fn tokens_are_verified_against_the_identity_jwks_and_survive_key_rotation() {
    let db = TestDb::create(&MIGRATOR).await;
    let identity = Identity {
        key: Arc::new(Mutex::new(KeyPair::generate("k1"))),
        down: Arc::default(),
    };
    identity.down.store(true, Ordering::SeqCst); // identity ещё не поднялся
    let url = identity.serve().await;
    let interval = Duration::from_millis(150);
    let verifier = Arc::new(TokenVerifier::remote(&url, Audience::Tenant, interval));
    let router = api::router(AppState::new(db.app.clone(), verifier));
    let tenant = TenantId::new();

    // ключа нет: запросы отклоняются, сервис жив
    assert_eq!(call(&router, &identity.token(tenant)).await, 401);
    let health = router
        .clone()
        .oneshot(
            axum::http::Request::get("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), 200);

    // identity поднялся: ключ подхватывается без перезапуска
    identity.down.store(false, Ordering::SeqCst);
    tokio::time::sleep(interval + Duration::from_millis(60)).await;
    assert_eq!(call(&router, &identity.token(tenant)).await, 200);

    // identity сменил ключ
    *identity.key.lock().unwrap() = KeyPair::generate("k2");
    tokio::time::sleep(interval + Duration::from_millis(60)).await;
    assert_eq!(call(&router, &identity.token(tenant)).await, 200);

    // токен, подписанный посторонним ключом, не принимается
    let stranger = {
        let claims = Claims::new(
            Uuid::new_v4(),
            Some(tenant),
            vec![Role::Owner],
            Audience::Tenant,
            600,
        );
        TokenSigner::new(&KeyPair::generate("evil"))
            .sign(&claims)
            .unwrap()
    };
    assert_eq!(call(&router, &stranger).await, 401);
    db.close().await;
}
