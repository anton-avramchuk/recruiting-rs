mod common;

use common::{App, setup};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::{Value, json};

struct Ctx {
    app: App,
    tenant: TenantId,
    token: String,
}

async fn ctx() -> Ctx {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    Ctx { app, tenant, token }
}

impl Ctx {
    /// Отклик нового человека на этапе `stage`, пробывший там `hours` часов.
    async fn aged(&self, name: &str, vacancy: &Value, stage: &str, hours: i64) -> Value {
        let p = self.app.add_person(self.tenant, name, &[]).await;
        let a = self.app.add_application(&self.token, p, vacancy).await;
        if stage != "new" {
            let (s, _) = self
                .app
                .send(
                    "POST",
                    &format!("/applications/{}/stage", a["id"].as_str().unwrap()),
                    Some(&self.token),
                    Some(json!({"stage": stage})),
                )
                .await;
            assert_eq!(s, 200);
        }
        self.app.age_stage(&a["id"], hours).await;
        a
    }

    async fn get(&self, query: &str) -> (u16, Value) {
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
    let mut out: Vec<String> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["full_name"].as_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn overdue_filter_keeps_only_sla_breaches() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    c.aged("Свежий", &v, "new", 2).await;
    c.aged("Старый", &v, "new", 30).await;
    c.aged("Тех4", &v, "tech", 4 * 24).await;
    c.aged("Тех6", &v, "tech", 6 * 24).await;
    c.aged("Отказ", &v, "rejected", 30 * 24).await;

    let (s, page) = c.get("?overdue=true").await;
    assert_eq!(s, 200, "{page}");
    assert_eq!(names(&page), ["Старый", "Тех6"]);
    assert_eq!(page["total"], 2);

    // фильтры комбинируются по «И»
    let (_, both) = c.get("?overdue=true&stage=tech").await;
    assert_eq!(names(&both), ["Тех6"]);
    // overdue=false не ограничивает выборку
    let (_, all) = c.get("?overdue=false").await;
    assert_eq!(all["total"], 5);
    c.app.close().await;
}

#[tokio::test]
async fn overdue_filter_agrees_with_the_flag_in_every_row() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    // границы: ровно сутки и плюс час для нового, ровно 5 суток и плюс час для активного
    for (hours, stage) in [
        (24, "new"),
        (25, "new"),
        (120, "hr"),
        (121, "hr"),
        (500, "hired"),
        (500, "rejected"),
    ] {
        c.aged(&format!("{stage}-{hours}"), &v, stage, hours).await;
    }
    let (_, all) = c.get("?page_size=200").await;
    let mut flagged: Vec<String> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["overdue"] == true)
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    let (_, filtered) = c.get("?overdue=true&page_size=200").await;
    let mut got: Vec<String> = filtered["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    flagged.sort();
    got.sort();
    assert_eq!(got, flagged);
    assert_eq!(got.len(), 2, "25 h in new and 121 h on an active stage");
    c.app.close().await;
}

#[tokio::test]
async fn bad_overdue_value_is_400() {
    let c = ctx().await;
    let (s, _) = c.get("?overdue=maybe").await;
    assert_eq!(s, 400);
    c.app.close().await;
}

#[tokio::test]
async fn equal_matches_put_the_longest_waiting_first() {
    let c = ctx().await;
    let v = c.app.add_vacancy(&c.token, json!({"title": "Go"})).await;
    let newer = c.aged("Новее", &v, "new", 1).await;
    let older = c.aged("Старее", &v, "new", 10).await;
    let (_, page) = c.get("?stage=new&sort=match&dir=desc").await;
    assert_eq!(page["items"][0]["id"], older["id"]);
    assert_eq!(page["items"][1]["id"], newer["id"]);
    c.app.close().await;
}
