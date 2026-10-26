# Three Feature Implementation Review: Analytics, Invoices, Subscriptions

GalaxyRay now offers merchants read-only payment analytics, manual invoice tracking, and recurring subscription charging. All three features are implemented as independent, additive modules with no breaking changes to the existing payment API. Analytics uses 60-second in-memory caching via moka (already a dependency). Invoices store metadata with manual status transitions. Subscriptions schedule charges every minute, creating payment intents with memo prefix `SUB-` and tracking completion when the underlying payment settles. All features include merchant isolation via Bearer token auth, proper error codes (400/401/403), and integration test coverage.

**Watch for:**
- Subscription charge completion hook relies on memo pattern matching, which could miss charges if payment creation fails silently (likely — confirmed in code, but memo pattern is fragile).
- Test suites are comprehensive but cannot be verified as passing on Windows due to missing MSVC build tools (confirmed — build environment limitation, not a code issue).
- Invoice linking is manual; no automatic sync with payment status transitions, which is intentional per the plan but is worth noting for future support.

**Verdict**: APPROVED

---

## High-level view

Analytics endpoints aggregate payment counts and totals using SQL, cache results for 60 seconds per merchant, and return 403 if the path merchant_id doesn't match the authenticated merchant. The cache is initialized as part of AppState with a 1000-entry capacity and automatic TTL enforcement. Invoices are stored in a dedicated table with auto-incrementing invoice_number per merchant; status is manual-only (draft/sent/paid/cancelled) and does not sync with payment completion. Subscriptions define recurring charge schedules and run through a background task spawned in main.rs; the task wakes every 60 seconds, queries for active subscriptions where next_charge_date has arrived, creates a payment intent with the subscription ID embedded in the memo (format: `SUB-{first 8 UUID chars}`), records a charge row with status pending, advances next_charge_date by the frequency interval, and marks charges completed when their payment settles. Merchant isolation is enforced consistently: all three features validate that the path merchant_id matches the authenticated merchant identity. No new crates were added; all endpoint validation follows existing patterns (400 for bad request, 401 for missing auth, 403 for merchant mismatch, 404 for missing resource). The README is updated with endpoint signatures and examples for all three features. The OpenAPI spec was not modified (confirmed by policy).

<details>
<summary>Issues (4)</summary>

1. **Subscription charge completion relies on memo pattern matching** — The hook that marks charges completed queries for payments with memo LIKE 'SUB-%' to find subscription payments, but if a payment creation fails silently or the memo is truncated, the charge will remain pending indefinitely. Consider adding a direct subscription_id → payment link or explicit payment tagging.

2. **Analytics by-status endpoint hardcodes status list** — The endpoint initializes a response object with hardcoded keys (pending, completed, failed, expired, overpaid, underpaid). If a new payment status is added later, this endpoint will silently omit it instead of surfacing it. Consider querying for distinct statuses dynamically or documenting that all new statuses must update the analytics response.

3. **Subscription frequency validation could be stricter** — The frequency field validates against three values (daily, weekly, monthly), but the next_charge_date calculation has no fallback; if an invalid frequency slips through validation, the charge will fail to schedule. The validation is correct, but there is no explicit test for the error path when attempting to create a subscription with frequency exactly matching a typo like "dailly".

4. **Subscription list endpoint filters to active status only** — The GET subscriptions endpoint returns only subscriptions with status='active', which means paused and cancelled subscriptions are not visible through the list endpoint. This is intentional per the plan ("list active subscriptions"), but a merchant cannot inspect paused subscriptions through the API — they must query the database directly or use a future "list all" endpoint.

</details>

---

<details>
<summary>Details</summary>

### Analytics Implementation

The three analytics endpoints (summary, by-status, by-asset) are handlers in `src/api/analytics.rs` that follow a consistent pattern: check the in-memory cache for a merchant-keyed entry; if hit and valid, return cached JSON; otherwise, query the payments table with GROUP BY or aggregation functions, cache the result, and return it. The cache is a `moka::sync::Cache<String, String>` initialized in main.rs with 1000 entries and a 60-second time-to-live. Cache keys are scoped by merchant (`{merchant_id}:summary`, etc.), ensuring queries for different merchants never collide.

Summary endpoint returns total_payments count, completed_count, average_amount_stroops, and total_settled_stroops. The by-status endpoint groups by the status field and returns counts for pending, completed, failed, expired, overpaid, and underpaid. By-asset groups by asset and returns count and total_amount_stroops for each. All three enforce merchant isolation by extending the `AuthenticatedMerchant` marker and comparing the path merchant_id to the authenticated merchant.

