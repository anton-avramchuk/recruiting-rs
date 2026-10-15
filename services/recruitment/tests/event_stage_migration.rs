//! Миграция 0002 заполняет этап у событий, записанных до неё.
use std::borrow::Cow;

use platform_core::tenant::TenantId;
use platform_db::testing::TestDb;
use recruitment::MIGRATOR;
use sqlx::migrate::Migrator;
use uuid::Uuid;

#[tokio::test]
async fn existing_events_get_their_stage_from_the_text() {
    // база на момент до миграции 0002
    let before = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .migrations
                .iter()
                .filter(|m| m.version == 1)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    let db = TestDb::create(&before).await;
    let tenant = TenantId::new().0;
    let (vacancy, person, app): (Uuid, Uuid, Uuid) =
        (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    sqlx::query("INSERT INTO vacancies (id, tenant_id, title, opened_on) VALUES ($1, $2, 'Go', '2026-01-01')")
        .bind(vacancy).bind(tenant).execute(&db.owner).await.unwrap();
    sqlx::query("INSERT INTO people (tenant_id, person_id, full_name) VALUES ($1, $2, 'Анна')")
        .bind(tenant)
        .bind(person)
        .execute(&db.owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO applications (id, tenant_id, person_id, vacancy_id, stage, stage_changed_at, created_at)
         VALUES ($1, $2, $3, $4, 'rejected', now(), now())",
    )
    .bind(app).bind(tenant).bind(person).bind(vacancy).execute(&db.owner).await.unwrap();
    for (kind, text) in [
        ("created", "Получен отклик"),
        ("stage", "Этап → Новый"),
        ("stage", "Этап → Скрининг"),
        ("stage", "Этап → Интервью HR"),
        ("stage", "Этап → Тех. интервью"),
        ("stage", "Этап → Финал"),
        ("stage", "Этап → Оффер"),
        ("stage", "Этап → Выход"),
        ("stage", "Этап → Отказ"),
        ("rejected", "Отказ: мимо"),
        ("letter", "Письмо с отказом отправлено"),
        ("stage", "Этап → что-то новое"),
    ] {
        sqlx::query(
            "INSERT INTO application_events (tenant_id, application_id, kind, text, at) VALUES ($1, $2, $3, $4, now())",
        )
        .bind(tenant).bind(app).bind(kind).bind(text).execute(&db.owner).await.unwrap();
    }

    MIGRATOR.run(&db.owner).await.unwrap();

    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT text, stage FROM application_events ORDER BY seq")
            .fetch_all(&db.owner)
            .await
            .unwrap();
    let stage = |text: &str| rows.iter().find(|(t, _)| t == text).unwrap().1.clone();
    assert_eq!(stage("Получен отклик"), None);
    assert_eq!(stage("Этап → Новый").as_deref(), Some("new"));
    assert_eq!(stage("Этап → Скрининг").as_deref(), Some("screening"));
    assert_eq!(stage("Этап → Интервью HR").as_deref(), Some("hr"));
    assert_eq!(stage("Этап → Тех. интервью").as_deref(), Some("tech"));
    assert_eq!(stage("Этап → Финал").as_deref(), Some("final"));
    assert_eq!(stage("Этап → Оффер").as_deref(), Some("offer"));
    assert_eq!(stage("Этап → Выход").as_deref(), Some("hired"));
    assert_eq!(stage("Этап → Отказ").as_deref(), Some("rejected"));
    assert_eq!(stage("Отказ: мимо").as_deref(), Some("rejected"));
    assert_eq!(stage("Письмо с отказом отправлено"), None);
    assert_eq!(
        stage("Этап → что-то новое"),
        None,
        "unknown text is left alone"
    );
    db.close().await;
}
