//! Список откликов: фильтр по заказчику и поиск по названию заказчика.
mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

struct World {
    app: App,
    tenant: TenantId,
    token: String,
    acme: Uuid,
    globex: Uuid,
}

async fn world() -> World {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let acme = app.add_client(tenant, "Acme Corp", false).await;
    let globex = app.add_client(tenant, "Globex", false).await;
    let go = app
        .add_vacancy(&token, json!({"title": "Go", "client_id": acme}))
        .await;
    let qa = app
        .add_vacancy(&token, json!({"title": "QA", "client_id": globex}))
        .await;
    for (name, vacancy) in [("Анна", &go), ("Борис", &go), ("Вера", &qa)] {
        let person = app.add_person(tenant, name, &[]).await;
        app.add_application(&token, person, vacancy).await;
    }
    World {
        app,
        tenant,
        token,
        acme,
        globex,
    }
}

impl World {
    async fn list(&self, query: &str) -> (u16, Value) {
        self.app
            .send(
                "GET",
                &format!("/applications{query}"),
                Some(&self.token),
                None,
            )
            .await
    }
}

fn names(page: &Value) -> Vec<String> {
    let mut v: Vec<String> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["full_name"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn the_list_can_be_narrowed_to_one_client() {
    let w = world().await;
    let (s, page) = w.list(&format!("?client_id={}", w.acme)).await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Анна", "Борис"]);
    assert_eq!(page["total"], 2);

    let (_, page) = w.list(&format!("?client_id={}", w.globex)).await;
    assert_eq!(names(&page), ["Вера"]);
    assert_eq!(page["items"][0]["client_name"], "Globex");

    let (_, all) = w.list("").await;
    assert_eq!(all["total"], 3);
    w.app.close().await;
}

#[tokio::test]
async fn the_client_combines_with_other_filters() {
    let w = world().await;
    // Борис ушёл на скрининг
    let (_, page) = w.list(&format!("?client_id={}", w.acme)).await;
    let boris = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["full_name"] == "Борис")
        .unwrap();
    let (s, _) = w
        .app
        .send(
            "POST",
            &format!("/applications/{}/stage", boris["id"].as_str().unwrap()),
            Some(&w.token),
            Some(json!({"stage": "screening"})),
        )
        .await;
    assert_eq!(s, 200);

    let (_, both) = w
        .list(&format!("?client_id={}&stage=screening", w.acme))
        .await;
    assert_eq!(names(&both), ["Борис"]);
    let (_, none) = w
        .list(&format!("?client_id={}&stage=screening", w.globex))
        .await;
    assert_eq!(none["total"], 0);
    w.app.close().await;
}

#[tokio::test]
async fn the_text_search_also_matches_the_client_name() {
    let w = world().await;
    let (s, page) = w.list("?q=acme").await;
    assert_eq!(s, 200);
    assert_eq!(names(&page), ["Анна", "Борис"]);
    let (_, page) = w.list("?q=GLOBEX").await;
    assert_eq!(names(&page), ["Вера"]);
    // спецсимволы поиска остаются буквальными
    let (_, page) = w.list("?q=%25").await;
    assert_eq!(page["total"], 0);
    w.app.close().await;
}

#[tokio::test]
async fn a_bad_id_is_rejected_and_another_organizations_client_is_empty() {
    let w = world().await;
    for bad in ["?client_id=мусор", "?client_id=123"] {
        let (s, _) = w.list(bad).await;
        assert_eq!(s, 400, "{bad}");
    }
    let other = w.app.token(TenantId::new(), &[Role::Recruiter]);
    let (s, page) = w
        .app
        .send(
            "GET",
            &format!("/applications?client_id={}", w.acme),
            Some(&other),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(page["total"], 0);
    let _ = w.tenant;
    w.app.close().await;
}
