use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Ошибка API в формате RFC 9457 `application/problem+json`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApiError {
    #[serde(skip)]
    status: StatusCode,
    title: String,
    #[serde(rename = "status")]
    status_code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl ApiError {
    pub fn new(status: StatusCode, title: impl Into<String>) -> Self {
        Self {
            status,
            title: title.into(),
            status_code: status.as_u16(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Bad Request").with_detail(detail)
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Unauthorized")
    }

    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "Forbidden")
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not Found")
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "Conflict").with_detail(detail)
    }

    /// Внутренняя ошибка: подробности только в логе, клиенту — без деталей.
    pub fn internal(source: &dyn std::error::Error) -> Self {
        tracing::error!(error = %source, "internal error");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::to_vec(&self).expect("problem serializes");
        (
            self.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            body,
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_json(res: Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn renders_problem_json() {
        let res = ApiError::conflict("slug taken").into_response();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        assert_eq!(
            res.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        let v = body_json(res).await;
        assert_eq!(v["status"], 409);
        assert_eq!(v["title"], "Conflict");
        assert_eq!(v["detail"], "slug taken");
    }

    #[tokio::test]
    async fn omits_detail_when_absent() {
        let v = body_json(ApiError::unauthorized().into_response()).await;
        assert_eq!(v["status"], 401);
        assert!(v.get("detail").is_none());
    }

    #[tokio::test]
    async fn internal_error_hides_source() {
        let src = std::io::Error::other("secret db password");
        let res = ApiError::internal(&src).into_response();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v = body_json(res).await;
        assert!(!v.to_string().contains("secret"));
    }
}
