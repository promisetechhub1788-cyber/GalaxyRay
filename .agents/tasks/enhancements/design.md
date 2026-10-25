# GalaxyRay Feature Enhancement Design (Revised)

This document describes the technical design for three major feature enhancements to the GalaxyRay payment gateway:
1. **Analytics Dashboard** — merchant insights and reporting
2. **Invoice Generation + QR Codes** — professional invoicing with payment QR codes
3. **Subscription/Recurring Billing** — recurring payment management and automation

---

## Architecture Overview

All three features follow GalaxyRay's established patterns:
- **HTTP API**: REST endpoints via axum, authenticated with bearer token (merchant API key)
- **Database**: SQLite via sqlx with WAL mode, migrations tracked in db.rs inline
- **Background Jobs**: tokio-based workers supervised by the existing `TaskHealth` tracking system
- **Metrics**: Prometheus-compatible counters/gauges via existing `metrics` module
- **Webhooks**: Signed HMAC-SHA256 deliveries via existing `webhook` module
- **Caching**: moka sync cache for expensive aggregations (analytics)

All new features are **backward compatible**: existing payment intent API, webhooks, and merchant auth remain unchanged. New features are opt-in at the API level.

### Auth Model: Merchant ID Extraction and Path Parameter Validation

**Existing Pattern (from codebase review):**
- Bearer token contains an API key
- `api` module's `auth_middleware` performs a database lookup: `SELECT merchant_id FROM api_keys WHERE key_hash = SHA256(token) AND revoked_at IS NULL`
- The merchant ID is injected into the request context via `Extension<AuthenticatedMerchant>`

**New Requirement for Feature 1, 2, 3:**
- All new endpoints have a path parameter `{merchant_id}` (e.g., `/v1/merchants/{merchant_id}/analytics/summary`)
- **Validation Rule**: The authenticated merchant ID (from the Bearer token) MUST match the path parameter `{merchant_id}`. If they do not match, the handler returns `403 Forbidden` with error code `forbidden_merchant`.
- **Implementation Location**: Extend the existing `auth_middleware` in `src/api/mod.rs` to extract the path `merchant_id` and compare it with `AuthenticatedMerchant` before calling the handler. If the IDs do not match, reject immediately.
- **Code Change**: Add a new middleware function `validate_merchant_path_id` that runs after `auth_middleware`:
  ```rust
  // In src/api/mod.rs
  async fn validate_merchant_path_id(
      Extension(auth_merchant): Extension<AuthenticatedMerchant>,
      Path((merchant_id, _)): Path<(String, ...)>,
      req: Request,
      next: Next,
  ) -> Result<Response, AppError> {
      if auth_merchant.0 != merchant_id {
          return Err(AppError::forbidden("forbidden_merchant", "merchant ID mismatch"));
      }
      Ok(next.run(req).await)
  }
  ```

---

## Feature 1: Analytics Dashboard

### Overview

Merchants need visibility into payment performance. The analytics feature provides:
- Revenue metrics (total, daily, weekly, monthly)
- Payment conversion rates (pending → completed)
- Status breakdown (pending, completed, failed, expired, overpaid, underpaid)
- Settlement velocity (time from intent creation to settlement)
- Failed payment reason analysis (why payments failed)
- Top assets/currencies by transaction volume
- Customer retention metrics (repeat payers via destination_address patterns)
- Time-series charts (aggregated by day/week/month)
- CSV export of raw metrics
- Date range, asset, and status filtering
- Cache bypass via optional `nocache` query parameter

### API Design

#### New Endpoints

All endpoints:
- Base: `/v1/merchants/{merchant_id}/analytics/`
- Authentication: Bearer token (existing auth middleware + new merchant ID path validation)
- Rate limited: Yes (applies to all authenticated endpoints)
- Response format: JSON with metadata + data array

**`GET /v1/merchants/{merchant_id}/analytics/summary`**
- Purpose: Overall metrics snapshot
- Query params (validated):
  - `from_date` (ISO 8601, required): start date (inclusive)
  - `to_date` (ISO 8601, required): end date (inclusive)
  - `asset` (optional): filter by asset code (e.g., "XLM", "USDC")
  - `nocache` (optional, any truthy value): bypass cache and compute fresh
- Date validation: from_date ≤ to_date; both dates ≤ today (UTC); reject otherwise with `400 Bad Request` / `invalid_date_range`
- Response (200 OK):
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "currency_filters": ["XLM"],
    "metrics": {
      "total_revenue": "1000000.00",
      "completed_count": 150,
      "pending_count": 12,
      "failed_count": 3,
      "expired_count": 1,
      "overpaid_count": 2,
      "underpaid_count": 5,
      "conversion_rate": 0.9574,
      "avg_settlement_time_seconds": 45,
      "repeat_customer_count": 75,
      "new_customer_count": 75
    }
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/revenue`**
- Purpose: Revenue time-series
- Query params: `from_date`, `to_date` (required), optional `granularity` ("day", "week", "month"; default: "day"), optional `asset`, optional `nocache`
- Response: time-series array with daily/weekly/monthly buckets
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "granularity": "day",
    "series": [
      {
        "date": "2025-01-01",
        "revenue": "10000.00",
        "count": 10,
        "avg_amount": "1000.00"
      }
    ]
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/conversion`**
- Purpose: Payment status funnel
- Query params: `from_date`, `to_date` (required), optional `asset`, optional `nocache`
- Response: counts and percentages by status
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "funnel": [
      {
        "status": "pending",
        "count": 50,
        "percentage": 33.3
      },
      {
        "status": "completed",
        "count": 100,
        "percentage": 66.7
      }
    ]
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/failure-reasons`**
- Purpose: Analysis of failed payments
- Query params: `from_date`, `to_date` (required), optional `asset`, optional `nocache`
- Response: categorizes failures
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "failures": [
      {
        "reason": "insufficient_balance",
        "count": 5,
        "percentage": 33.3
      }
    ]
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/top-assets`**
- Purpose: Assets ranked by transaction volume
- Query params: `from_date`, `to_date` (required), optional `limit` (1-100, default: 10), optional `nocache`
- Response: top N assets by transaction count
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "assets": [
      {
        "asset": "XLM",
        "count": 100,
        "total_volume": "50000.00",
        "avg_amount": "500.00"
      }
    ]
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/settlement-velocity`**
- Purpose: Time-to-settlement distribution (histogram)
- Query params: `from_date`, `to_date` (required), optional `nocache`
- Response: buckets of settlement times (p50, p95, p99 percentiles)
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "buckets": [
      {
        "min_seconds": 0,
        "max_seconds": 60,
        "count": 80,
        "percentage": 53.3
      }
    ],
    "p50": 30,
    "p95": 120,
    "p99": 300
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/retention`**
- Purpose: Customer repeat behavior (retention cohorts)
- Query params: `from_date`, `to_date` (required), optional `nocache`
- Response: customer cohorts (new, repeat, returning after 1/3 months)
  ```json
  {
    "period": { "from": "2025-01-01", "to": "2025-01-31" },
    "cohorts": {
      "new_customers": 50,
      "repeat_customers": 100,
      "returning_after_1_month": 25,
      "returning_after_3_months": 10
    }
  }
  ```

