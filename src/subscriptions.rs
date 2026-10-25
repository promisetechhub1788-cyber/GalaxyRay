//! Background task for scheduling subscription charges.
//!
//! Runs every 60 seconds, finds subscriptions with `next_charge_date <= today`,
//! creates payment intents with memo `SUB-{first 8 UUID chars}`, records charges,
//! and updates the next charge date.

use crate::{AppState, db};
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{info, warn, error};
use uuid::Uuid;

const MEMO_PREFIX: &str = "SUB-";
const MEMO_MAX_LENGTH: usize = 28;

/// Mark subscription charges as completed when their payment settles.
/// Called internally when a payment status changes to 'completed'.
pub async fn mark_charges_completed(
    pool: &sqlx::SqlitePool,
    payment_id: &str,
) -> anyhow::Result<()> {
    // Check if this payment has a memo matching our subscription format
    let memo_opt: Option<String> = sqlx::query_scalar(
        "SELECT memo FROM payments WHERE id = ? AND memo LIKE 'SUB-%'",
    )
    .bind(payment_id)
    .fetch_optional(pool)
    .await?;

    if memo_opt.is_none() {
        // Not a subscription payment, nothing to do
        return Ok(());
    }

    // Find pending subscription charges that match today's date
    // We match charges created today (when this payment was created)
    let affected = sqlx::query(
        "UPDATE subscription_charges 
         SET status = 'completed', last_attempt_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE status = 'pending' AND charge_date = date('now') AND subscription_id IN (
           SELECT id FROM subscriptions WHERE status = 'active'
         )",
    )
    .execute(pool)
    .await?;

    if affected.rows_affected() > 0 {
        info!(
            payment_id = %payment_id,
            rows = affected.rows_affected(),
            "marked subscription charges as completed"
        );
    }

    Ok(())
}

/// Background task that runs every 60 seconds to schedule subscription charges.
pub async fn run_scheduler(
    state: Arc<AppState>,
    mut shutdown: watch::Receiver<bool>,
) -> crate::supervise::TaskExit {
    let interval = std::time::Duration::from_secs(60);
    let mut ticker = tokio::time::interval(interval);

    info!("subscriptions scheduler started");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                info!("subscriptions scheduler shutting down");
                return crate::supervise::TaskExit::Ok;
            }
            _ = ticker.tick() => {
                if let Err(e) = process_due_subscriptions(&state).await {
                    warn!(error = %e, "error processing subscriptions");
                }
            }
        }
    }
}

/// Find subscriptions due for a charge and create payment intents.
async fn process_due_subscriptions(state: &Arc<AppState>) -> anyhow::Result<()> {
    // Find active subscriptions where next_charge_date <= TODAY
    let subscriptions = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, merchant_id, amount_stroops, asset, frequency 
         FROM subscriptions 
         WHERE status = 'active' AND next_charge_date <= date('now')
         ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool)
    .await?;

    for (sub_id, merchant_id, amount_stroops, asset, frequency) in subscriptions {
        // Create payment intent with memo SUB-{first 8 chars of UUID}
        let payment_uuid = Uuid::new_v4();
        let short_id = payment_uuid.to_string()[..8].to_string();
        let memo = format!("{}{}", MEMO_PREFIX, short_id);

        // Ensure memo fits within 28-byte limit
        if memo.len() > MEMO_MAX_LENGTH {
            warn!(
                subscription_id = %sub_id,
                "memo exceeds max length; skipping charge",
            );
            continue;
        }

        let payment_id = Uuid::new_v4().to_string();

        // Create payment intent
        if let Err(e) = sqlx::query(
            "INSERT INTO payments (id, merchant_id, destination_address, memo, amount, asset, status)
             VALUES (?, ?, ?, ?, ?, ?, 'pending')",
        )
        .bind(&payment_id)
        .bind(&merchant_id)
        .bind(&merchant_id) // Using merchant_id as placeholder destination for subscription intents
        .bind(&memo)
        .bind(&amount_stroops)
        .bind(&asset)
        .execute(&state.pool)
        .await
        {
            warn!(
                error = %e,
                subscription_id = %sub_id,
                "failed to create payment intent for subscription",
            );
            continue;
        }

        // Record charge
        let charge_id = Uuid::new_v4().to_string();
        if let Err(e) = sqlx::query(
            "INSERT INTO subscription_charges (id, subscription_id, charge_date, status)
             VALUES (?, ?, date('now'), 'pending')",
        )
        .bind(&charge_id)
        .bind(&sub_id)
        .execute(&state.pool)
        .await
        {
            warn!(
                error = %e,
                subscription_id = %sub_id,
                "failed to record subscription charge",
            );
            continue;
        }

        // Calculate next_charge_date based on frequency
        let next_date: String = match frequency.as_str() {
            "daily" => sqlx::query_scalar(
                "SELECT strftime('%Y-%m-%d', date('now'), '+1 day')"
            )
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "".to_string()),

            "weekly" => sqlx::query_scalar(
                "SELECT strftime('%Y-%m-%d', date('now'), '+7 days')"
            )
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "".to_string()),

            "monthly" => sqlx::query_scalar(
                "SELECT strftime('%Y-%m-%d', date('now'), '+1 month')"
            )
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "".to_string()),

            _ => {
                warn!(
                    frequency = %frequency,
                    subscription_id = %sub_id,
                    "unknown frequency"
                );
                continue;
            }
        };

        if next_date.is_empty() {
            warn!(
                subscription_id = %sub_id,
                "failed to calculate next charge date"
            );
            continue;
        }

        // Update subscription's next_charge_date
        if let Err(e) = sqlx::query(
            "UPDATE subscriptions SET next_charge_date = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?",
        )
        .bind(&next_date)
        .bind(&sub_id)
        .execute(&state.pool)
        .await
        {
            warn!(
                error = %e,
                subscription_id = %sub_id,
                "failed to update next charge date",
            );
        }

        info!(
            subscription_id = %sub_id,
            payment_id = %payment_id,
            charge_id = %charge_id,
            next_charge_date = %next_date,
            "subscription charge scheduled",
        );
    }

    Ok(())
}
