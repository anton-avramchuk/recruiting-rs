use client::MIGRATOR;
use platform_core::tenant::TenantId;
use platform_db::{
    TenantTx,
    testing::{TestDb, tables_without_rls},
};
use uuid::Uuid;

async fn insert_client(db: &TestDb, tenant: TenantId, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO clients (tenant_id, name, name_key) VALUES ($1, $2, lower($2)) RETURNING id",
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
    assert_eq!(tables, 4, "clients, positions, contacts, client_changes");
    db.close().await;
}

#[tokio::test]
async fn unfiltered_reads_see_only_the_current_tenant() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    insert_client(&db, a, "Acme").await;
    insert_client(&db, b, "Globex").await;

    let mut tx = TenantTx::begin(&db.app, a).await.unwrap();
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM clients")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(names, ["Acme"]);
    tx.rollback().await.unwrap();

    // без контекста тенанта не видно ничего
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM clients")
        .fetch_one(&db.app)
        .await
        .unwrap();
    assert_eq!(n, 0);
    db.close().await;
}

#[tokio::test]
async fn a_contact_cannot_point_to_another_tenants_client() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    let client_of_a = insert_client(&db, a, "Acme").await;
    let result = sqlx::query(
        "INSERT INTO contacts (tenant_id, client_id, full_name) VALUES ($1, $2, 'Чужой')",
    )
    .bind(b.0)
    .bind(client_of_a)
    .execute(&db.owner)
    .await;
    assert!(result.is_err(), "composite foreign key must reject it");
    db.close().await;
}

#[tokio::test]
async fn client_names_are_unique_per_tenant_only() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    insert_client(&db, a, "Acme").await;
    insert_client(&db, b, "Acme").await; // другой тенант - можно
    let dup =
        sqlx::query("INSERT INTO clients (tenant_id, name, name_key) VALUES ($1, 'ACME', 'acme')")
            .bind(a.0)
            .execute(&db.owner)
            .await;
    assert!(dup.is_err());
    db.close().await;
}