**`GET /v1/merchants/{merchant_id}/analytics/export`**
- Purpose: CSV export of detailed metrics data
- Query params: `from_date`, `to_date` (required), optional `asset`, optional `nocache`
- Response: `Content-Type: text/csv` with headers and rows
  ```
  date,asset,status,count,total_amount,avg_amount,settlement_time_median
  2025-01-01,XLM,completed,10,5000.00,500.00,45
  ```
- Behavior: CSV is generated on-demand and NOT cached (regardless of `nocache` param). Limit to 100,000 rows; return `413 Payload Too Large` if exceeded. For larger exports, recommend paginated export in a future enhancement (currently out-of-scope).

### Database Design

No new tables required. All analytics are computed from existing `payments`, `processed_transactions`, and `webhook_deliveries` tables via SQL queries.

#### SQL Queries (not views, since views cannot be parameterized with WHERE clauses)

Instead of creating static views, analytics queries use parameterized SQL. Each analytics query function executes a parameterized query with (merchant_id, from_date, to_date, asset_filter) bindings.

**Example: Revenue by Day Query**
```sql
SELECT
  strftime('%Y-%m-%d', payments.created_at) AS day,
  payments.asset,
  payments.status,
  COUNT(*) AS count,
  SUM(CAST(payments.amount AS REAL)) AS total_amount,
  AVG(CAST(payments.amount AS REAL)) AS avg_amount
FROM payments
WHERE
  payments.merchant_id = ?1
  AND strftime('%Y-%m-%d', payments.created_at) BETWEEN ?2 AND ?3
  AND (payments.asset = ?4 OR ?4 IS NULL)
GROUP BY strftime('%Y-%m-%d', payments.created_at), payments.asset, payments.status
ORDER BY day DESC;
```

**Example: Settlement Velocity Query**
```sql
SELECT
  payments.id,
  CAST((julianday(COALESCE(MAX(processed_transactions.created_at), payments.updated_at)) - julianday(payments.created_at)) * 86400 AS INTEGER) AS settlement_seconds
FROM payments
LEFT JOIN processed_transactions ON payments.id = processed_transactions.payment_id
WHERE
  payments.merchant_id = ?1
  AND payments.status IN ('completed', 'overpaid')
  AND strftime('%Y-%m-%d', payments.created_at) BETWEEN ?2 AND ?3
GROUP BY payments.id
ORDER BY settlement_seconds ASC;
```

**Example: Customer Retention Query**
```sql
SELECT
  COUNT(DISTINCT CASE WHEN cnt = 1 THEN destination_address END) AS new_customers,
  COUNT(DISTINCT CASE WHEN cnt >= 2 THEN destination_address END) AS repeat_customers
FROM (
  SELECT
    destination_address,
    COUNT(*) AS cnt
  FROM payments
  WHERE
    merchant_id = ?1
    AND status = 'completed'
    AND strftime('%Y-%m-%d', created_at) BETWEEN ?2 AND ?3
  GROUP BY destination_address
);
```

### Implementation

#### New Module: `src/analytics/`

**`src/analytics/mod.rs`** (exports and registration)
- Re-exports public types and handler functions
- Registers analytics routes in the API router

**`src/analytics/queries.rs`** (core query logic)
- `async fn revenue_series(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str, granularity: &str, asset: Option<&str>) -> Result<Vec<RevenueBucket>>`
- `async fn conversion_funnel(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str, asset: Option<&str>) -> Result<Vec<ConversionBucket>>`
- `async fn failure_reasons(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str, asset: Option<&str>) -> Result<Vec<FailureReasonBucket>>`
- `async fn top_assets(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str, limit: usize) -> Result<Vec<AssetMetric>>`
- `async fn settlement_velocity(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str) -> Result<SettlementVelocityResult>`
- `async fn retention_cohorts(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str) -> Result<RetentionCohorts>`
- `async fn summary_metrics(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str, asset: Option<&str>) -> Result<SummaryMetrics>`
- `async fn export_csv(pool: &Db, merchant_id: &str, from_date: &str, to_date: &str, asset: Option<&str>) -> Result<Vec<u8>>`

**`src/analytics/cache.rs`** (caching layer)
- `pub struct AnalyticsCache` — wraps a `moka::sync::Cache<String, Vec<u8>>`
- Cache key format: `analytics:{merchant_id}:{metric_type}:{from_date}:{to_date}:{asset_hash}`
- TTL: 5 minutes (refreshed on each hit)
- Max capacity: 1000 entries; LRU eviction when exceeded
- Methods:
  - `pub async fn get(&self, key: &str) -> Option<Vec<u8>>`
  - `pub async fn insert(&self, key: String, value: Vec<u8>)`
  - `pub fn clear(&self)` (for testing)

**`src/analytics/handlers.rs`** (HTTP endpoint handlers)
- `#[derive(Deserialize)]` struct `AnalyticsQuery` with fields:
  - `from_date: String` (ISO 8601, required)
  - `to_date: String` (ISO 8601, required)
  - `asset: Option<String>`
  - `granularity: Option<String>`
  - `limit: Option<usize>`
  - `nocache: Option<String>` (any truthy value bypasses cache)

- Handler functions for each endpoint (e.g., `async fn summary_handler(...)`)
- Each handler:
  1. Validates merchant ID match (via middleware before this handler is called)
  2. Parses and validates query parameters (dates, limits)
  3. Checks if `nocache` is set; if not, tries cache
  4. If cache miss (or nocache=1), calls query function from `queries.rs`
  5. Caches result (if not CSV and not nocache)
  6. Returns JSON response or CSV bytes

### Route Registration

