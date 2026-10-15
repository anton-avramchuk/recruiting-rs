mod common;

use common::setup;

const EXPECTED: &[(&str, &str)] = &[
    ("/persons", "get"),
    ("/persons", "post"),
    ("/persons/{id}", "get"),
    ("/persons/{id}", "patch"),
    ("/persons/{id}", "delete"),
    ("/persons/{id}/employments", "put"),
    ("/persons/{id}/educations", "put"),
    ("/internal/tenants/{tenant_id}/person-changes", "get"),
    ("/health", "get"),
];

#[tokio::test]
async fn openapi_document_lists_every_route_and_schema() {
    let app = setup().await;
    let (status, doc) = app.send("GET", "/openapi.json", None, None).await;
    assert_eq!(status, 200);
    assert!(doc["openapi"].as_str().unwrap().starts_with("3."));
    for (path, method) in EXPECTED {
        assert!(
            doc["paths"][path][method].is_object(),
            "missing {method} {path}"
        );
    }
    for schema in [
        "PersonDto",
        "PersonPage",
        "CreatePersonRequest",
        "UpdatePersonRequest",
        "ProblemDetails",
    ] {
        assert!(
            doc["components"]["schemas"][schema].is_object(),
            "missing schema {schema}"
        );
    }
    assert!(doc["components"]["securitySchemes"]["bearer"].is_object());
    app.close().await;
}
