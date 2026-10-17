//! Грид на настоящем Postgres: страницы, сортировка, операторы, изоляция организаций.
use grid_contracts::{Filter, GridQuery, Operator, SortKey};
use platform_core::tenant::TenantId;
use platform_db::{TenantTx, connect, testing::EmptyDb};
use platform_grid::{ColumnType, FetchError, GridColumn, GridSchema, fetch_page};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

const COLUMNS: &[GridColumn] = &[
    GridColumn::new("id", "id", ColumnType::Uuid),
    GridColumn::new("email", "email", ColumnType::Text),
    GridColumn::new("name", "name", ColumnType::Text),
    GridColumn::new("age", "age", ColumnType::Int),
    GridColumn::new("active", "active", ColumnType::Bool),
    GridColumn::new("created_at", "created_at", ColumnType::Timestamp),
    GridColumn::new("secret", "secret", ColumnType::Text)
        .no_filter()
        .no_sort(),
];
const SCHEMA: GridSchema = GridSchema {
    source: "people",
    key: "id",
    columns: COLUMNS,
    max_page_size: 100,
};
const SELECT: &str = "id, email, name, age, active";

#[allow(dead_code)]
#[derive(Debug, FromRow)]
struct Person {
    id: Uuid,
    email: String,
    name: Option<String>,
    age: Option<i32>,
    active: bool,
}

struct Db {
    empty: EmptyDb,
    owner: PgPool,
    app: PgPool,
}

async fn db() -> Db {
    let empty = EmptyDb::create("grid").await;
    let owner = connect(&empty.owner_url, 2).await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE people (
            id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id uuid NOT NULL,
            email text NOT NULL,
            name text,
            age int,
            active boolean NOT NULL DEFAULT true,
            secret text NOT NULL DEFAULT 'x',
            created_at timestamptz NOT NULL DEFAULT now()
        );
        ALTER TABLE people ENABLE ROW LEVEL SECURITY;
        ALTER TABLE people FORCE ROW LEVEL SECURITY;
        CREATE POLICY tenant_isolation ON people
            USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);",
    )
    .execute(&owner)
    .await
    .unwrap();
    let app = connect(&empty.app_url, 2).await.unwrap();
    Db { empty, owner, app }
}

