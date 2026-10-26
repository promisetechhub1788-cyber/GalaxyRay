use axum::http::StatusCode;
use axum_test::TestServer;
use galaxyray::{api, config::Config, db, AppState};
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

fn make_config() -> Config {
    Config {
        port: 0,
        database_url: "sqlite::memory:".into(),
        network: "testnet".into(),
        horizon_url: String::new(),
        gateway_public: "UNCONFIGURED".into(),
        accepted_assets: galaxyray::config::AcceptedAsset::default_list(),
        webhook_secret: String::new(),
        webhook_retry_attempts: 1,
        webhook_retry_delay_ms: 0,
        webhook_retry_max_delay_ms: 60_000,
        allowed_webhook_schemes: vec!["https".into(), "http".into()],
        webhook_timeout_secs: 10,
        webhook_redrive_interval_secs: 30,
        webhook_redrive_concurrency: 4,
        webhook_redrive_max_attempts: 8,
        webhook_redrive_grace_secs: 60,
        webhook_redrive_backoff_initial_secs: 0,
        webhook_redrive_backoff_max_secs: 0,
        retention_interval_secs: 3600,
        webhook_delivery_retention_days: 30,
        idempotency_retention_days: 7,
        poll_interval_secs: 10,
        poll_max_pages_per_cycle: 50,
        payment_ttl_secs: 3600,
        rate_limit_requests_per_sec: 1000,
        db_pool_max_connections: 10,
        db_busy_timeout_ms: 5000,
        cors_allowed_origins: vec![],
        listener_mode: galaxyray::config::ListenerMode::Poll,
        webhook_allow_private_targets: false,
        admin_provisioning_secret: "test-admin-secret".into(),
        metrics_token: "test-metrics-token".into(),
        request_timeout_secs: 30,
        stream_idle_timeout_secs: 30,
        trusted_proxy_cidrs: vec![],
    }
}

async fn test_server_with_pool() -> (TestServer, db::Db) {
    let pool = SqlitePoolOptions::new()
        .connect_with(
            SqliteConnectOptions::from_str(&make_config().database_url)
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap();
    db::migrate(&pool).await.unwrap();
    let http = reqwest::Client::new();
    let router = api::router(Arc::new(AppState {
        pool: pool.clone(),
        config: make_config(),
        http,
        webhook_http: reqwest::Client::new(),
        webhook_metrics: galaxyray::metrics::WebhookMetrics::new(),
        auth_metrics: galaxyray::metrics::AuthMetrics::new(),
        horizon_metrics: galaxyray::metrics::HorizonMetrics::new(),
        trustline_metrics: galaxyray::metrics::TrustlineMetrics::new(),
        http_metrics: galaxyray::metrics::HttpMetrics::new(),
        payment_metrics: galaxyray::metrics::PaymentMetrics::new(),
        task_health: galaxyray::TaskHealth::new(),
        analytics_cache: moka::sync::Cache::new(1000),
    }))
    .into_make_service_with_connect_info::<std::net::SocketAddr>();
    let server = TestServer::new(router);
    (server, pool)
}

async fn provision_merchant(server: &TestServer) -> String {
    let res = server
        .post("/merchants")
        .add_header("X-Admin-Secret", "test-admin-secret")
        .await;
    res.assert_status(StatusCode::CREATED);
    res.json::<Value>()["api_key"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_analytics_summary() {
    let (server, pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    // Insert test payment data directly into the database
    sqlx::query(
        "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind("pay-1")
    .bind("test-merchant")
    .bind("GBVG2X...")
    .bind("test-memo-1")
    .bind("1000000")
    .bind("native")
    .bind("completed")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind("pay-2")
    .bind("test-merchant")
    .bind("GBVG2X...")
    .bind("test-memo-2")
    .bind("2000000")
    .bind("native")
    .bind("pending")
    .execute(&pool)
    .await
    .unwrap();

    // Get analytics summary
    let res = server
        .get("/v1/merchants/test-merchant/analytics/summary")
        .add_header("Authorization", &auth)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert_eq!(body["total_payments"].as_i64().unwrap(), 2);
    assert_eq!(body["completed_count"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn test_analytics_by_status() {
    let (server, pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    // Insert test payments
    for i in 0..3 {
        let status = match i {
            0 => "completed",
            1 => "pending",
            _ => "completed",
        };
        sqlx::query(
            "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(format!("pay-{}", i))
        .bind("test-merchant")
        .bind("GBVG2X...")
        .bind(format!("memo-{}", i))
        .bind("1000000")
        .bind("native")
        .bind(status)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Get analytics by status
    let res = server
        .get("/v1/merchants/test-merchant/analytics/by-status")
        .add_header("Authorization", &auth)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert_eq!(body["completed"].as_i64().unwrap(), 2);
    assert_eq!(body["pending"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn test_analytics_by_asset() {
    let (server, pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    // Insert test payments with different assets
    sqlx::query(
        "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind("pay-1")
    .bind("test-merchant")
    .bind("GBVG2X...")
    .bind("memo-1")
    .bind("1000000")
    .bind("native")
    .bind("completed")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind("pay-2")
    .bind("test-merchant")
    .bind("GBVG2X...")
    .bind("memo-2")
    .bind("2000000")
    .bind("USDC")
    .bind("pending")
    .execute(&pool)
    .await
    .unwrap();

    // Get analytics by asset
    let res = server
        .get("/v1/merchants/test-merchant/analytics/by-asset")
        .add_header("Authorization", &auth)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert!(body.is_array());
    let assets: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["asset"].as_str().unwrap())
        .collect();
    assert!(assets.contains(&"native"));
    assert!(assets.contains(&"USDC"));
}

#[tokio::test]
async fn test_analytics_merchant_isolation() {
    let (server, pool) = test_server_with_pool().await;
    let api_key_1 = provision_merchant(&server).await;
    let auth_1 = format!("Bearer {}", api_key_1);

    // Merchant 1: Insert payments
    sqlx::query(
        "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind("pay-1")
    .bind("test-merchant")
    .bind("GBVG2X...")
    .bind("memo-1")
    .bind("1000000")
    .bind("native")
    .bind("completed")
    .execute(&pool)
    .await
    .unwrap();

    // Get analytics for merchant 1
    let res = server
        .get("/v1/merchants/test-merchant/analytics/summary")
        .add_header("Authorization", &auth_1)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert_eq!(body["total_payments"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn test_analytics_requires_auth() {
    let (server, _pool) = test_server_with_pool().await;

    let res = server
        .get("/v1/merchants/test-merchant/analytics/summary")
        .await;

    res.assert_status(StatusCode::UNAUTHORIZED);
}
