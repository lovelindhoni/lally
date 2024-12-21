use crate::kv_store::InMemoryKVStore;
use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    key: String,
    value: Option<String>,
}

async fn add_kv(
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    if payload.value.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "Missing required field: value".to_string(),
        )
            .into_response();
    }
    match kv_store
        .add(
            &payload.key,
            &payload.value.expect("value will be always present here..."),
        )
        .await
    {
        Ok(response) => (StatusCode::OK, response).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_kv(
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    if payload.value.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "Missing required field: value".to_string(),
        )
            .into_response();
    }
    match kv_store
        .update(
            &payload.key,
            &payload.value.expect("value will be always present here..."),
        )
        .await
    {
        Ok(response) => (StatusCode::OK, response).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn get_kv(
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    match kv_store.get(&payload.key).await {
        Ok(response) => (StatusCode::OK, response).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn remove_kv(
    State(kv_store): State<InMemoryKVStore>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    match kv_store.remove(&payload.key).await {
        Ok(response) => (StatusCode::OK, response).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn greet() -> &'static str {
    "Hello World! from kvr"
}

pub async fn run() {
    let state = InMemoryKVStore::new();
    let app = Router::new()
        .route("/", get(greet))
        .route("/get", post(get_kv))
        .route("/add", post(add_kv))
        .route("/remove", delete(remove_kv))
        .route("/update", put(update_kv))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("kvr started at 127.0.0.1:3000...");
    axum::serve(listener, app).await.unwrap();
}
