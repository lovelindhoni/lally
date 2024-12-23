use crate::kv_store::InMemoryKVStore;
use crate::wal::replay;
use anyhow::Result;
use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct Payload {
    pub key: String,
    pub value: Option<String>,
    pub force: Option<bool>,
}

async fn add_kv(
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    if payload.value.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": "Missing required field: value" })),
        );
    }

    match kv_store.add(&payload, true).await {
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

async fn update_kv(
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    if payload.value.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": "Missing required field: value" })),
        );
    }

    match kv_store.update(&payload, true).await {
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
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    match kv_store.get(&payload).await {
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
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    match kv_store.remove(&payload, true).await {
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
    "Hello World! from kvr"
}

pub async fn run() -> Result<()> {
    let state = InMemoryKVStore::new();
    replay(&state).await?;
    let app = Router::new()
        .route("/", get(greet))
        .route("/get", post(get_kv))
        .route("/add", post(add_kv))
        .route("/remove", delete(remove_kv))
        .route("/update", put(update_kv))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    println!("kvr started at 127.0.0.1:3000...");
    axum::serve(listener, app).await?;
    Ok(())
}