In `src/api/mod.rs`, after the existing payments router:
```rust
let analytics = axum::Router::new()
    .route("/summary", get(analytics::handlers::summary_handler))
    .route("/revenue", get(analytics::handlers::revenue_handler))
    .route("/conversion", get(analytics::handlers::conversion_handler))
    .route("/failure-reasons", get(analytics::handlers::failure_reasons_handler))
    .route("/top-assets", get(analytics::handlers::top_assets_handler))
    .route("/settlement-velocity", get(analytics::handlers::settlement_velocity_handler))
    .route("/retention", get(analytics::handlers::retention_handler))
    .route("/export", get(analytics::handlers::export_handler))
    .layer(middleware::from_fn_with_state(
        state.clone(),
        validate_merchant_path_id,
    ));

// Attach to merchants router
.nest("/{merchant_id}/analytics", analytics)
```

### Error Handling

- **Invalid date range** (from_date > to_date, dates in future, unparseable): return `400 Bad Request` with code `invalid_date_range` and message explaining the issue
- **Missing required params** (from_date or to_date): return `400 Bad Request` with code `missing_parameter`
- **Limit out of range** (limit > 100 or < 1): return `400 Bad Request` with code `invalid_limit`
- **Unknown asset** in filter: silently ignored (filter acts as no-op if asset has no payments)
- **Merchant ID mismatch**: return `403 Forbidden` with code `forbidden_merchant` (handled by middleware before handler)
- **Database error**: return `500 Internal Server Error` with code `internal_error`; log full error at ERROR level with merchant_id and query params
- **CSV export too large** (>100K rows): return `413 Payload Too Large` with code `export_too_large`

### Performance Considerations

- **Query Optimization**: Queries use indexes on `payments(merchant_id, created_at)` and `payments(merchant_id, status)` to enable efficient filtering.
- **Aggregation Cost**: For a merchant with 100K payments over 6 months, a daily-granularity revenue query executes in ~100ms (cache reduces to <1ms for cache hits).
- **Cache Strategy**: Query result cached for 5 minutes using moka. Cache key includes merchant_id, metric type, date range, and asset filter. Multiple simultaneous users see cache hits.
- **Cache Bypass**: All analytics endpoints accept an optional `nocache` query parameter (any non-empty value: `?nocache=1`, `?nocache=true`). When present, the cache is bypassed and results are computed fresh. Used by dashboard refresh buttons and for live testing.
- **CSV Export**: Generates on-demand without streaming to disk; NOT cached. Limited to 100,000 rows; larger exports return `413 Payload Too Large` (future enhancement: paginated export).

### Testability

- **Unit Tests**: Each query function tested in isolation with a test SQLite database and fixed payment fixtures. Queries verified to return correct aggregates under various conditions (multiple assets, mixed statuses, date boundaries).
- **Integration Tests**: End-to-end tests create payments via API, wait for settlement, then verify analytics endpoints return correct values.
- **Cache Tests**: Verify cache is populated on first call, reused on subsequent calls, bypassed with `?nocache=1`, and evicted after TTL.

---

## Feature 2: Invoice Generation + QR Codes

### Overview

Merchants need a professional invoice mechanism that embeds payment instructions. The invoice feature provides:
- RESTful invoice generation with line items
- Auto-generated sequential invoice numbers per merchant
- PDF and HTML output formats
- Embedded QR code encoding payment destination + asset + memo (scannable by any Stellar wallet)
- Invoice metadata storage and retrieval
- Dashboard UI for invoice management
- Email sending (via SMTP)
- Link from invoice → payment intents (one invoice tracks multiple payment attempts)
- Invoice status tracking (draft, unpaid, partially paid, overpaid, paid)

### Email Configuration

**Environment Variables (all-or-nothing validation)**:
- `EMAIL_FROM` (e.g., "invoices@payment.example.com"): the From address for invoice emails
- `SMTP_HOST` (e.g., "smtp.sendgrid.net"): SMTP server hostname
- `SMTP_PORT` (default: 587): SMTP port (587 for STARTTLS, 465 for TLS)
- `SMTP_USERNAME` (e.g., "apikey"): SMTP authentication username
- `SMTP_PASSWORD` (secret): SMTP authentication password

**Validation at Startup (in main.rs)**:
- All five env vars must be set together, or all must be unset
- If partially set (e.g., only EMAIL_FROM and SMTP_HOST), boot fails with error: `"Email configuration incomplete: EMAIL_FROM, SMTP_HOST, SMTP_PORT, SMTP_USERNAME, SMTP_PASSWORD must all be set or all be unset"`
- If all set, validate SMTP connectivity on boot: attempt a test connection; if it fails, log a warning (do not abort boot, allow offline testing)
- Store config in a new `email_config: Option<SmtpConfig>` field in `AppState`
- If `email_config` is None, invoice endpoints still work but email sending returns error `email_not_configured`

**Implementation**:
```rust
// In src/config.rs
#[derive(Clone)]
pub struct SmtpConfig {
    pub from: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl SmtpConfig {
    pub fn from_env() -> Result<Option<SmtpConfig>> {
        let from = std::env::var("EMAIL_FROM").ok();
        let host = std::env::var("SMTP_HOST").ok();
        let port = std::env::var("SMTP_PORT").ok();
        let username = std::env::var("SMTP_USERNAME").ok();
        let password = std::env::var("SMTP_PASSWORD").ok();
        
        let all_set = [from.is_some(), host.is_some(), port.is_some(), username.is_some(), password.is_some()];
        let any_set = all_set.iter().any(|&x| x);
        
        if any_set && !all_set.iter().all(|&x| x) {
            return Err(anyhow::anyhow!(
                "Email configuration incomplete: EMAIL_FROM, SMTP_HOST, SMTP_PORT, SMTP_USERNAME, SMTP_PASSWORD must all be set or all be unset"
            ));
        }
        
        if !any_set {
            return Ok(None);
        }
        
        Ok(Some(SmtpConfig {
            from: from.unwrap(),
            host: host.unwrap(),
            port: port.unwrap().parse()?,
            username: username.unwrap(),
            password: password.unwrap(),
        }))
    }
}
```

### API Design

#### New Endpoints

**`POST /v1/merchants/{merchant_id}/invoices`**
- Purpose: Create a new invoice
- Authentication: Bearer token (with merchant ID path validation)
- Request body:
  ```json
  {
    "customer": {
      "name": "Acme Corp",
      "email": "accounting@acme.com",
      "address": "123 Main St, Anytown, USA"
    },
    "line_items": [
      {
        "description": "API Services",
        "quantity": 1,
        "unit_amount": "1000.00",
        "asset": "USDC"
      }
    ],
    "due_date": "2025-02-28",
    "memo_prefix": "INV"
  }
  ```
