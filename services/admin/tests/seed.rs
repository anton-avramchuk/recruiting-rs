use admin::{MIGRATOR, infrastructure::admins};
use platform_auth::password;
use platform_db::testing::TestDb;

#[tokio::test]
async fn seeds_first_admin_with_hashed_password() {
    let db = TestDb::create(&MIGRATOR).await;
    assert!(
        admins::seed(&db.app, "Root@Platform.io", "long-enough-1")
            .await
            .unwrap()
    );

    let (email, hash): (String, String) =
        sqlx::query_as("SELECT email, password_hash FROM platform_admins")
            .fetch_one(&db.owner)
            .await
            .unwrap();
    assert_eq!(email, "root@platform.io");
    assert_ne!(hash, "long-enough-1");
    assert!(password::verify("long-enough-1", &hash));
    db.close().await;
}

#[tokio::test]
async fn seeding_is_idempotent_and_keeps_existing_password() {
    let db = TestDb::create(&MIGRATOR).await;
    assert!(
        admins::seed(&db.app, "root@platform.io", "first-password")
            .await
            .unwrap()
    );
    assert!(
        !admins::seed(&db.app, "root@platform.io", "other-password")
            .await
            .unwrap()
    );

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_admins")
        .fetch_one(&db.owner)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM platform_admins")
        .fetch_one(&db.owner)
        .await
        .unwrap();
    assert!(password::verify("first-password", &hash));
    db.close().await;
}

#[tokio::test]
async fn weak_seed_password_is_refused() {
    let db = TestDb::create(&MIGRATOR).await;
    assert!(
        admins::seed(&db.app, "root@platform.io", "short")
            .await
            .is_err()
    );
    db.close().await;
}
