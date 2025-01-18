use crate::cluster::services::GetKvResponse;
use crate::config::Config;
use crate::lally::Lally;
use crate::utils::{compare_timestamps, create_timestamp, timestamp_to_rfc3339, Operation};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info, span, warn, Level};

#[derive(Deserialize)]
pub struct Payload {
    pub key: String,
    pub value: Option<String>,
}

// helper utility to convert the payload to an operation struct, which will be used across all key-value operations
fn build_operation(payload: &Payload, operation_type: &str) -> Operation {
    Operation {
        key: payload.key.clone(),
        value: payload.value.clone(),
        level: String::from("INFO"),
        name: String::from(operation_type),
        timestamp: create_timestamp(),
    }
}

async fn add_kv(
    lally: web::Data<Arc<Lally>>,
    config: web::Data<Config>,
    payload: web::Json<Payload>,
) -> impl Responder {
    let trace_span = span!(Level::INFO, "ADD_KV");
    let _enter = trace_span.enter();

    if payload.value.is_none() {
        warn!(key = %payload.key, "Missing required field: value in payload");
        return HttpResponse::BadRequest().json(json!({
            "status": "error",
            "message": "Missing required field: value"
        }));
    }

    let operation = build_operation(&payload, "ADD");

    info!(key = %operation.key, "Incoming ADD operation");
    lally.hooks.invoke_all(&operation).await;
    let response = lally.store.add(&operation);
    let response_timestamp = response
        .timestamp
        .expect("timestamp will be present for ADD operation");

    info!(key = %operation.key, "Added key to local store, timestamp: {}", response_timestamp);

    // write_quorum - 1 means leaving out the current local node
    let needed_quorum_votes = config.write_quorum() - 1;
    let cluster_responses = lally.pool.add_kv(&operation, needed_quorum_votes).await;

    let is_quorum_achieved = cluster_responses.len() == needed_quorum_votes;
    let quorum_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };

    info!(key = %operation.key, quorum_state = %quorum_state);

    info!(key = %operation.key, "Key-Value successfully added");
    HttpResponse::Ok().json(json!({
        "status": quorum_state,
        "key": payload.key,
        "value": payload.value,
        "timestamp": timestamp_to_rfc3339(&response_timestamp),
        "quorum": {
            "required": config.write_quorum(),
            "achieved": cluster_responses.len() + 1
        },
        "message": if is_quorum_achieved {
            "Operation completed successfully."
        } else {
            "Partial quorum achieved; some nodes failed to respond."
        }
    }))
}

async fn get_kv(
    lally: web::Data<Arc<Lally>>,
    config: web::Data<Config>,
    payload: web::Json<Payload>,
) -> impl Responder {
    let trace_span = span!(Level::INFO, "GET_KV");
    let _enter = trace_span.enter();

    let operation = build_operation(&payload, "GET");

    info!(key = %operation.key, "Incoming GET operation");
    let needed_quorum_votes = config.read_quorum() - 1;

    info!(key = %operation.key, "Retrieving key from local store");
    let get_op = lally.store.get(&operation);

    let mut cluster_responses = lally.pool.get_kv(&operation, needed_quorum_votes).await;
    let get_op_converted = GetKvResponse {
        value: get_op.value,
        timestamp: get_op.timestamp,
    };
    cluster_responses.push(("local".to_string(), get_op_converted));
    let is_quorum_achieved = cluster_responses.len() == config.read_quorum();

    let quorum_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };

    info!(key = %operation.key, quorum_state = %quorum_state);
    // does read repair, prolly will be moved to a seperate function

    let max_timestamp = cluster_responses
        .iter()
        .filter_map(|(_, response)| response.timestamp)
        .max_by(compare_timestamps);

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
            debug!("Read repair triggered for nodes with outdated data");
            // Replicate to nodes with older or missing timestamps
            for (ip, _) in &cluster_responses {
                if nodes_with_latest_timestamp.contains(&ip) {
                    // Skip nodes with the latest timestamp
                    continue;
                }
                let read_repair_operation = Operation {
                    key: operation.key.to_string(),
                    value: latest_response.value.clone(),
                    name: String::from(if latest_response.value.is_some() {
                        // if it is None, the the latest operation occured on the key is REMOVE, so we
                        // need to remove, the kv on other nodes too, if else the key is
                        // Some(value), then we should ADD that to other nodes
                        "ADD"
                    } else {
                        "REMOVE"
                    }),
                    timestamp: latest_timestamp,
                    level: String::from("INFO"),
                };
                match &latest_response.value {
                    Some(_) => {
                        let lally_clone = Arc::clone(&lally);
                        let ip = ip.clone();
                        tokio::spawn(async move {
                            if ip == "local" {
                                // special case if the local node itself needs to be repaired
                                let _ = lally_clone.store.add(&read_repair_operation);
                            } else {
                                lally_clone
                                    .pool
                                    .solo_add_kv(&read_repair_operation, &ip)
                                    .await;
                            }
                        });
                    }
                    None => {
                        let lally_clone = Arc::clone(&lally);
                        let ip = ip.clone();
                        tokio::spawn(async move {
                            if ip == "local" {
                                let _ = lally_clone.store.remove(&read_repair_operation);
                            } else {
                                lally_clone
                                    .pool
                                    .solo_remove_kv(&read_repair_operation, &ip)
                                    .await;
                            }
                        });
                    }
                }
            }

            if let Some(value) = &latest_response.value {
                info!(key = %operation.key, "Key '{}' found with value '{}'", &operation.key, &value);
                return HttpResponse::Ok().json(json!({
                        "status": quorum_state,
                        "key": operation.key,
                        "value": value,
                        "timestamp": timestamp_to_rfc3339(&latest_timestamp),
                        "quorum": {
                            "required": config.read_quorum(),
                            "achieved": cluster_responses.len()
                        },
                        "message": format!("Key '{}' was fetched successfully.", operation.key)
                }));
            }
        }
    }

    warn!(key = %operation.key, "Key not found or quorum not achieved");
    HttpResponse::Ok().json(json!({
        "status": quorum_state,
        "key": operation.key,
        "value": null,
        "timestamp": null,
        "quorum": {
            "required": config.read_quorum(),
            "achieved": cluster_responses.len()
        },
        "message": format!("Key '{}' does not exist or quorum may not be reached", operation.key)
    }))
}