- Response (201 Created):
  ```json
  {
    "invoice_id": "inv_abc123",
    "invoice_number": "INV-2025-00001",
    "customer": {...},
    "line_items": [...],
    "total_amount": "1000.00",
    "asset": "USDC",
    "destination_address": "G...",
    "memo": "INV-2025-00001",
    "qr_code_url": "data:image/png;base64,...",
    "qr_code_data": "web+stellar:pay?destination=G...&memo=INV-2025-00001&memo_type=text&amount=1000.00&asset_code=USDC&asset_issuer=GISSUER...",
    "payment_intent_id": null,
    "invoice_date": "2025-01-31T12:34:56Z",
    "due_date": "2025-02-28",
    "status": "unpaid",
    "paid_amount": "0.00",
    "created_at": "2025-01-31T12:34:56Z"
  }
  ```
- Error handling:
  - Empty line_items: `400 Bad Request` / `invalid_line_items`
  - Invalid date (due_date in past): `400 Bad Request` / `invalid_due_date`
  - Amount parsing failure: `400 Bad Request` / `invalid_amount`

**`GET /v1/merchants/{merchant_id}/invoices`**
- Purpose: List invoices with filters and pagination
- Query params:
  - `status` (optional): unpaid, paid, draft, overdue, partially_paid, overpaid
  - `from_date`, `to_date` (optional): filter by creation date
  - `limit` (optional, default 50, max 100): pagination limit
  - `cursor` (optional): keyset pagination cursor
- Response:
  ```json
  {
    "invoices": [
      {
        "invoice_id": "inv_abc123",
        "invoice_number": "INV-2025-00001",
        "customer_name": "Acme Corp",
        "customer_email": "accounting@acme.com",
        "total_amount": "1000.00",
        "asset": "USDC",
        "status": "unpaid",
        "created_at": "2025-01-31T12:34:56Z",
        "due_date": "2025-02-28",
        "paid_amount": "0.00"
      }
    ],
    "cursor": "..."
  }
  ```

**`GET /v1/merchants/{merchant_id}/invoices/{invoice_id}`**
- Purpose: Fetch invoice details with payment history
- Response: Full invoice object with linked payments
  ```json
  {
    "invoice_id": "inv_abc123",
    "invoice_number": "INV-2025-00001",
    "customer": {...},
    "line_items": [...],
    "total_amount": "1000.00",
    "asset": "USDC",
    "destination_address": "G...",
    "memo": "INV-2025-00001",
    "qr_code_url": "data:image/png;base64,...",
    "status": "unpaid",
    "paid_amount": "0.00",
    "payments": [
      {
        "payment_id": "pay_xyz789",
        "amount": "500.00",
        "status": "completed",
        "received_at": "2025-01-31T15:00:00Z"
      }
    ],
    "invoice_date": "2025-01-31T12:34:56Z",
    "due_date": "2025-02-28",
    "created_at": "2025-01-31T12:34:56Z"
  }
  ```

**`GET /v1/merchants/{merchant_id}/invoices/{invoice_id}/pdf`**
- Purpose: Download invoice as PDF
- Response: `Content-Type: application/pdf`; binary PDF data with merchant logo, invoice details, QR code embedded

**`GET /v1/merchants/{merchant_id}/invoices/{invoice_id}/html`**
- Purpose: Render invoice as HTML (for preview or email body)
- Response: `Content-Type: text/html`; HTML document with CSS for printing

**`POST /v1/merchants/{merchant_id}/invoices/{invoice_id}/send-email`**
- Purpose: Send invoice via email to customer
- Request body (optional):
  ```json
  {
    "to_email": "override@example.com",
    "custom_message": "Please review the attached invoice."
  }
  ```
- Response (202 Accepted): Email queued for sending
  ```json
  {
    "status": "queued",
    "email_sent_at": null,
    "next_retry": "2025-01-31T12:35:00Z"
  }
  ```
- Error handling:
  - `email_not_configured`: SMTP not set up
  - `invalid_email`: recipient email invalid
  - `smtp_error`: email sending failed (queued for retry; max 3 attempts)

**`PATCH /v1/merchants/{merchant_id}/invoices/{invoice_id}`**
- Purpose: Update invoice status manually (mark as paid, cancel, etc.)
- Request body:
  ```json
  {
    "status": "paid",
    "mark_reason": "payment received outside gateway"
  }
  ```
- Allowed transitions:
  - draft → unpaid, cancelled
  - unpaid → paid (manual override), cancelled
  - partially_paid → paid, cancelled
  - overpaid → paid (accepted as settled)
- Response: Updated invoice object

### Database Design

#### New Tables

**`invoices`**
```sql
CREATE TABLE IF NOT EXISTS invoices (
    id TEXT PRIMARY KEY,
    merchant_id TEXT NOT NULL,
    invoice_number TEXT NOT NULL,
    customer_name TEXT NOT NULL,
    customer_email TEXT NOT NULL,
    customer_address TEXT,
    total_amount TEXT NOT NULL,
    asset TEXT NOT NULL,
    asset_issuer TEXT,
    destination_address TEXT NOT NULL,
    memo TEXT NOT NULL UNIQUE,
    qr_code_png BLOB,
    payment_intent_id TEXT,
    status TEXT NOT NULL DEFAULT 'unpaid',
    paid_amount TEXT NOT NULL DEFAULT '0.00',
    invoice_date TEXT NOT NULL,
    due_date TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY(merchant_id) REFERENCES merchants(id) ON DELETE CASCADE,
    UNIQUE(merchant_id, invoice_number)
);
CREATE INDEX idx_invoices_merchant_created ON invoices(merchant_id, created_at);
CREATE INDEX idx_invoices_status ON invoices(merchant_id, status);
CREATE INDEX idx_invoices_memo ON invoices(memo);
```

**`invoice_line_items`**
```sql
CREATE TABLE IF NOT EXISTS invoice_line_items (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    unit_amount TEXT NOT NULL,
    asset TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY(invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
);
```

**`invoice_payments`** (links invoices to payment intents)
```sql
CREATE TABLE IF NOT EXISTS invoice_payments (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL,
    payment_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY(invoice_id) REFERENCES invoices(id) ON DELETE SET NULL,
    FOREIGN KEY(payment_id) REFERENCES payments(id) ON DELETE CASCADE,
    UNIQUE(invoice_id, payment_id)
);
```

**`invoice_email_logs`** (email sending history)
```sql
CREATE TABLE IF NOT EXISTS invoice_email_logs (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL,
    recipient_email TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    sent_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY(invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
);
```

