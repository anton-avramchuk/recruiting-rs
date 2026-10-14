use platform_core::tenant::TenantId;
use platform_db::{TenantTx, testing::TestDb};
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("tests/migrations");

struct Fixture {
    db: TestDb,
    a: TenantId,
    b: TenantId,
}

async fn fixture() -> Fixture {
    let db = TestDb::create(&MIGRATOR).await;
    let (a, b) = (TenantId::new(), TenantId::new());
    for (t, name) in [(a, "a1"), (a, "a2"), (b, "b1")] {
        sqlx::query("INSERT INTO widgets (tenant_id, name) VALUES ($1, $2)")
            .bind(t.0)
            .bind(name)
            .execute(&db.owner) // суперпользователь обходит RLS
            .await
            .unwrap();
    }
    Fixture { db, a, b }
}

async fn names(tx: &mut TenantTx<'_>) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM widgets ORDER BY name")
        .fetch_all(&mut **tx)
        .await
        .unwrap()
}

#[tokio::test]
async fn select_without_filter_returns_only_current_tenant() {
    let f = fixture().await;
    let mut tx = TenantTx::begin(&f.db.app, f.a).await.unwrap();
    assert_eq!(names(&mut tx).await, ["a1", "a2"]);
    tx.rollback().await.unwrap();
    f.db.close().await;
}

#[tokio::test]
async fn insert_into_foreign_tenant_is_rejected() {
    let f = fixture().await;
    let mut tx = TenantTx::begin(&f.db.app, f.a).await.unwrap();
    let res = sqlx::query("INSERT INTO widgets (tenant_id, name) VALUES ($1, 'evil')")
        .bind(f.b.0)
        .execute(&mut *tx)
        .await;
    assert!(res.is_err(), "RLS WITH CHECK must reject foreign tenant_id");
    drop(tx);
    f.db.close().await;
}

#[tokio::test]
async fn update_and_delete_cannot_touch_foreign_rows() {
    let f = fixture().await;
    let mut tx = TenantTx::begin(&f.db.app, f.a).await.unwrap();
    let upd = sqlx::query("UPDATE widgets SET name = 'x' WHERE name = 'b1'")
        .execute(&mut *tx)
        .await
        .unwrap();
    let del = sqlx::query("DELETE FROM widgets WHERE name = 'b1'")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!((upd.rows_affected(), del.rows_affected()), (0, 0));
    tx.rollback().await.unwrap();
    f.db.close().await;
}

#[tokio::test]
async fn connection_without_tenant_context_sees_nothing() {
    let f = fixture().await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM widgets")
        .fetch_one(&f.db.app)
        .await
        .unwrap();
    assert_eq!(n, 0);
    f.db.close().await;
}

#[tokio::test]
async fn context_does_not_leak_through_pooled_connection() {
    let f = fixture().await;
    // пул на одно соединение: обе транзакции идут через него
    let mut tx_a = TenantTx::begin(&f.db.app, f.a).await.unwrap();
    assert_eq!(names(&mut tx_a).await, ["a1", "a2"]);
    tx_a.commit().await.unwrap();

    let mut tx_b = TenantTx::begin(&f.db.app, f.b).await.unwrap();
    assert_eq!(names(&mut tx_b).await, ["b1"]);
    tx_b.commit().await.unwrap();

    // и после завершения транзакции контекст пуст
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM widgets")
        .fetch_one(&f.db.app)
        .await
        .unwrap();
    assert_eq!(n, 0);
    f.db.close().await;
}
