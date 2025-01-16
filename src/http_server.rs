use crate::cluster::services::GetKvResponse;
use crate::config::Config;
use crate::lally::Lally;
use crate::utils::Operation;
use crate::utils::{compare_timestamps, CreateTimestamp};
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

fn build_operation(payload: &Payload, operation_type: &str) -> Operation {
    Operation {
        key: payload.key.clone(),
        value: payload.value.clone(),
        level: String::from("INFO"),
        name: String::from(operation_type),
        timestamp: CreateTimestamp::new(),
    }
}

async fn add_kv(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    if payload.value.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "message": "Missing required field: value" })),
        );
    }
    let operation = build_operation(&payload, "ADD");
    state.lally.hooks.invoke_all(&operation).await;
    let response = state.lally.store.add(&operation);
    let response_timestamp = response
        .timestamp
        .expect("timestamp will be present for ADD operation");
    let needed_quorum_votes = state.config.write_quorum() - 1;
    let (cluster_responses, is_quorum_achieved) = state
        .lally
        .cluster
        .add_kv(&operation, needed_quorum_votes)
        .await;
    let quorom_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };
    println!("{:?}", cluster_responses);
    (
        StatusCode::OK,
        // i might return the no of quorum votes too
        Json(
            json!({ "status": quorom_state, "data": response.message, "timestamp":  CreateTimestamp::to_rfc3339(&response_timestamp) }),
        ),
    )
}

async fn get_kv(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    let operation = build_operation(&payload, "GET");
    let needed_quorum_votes = state.config.read_quorum() - 1;
    let get_op = state.lally.store.get(&operation);
    let (mut cluster_responses, is_quorum_achieved) = state
        .lally
        .cluster
        .get_kv(&operation, needed_quorum_votes)
        .await;
    let get_op_converted = GetKvResponse {
        value: get_op.value,
        timestamp: get_op.timestamp,
    };
    cluster_responses.push(("local".to_string(), get_op_converted));
    let quorom_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };

    // does read repair, prolly moved to a seperate function

    let max_timestamp = cluster_responses
        .iter()
        .filter_map(|(_, response)| response.timestamp)
        .max_by(|a, b| compare_timestamps(a, b));

    let nodes_with_latest_timestamp: Vec<&String> = cluster_responses
        .iter()
        .filter(|(_, response)| response.timestamp.as_ref() == max_timestamp.as_ref())
        .map(|(ip, _)| ip)
        .collect();

    if let Some(latest_timestamp) = max_timestamp {
        // Find the latest response (assuming all nodes with the latest timestamp have the same value)
        if let Some((_, latest_response)) = cluster_responses
            .iter()
            .find(|(_, response)| response.timestamp.as_ref() == Some(&latest_timestamp))
        {
            // Step 5: Replicate to nodes with older or missing timestamps
            for (ip, _response) in &cluster_responses {
                if nodes_with_latest_timestamp.contains(&&ip) {
                    // Skip nodes with the latest timestamp
                    continue;
                }
                let read_repair_operation = Operation {
                    key: operation.key.to_string(),
                    value: latest_response.value.clone(),
                    name: String::from(if latest_response.value.is_some() {
                        "ADD"
                    } else {
                        "REMOVE"
                    }),
                    timestamp: latest_timestamp,
                    level: String::from("INFO"),
                };
                match &latest_response.value {
                    Some(_msg) => {
                        let lally_clone = Arc::clone(&state.lally);
                        let ip = ip.clone();
                        tokio::spawn(async move {
                            if ip == "local" {
                                let _ = lally_clone.store.add(&read_repair_operation);
                            } else {
                                lally_clone
                                    .cluster
                                    .solo_add_kv(&read_repair_operation, &ip)
                                    .await;
                                // remote grpc add_kv call
                            }
                        });
                    }
                    None => {
                        let lally_clone = Arc::clone(&state.lally);
                        let ip = ip.clone();
                        tokio::spawn(async move {
                            if ip == "local" {
                                let _ = lally_clone.store.remove(&read_repair_operation);
                            } else {
                                lally_clone
                                    .cluster
                                    .solo_remove_kv(&read_repair_operation, &ip)
                                    .await;
                                // remote grpc remove_kv call
                            }
                        });
                    }
                }
            }
            if let Some(message) = &latest_response.value {
                return (
                    StatusCode::OK,
                    Json(json!({ "status": quorom_state, "data": message })),
                );
            }
        }
    }
    (
        StatusCode::OK,
        Json(json!({ "status": quorom_state, "data": "No such kv exists" })),
    )
}

async fn remove_kv(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<Payload>,
) -> impl IntoResponse {
    let operation = build_operation(&payload, "REMOVE");
    state.lally.hooks.invoke_all(&operation).await;
    let remove_response = state.lally.store.remove(&operation);
    let needed_quorum_votes = state.config.write_quorum() - 1;
    println!("quorum needed, {}", needed_quorum_votes);
    let (cluster_responses, is_quorum_achieved) = state
        .lally
        .cluster
        .remove_kv(&operation, needed_quorum_votes)
        .await;
    let quorom_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };
    let mut is_removed = remove_response.success;
    println!("{:?}", cluster_responses);
    if !is_removed {
        for response in cluster_responses {
            if response.is_removed {
                is_removed = response.is_removed;
                break;
            }
        }
    }
    let response = if is_removed {
        format!("kv removed from store: key {}", operation.key)
    } else {
        format!(
            "kv not removed from store because it doesn't exists: {}",
            operation.key
        )
    };
    (
        StatusCode::OK,
        Json(
            json!({ "status": quorom_state, "data": response, "timestamp": if is_removed {
                Some(CreateTimestamp::to_rfc3339(&operation.timestamp))
            } else {
                None
            }}),
        ),
    )
}

async fn greet() -> &'static str {
    "Hello World! from lally"
}

#[derive(Clone)]
struct SharedState {
    pub lally: Arc<Lally>,
    pub config: Config,
}
pub async fn run(lally: Arc<Lally>, config: Config) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port())).await?;
    println!("lally started at 0.0.0.0:{}...", config.port());

    let state = Arc::new(SharedState { lally, config });
    let app = Router::new()
        .route("/", get(greet))
        .route("/get", post(get_kv))
        .route("/add", post(add_kv))
        .route("/remove", delete(remove_kv))
        .with_state(state);

    axum::serve(listener, app).await?;
    Ok(())
}
