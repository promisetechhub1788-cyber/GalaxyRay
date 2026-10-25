use crate::{AppState, api::AuthenticatedMerchant, db};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

use super::payments::{AppError, JsonBody};

#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    pub amount_stroops: String,
    #[serde(default)]
    pub asset: String,
    pub frequency: String,
    #[serde(default)]
    pub max_retries: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub id: String,
    pub merchant_id: String,
    pub amount_stroops: String,
    pub asset: String,
    pub frequency: String,
    pub next_charge_date: String,
    pub status: String,
    pub max_retries: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSubscriptionStatusRequest {
    pub status: String,
}

/// POST /v1/merchants/{merchant_id}/subscriptions
pub async fn create(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
    JsonBody(req): JsonBody<CreateSubscriptionRequest>,
) -> Result<(StatusCode, Json<SubscriptionResponse>), AppError> {
    // Validate frequency
    let valid_frequencies = vec!["daily", "weekly", "monthly"];
    if !valid_frequencies.contains(&req.frequency.as_str()) {
        return Err(AppError::bad_request(
            "invalid_frequency",
            "frequency must be one of: daily, weekly, monthly",
        ));
    }

    // Validate max_retries
    let max_retries = req.max_retries.unwrap_or(3);
    if max_retries > 10 {
        return Err(AppError::bad_request("invalid_max_retries", "max_retries must be between 0 and 10"));
    }

    let asset = if req.asset.is_empty() { "XLM" } else { &req.asset };

    let subscription_id = Uuid::new_v4().to_string();

    // Calculate next_charge_date (today + 1 day initially)
    let next_charge_date: String = sqlx::query_scalar(
        "SELECT strftime('%Y-%m-%d', 'now', '+1 day')",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    sqlx::query(
        "INSERT INTO subscriptions (id, merchant_id, amount_stroops, asset, frequency, next_charge_date, max_retries)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&subscription_id)
    .bind(&merchant_id)
    .bind(&req.amount_stroops)
    .bind(asset)
    .bind(&req.frequency)
    .bind(&next_charge_date)
    .bind(max_retries)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, u32, String, String)>(
        "SELECT id, merchant_id, amount_stroops, asset, frequency, next_charge_date, status, max_retries, created_at, updated_at FROM subscriptions WHERE id = ?",
    )
    .bind(&subscription_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    Ok((
        StatusCode::CREATED,
        Json(SubscriptionResponse {
            id: row.0,
            merchant_id: row.1,
            amount_stroops: row.2,
            asset: row.3,
            frequency: row.4,
            next_charge_date: row.5,
            status: row.6,
            max_retries: row.7,
            created_at: row.8,
            updated_at: row.9,
        }),
    ))
}

/// GET /v1/merchants/{merchant_id}/subscriptions
pub async fn list(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
) -> Result<Json<Vec<SubscriptionResponse>>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, String, String, u32, String, String)>(
        "SELECT id, merchant_id, amount_stroops, asset, frequency, next_charge_date, status, max_retries, created_at, updated_at FROM subscriptions WHERE merchant_id = ? AND status = 'active' ORDER BY created_at DESC",
    )
    .bind(&merchant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let subscriptions: Vec<SubscriptionResponse> = rows
        .into_iter()
        .map(|row| SubscriptionResponse {
            id: row.0,
            merchant_id: row.1,
            amount_stroops: row.2,
            asset: row.3,
            frequency: row.4,
            next_charge_date: row.5,
            status: row.6,
            max_retries: row.7,
            created_at: row.8,
            updated_at: row.9,
        })
        .collect();

    Ok(Json(subscriptions))
}

/// PATCH /v1/merchants/{merchant_id}/subscriptions/{subscription_id}
pub async fn update_status(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path((path_merchant_id, subscription_id)): Path<(String, String)>,
    JsonBody(req): JsonBody<UpdateSubscriptionStatusRequest>,
) -> Result<Json<SubscriptionResponse>, AppError> {
    if path_merchant_id != merchant_id {
        return Err(AppError::new(StatusCode::FORBIDDEN, "forbidden", "unauthorized"));
    }

    // Validate status
    let valid_statuses = vec!["active", "paused", "cancelled"];
    if !valid_statuses.contains(&req.status.as_str()) {
        return Err(AppError::bad_request("invalid_status", "status must be one of: active, paused, cancelled"));
    }

    // Update status and updated_at
    let result = sqlx::query(
        "UPDATE subscriptions SET status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ? AND merchant_id = ?",
    )
    .bind(&req.status)
    .bind(&subscription_id)
    .bind(&merchant_id)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    if result.rows_affected() == 0 {
        return Err(AppError::new(StatusCode::NOT_FOUND, "not_found", "subscription not found"));
    }

    // Fetch updated subscription
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, u32, String, String)>(
        "SELECT id, merchant_id, amount_stroops, asset, frequency, next_charge_date, status, max_retries, created_at, updated_at FROM subscriptions WHERE id = ?",
    )
    .bind(&subscription_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    Ok(Json(SubscriptionResponse {
        id: row.0,
        merchant_id: row.1,
        amount_stroops: row.2,
        asset: row.3,
        frequency: row.4,
        next_charge_date: row.5,
        status: row.6,
        max_retries: row.7,
        created_at: row.8,
        updated_at: row.9,
    }))
}
