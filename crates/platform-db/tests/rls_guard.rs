use platform_db::testing::{TestDb, tables_without_rls};
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("tests/migrations");

#[tokio::test]
async fn compliant_schema_has_no_offenders() {
    let db = TestDb::create(&MIGRATOR).await;
    assert_eq!(tables_without_rls(&db.owner).await, Vec::<String>::new());
    db.close().await;
}

#[tokio::test]
async fn guard_flags_table_without_rls() {
    let db = TestDb::create(&MIGRATOR).await;
    sqlx::raw_sql("CREATE TABLE leaky (id int, tenant_id uuid NOT NULL)")
        .execute(&db.owner)
        .await
        .unwrap();
    assert_eq!(tables_without_rls(&db.owner).await, ["leaky"]);
    db.close().await;
}

#[tokio::test]
async fn guard_flags_rls_enabled_but_not_forced_or_without_policy() {
    let db = TestDb::create(&MIGRATOR).await;
    sqlx::raw_sql(
        "CREATE TABLE not_forced (tenant_id uuid NOT NULL);
         ALTER TABLE not_forced ENABLE ROW LEVEL SECURITY;
         CREATE POLICY p ON not_forced USING (true);
         CREATE TABLE no_policy (tenant_id uuid NOT NULL);
         ALTER TABLE no_policy ENABLE ROW LEVEL SECURITY;
         ALTER TABLE no_policy FORCE ROW LEVEL SECURITY;",
    )
    .execute(&db.owner)
    .await
    .unwrap();
    assert_eq!(
        tables_without_rls(&db.owner).await,
        ["no_policy", "not_forced"]
    );
    db.close().await;
}
