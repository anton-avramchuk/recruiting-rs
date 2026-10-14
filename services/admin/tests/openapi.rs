mod common;

use common::setup;
use serde_json::Value;

const EXPECTED: &[(&str, &str)] = &[
    ("/auth/login", "post"),
    ("/me", "get"),
    ("/tenants", "get"),
    ("/tenants", "post"),
    ("/tenants/{id}", "get"),
    ("/tenants/{id}/suspend", "post"),
    ("/tenants/{id}/activate", "post"),
    ("/internal/tenants/by-slug/{slug}", "get"),
    ("/internal/tenants/{id}", "get"),
    ("/health", "get"),
];

#[tokio::test]
async fn openapi_document_lists_every_business_route() {
    let ctx = setup().await;
    let (status, doc) = ctx.send("GET", "/openapi.json", None, Value::Null).await;
    assert_eq!(status, 200);
    assert!(doc["openapi"].as_str().unwrap().starts_with("3."));
    for (path, method) in EXPECTED {
        assert!(
            doc["paths"][path][method].is_object(),
            "missing {method} {path}"
        );
    }
    assert!(doc["components"]["schemas"]["TenantDto"].is_object());
    assert!(doc["components"]["schemas"]["ProblemDetails"].is_object());
    ctx.close().await;
}
