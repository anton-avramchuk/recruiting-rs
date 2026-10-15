//! Заказчик и контакт в вакансиях: проверки по проекции и синхронизация с лентой client.
mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use client_contracts::{ClientChange, ClientChangesPage, ClientSnapshot, ContactSnapshot, Entity};
use common::{EmptyFeed, setup, setup_with_feeds};
use platform_auth::Role;
use platform_core::tenant::TenantId;
use recruitment::application::{ClientFeed, FeedError};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn a_new_vacancy_needs_an_active_client() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);

    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "A"})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("client_id")), "{p}");

    let unknown = Uuid::new_v4();
    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "A", "client_id": unknown})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("client_id")), "{p}");

    let archived = app.add_client(tenant, "Архивный", true).await;
    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "A", "client_id": archived})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("client_id")), "{p}");

    let (_, list) = app.send("GET", "/vacancies", Some(&token), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
    app.close().await;
}

#[tokio::test]
async fn the_vacancy_carries_client_and_contact_with_position() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let client = app.add_client(tenant, "Acme", false).await;
    let contact = app
        .add_contact(tenant, client, "Мария Петрова", Some("HR-директор"))
        .await;

    let v = app
        .add_vacancy(
            &token,
            json!({"title": "Go", "client_id": client, "contact_id": contact}),
        )
        .await;
    assert_eq!(v["client"]["name"], "Acme");
    assert_eq!(v["client"]["archived"], false);
    assert_eq!(v["contact"]["full_name"], "Мария Петрова");
    assert_eq!(v["contact"]["position"], "HR-директор");
    assert!(v.get("department").is_none() && v.get("hiring_manager").is_none());

    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());
    let (_, card) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(card["client"]["name"], "Acme");
    let (_, list) = app.send("GET", "/vacancies", Some(&token), None).await;
    assert_eq!(list[0]["contact"]["position"], "HR-директор");
    app.close().await;
}

#[tokio::test]
async fn a_contact_of_another_client_is_rejected() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let acme = app.add_client(tenant, "Acme", false).await;
    let globex = app.add_client(tenant, "Globex", false).await;
    let globex_contact = app.add_contact(tenant, globex, "Иван", None).await;

    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "A", "client_id": acme, "contact_id": globex_contact})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("contact_id")), "{p}");

    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "A", "client_id": acme, "contact_id": Uuid::new_v4()})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("contact_id")), "{p}");
    app.close().await;
}

#[tokio::test]
async fn another_organizations_client_cannot_be_used() {
    let app = setup().await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let token_b = app.token(b, &[Role::Recruiter]);
    let client_of_a = app.add_client(a, "Acme", false).await;
    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token_b),
            Some(json!({"title": "A", "client_id": client_of_a})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("client_id")), "{p}");
    app.close().await;
}

#[tokio::test]
async fn changing_the_client_resets_the_contact_and_the_client_cannot_be_removed() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let acme = app.add_client(tenant, "Acme", false).await;
    let globex = app.add_client(tenant, "Globex", false).await;
    let contact = app.add_contact(tenant, acme, "Мария", None).await;
    let v = app
        .add_vacancy(
            &token,
            json!({"title": "Go", "client_id": acme, "contact_id": contact}),
        )
        .await;
    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());

    let (s, updated) = app
        .send(
            "PATCH",
            &uri,
            Some(&token),
            Some(json!({"client_id": globex})),
        )
        .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["client"]["name"], "Globex");
    assert_eq!(updated["contact"], json!(null));
    assert_eq!(updated["title"], "Go");

    let (s, p) = app
        .send(
            "PATCH",
            &uri,
            Some(&token),
            Some(json!({"client_id": null})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("client_id")), "{p}");
    let (_, card) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(card["client"]["name"], "Globex");
    app.close().await;
}

