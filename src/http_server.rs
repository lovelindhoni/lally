use crate::cluster::services::GetKvResponse;
use crate::config::Config;
use crate::lally::Lally;
use crate::utils::Operation;
use crate::utils::{compare_timestamps, CreateTimestamp};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
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
    lally: web::Data<Arc<Lally>>,
    config: web::Data<Config>,
    payload: web::Json<Payload>,
) -> impl Responder {
    if payload.value.is_none() {
        return HttpResponse::BadRequest().json(json!({
            "status": "error",
            "message": "Missing required field: value"
        }));
    }

    let operation = build_operation(&payload, "ADD");
    lally.hooks.invoke_all(&operation).await;
    let response = lally.store.add(&operation);
    let response_timestamp = response
        .timestamp
        .expect("timestamp will be present for ADD operation");

    let needed_quorum_votes = config.write_quorum() - 1;
    let cluster_responses = lally.cluster.add_kv(&operation, needed_quorum_votes).await;

    let is_quorum_achieved = cluster_responses.len() == needed_quorum_votes;

    let quorum_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };

    println!("{:?}", cluster_responses);

    HttpResponse::Ok().json(json!({
        "status": quorum_state,
        "key": payload.key,
        "value": payload.value,
        "timestamp": CreateTimestamp::to_rfc3339(&response_timestamp),
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
    let operation = build_operation(&payload, "GET");
    let needed_quorum_votes = config.read_quorum() - 1;
    let get_op = lally.store.get(&operation);
    let mut cluster_responses = lally.cluster.get_kv(&operation, needed_quorum_votes).await;
    let get_op_converted = GetKvResponse {
        value: get_op.value,
        timestamp: get_op.timestamp,
    };
    cluster_responses.push(("local".to_string(), get_op_converted));
    let is_quorum_achieved = cluster_responses.len() == config.read_quorum();
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
                        let lally_clone = Arc::clone(&lally);
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
                        let lally_clone = Arc::clone(&lally);
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

            if let Some(value) = &latest_response.value {
                return HttpResponse::Ok().json(json!({
                        "status": quorom_state,
                        "key": operation.key,
                        "value": value,
                        "timestamp": CreateTimestamp::to_rfc3339(&latest_timestamp),
                        "quorum": {
                            "required": config.read_quorum(),
                            "achieved": cluster_responses.len()
                        },
                        "message": format!("Key '{}' was fetched successfully.", operation.key)
                }));
            }
        }
    }

    HttpResponse::Ok().json(json!({
        "status": quorom_state,
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
    let operation = build_operation(&payload, "REMOVE");

    // Invoke hooks
    lally.hooks.invoke_all(&operation).await;

    // Remove from local store
    let remove_response = lally.store.remove(&operation);

    // Determine quorum votes needed
    let needed_quorum_votes = config.write_quorum() - 1;
    println!("quorum needed, {}", needed_quorum_votes);

    // Remove from cluster
    let cluster_responses = lally
        .cluster
        .remove_kv(&operation, needed_quorum_votes)
        .await;

    let is_quorum_achieved = cluster_responses.len() == needed_quorum_votes;

    // Determine quorum state
    let quorum_state = if is_quorum_achieved {
        "success"
    } else {
        "partial"
    };

    // Check if the key was removed
    let mut is_removed = remove_response.success;
    println!("{:?}", cluster_responses);

    if !is_removed {
        for response in &cluster_responses {
            if response.is_removed {
                is_removed = response.is_removed;
                break;
            }
        }
    }

    let message = if is_removed {
        format!("Key '{}' was successfully removed.", operation.key)
    } else {
        format!(
            "Key '{}' does not exist in the store or quorum not achieved",
            operation.key
        )
    };

    // Prepare JSON response
    HttpResponse::Ok().json(json!({
        "status": quorum_state,
        "key": operation.key,
        "value": if is_removed { remove_response.value } else { None },
        "timestamp": if is_removed {
            Some(CreateTimestamp::to_rfc3339(&operation.timestamp))
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
    let addr = format!("0.0.0.0:{}", config.port());

    println!("lally started at {}", &addr);

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