impl Db {
    async fn add(
        &self,
        tenant: TenantId,
        email: &str,
        name: Option<&str>,
        age: Option<i32>,
        active: bool,
    ) {
        sqlx::query(
            "INSERT INTO people (tenant_id, email, name, age, active) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(tenant.0)
        .bind(email)
        .bind(name)
        .bind(age)
        .bind(active)
        .execute(&self.owner)
        .await
        .unwrap();
    }

    async fn page(
        &self,
        tenant: TenantId,
        query: GridQuery,
    ) -> Result<platform_grid::contracts::GridPage<Person>, FetchError> {
        let mut tx = TenantTx::begin(&self.app, tenant).await.unwrap();
        let result = fetch_page::<Person>(&mut tx, &SCHEMA, SELECT, &query).await;
        tx.rollback().await.unwrap();
        result
    }

    async fn emails(&self, tenant: TenantId, query: GridQuery) -> Vec<String> {
        self.page(tenant, query)
            .await
            .unwrap()
            .items
            .into_iter()
            .map(|p| p.email)
            .collect()
    }

    async fn close(self) {
        self.app.close().await;
        self.owner.close().await;
        self.empty.close().await;
    }
}

fn filter(f: Filter) -> GridQuery {
    GridQuery {
        filter: Some(f),
        sort: vec![SortKey::asc("email")],
        ..Default::default()
    }
}

#[tokio::test]
async fn pages_report_the_total_and_do_not_overlap() {
    let db = db().await;
    let t = TenantId::new();
    for i in 0..25 {
        db.add(t, &format!("u{i:02}@a.io"), None, None, true).await;
    }
    let query = |page| GridQuery {
        page,
        page_size: 10,
        sort: vec![SortKey::asc("email")],
        ..Default::default()
    };
    let second = db.page(t, query(2)).await.unwrap();
    assert_eq!((second.total, second.page, second.page_size), (25, 2, 10));
    assert_eq!(second.items.len(), 10);
    assert_eq!(second.items[0].email, "u10@a.io");
    assert_eq!(db.page(t, query(3)).await.unwrap().items.len(), 5);
    assert_eq!(db.page(t, query(4)).await.unwrap().items.len(), 0);
    db.close().await;
}

#[tokio::test]
async fn sorting_by_several_fields_is_stable() {
    let db = db().await;
    let t = TenantId::new();
    for (email, age) in [
        ("b@a.io", 30),
        ("a@a.io", 30),
        ("c@a.io", 20),
        ("d@a.io", 30),
    ] {
        db.add(t, email, None, Some(age), true).await;
    }
    let q = GridQuery {
        sort: vec![SortKey::desc("age"), SortKey::asc("email")],
        ..Default::default()
    };
    assert_eq!(
        db.emails(t, q).await,
        ["a@a.io", "b@a.io", "d@a.io", "c@a.io"]
    );
    // без сортировки порядок одинаков между запросами и страницы не пересекаются
    let by_key = |page| GridQuery {
        page,
        page_size: 2,
        ..Default::default()
    };
    let (p1, p2) = (db.emails(t, by_key(1)).await, db.emails(t, by_key(2)).await);
    assert!(p1.iter().all(|e| !p2.contains(e)));
    assert_eq!(p1.len() + p2.len(), 4);
    db.close().await;
}

#[tokio::test]
async fn operators_filter_as_documented() {
    let db = db().await;
    let t = TenantId::new();
    db.add(t, "anna@a.io", Some("Анна"), Some(30), true).await;
    db.add(t, "boris@b.io", Some(""), Some(17), false).await;
    db.add(t, "Vera@a.io", None, None, true).await;

    let emails = |f: Filter| db.emails(t, filter(f));
    assert_eq!(
        emails(Filter::leaf("email", Operator::Contains, "ANN")).await,
        ["anna@a.io"]
    );
    assert_eq!(
        emails(Filter::leaf("email", Operator::StartsWith, "vera")).await,
        ["Vera@a.io"]
    );
    assert_eq!(
        emails(Filter::leaf("email", Operator::EndsWith, "@B.io")).await,
        ["boris@b.io"]
    );
    assert_eq!(
        emails(Filter::leaf("email", Operator::Eq, "VERA@A.IO")).await,
        ["Vera@a.io"]
    );
    assert_eq!(
        emails(Filter::leaf("email", Operator::Neq, "anna@a.io")).await,
        ["boris@b.io", "Vera@a.io"]
    );
    assert_eq!(
        emails(Filter::leaf("age", Operator::Gte, 18)).await,
        ["anna@a.io"]
    );
    assert_eq!(
        emails(Filter::leaf("age", Operator::Lt, 18)).await,
        ["boris@b.io"]
    );
    assert_eq!(
        emails(Filter::nullary("age", Operator::IsNull)).await,
        ["Vera@a.io"]
    );
    assert_eq!(
        emails(Filter::nullary("age", Operator::IsNotNull)).await,
        ["anna@a.io", "boris@b.io"]
    );
    assert_eq!(
        emails(Filter::nullary("name", Operator::IsEmpty)).await,
        ["boris@b.io", "Vera@a.io"]
    );
    assert_eq!(
        emails(Filter::nullary("name", Operator::IsNotEmpty)).await,
        ["anna@a.io"]
    );
    assert_eq!(
        emails(Filter::leaf("active", Operator::Eq, false)).await,
        ["boris@b.io"]
    );
    assert_eq!(
        emails(Filter::and(vec![
            Filter::leaf("active", Operator::Eq, true),
            Filter::or(vec![
                Filter::leaf("email", Operator::Contains, "anna"),
                Filter::leaf("email", Operator::Contains, "vera"),
                Filter::leaf("email", Operator::Contains, "boris"),
            ]),
        ]))
        .await,
        ["anna@a.io", "Vera@a.io"]
    );
    let id = db
        .page(t, filter(Filter::leaf("email", Operator::Eq, "anna@a.io")))
        .await
        .unwrap()
        .items[0]
        .id;
    assert_eq!(
        emails(Filter::leaf("id", Operator::Eq, id.to_string())).await,
        ["anna@a.io"]
    );

    // total учитывает фильтр
    let page = db
        .page(t, filter(Filter::leaf("active", Operator::Eq, true)))
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    db.close().await;
}

#[tokio::test]
async fn wildcards_and_quotes_are_matched_literally() {
    let db = db().await;
    let t = TenantId::new();
    db.add(t, "100%@a.io", None, None, true).await;
    db.add(t, "1000@a.io", None, None, true).await;
    db.add(t, "o'brien@a.io", None, None, true).await;
    let emails = |part: &str| db.emails(t, filter(Filter::leaf("email", Operator::Contains, part)));
    assert_eq!(emails("100%").await, ["100%@a.io"]);
    assert_eq!(emails("o'b").await, ["o'brien@a.io"]);
    assert!(emails("'; DROP TABLE people; --").await.is_empty());
    assert_eq!(db.page(t, GridQuery::default()).await.unwrap().total, 3);
    db.close().await;
}

#[tokio::test]
async fn invalid_queries_never_reach_the_database() {
    let db = db().await;
    let t = TenantId::new();
    let err = db
        .page(t, filter(Filter::leaf("secret", Operator::Eq, "x")))
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Grid(e) if e.param == "filter"));
    db.close().await;
}