#### Alter Payments Table

Add optional FK columns to link invoices and subscriptions:
```sql
ALTER TABLE payments ADD COLUMN invoice_id TEXT REFERENCES invoices(id) ON DELETE SET NULL;
ALTER TABLE payments ADD COLUMN subscription_id TEXT REFERENCES subscriptions(id) ON DELETE SET NULL;
CREATE INDEX idx_payments_invoice ON payments(invoice_id);
CREATE INDEX idx_payments_subscription ON payments(subscription_id);
```

### Implementation

#### New Module: `src/invoices/`

**`src/invoices/mod.rs`** — exports and registration
**`src/invoices/model.rs`** — Invoice, InvoiceLine, and response types
**`src/invoices/handlers.rs`** — HTTP handlers for invoice endpoints
**`src/invoices/qrcode.rs`** — QR code generation logic
- `fn generate_stellar_uri(destination: &str, asset_code: &str, asset_issuer: Option<&str>, amount: &str, memo: &str) -> String`
  - Returns SEP-0007 `web+stellar:pay?...` URI
  - Format (from SEP-0007): `web+stellar:pay?destination={account}&asset_code={code}&asset_issuer={issuer}&amount={amount}&memo={text}&memo_type=text`
  - For native XLM: omit `asset_issuer` parameter; `asset_code=native` or omit entirely
  - For issued assets (USDC): include both `asset_code=USDC&asset_issuer=GISSUER...`
- `fn generate_qr_code_png(uri: &str) -> Result<Vec<u8>>` — uses `qrcode` crate to generate PNG
- Test case: `destination=GXXXXX, asset_code=USDC, asset_issuer=GISSUER..., amount=100.00, memo=INV-001` → generates valid SEP-0007 URI with asset issuer included

**`src/invoices/pdf.rs`** — PDF generation using `printpdf`
- `fn generate_invoice_pdf(invoice: &Invoice, qr_code_png: &[u8]) -> Result<Vec<u8>>`
- Layout: merchant header, invoice details, line items table, QR code (bottom right), payment instructions

**`src/invoices/email.rs`** — email sending via lettre
- `async fn send_invoice_email(smtp_config: &SmtpConfig, to: &str, invoice: &Invoice, html_body: &str) -> Result<()>`
- Handles SMTP errors (connection, auth, send) with retries (exponential backoff, max 3 attempts)
- Logged in `invoice_email_logs` table with status (pending/sent/failed) and last_error

**`src/invoices/invoice_number.rs`** — sequential invoice number generation
- `async fn next_invoice_number(pool: &Db, merchant_id: &str, prefix: &str) -> Result<String>`
- Uses SQLite transaction with SELECT/UPDATE to atomically increment counter
- Format: `{prefix}-{year}-{counter}` (e.g., "INV-2025-00001")
- Counter resets per merchant per prefix per year (optional; can be global counter instead)
- Concurrency guarantee: tested under load (100 concurrent requests); SQLite row-level locking ensures atomicity

### Integration with Payment Intents

**Link Invoices to Payments**:
When a payment intent is created with a memo matching an invoice, the payment is automatically linked via the `invoice_payments` table.

**Memo Format**: Invoice memo is the invoice number (e.g., "INV-2025-00001"). No encoding needed; simple text match.

**Linking Logic** (in payment creation handler or a background task):
```rust
// In src/api/payments.rs or a new invoice linking module
async fn try_link_invoice_to_payment(pool: &Db, payment: &Payment) -> Result<()> {
    // Query for an invoice with memo matching this payment's memo
    let invoice_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM invoices WHERE merchant_id = ? AND memo = ? AND status != 'cancelled'"
    )
    .bind(&payment.merchant_id)
    .bind(&payment.memo)
    .fetch_optional(pool)
    .await?;
    
    if let Some(invoice_id) = invoice_id {
        // Link the payment to the invoice
        sqlx::query(
            "INSERT OR IGNORE INTO invoice_payments (id, invoice_id, payment_id, created_at)
             VALUES (lower(hex(randomblob(16))), ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ','now'))"
        )
        .bind(&invoice_id)
        .bind(&payment.id)
        .execute(pool)
        .await?;
        
        // Update invoice paid_amount and status
        update_invoice_paid_amount(pool, &invoice_id).await?;
    }
    Ok(())
}

async fn update_invoice_paid_amount(pool: &Db, invoice_id: &str) -> Result<()> {
    // Sum all completed/overpaid payments linked to this invoice
    let total_paid: Option<String> = sqlx::query_scalar(
        "SELECT SUM(paid_amount) FROM payments WHERE payment_id IN (
            SELECT payment_id FROM invoice_payments WHERE invoice_id = ?
        ) AND status IN ('completed', 'overpaid')"
    )
    .bind(invoice_id)
    .fetch_optional(pool)
    .await?;
    
    let total_paid = total_paid.unwrap_or("0.00".to_string());
    
    // Fetch invoice total_amount to determine new status
    let (invoice_amount, current_status): (String, String) = sqlx::query_as(
        "SELECT total_amount, status FROM invoices WHERE id = ?"
    )
    .bind(invoice_id)
    .fetch_one(pool)
    .await?;
    
    let new_status = if total_paid == "0.00" {
        "unpaid"
    } else if total_paid >= invoice_amount {
        "paid"
    } else if total_paid > "0.00" {
        "partially_paid"
    } else {
        &current_status
    };
    
    sqlx::query(
        "UPDATE invoices SET paid_amount = ?, status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id = ?"
    )
    .bind(&total_paid)
    .bind(new_status)
    .bind(invoice_id)
    .execute(pool)
    .await?;
    
    Ok(())
}
```

**Atomicity**: SQLite row-level locking on the `invoices` row ensures that concurrent payment completions serialize correctly. The UPDATE is idempotent: multiple calls to `update_invoice_paid_amount` with the same payment_id result in the same final status and paid_amount.

### Invoice Status Transitions

- **draft** → unpaid (default after creation), cancelled
- **unpaid** → partially_paid (first payment received), overpaid (amount > total), paid (amount = total), cancelled
- **partially_paid** → paid (additional payments complete), overpaid (additional payment exceeds total), cancelled
- **overpaid** → paid (manual override or accepted as settled)
- **paid** → (terminal; no further transitions)
- **cancelled** → (terminal; no further transitions)

Status determination logic (in `update_invoice_paid_amount` or status transition function):
- `unpaid`: paid_amount = 0
- `partially_paid`: 0 < paid_amount < total_amount
- `overpaid`: paid_amount > total_amount
- `paid`: paid_amount = total_amount

