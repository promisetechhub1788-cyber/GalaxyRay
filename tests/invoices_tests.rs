use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;
use galaxyray::{
    AppState, api,
    config::Config,
    db,
};

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
async fn test_create_invoice() {
    let (server, _pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    let payload = json!({
        "customer_name": "John Doe",
        "customer_email": "john@example.com",
        "amount_stroops": "1000000",
        "asset": "native"
    });

    let res = server
        .post("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth)
        .json(&payload)
        .await;
    
    res.assert_status(StatusCode::CREATED);
    let body = res.json::<Value>();
    assert_eq!(body["invoice_number"].as_str().unwrap(), "1");
    assert_eq!(body["customer_name"].as_str().unwrap(), "John Doe");
    assert_eq!(body["status"].as_str().unwrap(), "draft");
}

#[tokio::test]
async fn test_list_invoices() {
    let (server, _pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    // Create two invoices
    let payload1 = json!({
        "customer_name": "Alice",
        "customer_email": "alice@example.com",
        "amount_stroops": "1000000",
        "asset": "native"
    });

    server
        .post("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth)
        .json(&payload1)
        .await
        .assert_status(StatusCode::CREATED);

    let payload2 = json!({
        "customer_name": "Bob",
        "customer_email": "bob@example.com",
        "amount_stroops": "2000000",
        "asset": "native"
    });

    server
        .post("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth)
        .json(&payload2)
        .await
        .assert_status(StatusCode::CREATED);

    // List invoices
    let res = server
        .get("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_update_invoice_status() {
    let (server, _pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    // Create invoice
    let payload = json!({
        "customer_name": "Charlie",
        "customer_email": "charlie@example.com",
        "amount_stroops": "5000000",
        "asset": "native"
    });

    let create_res = server
        .post("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth)
        .json(&payload)
        .await;

    let invoice_id = create_res.json::<Value>()["id"].as_str().unwrap().to_string();

    // Update status to sent
    let update_payload = json!({ "status": "sent" });
    let res = server
        .patch(&format!("/v1/merchants/test-merchant/invoices/{}", invoice_id))
        .add_header("Authorization", &auth)
        .json(&update_payload)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert_eq!(body["status"].as_str().unwrap(), "sent");

    // Update status to paid
    let update_payload = json!({ "status": "paid" });
    let res = server
        .patch(&format!("/v1/merchants/test-merchant/invoices/{}", invoice_id))
        .add_header("Authorization", &auth)
        .json(&update_payload)
        .await;

    res.assert_status(StatusCode::OK);
    let body = res.json::<Value>();
    assert_eq!(body["status"].as_str().unwrap(), "paid");
}

#[tokio::test]
async fn test_invalid_invoice_status_returns_400() {
    let (server, _pool) = test_server_with_pool().await;
    let api_key = provision_merchant(&server).await;
    let auth = format!("Bearer {}", api_key);

    // Create invoice
    let payload = json!({
        "customer_name": "Dave",
        "customer_email": "dave@example.com",
        "amount_stroops": "3000000",
        "asset": "native"
    });

    let create_res = server
        .post("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth)
        .json(&payload)
        .await;

    let invoice_id = create_res.json::<Value>()["id"].as_str().unwrap().to_string();

    // Try invalid status
    let update_payload = json!({ "status": "invalid_status" });
    let res = server
        .patch(&format!("/v1/merchants/test-merchant/invoices/{}", invoice_id))
        .add_header("Authorization", &auth)
        .json(&update_payload)
        .await;

    res.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_invoice_merchant_isolation() {
    let (server, pool) = test_server_with_pool().await;

    // Create two merchants
    let api_key_1 = provision_merchant(&server).await;
    let auth_1 = format!("Bearer {}", api_key_1);

    // Merchant 1 creates an invoice
    let payload = json!({
        "customer_name": "Eve",
        "customer_email": "eve@example.com",
        "amount_stroops": "1000000",
        "asset": "native"
    });

    let create_res = server
        .post("/v1/merchants/test-merchant/invoices")
        .add_header("Authorization", &auth_1)
        .json(&payload)
        .await;

    let invoice_id = create_res.json::<Value>()["id"].as_str().unwrap().to_string();

    // Provision second merchant and try to access first merchant's invoice
    let api_key_2 = provision_merchant(&server).await;
    let auth_2 = format!("Bearer {}", api_key_2);

    let res = server
        .get(&format!("/v1/merchants/test-merchant/invoices/{}", invoice_id))
        .add_header("Authorization", &auth_2)
        .await;

    // Should be forbidden (403) when merchant_id in URL doesn't match authenticated merchant
    res.assert_status(StatusCode::FORBIDDEN);
}