#[tokio::test]
async fn an_old_vacancy_without_a_client_works_and_can_get_one() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    // вакансия, созданная до появления заказчиков
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO vacancies (id, tenant_id, title, department, hiring_manager, opened_on)
         VALUES ($1, $2, 'Старая', 'Отдел', 'Иванов', current_date)",
    )
    .bind(id)
    .bind(tenant.0)
    .execute(&app.db.owner)
    .await
    .unwrap();
    let uri = format!("/vacancies/{id}");

    let (s, card) = app.send("GET", &uri, Some(&token), None).await;
    assert_eq!(s, 200);
    assert_eq!(card["client"], json!(null));
    assert_eq!(card["contact"], json!(null));
    assert!(
        card.get("department").is_none(),
        "legacy text is not exposed"
    );

    // правка без ссылок разрешена
    let (s, v) = app
        .send("PATCH", &uri, Some(&token), Some(json!({"openings": 2})))
        .await;
    assert_eq!(s, 200, "{v}");
    // контакт без заказчика назначить нельзя
    let contact = Uuid::new_v4();
    let (s, p) = app
        .send(
            "PATCH",
            &uri,
            Some(&token),
            Some(json!({"contact_id": contact})),
        )
        .await;
    assert_eq!((s, p["field"].as_str()), (400, Some("contact_id")), "{p}");
    // а заказчика назначить можно
    let client = app.add_client(tenant, "Acme", false).await;
    let (s, v) = app
        .send(
            "PATCH",
            &uri,
            Some(&token),
            Some(json!({"client_id": client})),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["client"]["name"], "Acme");
    app.close().await;
}

#[tokio::test]
async fn an_archived_client_does_not_block_editing_an_existing_vacancy() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let client = app.add_client(tenant, "Acme", false).await;
    let v = app
        .add_vacancy(&token, json!({"title": "Go", "client_id": client}))
        .await;
    sqlx::query("UPDATE clients SET archived = true WHERE client_id = $1")
        .bind(client)
        .execute(&app.db.owner)
        .await
        .unwrap();
    let uri = format!("/vacancies/{}", v["id"].as_str().unwrap());
    let (s, updated) = app
        .send("PATCH", &uri, Some(&token), Some(json!({"openings": 3})))
        .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["client"]["archived"], true);
    app.close().await;
}

#[tokio::test]
async fn the_list_filters_by_client() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let acme = app.add_client(tenant, "Acme", false).await;
    let globex = app.add_client(tenant, "Globex", false).await;
    app.add_vacancy(&token, json!({"title": "A1", "client_id": acme}))
        .await;
    app.add_vacancy(&token, json!({"title": "A2", "client_id": acme}))
        .await;
    app.add_vacancy(&token, json!({"title": "G1", "client_id": globex}))
        .await;

    let (s, list) = app
        .send(
            "GET",
            &format!("/vacancies?client_id={acme}"),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(list.as_array().unwrap().len(), 2);
    let (_, all) = app.send("GET", "/vacancies", Some(&token), None).await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    app.close().await;
}

