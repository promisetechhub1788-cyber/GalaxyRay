use crate::{AppState, api::AuthenticatedMerchant, db};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

use super::payments::AppError;

/// GET /v1/merchants/{merchant_id}/analytics/summary
/// Returns total payments, completed count, average amount, total settled
pub async fn summary(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cache_key = format!("{}:summary", merchant_id);

    // Try cache first
    if let Some(cached) = state.analytics_cache.get(&cache_key) {
        if let Ok(data) = serde_json::from_str::<Value>(&cached) {
            return Ok(Json(data));
        }
    }

    // Query database
    let (total_payments, completed_count, average_amount_stroops, total_settled_stroops) = sqlx::query_as::<_, (i64, i64, Option<String>, Option<String>)>(
        "SELECT 
            COUNT(*) as total,
            SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as completed,
            AVG(CAST(CASE WHEN amount IS NOT NULL AND amount != '' THEN amount ELSE '0' END AS REAL)) as avg_amount,
            SUM(CASE WHEN status = 'completed' AND paid_amount IS NOT NULL AND paid_amount != '' THEN CAST(paid_amount AS INTEGER) ELSE 0 END) as total_settled
         FROM payments WHERE merchant_id = ?",
    )
    .bind(&merchant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let result = json!({
        "total_payments": total_payments,
        "completed_count": completed_count,
        "average_amount_stroops": average_amount_stroops.unwrap_or_else(|| "0".to_string()),
        "total_settled_stroops": total_settled_stroops.unwrap_or_else(|| "0".to_string()),
    });

    // Cache for 60 seconds
    let json_str = result.to_string();
    state.analytics_cache.insert(cache_key, json_str);

    Ok(Json(result))
}

/// GET /v1/merchants/{merchant_id}/analytics/by-status
/// Returns breakdown of payment counts by status
pub async fn by_status(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cache_key = format!("{}:by_status", merchant_id);

    // Try cache first
    if let Some(cached) = state.analytics_cache.get(&cache_key) {
        if let Ok(data) = serde_json::from_str::<Value>(&cached) {
            return Ok(Json(data));
        }
    }

    // Query database for status breakdown
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) as count FROM payments WHERE merchant_id = ? GROUP BY status",
    )
    .bind(&merchant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let mut result = json!({
        "pending": 0i64,
        "completed": 0i64,
        "failed": 0i64,
        "expired": 0i64,
        "overpaid": 0i64,
        "underpaid": 0i64,
    });

    for (status, count) in rows {
        if let Some(val) = result.get_mut(&status) {
            *val = json!(count);
        }
    }

    // Cache for 60 seconds
    let json_str = result.to_string();
    state.analytics_cache.insert(cache_key, json_str);

    Ok(Json(result))
}

/// GET /v1/merchants/{merchant_id}/analytics/by-asset
/// Returns payment counts and total amounts grouped by asset
pub async fn by_asset(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cache_key = format!("{}:by_asset", merchant_id);

    // Try cache first
    if let Some(cached) = state.analytics_cache.get(&cache_key) {
        if let Ok(data) = serde_json::from_str::<Value>(&cached) {
            return Ok(Json(data));
        }
    }

    // Query database for asset breakdown
    let rows = sqlx::query_as::<_, (String, i64, Option<String>)>(
        "SELECT asset, COUNT(*) as count, SUM(CAST(amount AS INTEGER)) as total_amount FROM payments WHERE merchant_id = ? GROUP BY asset",
    )
    .bind(&merchant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let results: Vec<Value> = rows
        .into_iter()
        .map(|(asset, count, total_amount)| {
            json!({
                "asset": asset,
                "count": count,
                "total_amount_stroops": total_amount.unwrap_or_else(|| "0".to_string()),
            })
        })
        .collect();

    let result = json!(results);

    // Cache for 60 seconds
    let json_str = result.to_string();
    state.analytics_cache.insert(cache_key, json_str);

    Ok(Json(result))
}
