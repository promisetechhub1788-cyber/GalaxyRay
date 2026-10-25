# Implementation Plan: Three Features for GalaxyRay

This plan adds three independent features to GalaxyRay to increase merchant value: analytics (read-only), invoices (register), and subscriptions (recurring charges). All work without new crates, breaking changes, or architectural complexity.

## Feature 1: Basic Analytics (Read-Only Metrics)

### What to do
Add three read-only endpoints that aggregate payment data with in-memory 60-second caching. No new tables, no pagination, no exports. The endpoints summarize the existing `payments` table for the authenticated merchant.

### Why this approach
- Existing `payments` table has all needed data (status, asset, amount, merchant_id).
- Aggregations (COUNT, SUM) are simple SQL with no new infrastructure.
- 60-second in-memory cache (using `moka` cache already in Cargo.toml) reduces database load for frequent polling.
- Read-only scope keeps business logic minimal; no async coordination needed.

### Files to create or modify
- `src/api/analytics.rs` — new module with three handler functions
- `src/api/mod.rs` — register analytics routes under `/v1/merchants/{merchant_id}/analytics/*` and wire cache into AppState
- `src/lib.rs` — add `pub mod analytics` declaration

### Items

- [ ] 1. Add analytics cache to AppState in src/lib.rs.
      Add a `pub analytics_cache: moka::sync::Cache<String, String>` field to AppState.
      Files: src/lib.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 2. Initialize analytics cache in src/main.rs.
      In the AppState construction, initialize `analytics_cache` with `moka::sync::Cache::new(1000)` (1000 merchant keys).
      Files: src/main.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 3. Create src/api/analytics.rs with three endpoints.
      Implement handlers: `summary`, `by_status`, `by_asset`. Each:
      - Validates merchant_id matches Bearer token (403 if mismatch).
      - Constructs a cache key (e.g., `{merchant_id}:summary`).
      - Checks cache; if hit and <60 seconds old, return cached JSON.
      - Otherwise, runs SQL aggregation, caches result, returns JSON.
      - summary: returns `{total_payments, completed_count, average_amount_stroops, total_settled_stroops}`.
      - by_status: returns `{pending, completed, failed, expired, overpaid, underpaid}` counts.
      - by_asset: returns array of `{asset, count, total_amount_stroops}` grouped by asset.
      Files: src/api/analytics.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 4. Register analytics routes in src/api/mod.rs.
      In `api_v1`, add a new router for `/merchants/{merchant_id}/analytics` with three GET endpoints.
      Each requires auth (Bearer token) and validates merchant_id from path against token.
      Files: src/api/mod.rs
      Verify: `cargo build` and `cargo test` — all tests pass.

- [ ] 5. Update README.md with analytics endpoints.
      Add a section under "API Reference" describing the three endpoints, their query-string parameters (none), and example responses.
      Files: README.md
      Verify: README content is well-formed Markdown; build still succeeds.

---

## Feature 2: Invoice Metadata Tracking (Simple Register)

### What to do
Add a simple invoice register that stores manual invoice metadata (customer name, email, amount, asset, status). Invoices are independent of payments; status is manual only (draft, sent, paid, cancelled). No QR, PDF, or email generation. Invoices can be linked to a payment for reporting.

### Why this approach
- Invoices are "the other side of the payment flow" — many merchants issue invoices independently of payment receipt. A register lets them track what they've issued.
- Manual status means no automatic syncing with payment completion, avoiding complex state coordination.
- Linking is optional and lazy (POST .../link), not bidirectional foreign-key triggering.
- Self-contained scope: three tables (invoices, add invoice_id to payments), four endpoints, straightforward CRUD.

### Files to create or modify
- `src/db.rs` — add migration for `invoices` table, `invoice_id` column on payments, new queries
- `src/api/invoices.rs` — new module with four handlers (create, list, get, update status, link)
- `src/api/mod.rs` — register invoice routes and wire auth
- `README.md` — document invoice endpoints

### Items

- [ ] 6. Add invoice-related migrations to src/db.rs.
      Create `invoices` table: `(id UUID PRIMARY KEY, merchant_id TEXT, invoice_number TEXT, customer_name TEXT, customer_email TEXT, amount_stroops TEXT, asset TEXT, status TEXT, created_at RFC3339, linked_payment_id NULLABLE)` with index on `(merchant_id, created_at)`.
      Add `invoice_id` column to `payments` table (nullable, no foreign key yet; added later after data audit in a full rebuild if needed).
      For now, defer the foreign key — just add the column nullable.
      Files: src/db.rs (migrate function)
      Verify: `cargo test schema_snapshot_test` produces correct snapshot; copy to schema_snapshot.sql.