#[tokio::test]
async fn a_deleted_contact_is_shown_as_missing() {
    let app = setup().await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let client = app.add_client(tenant, "Acme", false).await;
    let contact = app.add_contact(tenant, client, "Мария", None).await;
    let v = app
        .add_vacancy(
            &token,
            json!({"title": "Go", "client_id": client, "contact_id": contact}),
        )
        .await;
    sqlx::query("DELETE FROM client_contacts WHERE contact_id = $1")
        .bind(contact)
        .execute(&app.db.owner)
        .await
        .unwrap();
    let (s, card) = app
        .send(
            "GET",
            &format!("/vacancies/{}", v["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(card["contact"], json!(null));
    assert_eq!(card["client"]["name"], "Acme");
    app.close().await;
}

// ---------- синхронизация ----------

/// Лента, которой управляет тест: записи и признак недоступности.
#[derive(Default)]
struct ScriptedFeed {
    items: Mutex<Vec<ClientChange>>,
    down: Mutex<bool>,
}

impl ScriptedFeed {
    fn push(&self, change: ClientChange) {
        self.items.lock().unwrap().push(change);
    }
}

#[async_trait]
impl ClientFeed for ScriptedFeed {
    async fn changes(
        &self,
        _tenant: TenantId,
        after: i64,
        limit: i64,
    ) -> Result<ClientChangesPage, FeedError> {
        if *self.down.lock().unwrap() {
            return Err(FeedError::Unavailable("down".into()));
        }
        let items: Vec<_> = self
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.seq > after)
            .take(limit as usize)
            .cloned()
            .collect();
        let next = items.last().map_or(after, |c| c.seq);
        Ok(ClientChangesPage { items, next })
    }
}

fn client_change(seq: i64, id: Uuid, name: &str, archived: bool) -> ClientChange {
    ClientChange {
        seq,
        entity: Entity::Client,
        entity_id: id,
        deleted: false,
        client: Some(ClientSnapshot {
            name: name.into(),
            archived,
        }),
        contact: None,
    }
}

fn contact_change(seq: i64, id: Uuid, client: Uuid, name: &str, position: &str) -> ClientChange {
    ClientChange {
        seq,
        entity: Entity::Contact,
        entity_id: id,
        deleted: false,
        client: None,
        contact: Some(ContactSnapshot {
            client_id: client,
            full_name: name.into(),
            position: Some(position.into()),
        }),
    }
}

#[tokio::test]
async fn the_projection_catches_up_on_the_next_read() {
    let feed = Arc::new(ScriptedFeed::default());
    let app = setup_with_feeds(Arc::new(EmptyFeed), feed.clone()).await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let (client, contact) = (Uuid::new_v4(), Uuid::new_v4());
    feed.push(client_change(1, client, "Acme", false));
    feed.push(contact_change(2, contact, client, "Мария", "HR"));

    // создание само догоняет ленту, заказчика не нужно класть вручную
    let (s, v) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "Go", "client_id": client, "contact_id": contact})),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["client"]["name"], "Acme");
    assert_eq!(v["contact"]["position"], "HR");

    // переименование заказчика и должности видно при следующем чтении
    feed.push(client_change(3, client, "Acme Corp", false));
    feed.push(contact_change(4, contact, client, "Мария", "HR-директор"));
    let (_, card) = app
        .send(
            "GET",
            &format!("/vacancies/{}", v["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(card["client"]["name"], "Acme Corp");
    assert_eq!(card["contact"]["position"], "HR-директор");

    // удаление контакта из ленты убирает его из проекции
    feed.push(ClientChange {
        seq: 5,
        entity: Entity::Contact,
        entity_id: contact,
        deleted: true,
        client: None,
        contact: None,
    });
    let (_, card) = app
        .send(
            "GET",
            &format!("/vacancies/{}", v["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(card["contact"], json!(null));
    app.close().await;
}

#[tokio::test]
async fn when_the_source_is_down_known_data_is_served_and_unknown_clients_give_503() {
    let feed = Arc::new(ScriptedFeed::default());
    let app = setup_with_feeds(Arc::new(EmptyFeed), feed.clone()).await;
    let tenant = TenantId::new();
    let token = app.token(tenant, &[Role::Recruiter]);
    let known = Uuid::new_v4();
    feed.push(client_change(1, known, "Acme", false));
    let v = app
        .add_vacancy(&token, json!({"title": "Go", "client_id": known}))
        .await;

    *feed.down.lock().unwrap() = true;
    // чтение по последним известным данным
    let (s, card) = app
        .send(
            "GET",
            &format!("/vacancies/{}", v["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(card["client"]["name"], "Acme");
    // известный заказчик по-прежнему подходит
    let (s, _) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "Rust", "client_id": known})),
        )
        .await;
    assert_eq!(s, 201);
    // неизвестный - 503, а не ложное «не найден»
    let (s, p) = app
        .send(
            "POST",
            "/vacancies",
            Some(&token),
            Some(json!({"title": "X", "client_id": Uuid::new_v4()})),
        )
        .await;
    assert_eq!(s, 503, "{p}");
    app.close().await;
}
