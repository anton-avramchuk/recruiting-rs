use std::sync::Arc;

use axum::{Router, body::Body, extract::FromRef, http::Request, routing::get};
use platform_auth::{
    Audience, AuthUser, Claims, KeyPair, Permission, Role, TokenSigner, TokenVerifier, jwks,
};
use platform_core::tenant::TenantId;
use tower::ServiceExt;
use uuid::Uuid;

fn claims(aud: Audience, roles: Vec<Role>, ttl_secs: i64) -> Claims {
    Claims::new(Uuid::new_v4(), Some(TenantId::new()), roles, aud, ttl_secs)
}

#[test]
fn signed_token_roundtrips() {
    let kp = KeyPair::generate("k1");
    let signer = TokenSigner::new(&kp);
    let verifier = TokenVerifier::new(&kp.public_key(), Audience::Tenant);
    let c = claims(Audience::Tenant, vec![Role::Recruiter], 600);
    let token = signer.sign(&c).unwrap();
    let got = verifier.verify(&token).unwrap();
    assert_eq!(got.sub, c.sub);
    assert_eq!(got.tid, c.tid);
    assert_eq!(got.roles, vec![Role::Recruiter]);
}

#[test]
fn expired_token_is_rejected() {
    let kp = KeyPair::generate("k1");
    let token = TokenSigner::new(&kp)
        .sign(&claims(Audience::Tenant, vec![], -120))
        .unwrap();
    assert!(
        TokenVerifier::new(&kp.public_key(), Audience::Tenant)
            .verify(&token)
            .is_err()
    );
}

#[test]
fn wrong_audience_is_rejected() {
    let kp = KeyPair::generate("k1");
    let token = TokenSigner::new(&kp)
        .sign(&claims(Audience::Tenant, vec![], 600))
        .unwrap();
    assert!(
        TokenVerifier::new(&kp.public_key(), Audience::Platform)
            .verify(&token)
            .is_err()
    );
}

#[test]
fn foreign_signature_is_rejected() {
    let (a, b) = (KeyPair::generate("k1"), KeyPair::generate("k1"));
    let token = TokenSigner::new(&a)
        .sign(&claims(Audience::Tenant, vec![], 600))
        .unwrap();
    assert!(
        TokenVerifier::new(&b.public_key(), Audience::Tenant)
            .verify(&token)
            .is_err()
    );
}

#[test]
fn jwks_roundtrips_public_key() {
    let kp = KeyPair::generate("k1");
    let doc = jwks::to_jwks(&kp.public_key());
    assert_eq!(doc["keys"][0]["kty"], "OKP");
    assert_eq!(doc["keys"][0]["kid"], "k1");
    let pk = jwks::from_jwks(&doc).unwrap();
    let token = TokenSigner::new(&kp)
        .sign(&claims(Audience::Tenant, vec![], 600))
        .unwrap();
    assert!(
        TokenVerifier::new(&pk, Audience::Tenant)
            .verify(&token)
            .is_ok()
    );
}

#[test]
fn permissions_follow_roles() {
    assert!(Role::Owner.permissions().contains(&Permission::ManageUsers));
    assert!(
        !Role::Viewer
            .permissions()
            .contains(&Permission::ManageUsers)
    );
    assert!(
        Role::Recruiter
            .permissions()
            .contains(&Permission::WriteData)
    );
    assert!(!Role::Viewer.permissions().contains(&Permission::WriteData));
    assert!(Role::Viewer.permissions().contains(&Permission::ReadData));
    assert!(
        Role::PlatformAdmin
            .permissions()
            .contains(&Permission::ManageTenants)
    );
    assert!(
        !Role::Owner
            .permissions()
            .contains(&Permission::ManageTenants)
    );
}

#[derive(Clone)]
struct AppState {
    verifier: Arc<TokenVerifier>,
}
impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(s: &AppState) -> Self {
        s.verifier.clone()
    }
}

async fn manage_users(user: AuthUser) -> Result<&'static str, platform_core::error::ApiError> {
    user.require(Permission::ManageUsers)?;
    Ok("ok")
}

fn app(verifier: TokenVerifier) -> Router {
    Router::new()
        .route("/manage", get(manage_users))
        .with_state(AppState {
            verifier: Arc::new(verifier),
        })
}

async fn call(app: Router, bearer: Option<&str>) -> u16 {
    let mut req = Request::get("/manage");
    if let Some(t) = bearer {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    app.oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn extractor_enforces_authentication_and_permissions() {
    let kp = KeyPair::generate("k1");
    let signer = TokenSigner::new(&kp);
    let mk = || app(TokenVerifier::new(&kp.public_key(), Audience::Tenant));

    assert_eq!(call(mk(), None).await, 401);
    assert_eq!(call(mk(), Some("garbage")).await, 401);

    let viewer = signer
        .sign(&claims(Audience::Tenant, vec![Role::Viewer], 600))
        .unwrap();
    assert_eq!(call(mk(), Some(&viewer)).await, 403);

    let owner = signer
        .sign(&claims(Audience::Tenant, vec![Role::Owner], 600))
        .unwrap();
    assert_eq!(call(mk(), Some(&owner)).await, 200);
}

#[tokio::test]
async fn jwks_endpoint_serves_public_key() {
    use http_body_util::BodyExt;
    let kp = KeyPair::generate("k9");
    let res = jwks::router::<()>(&kp.public_key())
        .oneshot(
            Request::get("/.well-known/jwks.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(jwks::from_jwks(&doc).unwrap(), kp.public_key());
}

#[test]
fn private_key_survives_base64_roundtrip() {
    let kp = KeyPair::generate("k1");
    let restored = KeyPair::from_pkcs8_base64("k1", &kp.to_pkcs8_base64()).unwrap();
    assert_eq!(restored.public_key(), kp.public_key());
}

#[test]
fn wrong_audience_is_reported_distinctly_from_a_bad_signature() {
    let (a, b) = (KeyPair::generate("k1"), KeyPair::generate("k2"));
    let tenant_token = TokenSigner::new(&a)
        .sign(&claims(Audience::Tenant, vec![], 600))
        .unwrap();
    let verifier = TokenVerifier::new(&a.public_key(), Audience::Platform);
    assert!(matches!(
        verifier.verify(&tenant_token),
        Err(platform_auth::AuthError::WrongAudience)
    ));
    let foreign = TokenSigner::new(&b)
        .sign(&claims(Audience::Tenant, vec![], 600))
        .unwrap();
    assert!(matches!(
        verifier.verify(&foreign),
        Err(platform_auth::AuthError::Invalid(_))
    ));
}

#[tokio::test]
async fn tenant_token_on_platform_endpoint_is_forbidden() {
    let kp = KeyPair::generate("k1");
    let tenant_token = TokenSigner::new(&kp)
        .sign(&claims(Audience::Tenant, vec![Role::Owner], 600))
        .unwrap();
    let platform_app = app(TokenVerifier::new(&kp.public_key(), Audience::Platform));
    assert_eq!(call(platform_app, Some(&tenant_token)).await, 403);
}
