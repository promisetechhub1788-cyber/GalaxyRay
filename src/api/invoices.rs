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
pub struct CreateInvoiceRequest {
    pub customer_name: String,
    pub customer_email: String,
    pub amount_stroops: String,
    #[serde(default)]
    pub asset: String,
}

#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    pub id: String,
    pub merchant_id: String,
    pub invoice_number: String,
    pub customer_name: String,
    pub customer_email: String,
    pub amount_stroops: String,
    pub asset: String,
    pub status: String,
    pub created_at: String,
    pub linked_payment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateInvoiceStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct LinkInvoiceRequest {
    pub payment_id: String,
}

/// POST /v1/merchants/{merchant_id}/invoices
pub async fn create(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
    JsonBody(req): JsonBody<CreateInvoiceRequest>,
) -> Result<(StatusCode, Json<InvoiceResponse>), AppError> {
    let asset = if req.asset.is_empty() {
        "XLM".to_string()
    } else {
        req.asset
    };

    let invoice_id = Uuid::new_v4().to_string();

    // Get next invoice number for this merchant (just count existing invoices + 1)
    let invoice_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoices WHERE merchant_id = ?",
    )
    .bind(&merchant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let next_number = (invoice_count + 1).to_string();

    sqlx::query(
        "INSERT INTO invoices (id, merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&invoice_id)
    .bind(&merchant_id)
    .bind(&next_number)
    .bind(&req.customer_name)
    .bind(&req.customer_email)
    .bind(&req.amount_stroops)
    .bind(&asset)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, Option<String>)>(
        "SELECT id, merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset, status, created_at, linked_payment_id FROM invoices WHERE id = ?",
    )
    .bind(&invoice_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    Ok((
        StatusCode::CREATED,
        Json(InvoiceResponse {
            id: row.0,
            merchant_id: row.1,
            invoice_number: row.2,
            customer_name: row.3,
            customer_email: row.4,
            amount_stroops: row.5,
            asset: row.6,
            status: row.7,
            created_at: row.8,
            linked_payment_id: row.9,
        }),
    ))
}

/// GET /v1/merchants/{merchant_id}/invoices
pub async fn list(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path(_merchant_path_id): Path<String>,
) -> Result<Json<Vec<InvoiceResponse>>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, Option<String>)>(
        "SELECT id, merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset, status, created_at, linked_payment_id FROM invoices WHERE merchant_id = ? ORDER BY created_at DESC",
    )
    .bind(&merchant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    let invoices: Vec<InvoiceResponse> = rows
        .into_iter()
        .map(|row| InvoiceResponse {
            id: row.0,
            merchant_id: row.1,
            invoice_number: row.2,
            customer_name: row.3,
            customer_email: row.4,
            amount_stroops: row.5,
            asset: row.6,
            status: row.7,
            created_at: row.8,
            linked_payment_id: row.9,
        })
        .collect();

    Ok(Json(invoices))
}

/// GET /v1/merchants/{merchant_id}/invoices/{invoice_id}
pub async fn get(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path((path_merchant_id, invoice_id)): Path<(String, String)>,
) -> Result<Json<InvoiceResponse>, AppError> {
    if path_merchant_id != merchant_id {
        return Err(AppError::new(StatusCode::FORBIDDEN, "forbidden", "unauthorized"));
    }

    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, Option<String>)>(
        "SELECT id, merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset, status, created_at, linked_payment_id FROM invoices WHERE id = ? AND merchant_id = ?",
    )
    .bind(&invoice_id)
    .bind(&merchant_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?
    .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "not_found", "invoice not found"))?;

    Ok(Json(InvoiceResponse {
        id: row.0,
        merchant_id: row.1,
        invoice_number: row.2,
        customer_name: row.3,
        customer_email: row.4,
        amount_stroops: row.5,
        asset: row.6,
        status: row.7,
        created_at: row.8,
        linked_payment_id: row.9,
    }))
}

/// PATCH /v1/merchants/{merchant_id}/invoices/{invoice_id}
pub async fn update_status(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path((path_merchant_id, invoice_id)): Path<(String, String)>,
    JsonBody(req): JsonBody<UpdateInvoiceStatusRequest>,
) -> Result<Json<InvoiceResponse>, AppError> {
    if path_merchant_id != merchant_id {
        return Err(AppError::new(StatusCode::FORBIDDEN, "forbidden", "unauthorized"));
    }

    // Validate status
    let valid_statuses = vec!["draft", "sent", "paid", "cancelled"];
    if !valid_statuses.contains(&req.status.as_str()) {
        return Err(AppError::bad_request("invalid_status", "status must be one of: draft, sent, paid, cancelled"));
    }

    // Update status
    let result = sqlx::query(
        "UPDATE invoices SET status = ? WHERE id = ? AND merchant_id = ?",
    )
    .bind(&req.status)
    .bind(&invoice_id)
    .bind(&merchant_id)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    if result.rows_affected() == 0 {
        return Err(AppError::new(StatusCode::NOT_FOUND, "not_found", "invoice not found"));
    }

    // Fetch updated invoice
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, Option<String>)>(
        "SELECT id, merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset, status, created_at, linked_payment_id FROM invoices WHERE id = ?",
    )
    .bind(&invoice_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    Ok(Json(InvoiceResponse {
        id: row.0,
        merchant_id: row.1,
        invoice_number: row.2,
        customer_name: row.3,
        customer_email: row.4,
        amount_stroops: row.5,
        asset: row.6,
        status: row.7,
        created_at: row.8,
        linked_payment_id: row.9,
    }))
}

/// POST /v1/merchants/{merchant_id}/invoices/{invoice_id}/link
pub async fn link_payment(
    State(state): State<Arc<AppState>>,
    Extension(AuthenticatedMerchant(merchant_id)): Extension<AuthenticatedMerchant>,
    Path((path_merchant_id, invoice_id)): Path<(String, String)>,
    JsonBody(req): JsonBody<LinkInvoiceRequest>,
) -> Result<Json<InvoiceResponse>, AppError> {
    if path_merchant_id != merchant_id {
        return Err(AppError::new(StatusCode::FORBIDDEN, "forbidden", "unauthorized"));
    }

    // Verify invoice exists and belongs to merchant
    let invoice_exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM invoices WHERE id = ? AND merchant_id = ?",
    )
    .bind(&invoice_id)
    .bind(&merchant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    if !invoice_exists {
        return Err(AppError::new(StatusCode::NOT_FOUND, "not_found", "invoice not found"));
    }

    // Verify payment exists and belongs to merchant
    let payment_exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM payments WHERE id = ? AND merchant_id = ?",
    )
    .bind(&req.payment_id)
    .bind(&merchant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    if !payment_exists {
        return Err(AppError::new(StatusCode::NOT_FOUND, "not_found", "payment not found"));
    }

    // Link payment to invoice
    sqlx::query(
        "UPDATE invoices SET linked_payment_id = ? WHERE id = ?",
    )
    .bind(&req.payment_id)
    .bind(&invoice_id)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    // Fetch updated invoice
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, Option<String>)>(
        "SELECT id, merchant_id, invoice_number, customer_name, customer_email, amount_stroops, asset, status, created_at, linked_payment_id FROM invoices WHERE id = ?",
    )
    .bind(&invoice_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "database error"))?;

    Ok(Json(InvoiceResponse {
        id: row.0,
        merchant_id: row.1,
        invoice_number: row.2,
        customer_name: row.3,
        customer_email: row.4,
        amount_stroops: row.5,
        asset: row.6,
        status: row.7,
        created_at: row.8,
        linked_payment_id: row.9,
    }))
}