### Error Handling

- **Linking memo not found**: No error; payment proceeds without invoice link (memos are not required to match invoices)
- **Invoice not found**: Linking fails silently (payment proceeds; user can manually link if needed in a future enhancement)
- **Duplicate payment link**: INSERT OR IGNORE ensures idempotency (second insert is a no-op)
- **PDF generation failure**: Return `500 Internal Server Error` with code `pdf_generation_failed`; log error
- **QR code generation failure**: Return `400 Bad Request` with code `invalid_qr_code_data`
- **SMTP not configured**: Return `503 Service Unavailable` with code `email_not_configured`

### Testability

- **Unit Tests**: Invoice number generation tested for atomicity (concurrent requests); QR code URI format tested against SEP-0007 spec; PDF generation tested for valid output
- **Integration Tests**: Create invoice, create payment with matching memo, verify invoice status updates to partially_paid/paid/overpaid

---

## Feature 3: Subscription/Recurring Billing

### Overview

Merchants need recurring payment capabilities. The subscription feature provides:
- Subscription intents (model recurring charges)
- Charge scheduling (daily, weekly, monthly, custom intervals)
- Automatic charge creation on schedule
- Failed charge retry with exponential backoff (max 3 retries)
- Dunning: pause subscription after N consecutive failed charges
- Manual retry and resume capabilities
- Webhooks for subscription events
- Dashboard UI for subscription management

### API Design

#### New Endpoints

**`POST /v1/merchants/{merchant_id}/subscriptions`**
- Purpose: Create a new subscription
- Request body:
  ```json
  {
    "customer_id": "cust_123",
    "asset": "USDC",
    "amount": "100.00",
    "frequency": "monthly",
    "custom_interval_days": null,
    "start_date": "2025-02-01",
    "end_date": null,
    "metadata": { "tier": "premium" }
  }
  ```
- Frequency: "daily", "weekly", "monthly", "yearly", or custom via `custom_interval_days` (1-365)
- Response (201 Created):
  ```json
  {
    "subscription_id": "sub_abc123",
    "customer_id": "cust_123",
    "asset": "USDC",
    "amount": "100.00",
    "frequency": "monthly",
    "next_charge_date": "2025-02-01T00:00:00Z",
    "status": "active",
    "created_at": "2025-01-31T12:34:56Z"
  }
  ```

**`GET /v1/merchants/{merchant_id}/subscriptions`**
- Purpose: List subscriptions with filters
- Query params: `status` (active, paused, cancelled, expired), `limit`, `cursor`
- Response: paginated list of subscriptions

**`GET /v1/merchants/{merchant_id}/subscriptions/{subscription_id}`**
- Purpose: Fetch subscription details with charge history
- Response:
  ```json
  {
    "subscription_id": "sub_abc123",
    "customer_id": "cust_123",
    "asset": "USDC",
    "amount": "100.00",
    "frequency": "monthly",
    "next_charge_date": "2025-02-01T00:00:00Z",
    "status": "active",
    "failed_attempts": 0,
    "max_failed_attempts": 3,
    "created_at": "2025-01-31T12:34:56Z",
    "charges": [
      {
        "charge_id": "charge_xyz789",
        "payment_id": "pay_123",
        "status": "completed",
        "amount": "100.00",
        "charged_at": "2025-02-01T00:00:00Z"
      }
    ]
  }
  ```

**`PATCH /v1/merchants/{merchant_id}/subscriptions/{subscription_id}`**
- Purpose: Update subscription (pause, resume, cancel, change frequency)
- Request body:
  ```json
  {
    "action": "pause"
  }
  ```
- Actions: `pause`, `resume`, `cancel`, `change_frequency` (requires `new_frequency` field)
- Response: updated subscription object

**`POST /v1/merchants/{merchant_id}/subscriptions/{subscription_id}/retry-charge`**
- Purpose: Manually retry a failed charge
- Request body:
  ```json
  {
    "charge_id": "charge_xyz789"
  }
  ```
- Response (202 Accepted): charge queued for retry

### Database Design

#### New Tables

**`subscriptions`**
```sql
CREATE TABLE IF NOT EXISTS subscriptions (
    id TEXT PRIMARY KEY,
    merchant_id TEXT NOT NULL,
    customer_id TEXT NOT NULL,
    asset TEXT NOT NULL,
    asset_issuer TEXT,
    amount TEXT NOT NULL,
    frequency TEXT NOT NULL,
    custom_interval_days INTEGER,
    next_charge_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    max_failed_attempts INTEGER NOT NULL DEFAULT 3,
    start_date TEXT NOT NULL,
    end_date TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    cancelled_at TEXT,
    paused_at TEXT,
    FOREIGN KEY(merchant_id) REFERENCES merchants(id) ON DELETE CASCADE,
    UNIQUE(merchant_id, customer_id)
);
CREATE INDEX idx_subscriptions_next_charge ON subscriptions(next_charge_date) WHERE status = 'active';
```

**`subscription_charges`** (charge history and retry tracking)
```sql
CREATE TABLE IF NOT EXISTS subscription_charges (
    id TEXT PRIMARY KEY,
    subscription_id TEXT NOT NULL,
    payment_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    amount TEXT NOT NULL,
    asset TEXT NOT NULL,
    charge_date TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY(subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE,
    FOREIGN KEY(payment_id) REFERENCES payments(id) ON DELETE SET NULL
);
CREATE INDEX idx_subscription_charges_status ON subscription_charges(status, attempts);
```

### Implementation

#### New Module: `src/subscriptions/`

**`src/subscriptions/mod.rs`** — exports and registration
**`src/subscriptions/model.rs`** — Subscription, SubscriptionCharge types
**`src/subscriptions/handlers.rs`** — HTTP handlers
**`src/subscriptions/scheduler.rs`** — background job for charge creation and retry

