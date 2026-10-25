# Three Independent Features for GalaxyRay

Three new features (analytics, invoices, subscriptions) were implemented to add merchant value without architectural complexity. The implementation follows the plan closely with read-only aggregations for analytics, manual invoice register for invoices, and background scheduler for recurring charges. All features integrate cleanly with the existing payment API.

**Watch for:** Test files are placeholder stubs with no actual test logic. No recorded evidence of `cargo build` and `cargo test` passes in code-progress.md, breaking the prerequisite for approval. README was not updated with endpoint documentation as required by the plan. Missing verification means blocking concerns cannot be ruled out.

**Verdict**: CHANGES_REQUESTED

---

## High-level view

Analytics reads the existing payments table through three endpoints (summary, by-status, by-asset) and caches results for 60 seconds per merchant using moka, reducing database load. The cache is initialized in AppState and accessed by handlers without permission checks beyond auth. Invoices adds a simple manual register with five endpoints (create with auto-incrementing invoice_number, list, fetch, update status, link to payment), storing data in a new `invoices` table and tracking an optional `invoice_id` on payments; status is strictly manual and never automatic. Subscriptions adds a background scheduler that runs every 60 seconds, finds subscriptions due by next_charge_date, creates payment intents with a memo prefix `SUB-{first 8 UUID chars}`, records charges with pending status, and updates next_charge_date based on frequency (daily/weekly/monthly); it's integrated into the supervision framework as a required worker. All three features use existing auth middleware and 403 on merchant_id mismatch. No new crates were added. The code paths follow project patterns well — AppError for consistent error responses, sqlx for database access, straightforward handler signatures.

---

<details>
<summary>Issues (5)</summary>

1. **Test stubs — no actual test logic** — All three features have placeholder test files with TODO comments but zero executable test cases. The plan required basic integration tests; the recorded evidence must show `cargo test` passes to validate the endpoints work at all.

2. **Missing build/test evidence in code-progress.md** — The progress document does not record the output of `cargo build` or `cargo test`. The instruction explicitly stated "the coder already ran them and recorded results; re-running burns the step." No recorded evidence means blocking concerns cannot be ruled out.

3. **README not updated** — The plan required README updates documenting all new endpoints and the subscription scheduling behavior. No such updates are present, leaving operators/developers without API documentation.

4. **Subscription charge completion hook missing** — Plan item 18 required that when a payment status changes to 'completed', pending subscription_charges linked to that payment are marked 'completed' as well. This internal signal hook does not appear to be implemented; subscription charges stay pending forever even if the underlying payment settles.

5. **Analytics cache timeout never expires** — The moka cache is initialized with no explicit TTL configuration. The plan required a 60-second TTL, but `moka::sync::Cache::new(1000)` only sets capacity, not expiry. Items cached will persist indefinitely unless a separate expiration policy is applied (e.g., via `time_to_idle`).

</details>

---

<details>
<summary>Details</summary>

### Analytics: Read-Only Aggregations with In-Memory Cache

Three endpoints (`/summary`, `/by-status`, `/by-asset`) aggregate the payments table for a merchant with a 60-second cache per merchant_id. The cache is a `moka::sync::Cache<String, String>` added to AppState and initialized in main.rs with capacity 1000. Each handler constructs a cache key like `{merchant_id}:summary`, checks cache first, runs a SQL aggregation if miss, inserts the JSON string into the cache, and returns the result. The handlers call `state.analytics_cache.insert()` without explicit TTL, relying on moka to expire entries; however, `moka::sync::Cache::new(1000)` only sets capacity and does not configure `time_to_idle` or `time_to_live`, so cached entries will remain until the next update for the same key. This violates the plan's 60-second requirement. A correct implementation would use `Cache::builder().time_to_idle(Duration::from_secs(60)).build()`.

The cache key approach is sound — a merchant's analytics are isolated by merchant_id, and cache hits return pre-computed JSON. Merchant isolation is implicit: the SQL query filters by merchant_id and the cache key includes it, so one merchant cannot read another's cached data. Auth validation happens via the middleware before the handler runs. The three aggregation queries (COUNT, SUM, GROUP BY) are straightforward and match the plan. No breaking changes to the payment API. The serialization path (query results to JSON, JSON to string cache) is standard; the deserialization path (string from cache to serde_json::Value) relies on `serde_json::from_str`, which is safe for pre-validated JSON.

### Invoices: Manual Register with Auto-Incrementing Invoice Number

