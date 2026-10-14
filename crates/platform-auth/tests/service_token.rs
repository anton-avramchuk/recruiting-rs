use std::sync::Arc;

use axum::{Router, body::Body, extract::FromRef, http::Request, routing::get};
use platform_auth::{ServiceCaller, ServiceToken};
use tower::ServiceExt;

#[derive(Clone)]
struct S(Arc<ServiceToken>);
impl FromRef<S> for Arc<ServiceToken> {
    fn from_ref(s: &S) -> Self {
        s.0.clone()
    }
}

async fn internal(_: ServiceCaller) -> &'static str {
    "ok"
}

async fn call(header: Option<&str>) -> u16 {
    let app = Router::new()
        .route("/i", get(internal))
        .with_state(S(Arc::new(ServiceToken::new("s3cret-service-token"))));
    let mut req = Request::get("/i");
    if let Some(h) = header {
        req = req.header("authorization", h);
    }
    app.oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn only_the_exact_service_token_is_accepted() {
    assert_eq!(call(None).await, 401);
    assert_eq!(call(Some("Bearer wrong")).await, 401);
    assert_eq!(call(Some("Bearer s3cret-service-token-x")).await, 401);
    assert_eq!(call(Some("s3cret-service-token")).await, 401);
    assert_eq!(call(Some("Bearer s3cret-service-token")).await, 200);
}

#[test]
fn empty_token_never_matches() {
    assert!(!ServiceToken::new("").matches(""));
}
