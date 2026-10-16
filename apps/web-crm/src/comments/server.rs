//! Вызовы сервиса комментариев через BFF.
use comment_contracts::{CommentDto, CommentPage, CreateCommentRequest, UpdateCommentRequest};
use leptos::prelude::*;
use ui_kit::api::Method;

use crate::{
    candidates::server::uuid_like,
    clients::server::classify_empty,
    funnel::server::{Reply, classify, typed},
};

fn valid(id: &str) -> bool {
    id.parse::<uuid_like::Id>().is_ok()
}

/// Лента объекта: первые `limit` комментариев либо продолжение после курсора.
pub async fn list_comments(
    target_type: String,
    target_id: String,
    after: Option<String>,
    limit: i64,
) -> Result<Reply<CommentPage>, ServerFnError> {
    use ui_kit::encode_query_value as enc;

    if !valid(&target_id) || !["client", "vacancy", "person"].contains(&target_type.as_str()) {
        return Ok(Reply::NotFound);
    }
    let mut path = format!(
        "/api/comments?target_type={target_type}&target_id={target_id}&limit={}",
        limit.clamp(1, 200)
    );
    if let Some(cursor) = after {
        path.push_str(&format!("&after={}", enc(&cursor)));
    }
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_comment(
    target_type: String,
    target_id: String,
    body: String,
) -> Result<Reply<CommentDto>, ServerFnError> {
    if !valid(&target_id) {
        return Ok(Reply::NotFound);
    }
    let Ok(target_uuid) = target_id.parse() else {
        return Ok(Reply::NotFound);
    };
    let request = CreateCommentRequest {
        target_type,
        target_id: target_uuid,
        body,
    };
    let body = serde_json::to_value(&request).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply = ui_kit::api::call(Method::Post, "/api/comments", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn edit_comment(id: String, body: String) -> Result<Reply<CommentDto>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let request = UpdateCommentRequest { body };
    let body = serde_json::to_value(&request).map_err(|e| ServerFnError::new(e.to_string()))?;
    let reply =
        ui_kit::api::call(Method::Patch, &format!("/api/comments/{id}"), Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn delete_comment(id: String) -> Result<Reply<()>, ServerFnError> {
    if !valid(&id) {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Delete, &format!("/api/comments/{id}"), None).await?;
    Ok(classify_empty(reply.status, &reply.body))
}