Five endpoints create, list, fetch, update status, and link invoices to payments. The `invoices` table schema matches the plan: id, merchant_id, invoice_number, customer fields, amount, asset, status (default 'draft'), created_at, and linked_payment_id. Invoice creation auto-increments invoice_number per merchant by counting existing invoices and adding 1; this is a non-atomic race-prone approach (two concurrent creates for the same merchant could mint the same number), but it's simple and matches the plan's "auto-increment" description. Status is manual only — endpoints accept one of draft/sent/paid/cancelled and update the column; no automatic transition on payment settlement. Linking is lazy: a separate POST endpoint lets a merchant manually link an invoice_id to a payment_id after the fact, validating that both belong to the merchant before updating. Merchant isolation is enforced by WHERE clauses on every query.

The auto-increment race condition is a concern: if two requests arrive concurrently for the same merchant, the COUNT queries may see the same current max, both compute the same next number, and both insert that number. The invoices table has a unique index on (merchant_id, invoice_number), so the second insert fails with a database constraint error. The handler returns a 500 instead of retrying with a new number. A production implementation would either use a database-level auto-increment column or implement optimistic locking with retry. However, the plan's language ("auto-increment invoice_number") does not mandate atomic semantics, and the current implementation is functional for the single-merchant, low-concurrency testing scenario the plan contemplates.

The invoice_id column was added to the payments table as nullable with no foreign key constraint. The plan said "defer the foreign key — just add the column nullable" for an audit pass later, which is exactly what was done. The linking endpoint validates both the invoice and payment exist before updating, so orphaned or cross-merchant links are rejected. All endpoints return merchant_id mismatch as 403, payment/invoice not found as 404. Status validation returns 400 for invalid status values. Good error handling.

### Subscriptions: Background Scheduler with Recurring Charges

A background task (`src/subscriptions.rs`) spawned in main.rs runs every 60 seconds, queries for active subscriptions where `next_charge_date <= today`, creates payment intents with memo prefix `SUB-{first 8 chars of UUID}`, records subscription_charges with pending status, and advances next_charge_date based on frequency. The scheduler is integrated into the supervision framework as a required worker (will restart on panic) and wired to the shutdown signal. Three endpoints (create, list, update status) let merchants define subscriptions with frequency (daily/weekly/monthly), amount, asset, and optional max_retries (default 3, bounds 0-10).

The scheduler queries for due subscriptions correctly: `next_charge_date <= date('now')` in SQLite. For each due subscription it creates a payment intent with the memo format `SUB-{first 8 uuid_chars}` and records a subscription_charge row with status='pending'. The memo length check (ensuring it fits in 28 bytes) is good defensive coding. The next_charge_date calculation uses SQLite date functions ('+1 day', '+7 days', '+1 month'), which is correct and timezone-agnostic. Frequency validation on create is strict (only daily/weekly/monthly allowed, 400 on others). The max_retries bounds check (0-10) matches the plan.

However, the scheduler never marks subscription_charges as completed. Plan item 18 required that when a payment status changes to 'completed', any pending subscription_charges for that subscription (matched by subscription_id and charge_date) should be marked 'completed'. This hook is not implemented. As a result, subscription_charges remain pending forever even if the payment settles, breaking the charge lifecycle. The payment webhook/reconciliation path that updates payment status to 'completed' does not call any subscription completion logic. This is a significant omission: merchants cannot distinguish a charge that succeeded from one that is still waiting.

The list subscriptions endpoint filters to `status = 'active'` only, so paused and cancelled subscriptions are not returned. This matches the plan's "list active subscriptions" language, though it means merchants cannot review paused or cancelled subscriptions through the list endpoint (they would need a fetch by ID to see a paused one). The status values are active/paused/cancelled; the scheduler respects the active status and skips paused/cancelled rows.

Merchant isolation is enforced by all queries filtering on merchant_id. All endpoints require auth and validate merchant_id from the path against the token. The API returns 400 for invalid input (bad frequency, out-of-bounds retries), 403 for merchant mismatch, 404 for not found, consistent with the existing error model.

### Database Migrations and Schema

The `migrate()` function in db.rs adds three new tables (invoices, subscriptions, subscription_charges) and adds the invoice_id column to payments. The migration is idempotent using `CREATE TABLE IF NOT EXISTS` and checking for column existence before adding with `ALTER TABLE`. Indexes are created on (merchant_id, created_at) for invoices, (merchant_id, status, next_charge_date) for subscriptions, and (subscription_id, charge_date) for subscription_charges. The migration runs on every startup (as part of db::migrate called from main.rs), which is safe but adds startup latency; existing databases pick up the new tables/columns on first run. No data loss or migration reversal is needed.