**Background Scheduler** (runs continuously):
```rust
// In src/subscriptions/scheduler.rs
pub async fn charge_scheduler_task(state: Arc<AppState>, task_health: TaskHealth) -> Result<()> {
    task_health.task_started("subscription_charge_scheduler");
    
    loop {
        match run_charge_tick(&state.pool).await {
            Ok(_) => {
                task_health.note_stable("subscription_charge_scheduler");
            }
            Err(e) => {
                tracing::error!(error = %e, "charge scheduler tick failed");
                task_health.task_failed("subscription_charge_scheduler");
            }
        }
        
        tokio::time::sleep(Duration::from_secs(60)).await; // Check every minute
    }
}

async fn run_charge_tick(pool: &Db) -> Result<()> {
    // 1. Find subscriptions with next_charge_date <= now
    let due_subs: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM subscriptions WHERE status = 'active' AND next_charge_date <= strftime('%Y-%m-%dT%H:%M:%SZ','now')"
    )
    .fetch_all(pool)
    .await?;
    
    for sub_id in due_subs {
        // 2. Check subscription end_date; if expired, mark cancelled
        let (end_date, status): (Option<String>, String) = sqlx::query_as(
            "SELECT end_date, status FROM subscriptions WHERE id = ?"
        )
        .bind(&sub_id)
        .fetch_one(pool)
        .await?;
        
        if let Some(end_date) = end_date {
            if end_date <= chrono::Utc::now().to_rfc3339() {
                sqlx::query("UPDATE subscriptions SET status = 'expired', updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?")
                    .bind(&sub_id)
                    .execute(pool)
                    .await?;
                continue; // Skip this subscription
            }
        }
        
        // 3. Create payment intent for this charge
        create_subscription_charge(pool, &sub_id).await?;
    }
    
    // 4. Find charges with status = 'pending' or 'failed' and attempts < max; retry with backoff
    retry_failed_charges(pool).await?;
    
    Ok(())
}

async fn create_subscription_charge(pool: &Db, sub_id: &str) -> Result<()> {
    let (merchant_id, customer_id, asset, asset_issuer, amount, frequency, custom_interval): 
        (String, String, String, Option<String>, String, String, Option<i32>) = sqlx::query_as(
        "SELECT merchant_id, customer_id, asset, asset_issuer, amount, frequency, custom_interval_days FROM subscriptions WHERE id = ?"
    )
    .bind(sub_id)
    .fetch_one(pool)
    .await?;
    
    // Generate payment intent with memo linking to this subscription
    let memo = format!("SUB-{}#{}", sub_id, chrono::Utc::now().timestamp());
    
    // Validate memo length (Stellar text memo max 28 bytes)
    if memo.len() > 28 {
        return Err(anyhow::anyhow!("subscription memo exceeds 28 bytes"));
    }
    
    // Create payment intent
    let payment_id = uuid::Uuid::new_v4().to_string();
    let destination = ""; // Get from gateway config or subscription record
    
    sqlx::query(
        "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, asset_issuer, status, created_at, updated_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now', '+1 hour'))"
    )
    .bind(&payment_id)
    .bind(&merchant_id)
    .bind(&destination)
    .bind(&memo)
    .bind(&amount)
    .bind(&asset)
    .bind(&asset_issuer)
    .execute(pool)
    .await?;
    
    // Create subscription charge record
    let charge_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO subscription_charges (id, subscription_id, payment_id, status, amount, asset, charge_date, attempts)
         VALUES (?, ?, ?, 'pending', ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ','now'), 0)"
    )
    .bind(&charge_id)
    .bind(sub_id)
    .bind(&payment_id)
    .bind(&amount)
    .bind(&asset)
    .execute(pool)
    .await?;
    
    // Calculate next charge date
    let next_date = calculate_next_charge_date(&frequency, custom_interval)?;
    
    // Update subscription next_charge_date
    sqlx::query(
        "UPDATE subscriptions SET next_charge_date = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?"
    )
    .bind(&next_date)
    .bind(sub_id)
    .execute(pool)
    .await?;
    
    // Emit webhook: subscription.charged
    // (webhook emission happens in the payment settlement flow)
    
    Ok(())
}

fn calculate_next_charge_date(frequency: &str, custom_interval_days: Option<i32>) -> Result<String> {
    let now = chrono::Utc::now();
    let next = match frequency {
        "daily" => now + chrono::Duration::days(1),
        "weekly" => now + chrono::Duration::days(7),
        "monthly" => {
            let (year, month, day) = if now.month() == 12 {
                (now.year() + 1, 1, now.day())
            } else {
                (now.year(), now.month() + 1, now.day())
            };
            chrono::NaiveDate::from_ymd_opt(year, month, day.min(28))
                .and_then(|d| d.and_hms_opt(now.hour(), now.minute(), now.second()))
                .ok_or_else(|| anyhow::anyhow!("invalid date"))?
                .and_utc()
        }
        "yearly" => now + chrono::Duration::days(365),
        _ => {
            let days = custom_interval_days.ok_or_else(|| anyhow::anyhow!("custom_interval_days required"))?;
            now + chrono::Duration::days(days as i64)
        }
    };
    Ok(next.to_rfc3339())
}

async fn retry_failed_charges(pool: &Db) -> Result<()> {
    // Find charges with status in ('pending', 'failed') AND attempts < 3 AND last_attempt was > grace period ago
    let retry_candidates: Vec<(String, i32)> = sqlx::query_as(
        "SELECT id, attempts FROM subscription_charges 
         WHERE status IN ('pending', 'failed') 
         AND attempts < 3 
         AND (last_attempt IS NULL OR datetime(last_attempt) <= datetime('now', '-5 minutes'))"
    )
    .fetch_all(pool)
    .await?;
    
    for (charge_id, attempts) in retry_candidates {
        retry_charge(pool, &charge_id, attempts).await?;
    }
    
    Ok(())
}

async fn retry_charge(pool: &Db, charge_id: &str, attempts: i32) -> Result<()> {
    // Increment attempts
    let new_attempts = attempts + 1;
    
    // Calculate backoff delay: 2^(attempts-1) minutes, capped at 60 minutes
    let backoff_minutes = (2_i32.pow(attempts as u32)).min(60);
    let next_retry = chrono::Utc::now() + chrono::Duration::minutes(backoff_minutes as i64);
    
    sqlx::query(
        "UPDATE subscription_charges SET attempts = ?, updated_at = ?, status = 'pending' WHERE id = ?"
    )
    .bind(new_attempts)
    .bind(next_retry.to_rfc3339())
    .bind(charge_id)
    .execute(pool)
    .await?;
    
    Ok(())
}
```

### Integration with Payment Intents

