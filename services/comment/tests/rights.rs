mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

struct Team {
    app: App,
    author: String,
    colleague: String,
    owner: String,
    viewer: String,
    target: Uuid,
}

async fn team() -> Team {
    let app = setup().await;
    let tenant = TenantId::new();
    let target = Uuid::new_v4();
    let author = app.user_token(tenant, Uuid::new_v4(), Some("Автор"), &[Role::Recruiter]);
    let colleague = app.user_token(tenant, Uuid::new_v4(), Some("Коллега"), &[Role::Recruiter]);
    let owner = app.user_token(tenant, Uuid::new_v4(), Some("Владелец"), &[Role::Owner]);
    let viewer = app.user_token(tenant, Uuid::new_v4(), Some("Наблюдатель"), &[Role::Viewer]);
    Team {
        app,
        author,
        colleague,
        owner,
        viewer,
        target,
    }
}

impl Team {
    async fn comment(&self, token: &str, text: &str) -> Value {
        let (s, c) = self
            .app
            .send(
                "POST",
                "/comments",
                Some(token),
                Some(json!({"target_type": "client", "target_id": self.target, "body": text})),
            )
            .await;
        assert_eq!(s, 201, "{c}");
        c
    }

    fn feed_uri(&self) -> String {
        format!("/comments?target_type=client&target_id={}", self.target)
    }

    async fn all(&self) -> Vec<Value> {
        let (_, page) = self
            .app
            .send("GET", &self.feed_uri(), Some(&self.author), None)
            .await;
        page["items"].as_array().unwrap().clone()
    }
}

#[tokio::test]
async fn the_author_edits_and_the_edit_time_appears() {
    let t = team().await;
    let c = t.comment(&t.author, "черновик").await;
    let id = c["id"].as_str().unwrap();
    let (s, edited) = t
        .app
        .send(
            "PATCH",
            &format!("/comments/{id}"),
            Some(&t.author),
            Some(json!({"body": " готово "})),
        )
        .await;
    assert_eq!(s, 200, "{edited}");
    assert_eq!(edited["body"], "готово");
    assert!(edited["edited_at"].as_str().is_some());
    assert_eq!(edited["created_at"], c["created_at"]);

    let (s, p) = t
        .app
        .send(
            "PATCH",
            &format!("/comments/{id}"),
            Some(&t.author),
            Some(json!({"body": "  "})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("body")));
    assert_eq!(t.all().await[0]["body"], "готово");
    t.app.close().await;
}

#[tokio::test]
async fn a_colleague_cannot_touch_it_and_the_owner_can_only_delete() {
    let t = team().await;
    let c = t.comment(&t.author, "мой комментарий").await;
    let id = c["id"].as_str().unwrap();
    let uri = format!("/comments/{id}");

    let (s, _) = t
        .app
        .send(
            "PATCH",
            &uri,
            Some(&t.colleague),
            Some(json!({"body": "подмена"})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = t.app.send("DELETE", &uri, Some(&t.colleague), None).await;
    assert_eq!(s, 403);
    // владелец чужой комментарий править не может
    let (s, _) = t
        .app
        .send(
            "PATCH",
            &uri,
            Some(&t.owner),
            Some(json!({"body": "подмена"})),
        )
        .await;
    assert_eq!(s, 403);
    assert_eq!(t.all().await[0]["body"], "мой комментарий");

    // но удалить может
    let (s, _) = t.app.send("DELETE", &uri, Some(&t.owner), None).await;
    assert_eq!(s, 204);
    assert!(t.all().await.is_empty());
    let (s, _) = t.app.send("DELETE", &uri, Some(&t.owner), None).await;
    assert_eq!(s, 404);
    t.app.close().await;
}

#[tokio::test]
async fn the_author_deletes_their_own_comment() {
    let t = team().await;
    let c = t.comment(&t.author, "лишнее").await;
    let (s, _) = t
        .app
        .send(
            "DELETE",
            &format!("/comments/{}", c["id"].as_str().unwrap()),
            Some(&t.author),
            None,
        )
        .await;
    assert_eq!(s, 204);
    assert!(t.all().await.is_empty());
    t.app.close().await;
}

#[tokio::test]
async fn a_viewer_reads_but_cannot_write() {
    let t = team().await;
    let c = t.comment(&t.author, "видно всем").await;
    let id = c["id"].as_str().unwrap();
    let (s, page) = t
        .app
        .send("GET", &t.feed_uri(), Some(&t.viewer), None)
        .await;
    assert_eq!(s, 200);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);

    let (s, _) = t
        .app
        .send(
            "POST",
            "/comments",
            Some(&t.viewer),
            Some(json!({"target_type": "client", "target_id": t.target, "body": "x"})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = t
        .app
        .send(
            "PATCH",
            &format!("/comments/{id}"),
            Some(&t.viewer),
            Some(json!({"body": "x"})),
        )
        .await;
    assert_eq!(s, 403);
    let (s, _) = t
        .app
        .send("DELETE", &format!("/comments/{id}"), Some(&t.viewer), None)
        .await;
    assert_eq!(s, 403);
    t.app.close().await;
}

#[tokio::test]
async fn without_a_token_or_a_tenant_nothing_is_open() {
    let t = team().await;
    for (method, uri) in [
        ("GET", t.feed_uri()),
        ("POST", "/comments".to_string()),
        ("PATCH", format!("/comments/{}", Uuid::new_v4())),
        ("DELETE", format!("/comments/{}", Uuid::new_v4())),
    ] {
        let (s, _) = t.app.send(method, &uri, None, Some(json!({}))).await;
        assert_eq!(s, 401, "{method} {uri}");
    }
    let (s, _) = t
        .app
        .send("GET", &t.feed_uri(), Some(&t.app.tenantless_token()), None)
        .await;
    assert_eq!(s, 403);
    let (s, _) = t
        .app
        .send("GET", &t.feed_uri(), Some(&t.app.platform_token()), None)
        .await;
    assert_eq!(s, 403);
    t.app.close().await;
}