### Auth and Error Handling

All new endpoints require the Bearer token auth middleware (auth_middleware), which extracts the token, looks up the merchant by key hash, and injects AuthenticatedMerchant into extensions. The middleware runs before the handler, so credentials are validated first. Each handler receives the authenticated merchant_id and compares it to the path parameter merchant_id; a mismatch returns 403. This is consistent with the existing payment API. Request validation (invoice status, subscription frequency, max_retries) returns 400 Bad Request with a code. Database errors return 500 Internal Server Error. All endpoints serialize responses as JSON and use the existing AppError struct for consistency.

### No New Dependencies

Cargo.toml is unchanged except for the edition fix (2021). The implementation uses only existing dependencies: sqlx for database access, moka for the analytics cache, tokio for async/concurrency, axum for the HTTP handlers. No PDF/QR/email libraries were added, as required by the plan. The in-memory cache is sufficient for a single-process analytics endpoint; the plan noted this is single-process only.

### Missing Test Coverage

The three test files (analytics_tests.rs, invoices_tests.rs, subscriptions_tests.rs) contain only placeholder test stubs with TODO comments. No actual assertions or test logic exists. The instructions stated that the coder must record evidence of `cargo test` passing; without this evidence, it is not possible to confirm the endpoints compile, route correctly, validate input, enforce auth, or return the expected responses. The analytics_tests.rs file has one test that sets up a router and calls an endpoint, but it does not check the response or populate test data. The subscriptions and invoices tests are entirely empty loops. This is a blocking gap: the plan required "basic integration tests exist for each feature."

### README Documentation

The README was not updated with the new endpoints. The plan required "Update README.md with analytics endpoints," "document invoice endpoints," and "document subscription endpoints." No such sections exist. Operators and developers looking at the gateway cannot discover or understand these new features from the documentation. The OpenAPI spec was not touched, which is correct per the plan.

### Subscription Charge Completion Lifecycle Gap

The most significant behavioral issue: subscription charges are created with status='pending' but never transition to 'completed' even when the underlying payment settles. The plan item 18 read:

> When a payment status changes to 'completed', find any pending subscription_charges linked to that payment (via subscription_id + charge_date match) and update their status to 'completed'. This is an internal signal; no new endpoint.

No such logic exists in the codebase. The payment reconciliation path (wherever payment status is updated to 'completed') does not call a subscription charge completion function. As a result, merchants cannot tell whether a subscription charge attempt succeeded — it stays pending indefinitely. The charge_date column in subscription_charges is set to `date('now')` (the date the charge was created), but there's no query logic to match a pending subscription_charge with a completed payment by date and subscription_id. This breaks the subscription feature's core promise: that merchants can track recurring charges through to completion.

</details>

---

## File map

<details>
<summary>Changed files</summary>

- `src/lib.rs` — Added `pub mod subscriptions` declaration and `pub analytics_cache: moka::sync::Cache<String, String>` field to AppState.
- `src/main.rs` — Initialized analytics_cache with `Cache::new(1000)` in AppState constructor; spawned subscriptions scheduler with `supervise::supervise`; added "subscriptions" to required workers list.
- `src/api/mod.rs` — Registered analytics, invoices, and subscriptions route groups under `/v1/merchants/{merchant_id}/*`; wired auth middleware on all new endpoints.
- `src/api/analytics.rs` — New file, 145 lines. Three read-only endpoint handlers (summary, by_status, by_asset) with in-memory caching.
- `src/api/invoices.rs` — New file, 254 lines. Five endpoints for invoice CRUD and payment linking.
- `src/api/subscriptions.rs` — New file, 187 lines. Three endpoints for subscription lifecycle (create, list, update status).
- `src/subscriptions.rs` — New file, 161 lines. Background scheduler task spawned in main.rs.
- `src/db.rs` — Added migrations for invoices, subscriptions, subscription_charges tables; added invoice_id column to payments; added indexes on all new tables.
- `tests/analytics_tests.rs` — New file with placeholder test stubs.
- `tests/invoices_tests.rs` — New file with placeholder test stubs.
- `tests/subscriptions_tests.rs` — New file with placeholder test stubs.

[Full diff available via git](file:///.agents/tasks/features/review.md)

</details>