#[tokio::test]
async fn an_organization_sees_only_its_own_rows() {
    let db = db().await;
    let (a, b) = (TenantId::new(), TenantId::new());
    db.add(a, "a1@a.io", None, None, true).await;
    db.add(a, "a2@a.io", None, None, true).await;
    db.add(b, "b1@b.io", None, None, true).await;
    let page = db.page(a, GridQuery::default()).await.unwrap();
    assert_eq!(page.total, 2);
    assert!(page.items.iter().all(|p| p.email.ends_with("@a.io")));
    // и фильтр не помогает увидеть чужое
    assert!(
        db.emails(a, filter(Filter::leaf("email", Operator::Contains, "b1")))
            .await
            .is_empty()
    );
    assert_eq!(db.page(b, GridQuery::default()).await.unwrap().total, 1);
    db.close().await;
}

#[tokio::test]
async fn in_selects_listed_values_and_stays_inside_the_organization() {
    let db = db().await;
    let (mine, other) = (TenantId::new(), TenantId::new());
    db.add(mine, "a@a.io", None, Some(20), true).await;
    db.add(mine, "b@a.io", None, Some(30), true).await;
    db.add(mine, "c@a.io", None, Some(40), true).await;
    db.add(other, "d@b.io", None, Some(30), true).await;

    let ages = filter(Filter::leaf(
        "age",
        Operator::In,
        serde_json::json!([20, 40, 99]),
    ));
    assert_eq!(db.emails(mine, ages).await, ["a@a.io", "c@a.io"]);

    let emails = filter(Filter::leaf(
        "email",
        Operator::In,
        serde_json::json!(["B@A.io", "d@b.io"]),
    ));
    // чужая запись не видна даже по прямому совпадению
    assert_eq!(db.emails(mine, emails).await, ["b@a.io"]);

    let none = filter(Filter::leaf("age", Operator::In, serde_json::json!([])));
    assert!(db.emails(mine, none).await.is_empty());

    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM people WHERE email IN ('a@a.io','d@b.io')")
            .fetch_all(&db.owner)
            .await
            .unwrap();
    let by_id = filter(Filter::leaf("id", Operator::In, serde_json::json!(ids)));
    assert_eq!(db.emails(mine, by_id).await, ["a@a.io"]);
    db.close().await;
}