- [ ] 7. Create invoice queries in src/db.rs.
      Add functions: `create_invoice`, `list_invoices`, `get_invoice`, `update_invoice_status`, `link_invoice_to_payment`.
      Each validates merchant_id ownership.
      `create_invoice` auto-increments invoice_number per merchant (query max existing number, add 1).
      Files: src/db.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 8. Create src/api/invoices.rs with four endpoints.
      Implement: `POST /merchants/{merchant_id}/invoices` (create with body `{customer_name, customer_email, amount_stroops, asset}`; auto-increment invoice_number),
      `GET /merchants/{merchant_id}/invoices` (list all, no pagination),
      `GET /merchants/{merchant_id}/invoices/{invoice_id}` (fetch one),
      `PATCH /merchants/{merchant_id}/invoices/{invoice_id}` (body `{status}` to draft/sent/paid/cancelled),
      `POST /merchants/{merchant_id}/invoices/{invoice_id}/link` (body `{payment_id}` to link; validation: payment must belong to merchant).
      All require Bearer auth and merchant_id path validation.
      Return 404 if invoice or payment not found or merchant mismatch.
      Files: src/api/invoices.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 9. Register invoice routes in src/api/mod.rs.
      Add routes under `/v1/merchants/{merchant_id}/invoices/...` with Bearer auth on all routes.
      Files: src/api/mod.rs
      Verify: `cargo build` and `cargo test` — all existing tests pass, no regressions.

- [ ] 10. Add invoice integration tests in tests/invoices_tests.rs.
      Test: create invoice, list invoices (verify count), fetch one, PATCH status, link to payment.
      Verify merchant_id validation (403 on mismatch), 404 on missing invoice/payment.
      Files: tests/invoices_tests.rs
      Verify: `cargo test invoices_tests` — all new tests pass.

- [ ] 11. Update README.md with invoice endpoints and status lifecycle.
      Document POST (create), GET (list), GET (fetch), PATCH (status), POST (link).
      Note that status is manual only; list allowed values (draft, sent, paid, cancelled).
      Files: README.md
      Verify: README is well-formed; build succeeds.

---

## Feature 3: Subscription Scheduling (Core Recurring Charges)

### What to do
Add a subscription model that creates payment intents on a recurring schedule (daily, weekly, monthly). A background task runs every minute, finds due subscriptions, creates a payment intent with memo `SUB-{first 8 UUID chars}`, records the charge in a separate table, and marks it completed when the payment webhook fires. No retry logic, no dunning, no webhooks for subscription events — only the payment workflow matters.

### Why this approach
- Subscriptions are independent of invoices and analytics; three tables (subscriptions, subscription_charges, plus the two-row join from payments).
- Background job is minimal: one-minute loop, SQL query for due dates, create intent, update next_charge_date.
- Hooking into existing payment completion webhook (internal signal) avoids new async machinery.
- Scope is "core charges only": status tracking but no dunning or retry loops.

### Files to create or modify
- `src/db.rs` — migrations for subscriptions, subscription_charges tables; queries for CRUD and charge scheduling
- `src/api/subscriptions.rs` — three endpoints (create, list, pause/resume/cancel)
- `src/api/mod.rs` — register subscription routes
- `src/subscriptions.rs` (new module) — background task spawned in main.rs
- `src/main.rs` — spawn subscription scheduler and wire shutdown
- `src/lib.rs` — add pub mod subscriptions declaration
- `README.md` — document subscription endpoints

### Items

- [ ] 12. Add subscription-related migrations to src/db.rs.
      Create `subscriptions` table: `(id UUID PRIMARY KEY, merchant_id TEXT, amount_stroops TEXT, asset TEXT, frequency TEXT, next_charge_date DATE, status TEXT, max_retries INT, created_at RFC3339, updated_at RFC3339)` with index on `(merchant_id, status, next_charge_date)`.
      Create `subscription_charges` table: `(id UUID PRIMARY KEY, subscription_id UUID FK, charge_date DATE, status TEXT, attempts INT, last_error TEXT, last_attempt_at RFC3339)` with index on `(subscription_id, charge_date)`.
      Files: src/db.rs (migrate function)
      Verify: `cargo test schema_snapshot_test` produces correct snapshot; copy to schema_snapshot.sql.