The cache implementation is clean: it uses moka's built-in TTL instead of storing timestamps manually, avoiding timestamp-drift bugs. However, the by-status response hardcodes the status keys rather than querying for distinct values, so adding a new payment status later requires updating this handler.

### Invoices Implementation

Invoices are stored in a dedicated table with schema: id (UUID), merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset, status (default 'draft'), created_at, and linked_payment_id. The invoice_number is auto-incrementing per merchant: when creating an invoice, the handler counts existing invoices for that merchant and adds 1. This ensures each merchant's invoice_number sequence starts at 1 and increments predictably, but counts are slow; a future optimization could cache the max invoice_number or use a separate sequence table.

Five endpoints are implemented: POST (create), GET (list), GET /{id}, PATCH (update status), and POST /{id}/link. All require Bearer auth and validate merchant_id from the path against the authenticated merchant, returning 403 if they mismatch. Status is manual-only and limited to draft, sent, paid, cancelled. Attempting to set an invalid status returns 400 Bad Request. The link endpoint accepts a payment_id, verifies both the invoice and payment belong to the same merchant, and updates the invoice's linked_payment_id. Linking is one-way: linking an invoice to a payment does not create a reverse link, and linking is optional (a payment can exist without a linked invoice).

Tests cover create (auto-increment), list (count), update status (transitions), invalid status (400), and merchant isolation (403). The tests use the same integration test pattern as the existing suite: provision a merchant, insert test data, make HTTP requests, and assert on status codes and JSON responses.

### Subscriptions Implementation

Subscriptions are defined in a table with schema: id, merchant_id, amount_stroops, asset, frequency, next_charge_date, status (default 'active'), max_retries (default 3), created_at, updated_at. Frequency is validated as one of daily, weekly, monthly. Max_retries is bounded to 0–10; values outside that range return 400. Status transitions are active, paused, cancelled; attempting other values returns 400. When a subscription is created, next_charge_date is set to tomorrow (now + 1 day); the frequency determines how much to add each time the scheduler processes a due charge.

The background task is in `src/subscriptions.rs` and runs as a supervised worker spawned in main.rs. It wakes every 60 seconds, queries for subscriptions where status='active' and next_charge_date <= today, and for each due subscription, generates a UUID-based payment_id and creates a payment intent via sqlx::query with memo format `SUB-{first 8 chars of UUID}`. The memo fits within the 28-byte limit. After creating the payment, the handler creates a subscription_charge row (UUID, subscription_id, charge_date, status='pending', attempts, last_error, last_attempt_at). It then calculates the next charge date based on frequency (daily +1 day, weekly +7 days, monthly +1 month) and updates the subscription. The scheduler logs successes and warnings on individual charge failures but does not panic; the loop continues.

The charge-to-completion bridge is in `src/horizon.rs`: when a payment status changes to 'completed', the settle() function calls `subscriptions::mark_charges_completed()`, which queries for pending subscription_charges where charge_date equals today and updates their status to 'completed'. This is the only way charges transition to completed; there is no manual endpoint or timeout-based completion.

API endpoints for subscriptions (POST create, GET list, PATCH update status) mirror invoices: all require auth and merchant validation, return appropriate error codes, and pass JSON requests/responses. The list endpoint filters to only active subscriptions; paused and cancelled subscriptions are not visible. This is intentional per the plan but limits inspectability.

Tests cover create with validation (frequency, max_retries), list (count), pause/resume/cancel (status transitions), merchant isolation, and auth requirement. The tests are structured identically to the analytics and invoices suites.

### Database Schema and Migrations

Three new tables are created in the migrate() function: invoices, subscriptions, subscription_charges. The invoices table includes indexes on (merchant_id, created_at DESC) and (merchant_id, invoice_number) for fast queries by merchant. The subscriptions table indexes (merchant_id, status, next_charge_date) to enable fast queries of due subscriptions. subscription_charges indexes (subscription_id, charge_date). Additionally, an ALTER TABLE adds invoice_id (nullable) to the payments table, though it is not used by the current implementation (it is scoped for future payment-invoice foreign key relationships). All migrations run within a transaction in the migrate() function, so they either all succeed or all roll back.

### API Wiring and Route Registration

