mod common;

use common::setup;

#[tokio::test]
async fn openapi_document_lists_every_route_and_schema() {
    let app = setup().await;
    let (status, doc) = app.send("GET", "/openapi.json", None, None).await;
    assert_eq!(status, 200);
    for (path, method) in [
        ("/comments", "get"),
        ("/comments", "post"),
        ("/comments/{id}", "patch"),
        ("/comments/{id}", "delete"),
        ("/health", "get"),
    ] {
        assert!(
            doc["paths"][path][method].is_object(),
            "missing {method} {path}"
        );
    }
    for schema in [
        "CommentDto",
        "CommentPage",
        "CreateCommentRequest",
        "ProblemDetails",
    ] {
        assert!(
            doc["components"]["schemas"][schema].is_object(),
            "missing schema {schema}"
        );
    }
    app.close().await;
}
