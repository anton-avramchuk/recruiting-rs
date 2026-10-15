mod common;

use common::setup;
use platform_auth::Role;
use platform_core::tenant::TenantId;
use serde_json::json;

#[tokio::test]
async fn city_is_trimmed_and_note_is_stored_on_create() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (s, p) = app
        .send(
            "POST",
            "/persons",
            Some(&token),
            Some(json!({"full_name": "Анна", "city": "  Санкт-Петербург ", "note": " позвонить вечером "})),
        )
        .await;
    assert_eq!(s, 201, "{p}");
    assert_eq!(p["city"], "Санкт-Петербург");
    assert_eq!(p["note"], "позвонить вечером");
    let (_, card) = app
        .send(
            "GET",
            &format!("/persons/{}", p["id"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(card["city"], "Санкт-Петербург");
    app.close().await;
}

#[tokio::test]
async fn empty_note_clears_it_without_touching_other_fields() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    let (_, p) = app
        .send(
            "POST",
            "/persons",
            Some(&token),
            Some(json!({"full_name": "Анна", "city": "Казань", "note": "заметка", "source": "hh.ru"})),
        )
        .await;
    let uri = format!("/persons/{}", p["id"].as_str().unwrap());
    let (s, updated) = app
        .send("PATCH", &uri, Some(&token), Some(json!({"note": ""})))
        .await;
    assert_eq!(s, 200, "{updated}");
    assert_eq!(updated["note"], json!(null));
    assert_eq!(updated["city"], "Казань");
    assert_eq!(updated["source"], "hh.ru");

    let (_, updated) = app
        .send("PATCH", &uri, Some(&token), Some(json!({"city": null})))
        .await;
    assert_eq!(updated["city"], json!(null));
    app.close().await;
}

#[tokio::test]
async fn search_finds_people_by_city_case_insensitively() {
    let app = setup().await;
    let token = app.token(TenantId::new(), &[Role::Recruiter]);
    for (name, city) in [("Анна", "Казань"), ("Борис", "Москва")] {
        let (s, _) = app
            .send(
                "POST",
                "/persons",
                Some(&token),
                Some(json!({"full_name": name, "city": city})),
            )
            .await;
        assert_eq!(s, 201);
    }
    let (s, page) = app
        .send(
            "GET",
            "/persons?q=%D0%BA%D0%B0%D0%B7%D0%B0%D0%BD%D1%8C",
            Some(&token),
            None,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(page["total"], 1, "{page}");
    assert_eq!(page["items"][0]["full_name"], "Анна");
    assert_eq!(page["items"][0]["city"], "Казань");
    app.close().await;
}