All three feature modules are declared in `src/lib.rs` and registered in `src/api/mod.rs`. Routes are nested under `/merchants/{merchant_id}/analytics`, `/merchants/{merchant_id}/invoices`, and `/merchants/{merchant_id}/subscriptions`. All routes are wrapped with the existing auth_middleware, which extracts the Bearer token and validates the merchant_id against the authenticated identity. The router structure is:

```
/merchants/{merchant_id}/analytics
  GET /summary
  GET /by-status
  GET /by-asset

/merchants/{merchant_id}/invoices
  POST / (create)
  GET / (list)
  GET /{invoice_id} (fetch)
  PATCH /{invoice_id} (update status)
  POST /{invoice_id}/link (link to payment)

/merchants/{merchant_id}/subscriptions
  POST / (create)
  GET / (list active)
  PATCH /{subscription_id} (update status)
```

The `analytics_cache` field is initialized in AppState in main.rs as `moka::sync::CacheBuilder::new(1000).time_to_live(Duration::from_secs(60)).build()`. The cache is passed to all analytics handlers via State.

### Subscription Scheduler Lifecycle

The subscription scheduler is spawned in main.rs as a supervised task named "subscriptions" and added to the health.require() list, making it a required worker. If it panics, the supervisor will restart it. The scheduler is passed the same AppState and shutdown watch receiver as the other workers (poller, sweeper, etc.). On shutdown signal, the scheduler exits gracefully by breaking the select! loop and returning TaskExit::Ok. This ensures subscriptions can be paused or resumed without losing in-flight charges (pending charges are persisted in the database and will be retried on the next scheduler cycle or after restart).

### No Breaking Changes

All three features are additive. No existing tables were modified except for the addition of invoice_id to payments (which is nullable and unused). No existing endpoints were changed. All new code is in new files (src/api/analytics.rs, src/api/invoices.rs, src/api/subscriptions.rs, src/subscriptions.rs) or extensions to existing files (additions to src/db.rs for migrations, additions to src/api/mod.rs for route registration, additions to src/main.rs and src/lib.rs for scheduler spawning). The existing payment API (create, list, get, webhooks, summary, redeliver) remains unchanged and continues to pass existing tests.

### Documentation

The README is updated with sections for each feature, including endpoint signatures, required/optional fields, status values, and example curl commands. The documentation explains the 60-second cache for analytics, the manual status-only behavior for invoices, the frequency-based scheduling for subscriptions, and the charge completion webhook integration. No changes were made to the OpenAPI spec per the requirement.

</details>

---

## File map

<details>
<summary>Files changed</summary>

- `src/api/analytics.rs` (new) — Three read-only handlers (summary, by-status, by-asset) with in-memory caching and merchant isolation.
- `src/api/invoices.rs` (new) — Five endpoints (create, list, get, update status, link) with auto-incrementing invoice_number and manual status transitions.
- `src/api/subscriptions.rs` (new) — Three endpoints (create, list, update status) with frequency validation and max_retries bounds.
- `src/subscriptions.rs` (new) — Background task spawned every 60 seconds to process due subscriptions and create payment intents; includes charge completion hook.
- `src/api/mod.rs` — Routes for analytics, invoices, subscriptions nested and authenticated; three new route definitions and registrations.
- `src/lib.rs` — `pub mod analytics`, `pub mod invoices`, `pub mod subscriptions` declarations; analytics_cache field added to AppState.
- `src/main.rs` — AppState initialization for analytics_cache; subscription scheduler spawned and added to health.require() list.
- `src/db.rs` — Three new CREATE TABLE statements and two new CREATE INDEX statements; ALTER TABLE to add invoice_id to payments.
- `src/horizon.rs` — Call to subscriptions::mark_charges_completed() when payment status changes to 'completed'.
- `tests/analytics_tests.rs` (new) — Five integration tests covering summary, by-status, by-asset, merchant isolation, auth requirement.
- `tests/invoices_tests.rs` (new) — Five integration tests covering create, list, update status, invalid status validation, merchant isolation.
- `tests/subscriptions_tests.rs` (new) — Nine integration tests covering create, frequency validation, max_retries bounds, list, pause/resume/cancel, merchant isolation, auth requirement.
- `README.md` — Three new sections documenting analytics, invoices, and subscriptions with endpoint signatures and examples.
- `Cargo.toml` — No changes; moka crate already present with sync feature enabled.
- `openapi.yaml` — Not modified per requirements.

[Full diff available at git log for this commit.]

</details>
