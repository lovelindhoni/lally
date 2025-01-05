use crate::lally::Lally;
use crate::types::Operation;
use anyhow::Result;
use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct Payload {
    pub key: String,
    pub value: Option<String>,
}

async fn add_kv(
    State(lally): State<Arc<Lally>>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    if payload.value.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": "Missing required field: value" })),
        );
    }
    // perhaps creation of this operation struct can be made as a middleware?
    let operation = Operation {
        key: payload.key,
        value: payload.value,
        level: String::from("INFO"),
        name: String::from("ADD"),
    };
    lally.hooks.invoke_all(&operation).await;
    match lally.store.add(operation).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!({ "status": "success", "data": response })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn get_kv(
    State(lally): State<Arc<Lally>>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    let operation = Operation {
        key: payload.key,
        value: payload.value,
        level: String::from("INFO"),
        name: String::from("GET"),
    };

    match lally.store.get(operation).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!({ "status": "success", "data": response })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn remove_kv(
    State(lally): State<Arc<Lally>>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    let operation = Operation {
        key: payload.key,
        value: payload.value,
        level: String::from("INFO"),
        name: String::from("REMOVE"),
    };
    lally.hooks.invoke_all(&operation).await;
    match lally.store.remove(operation).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!({ "status": "success", "data": response })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn greet() -> &'static str {
    "Hello World! from lally"
}

pub async fn run(lally: Arc<Lally>, port: u32) -> Result<()> {
    let app = Router::new()
        .route("/", get(greet))
        .route("/get", post(get_kv))
        .route("/add", post(add_kv))
        .route("/remove", delete(remove_kv))
        .with_state(lally);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    println!("lally started at 0.0.0.0:{}...", port);
    axum::serve(listener, app).await?;
    Ok(())
}
