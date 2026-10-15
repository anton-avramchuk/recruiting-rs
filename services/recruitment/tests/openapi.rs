mod common;

use common::setup;

const EXPECTED: &[(&str, &str)] = &[
    ("/vacancies", "get"),
    ("/vacancies", "post"),
    ("/vacancies/{id}", "get"),
    ("/vacancies/{id}", "patch"),
    ("/applications", "get"),
    ("/applications", "post"),
    ("/applications/bulk", "post"),
    ("/applications/batch", "post"),
    ("/applications/{id}", "get"),
    ("/applications/{id}/stage", "post"),
    ("/applications/{id}/next", "post"),
    ("/applications/{id}/reject", "post"),
    ("/applications/{id}/interviews", "post"),
    ("/applications/{id}/handoff", "post"),
    ("/overview", "get"),
    ("/interviews", "get"),
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
        "VacancyDto",
        "ApplicationDto",
        "ApplicationPage",
        "ApplicationDetailDto",
        "CreateVacancyRequest",
        "BulkRequest",
        "OverviewDto",
        "InterviewItemDto",
        "Stage",
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
