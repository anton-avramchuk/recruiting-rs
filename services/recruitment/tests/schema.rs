use platform_core::tenant::TenantId;
use platform_db::{
    TenantTx,
    testing::{TestDb, tables_without_rls},
};
use recruitment::MIGRATOR;
use uuid::Uuid;

async fn vacancy(db: &TestDb, tenant: TenantId, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO vacancies (tenant_id, title, opened_on) VALUES ($1, $2, '2026-01-01') RETURNING id",
    )
    .bind(tenant.0)
    .bind(title)
    .fetch_one(&db.owner)
    .await
    .unwrap()
}

async fn person(db: &TestDb, tenant: TenantId, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO people (tenant_id, person_id, full_name) VALUES ($1, $2, $3)")
        .bind(tenant.0)
        .bind(id)
        .bind(name)
        .execute(&db.owner)
        .await
        .unwrap();
    id
}

async fn application(
    db: &TestDb,
    tenant: TenantId,
    person: Uuid,
    vacancy: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO applications (tenant_id, person_id, vacancy_id, stage_changed_at, created_at)
         VALUES ($1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(tenant.0)
    .bind(person)
    .bind(vacancy)
    .fetch_one(&db.owner)
    .await
}

#[tokio::test]
async fn every_tenant_table_is_protected_by_rls() {
    let db = TestDb::create(&MIGRATOR).await;
    assert_eq!(tables_without_rls(&db.owner).await, Vec::<String>::new());
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT table_name) FROM information_schema.columns
         WHERE column_name = 'tenant_id' AND table_schema = 'public'",
    )
    .fetch_one(&db.owner)
    .await
    .unwrap();
    assert_eq!(
        tables, 6,
        "vacancies, people, people_sync, applications, application_events, interviews"
    );
    db.close().await;
}

#[tokio::test]
async fn unfiltered_reads_see_only_the_current_tenant() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    vacancy(&db, a, "Go").await;
    vacancy(&db, b, "Rust").await;

    let mut tx = TenantTx::begin(&db.app, a).await.unwrap();
    let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM vacancies")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(titles, ["Go"]);
    tx.rollback().await.unwrap();

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM vacancies")
        .fetch_one(&db.app)
        .await
        .unwrap();
    assert_eq!(n, 0, "no tenant context, no rows");
    db.close().await;
}

#[tokio::test]
async fn writing_into_a_foreign_tenant_is_rejected() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let mut tx = TenantTx::begin(&db.app, a).await.unwrap();
    let res = sqlx::query(
        "INSERT INTO vacancies (tenant_id, title, opened_on) VALUES ($1, 'Evil', '2026-01-01')",
    )
    .bind(b.0)
    .execute(&mut *tx)
    .await;
    assert!(res.is_err());
    drop(tx);
    db.close().await;
}

#[tokio::test]
async fn an_application_cannot_link_a_person_and_vacancy_of_another_tenant() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let (va, pa) = (vacancy(&db, a, "Go").await, person(&db, a, "Анна").await);
    let (vb, pb) = (vacancy(&db, b, "Rust").await, person(&db, b, "Борис").await);
    assert!(application(&db, a, pa, va).await.is_ok());
    assert!(application(&db, a, pb, va).await.is_err(), "foreign person");
    assert!(
        application(&db, a, pa, vb).await.is_err(),
        "foreign vacancy"
    );
    assert!(application(&db, b, pb, vb).await.is_ok());
    db.close().await;
}

#[tokio::test]
async fn a_person_can_apply_to_a_vacancy_only_once() {
    let db = TestDb::create(&MIGRATOR).await;
    let a = TenantId::new();
    let (v, p) = (vacancy(&db, a, "Go").await, person(&db, a, "Анна").await);
    application(&db, a, p, v).await.unwrap();
    assert!(application(&db, a, p, v).await.is_err());
    let v2 = vacancy(&db, a, "Rust").await;
    assert!(
        application(&db, a, p, v2).await.is_ok(),
        "other vacancy is fine"
    );
    db.close().await;
}

#[tokio::test]
async fn removing_a_person_removes_applications_history_and_interviews() {
    let db = TestDb::create(&MIGRATOR).await;
    let a = TenantId::new();
    let (v, p) = (vacancy(&db, a, "Go").await, person(&db, a, "Анна").await);
    let app = application(&db, a, p, v).await.unwrap();
    sqlx::query(
        "INSERT INTO application_events (tenant_id, application_id, kind, text, at)
         VALUES ($1, $2, 'created', 'Получен отклик', now())",
    )
    .bind(a.0)
    .bind(app)
    .execute(&db.owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO interviews (tenant_id, application_id, scheduled_at, created_at)
         VALUES ($1, $2, now(), now())",
    )
    .bind(a.0)
    .bind(app)
    .execute(&db.owner)
    .await
    .unwrap();

    sqlx::query("DELETE FROM people WHERE person_id = $1")
        .bind(p)
        .execute(&db.owner)
        .await
        .unwrap();
    for table in ["applications", "application_events", "interviews"] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&db.owner)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    db.close().await;
}

#[tokio::test]
async fn unknown_stage_is_rejected_by_the_database() {
    let db = TestDb::create(&MIGRATOR).await;
    let a = TenantId::new();
    let (v, p) = (vacancy(&db, a, "Go").await, person(&db, a, "Анна").await);
    let res = sqlx::query(
        "INSERT INTO applications (tenant_id, person_id, vacancy_id, stage, stage_changed_at, created_at)
         VALUES ($1, $2, $3, 'done', now(), now())",
    )
    .bind(a.0)
    .bind(p)
    .bind(v)
    .execute(&db.owner)
    .await;
    assert!(res.is_err());
    db.close().await;
}
