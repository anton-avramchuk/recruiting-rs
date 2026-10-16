use comment::MIGRATOR;
use platform_core::tenant::TenantId;
use platform_db::{
    TenantTx,
    testing::{TestDb, tables_without_rls},
};
use uuid::Uuid;

async fn insert(db: &TestDb, tenant: TenantId, body: &str) {
    sqlx::query(
        "INSERT INTO comments (tenant_id, target_type, target_id, author_id, author_name, body)
         VALUES ($1, 'person', $2, $3, 'Автор', $4)",
    )
    .bind(tenant.0)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(body)
    .execute(&db.owner) // суперпользователь обходит RLS
    .await
    .unwrap();
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
    assert_eq!(tables, 1, "comments");
    db.close().await;
}

#[tokio::test]
async fn unfiltered_reads_see_only_the_current_tenant() {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    insert(&db, a, "из A").await;
    insert(&db, b, "из B").await;

    let mut tx = TenantTx::begin(&db.app, a).await.unwrap();
    let bodies: Vec<String> = sqlx::query_scalar("SELECT body FROM comments")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(bodies, ["из A"]);
    tx.rollback().await.unwrap();

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM comments")
        .fetch_one(&db.app)
        .await
        .unwrap();
    assert_eq!(n, 0, "без контекста тенанта не видно ничего");
    db.close().await;
}

#[tokio::test]
async fn an_unknown_target_type_is_rejected_by_the_table() {
    let db = TestDb::create(&MIGRATOR).await;
    let result = sqlx::query(
        "INSERT INTO comments (tenant_id, target_type, target_id, author_id, author_name, body)
         VALUES ($1, 'candidate', $2, $3, 'А', 'x')",
    )
    .bind(TenantId::new().0)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .execute(&db.owner)
    .await;
    assert!(result.is_err());
    db.close().await;
}
