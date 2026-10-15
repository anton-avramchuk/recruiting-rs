mod common;

use common::setup;

const EXPECTED: &[(&str, &str)] = &[
    ("/clients", "get"),
    ("/clients", "post"),
    ("/clients/{id}", "get"),
    ("/clients/{id}", "patch"),
    ("/clients/{id}/contacts", "post"),
    ("/contacts/{id}", "patch"),
    ("/contacts/{id}", "delete"),
    ("/positions", "get"),
    ("/positions", "post"),
    ("/positions/{id}", "patch"),
    ("/positions/{id}", "delete"),
    ("/internal/tenants/{tenant_id}/client-changes", "get"),
    ("/health", "get"),
];

#[tokio::test]
async fn openapi_document_lists_every_route_and_schema() {
    let app = setup().await;
    let (status, doc) = app.send("GET", "/openapi.json", None, None).await;
    assert_eq!(status, 200);
    for (path, method) in EXPECTED {
        assert!(
            doc["paths"][path][method].is_object(),
            "missing {method} {path}"
        );
    }
    for schema in [
        "ClientDto",
        "ClientDetailDto",
        "ContactDto",
        "PositionDto",
        "ClientChangesPage",
        "ProblemDetails",
    ] {
        assert!(
            doc["components"]["schemas"][schema].is_object(),
            "missing schema {schema}"
        );
    }
    app.close().await;
}