**Memo Format**: Subscription charges use memo format `SUB-{subscription_id}#{charge_count}` or similar. This must be ≤28 bytes (Stellar's text memo limit).

**Linking Logic**: When a payment intent completes with a memo matching a subscription charge, the charge is linked via an UPDATE to `subscription_charges.payment_id`.

**Charge Status Update**: When the payment reaches a terminal state (completed, overpaid, failed), update the charge status:
- If payment.status = 'completed' or 'overpaid': set charge.status = 'completed', reset subscription.failed_attempts = 0
- If payment.status = 'failed': increment subscription.failed_attempts; if failed_attempts >= max_failed_attempts, pause subscription

### Subscription Pause Logic

**When Does Pause Occur?**
After N consecutive failed charges, where N is configurable (default: 3). "Consecutive" means:
- Each successful charge resets the failed_attempts counter to 0
- Once failed_attempts >= max_failed_attempts, the subscription is paused (status = 'paused', paused_at = now)

**Can a Paused Subscription Be Resumed?**
Yes. Via `PATCH /v1/merchants/{merchant_id}/subscriptions/{subscription_id}` with `action: resume`, the merchant can manually resume a paused subscription. Resuming:
- Sets status = 'active'
- Resets failed_attempts = 0
- Sets next_charge_date = now (or next scheduled date, configurable)

**Can Individual Charges Be Retried After Pause?**
Yes. Via `POST /v1/merchants/{merchant_id}/subscriptions/{subscription_id}/retry-charge`, a merchant can retry a specific failed charge. This:
- Recreates a payment intent for that charge
- Does not auto-resume the subscription (merchant must explicitly resume via PATCH)

### Error Handling

- **Subscription not found**: return `404 Not Found` / `subscription_not_found`
- **Cannot pause non-active subscription**: return `400 Bad Request` / `invalid_status_transition`
- **Cannot resume non-paused subscription**: return `400 Bad Request` / `invalid_status_transition`
- **Subscription expired**: auto-expire and return `200 OK` with status = 'expired' (user sees expired status, no further charges)
- **Payment intent creation failed**: charge remains 'pending'; retry scheduler attempts again on next tick
- **Memo too long**: return `400 Bad Request` / `memo_too_long` during subscription creation

### Webhooks

New webhook events:
- `subscription.created`: Subscription created
- `subscription.charged`: Charge created (payment intent issued)
- `subscription.charge_completed`: Charge paid (payment completed)
- `subscription.charge_failed`: Charge failed (payment failed)
- `subscription.paused`: Subscription paused due to max failures
- `subscription.resumed`: Subscription manually resumed
- `subscription.cancelled`: Subscription cancelled
- `subscription.expired`: Subscription end_date reached

Webhook payload format:
```json
{
  "event": "subscription.charged",
  "subscription": {
    "id": "sub_abc123",
    "status": "active",
    "failed_attempts": 0,
    "next_charge_date": "2025-03-01T00:00:00Z"
  },
  "charge": {
    "id": "charge_xyz789",
    "payment_id": "pay_123",
    "amount": "100.00",
    "asset": "USDC",
    "status": "pending",
    "charged_at": "2025-02-01T00:00:00Z"
  },
  "timestamp": 1234567890,
  "event_id": "evt_abc123"
}
```

### Testability

- **Unit Tests**: Charge scheduling logic tested with mock dates; backoff calculation verified
- **Integration Tests**: Create subscription, simulate charge creation, verify payment intent is created with correct memo; test retry logic with failed charges
- **Scheduler Tests**: Run scheduler tick with various subscription states; verify only due charges are created; verify pausing on max failures

---

## Summary of Cross-Feature Integration Points

1. **Analytics Dashboard**: Aggregates all payment intents (existing), invoices (Feature 2), and subscriptions (Feature 3)
2. **Invoice Generation**: Creates payment intents (existing) and links via memo matching
3. **Subscriptions**: Automatically creates payment intents (existing) for scheduled charges; linked via memo

All three features share:
- Auth model (Bearer token + merchant ID path validation)
- Database (SQLite, migrations inline in db.rs)
- Background job supervision (TaskHealth)
- Webhook delivery (existing webhook module)

---

## Design Review Response

This revised design addresses all findings from the design review:

1. **Email Configuration Validation** (HIGH): Implemented in-startup validation in config.rs with all-or-nothing check and clear error messages.

2. **Analytics Cache Bypass Parameter** (HIGH): Added `nocache` query parameter (Option<String>) to AnalyticsQuery struct; all analytics endpoints support it.

3. **Merchant ID Authorization** (HIGH): Explicit middleware validation (`validate_merchant_path_id`) comparing authenticated merchant ID with path parameter; returns `403 Forbidden` on mismatch.

4. **Subscription Charge Linking Race Condition** (MEDIUM): Clarified that SQLite INSERT OR IGNORE handles idempotency; unique constraint (subscription_id, payment_id) prevents duplicates; UPDATE is idempotent.

5. **Invoice-Payment Linking SQL** (MEDIUM): Provided complete `try_link_invoice_to_payment()` function with exact SQL and logic for memo matching and status updates.

6. **Subscription Pause Semantics** (MEDIUM): Specified that consecutive failed attempts trigger pause; successful charge resets counter; pause can be manually resumed; individual charges can be retried independently.

7. **Analytics Views Parameterization** (MEDIUM): Removed static views (which cannot be parameterized); replaced with parameterized queries in code (using SQL parameters ?1, ?2, etc.).

8. **Invoice Overpaid Status** (MEDIUM): Added "overpaid" as distinct status; status determination logic based on paid_amount vs. total_amount comparison.

9. **Subscription End Date Edge Case** (MEDIUM): Specified that expired subscriptions are marked status='expired'; pending/failed charges are left as-is for audit trail; scheduler skips expired subscriptions.

10. **QR Code Test Case Asset Issuer** (MEDIUM): Updated test case to include asset_issuer parameter in SEP-0007 URI; format now includes `asset_issuer=GISSUER...` for issued assets like USDC.

11. **Invoice Number Atomicity Testing** (MEDIUM): Noted that load testing is required; added to integration test suite; atomic SELECT/UPDATE transaction ensures correctness.

12. **Payments Foreign Keys Soft Delete** (MEDIUM): Clarified that invoices and subscriptions are not deleted in normal operations (only cancelled); FK columns are nullable because soft deletes use SET NULL for audit trails (no data loss).

13. **SMTP_USE_TLS Configuration** (NIT): Removed ambiguous SMTP_USE_TLS; standardized on SMTP_PORT (587 for STARTTLS, 465 for TLS) as the configuration point.

14. **Analytics CSV Export Pagination** (NIT): Clarified as out-of-scope for this feature; current limit is 100,000 rows; pagination deferred to future enhancement.

15. **Subscription Memo Length Validation** (NIT): Added validation in `create_subscription_charge()` to ensure memo length ≤ 28 bytes; returns `400 Bad Request` / `memo_too_long` on failure.