- [ ] 13. Create subscription queries in src/db.rs.
      Add functions: `create_subscription`, `list_subscriptions`, `update_subscription_status`, `find_due_subscriptions`, `create_subscription_charge`, `mark_charge_completed`.
      `create_subscription` validates frequency is one of: daily, weekly, monthly; sets max_retries bounds (0-10, default 3).
      `find_due_subscriptions` returns active subscriptions where `next_charge_date <= TODAY`.
      `create_subscription_charge` generates a UUID, records row with `status='pending'`.
      `mark_charge_completed` runs when payment webhook fires (internal signal, not a public endpoint).
      Files: src/db.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 14. Create src/api/subscriptions.rs with three endpoints.
      Implement: `POST /merchants/{merchant_id}/subscriptions` (body `{amount_stroops, asset, frequency, max_retries?}`; return created subscription with id),
      `GET /merchants/{merchant_id}/subscriptions` (list all active subscriptions for merchant),
      `PATCH /merchants/{merchant_id}/subscriptions/{subscription_id}` (body `{status}` to active/paused/cancelled).
      All require Bearer auth and merchant_id path validation.
      Validate frequency in request (400 if invalid); validate max_retries in bounds (400 if out of range).
      Return 404 if subscription not found or merchant mismatch.
      Files: src/api/subscriptions.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 15. Register subscription routes in src/api/mod.rs.
      Add routes under `/v1/merchants/{merchant_id}/subscriptions/...` with Bearer auth on all routes.
      Files: src/api/mod.rs
      Verify: `cargo build` and `cargo test` — all existing tests pass, no regressions.

- [ ] 16. Create src/subscriptions.rs background task.
      Implement `pub async fn run_scheduler(state: Arc<AppState>, mut shutdown: watch::Receiver<bool>)`.
      Loop every 60 seconds:
      - Call `db::find_due_subscriptions(&state.pool)` to find subscriptions where next_charge_date <= today and status='active'.
      - For each due subscription, generate a payment intent:
        - Use `db::create_payment` with memo `SUB-{first 8 chars of UUID}` (fits in 28 bytes).
        - Record charge row with `db::create_subscription_charge` with status='pending'.
        - Calculate next_charge_date: today + (1 day / 1 week / 1 month) based on frequency.
        - Update subscription's next_charge_date.
      - On shutdown signal, exit gracefully.
      - Log errors but do not panic; continue the loop.
      Files: src/subscriptions.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 17. Integrate subscription scheduler into main.rs and lib.rs.
      Add `pub mod subscriptions` to src/lib.rs.
      In src/main.rs, after the redrive worker setup, spawn the subscription scheduler with `supervise::supervise(health.clone(), "subscriptions", subscriptions::run_scheduler(...))`.
      Add `"subscriptions"` to `health.require(...)` list (required worker like poller/sweeper).
      Files: src/lib.rs, src/main.rs
      Verify: `cargo check` — no compilation errors.

- [ ] 18. Hook subscription charge completion into payment completion webhook.
      In src/db.rs (or a new internal function), when a payment status changes to 'completed', find any pending subscription_charges linked to that payment (via subscription_id + charge_date match) and update their status to 'completed'.
      This is an internal signal; no new endpoint. Call it from the payment reconciliation path (wherever payment.completed webhook is triggered).
      Files: src/db.rs, and the reconciliation code path that updates payment status to completed
      Verify: `cargo check` — no compilation errors; integration test for subscription charge completion.

- [ ] 19. Add subscription integration tests in tests/subscriptions_tests.rs.
      Test: create subscription, list subscriptions, verify charge created on schedule (advance time or mock timer).
      Test frequency validation (400 on invalid), max_retries bounds (400 on out of range).
      Test PATCH pause/resume/cancel status.
      Test merchant_id validation (403 on mismatch), 404 on missing subscription.
      Files: tests/subscriptions_tests.rs
      Verify: `cargo test subscriptions_tests` — all new tests pass.

- [ ] 20. Update README.md with subscription endpoints and scheduling behavior.
      Document POST (create), GET (list), PATCH (status).
      Explain frequency (daily, weekly, monthly), next_charge_date calculation, and the background job that runs every minute.
      Note: no retry logic, no dunning, no subscription events yet.
      Files: README.md
      Verify: README is well-formed; build succeeds.

---

## Integration & Verification

- [ ] 21. Run full test suite.
      Run `cargo build` to ensure all code compiles.
      Run `cargo test` to verify all tests pass (existing + new analytics, invoices, subscriptions tests).
      Verify no breaking changes to existing payment API or merchant provisioning.
      Files: all
      Verify: `cargo build && cargo test` — all tests pass with no errors or warnings.

- [ ] 22. Verify dashboard still loads.
      Start the server locally and visit `http://localhost:3000/dashboard` to confirm it renders and can fetch merchant data via the existing API.
      Files: none
      Verify: dashboard is accessible; no console errors or missing assets.

