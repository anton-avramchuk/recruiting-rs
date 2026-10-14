use utoipa::OpenApi;

use crate::bff::{__path_login, __path_logout, __path_session, login, logout, session};

#[derive(OpenApi)]
#[openapi(paths(login, logout, session))]
struct BffDoc;

/// OpenAPI-документ служебных маршрутов BFF (`/bff/*`) под названием слоя.
pub fn openapi(title: &str) -> utoipa::openapi::OpenApi {
    let mut doc = BffDoc::openapi();
    doc.info.title = title.to_string();
    doc.info.version = "0.1.0".to_string();
    doc
}