async fn remove_kv(
    lally: web::Data<Arc<Lally>>,
    config: web::Data<Config>,
    payload: web::Json<Payload>,
) -> impl Responder {
    let trace_span = span!(Level::INFO, "REMOVE_KV");
    let _enter = trace_span.enter();

    let operation = build_operation(&payload, "REMOVE");

    info!(key = %operation.key, "Incoming REMOVE operation");

    lally.hooks.invoke_all(&operation).await;

    debug!("Attempting to remove key from local node");
    let remove_response = lally.store.remove(&operation);

    let needed_quorum_votes = config.write_quorum() - 1;

    let cluster_responses = lally.pool.remove_kv(&operation, needed_quorum_votes).await;

    let is_quorum_achieved = cluster_responses.len() == needed_quorum_votes;
    let quorum_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };

    info!(key = %operation.key, quorum_state = %quorum_state);

    // Check if the key was removed, on atleast one of a node in pool
    let mut is_removed = remove_response.success;
    if !is_removed {
        for response in &cluster_responses {
            if response.is_removed {
                is_removed = response.is_removed;
                break;
            }
        }
    }

    let message = if is_removed {
        info!(key = %operation.key, "Key successfully removed");
        format!("Key '{}' was successfully removed.", operation.key)
    } else {
        warn!(key = %operation.key, "Key removal failed or quorum not achieved");
        format!(
            "Key '{}' does not exist in the store or quorum not achieved",
            operation.key
        )
    };

    HttpResponse::Ok().json(json!({
        "status": quorum_state,
        "key": operation.key,
        "value": if is_removed { remove_response.value } else { None },
        "timestamp": if is_removed {
            Some(timestamp_to_rfc3339(&operation.timestamp))
        } else {
            None
        },
        "quorum": {
            "required": config.write_quorum(),
            "achieved": cluster_responses.len() + 1
        },
        "message": message
    }))
}

async fn greet() -> impl Responder {
    "Hello World! from lally"
}

pub async fn run(lally: Arc<Lally>, config: Config) -> std::io::Result<()> {
    let addr = format!("0.0.0.0:{}", config.http_port());

    info!("HTTP Server started at {}", &addr);

    // passing config as a shareable state, maybe i am retarded
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&lally)))
            .app_data(web::Data::new(config.clone()))
            .route("/add", web::post().to(add_kv))
            .route("/get", web::post().to(get_kv))
            .route("/remove", web::delete().to(remove_kv))
            .route("/greet", web::get().to(greet))
    })
    .bind(addr)?
    .run()
    .await
}
