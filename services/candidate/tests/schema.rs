use candidate::MIGRATOR;
use platform_core::tenant::TenantId;
use platform_db::{
    TenantTx,
    testing::{TestDb, tables_without_rls},
};
use uuid::Uuid;

async fn insert_person(db: &TestDb, tenant: TenantId, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO persons (tenant_id, full_name, normalized_name) VALUES ($1, $2, lower($2)) RETURNING id",
    )
    .bind(tenant.0)
    .bind(name)
    .fetch_one(&db.owner) // суперпользователь обходит RLS
    .await
    .unwrap()
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
        tables, 4,
        "persons, employments, educations, person_changes"
    );
    db.close().await;
}

#[tokio::test]
async fn unfiltered_reads_see_only_the_current_tenant() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    insert_person(&db, a, "Anna").await;
    insert_person(&db, b, "Boris").await;

    let mut tx = TenantTx::begin(&db.app, a).await.unwrap();
    let names: Vec<String> = sqlx::query_scalar("SELECT full_name FROM persons")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(names, ["Anna"]);
    tx.rollback().await.unwrap();

    // без контекста тенанта не видно ничего
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM persons")
        .fetch_one(&db.app)
        .await
        .unwrap();
    assert_eq!(n, 0);
    db.close().await;
}

#[tokio::test]
async fn writing_into_a_foreign_tenant_is_rejected() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let mut tx = TenantTx::begin(&db.app, a).await.unwrap();
    let res = sqlx::query(
        "INSERT INTO persons (tenant_id, full_name, normalized_name) VALUES ($1, 'Evil', 'evil')",
    )
    .bind(b.0)
    .execute(&mut *tx)
    .await;
    assert!(res.is_err());
    drop(tx);
    db.close().await;
}

#[tokio::test]
async fn history_cannot_point_at_a_person_of_another_tenant() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let person_a = insert_person(&db, a, "Anna").await;
    // даже суперпользователь не может привязать место работы тенанта B к человеку тенанта A
    let res = sqlx::query(
        "INSERT INTO employments (tenant_id, person_id, idx, company, position_title, start_date)
         VALUES ($1, $2, 0, 'Acme', 'Dev', '2020-01-01')",
    )
    .bind(b.0)
    .bind(person_a)
    .execute(&db.owner)
    .await;
    assert!(res.is_err(), "composite foreign key (tenant_id, person_id)");
    db.close().await;
}

#[tokio::test]
async fn deleting_a_person_removes_the_history() {
    let db = TestDb::create(&MIGRATOR).await;
    let a = TenantId::new();
    let person = insert_person(&db, a, "Anna").await;
    sqlx::query(
        "INSERT INTO employments (tenant_id, person_id, idx, company, position_title, start_date)
         VALUES ($1, $2, 0, 'Acme', 'Dev', '2020-01-01')",
    )
    .bind(a.0)
    .bind(person)
    .execute(&db.owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO educations (tenant_id, person_id, idx, institution) VALUES ($1, $2, 0, 'MSU')",
    )
    .bind(a.0)
    .bind(person)
    .execute(&db.owner)
    .await
    .unwrap();

    sqlx::query("DELETE FROM persons WHERE id = $1")
        .bind(person)
        .execute(&db.owner)
        .await
        .unwrap();
    for table in ["employments", "educations"] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&db.owner)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    db.close().await;
}

#[tokio::test]
async fn invalid_periods_are_rejected_by_the_database() {
    let db = TestDb::create(&MIGRATOR).await;
    let a = TenantId::new();
    let person = insert_person(&db, a, "Anna").await;
    let bad_job = sqlx::query(
        "INSERT INTO employments (tenant_id, person_id, idx, company, position_title, start_date, end_date)
         VALUES ($1, $2, 0, 'Acme', 'Dev', '2021-01-01', '2020-01-01')",
    )
    .bind(a.0)
    .bind(person)
    .execute(&db.owner)
    .await;
    assert!(bad_job.is_err());
    let bad_edu = sqlx::query(
        "INSERT INTO educations (tenant_id, person_id, idx, institution, year_from, year_to)
         VALUES ($1, $2, 0, 'MSU', 2015, 2010)",
    )
    .bind(a.0)
    .bind(person)
    .execute(&db.owner)
    .await;
    assert!(bad_edu.is_err());
    db.close().await;
}
