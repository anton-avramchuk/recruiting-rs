use identity::MIGRATOR;
use platform_db::testing::{TestDb, tables_without_rls};
use uuid::Uuid;

async fn insert_user(db: &TestDb, tenant: Uuid, email: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO users (tenant_id, email, password_hash) VALUES ($1, $2, 'h')")
        .bind(tenant)
        .bind(email)
        .execute(&db.owner)
        .await
        .map(|_| ())
}

#[tokio::test]
async fn same_email_is_allowed_in_different_tenants() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    insert_user(&db, a, "x@example.com").await.unwrap();
    insert_user(&db, b, "x@example.com").await.unwrap();
    db.close().await;
}

#[tokio::test]
async fn duplicate_email_within_tenant_is_rejected() {
    let db = TestDb::create(&MIGRATOR).await;
    let a = Uuid::new_v4();
    insert_user(&db, a, "x@example.com").await.unwrap();
    assert!(insert_user(&db, a, "x@example.com").await.is_err());
    db.close().await;
}

#[tokio::test]
async fn all_tenant_tables_are_protected_by_rls() {
    let db = TestDb::create(&MIGRATOR).await;
    assert_eq!(tables_without_rls(&db.owner).await, Vec::<String>::new());
    // гарантируем, что страж не пуст из-за отсутствия таблиц
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns WHERE column_name = 'tenant_id'",
    )
    .fetch_one(&db.owner)
    .await
    .unwrap();
    assert!(n >= 2);
    db.close().await;
}
