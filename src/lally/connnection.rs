use crate::cluster::services::cluster_management_client::ClusterManagementClient;
use crate::cluster::services::kv_store_client::KvStoreClient;
use crate::cluster::services::{
    AddKvResponse, AddNodeRequest, GetKvResponse, KvData, KvOperation, NoContentRequest,
    RemoveKvResponse,
};
use crate::utils::Operation;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tonic::transport::{Channel, Uri};
use tonic::Request;
use tracing::{debug, error, info, span, Level};

// this module needs some good refactoring

#[derive(Default)]
pub struct Connections {
    pub cluster: Arc<RwLock<HashMap<String, Channel>>>,
}

impl Connections {
    pub async fn get_ips(&self) -> Vec<String> {
        let cluster = self.cluster.read().await;
        cluster.keys().cloned().collect()
    }
    pub async fn remove(&self, ip: &String) -> Result<String> {
        let mut cluster = self.cluster.write().await;

        match cluster.remove(ip) {
            Some(_channel) => {
                info!(ip = %ip, "Node removed successfully");
                Ok("Removed Node".to_string())
            }
            None => {
                error!(ip = %ip, "Node does not exist, removal failed");
                Err(anyhow!("Node doesn't exist"))
            }
        }
    }
    pub async fn leave(&self) {
        let cluster = self.cluster.write().await;
        info!("Starting to leave the cluster");
        let mut futures_set = JoinSet::new();
        for (ip, channel) in cluster.iter() {
            let channel = channel.clone();
            let ip = ip.clone();
            let remove_span = span!(Level::INFO, "remove_node", ip = %ip);
            let _enter = remove_span.enter();
            let request = Request::new(NoContentRequest {});
            futures_set.spawn(async move {
                let mut conn = ClusterManagementClient::new(channel);
                match conn.remove(request).await {
                    Ok(msg) => {
                        info!(ip = %ip, "Successfully removed node: {}", msg.into_inner().message)
                    }
                    Err(e) => error!(ip = %ip, "Error removing node: {}", e),
                }
            });
        }
        futures_set.join_all().await;
        info!("Completed leaving the cluster");
    }

    pub async fn conn_make(&self, addr: &String) -> Result<Channel> {
        let conn_span = span!(Level::INFO, "conn_make", addr = %addr);
        let _enter = conn_span.enter();

        let cluster = Arc::clone(&self.cluster);

        let cluster_guard = cluster.read().await;

        if let Some(channel) = cluster_guard.get(addr) {
            return Ok(channel.clone());
        }

        drop(cluster_guard);
        let mut cluster_guard = cluster.write().await;
        // we check again, in case another thread added the node between the read and write lock
        if let Some(channel) = cluster_guard.get(addr) {
            return Ok(channel.clone());
        }
        info!(
            "Attempting to make a connection to node at address: {}",
            addr
        );

        let client_uri = format!("http://{}", addr)
            .parse::<Uri>()
            .context("Failed to parse the client URI")?; // use anyhow to provide a better error message

        match Channel::builder(client_uri).connect().await {
            Ok(channel) => {
                info!("Successfully connected to {}", addr);

                cluster_guard.insert(addr.clone(), channel.clone());
                Ok(channel)
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", addr, e);
                Err(anyhow::anyhow!("Failed to connect to the channel").context(e))
            }
        }
    }

    pub async fn bulk_conn_make(&self, ip_addrs: &[String]) {
        let bulk_span = span!(Level::INFO, "bulk_conn_make", num_ips = ip_addrs.len());
        let _enter = bulk_span.enter();

        info!(
            "Starting bulk connection setup for {} IP addresses.",
            ip_addrs.len()
        );
        let cluster = Arc::clone(&self.cluster);
        let mut futures_set = JoinSet::new();
        for ip in ip_addrs.iter() {
            let ip = ip.clone();
            let client_uri = format!("https://{}", ip);
            futures_set.spawn(async move {
                let client_uri = match client_uri.parse::<Uri>() {
                    Ok(uri) => uri,
                    Err(err) => {
                        error!("Failed to parse URI for IP {}: {}", ip, err);
                        return None;
                    }
                };
                info!("Attempting to connect to IP: {}", ip);
                match Channel::builder(client_uri).connect().await {
                    Ok(channel) => {
                        info!("Successfully connected to IP: {}", ip);
                        Some((ip, channel))
                    }
                    Err(err) => {
                        error!("Failed to connect to {}: {}", ip, err);
                        None
                    }
                }
            });
        }

        let results = futures_set.join_all().await;
        for result in results.into_iter().flatten() {
            let mut cluster_guard = cluster.write().await;
            cluster_guard.insert(result.0, result.1);
        }
        info!("Finished processing bulk connection setup.");
    }

    pub async fn gossip(&self, addr: String) {
        let gossip_span = span!(Level::INFO, "gossip", addr = addr.clone());
        let _enter = gossip_span.enter();

        info!(
            "Starting gossip to add node with address: {} to cluster",
            addr
        );

        let cluster = Arc::clone(&self.cluster);
        let mut futures_set = JoinSet::new();
        let cluster_guard = cluster.read().await;

        for channel in cluster_guard.values() {
            let request = Request::new(AddNodeRequest { ip: addr.clone() });
            let channel = channel.clone();
            let addr = addr.clone();
            futures_set.spawn(async move {
                info!("Gossiping to node with address: {}", addr);
                let mut conn = ClusterManagementClient::new(channel);
                match conn.add(request).await {
                    Ok(msg) => {
                        info!("Successfully added node: {}", addr);
                        debug!("{}", msg.into_inner().message);
                    }
                    Err(e) => {
                        error!("Failed to gossip to {}: {}", addr, e);
                    }
                }
            });
        }
        futures_set.join_all().await;
        info!(
            "Completed gossip to add node with address: {} to cluster",
            addr
        );
    }

    pub async fn join(&self, addr: String) -> Result<Vec<KvData>> {
        let join_span = span!(Level::INFO, "join", addr = addr.clone());
        let _enter = join_span.enter();

        info!(
            "Starting join process to cluster through seed node: {}",
            addr
        );
        let seed_node_channel = self
            .conn_make(&addr)
            .await
            .context("failed to make connection to seed node")?;

        let request = Request::new(NoContentRequest {});
        let mut seed_node = ClusterManagementClient::new(seed_node_channel);
        let response = match seed_node.join(request).await {
            Ok(res) => {
                info!(
                    "Successfully joined the cluster through seed node: {}",
                    addr
                );
                res
            }
            Err(err) => {
                error!("Failed to join cluster through seed node {}: {}", addr, err);
                return Err(anyhow!("Failed to join cluster through seed node").into());
            }
        };
        let message = response.into_inner();
        self.bulk_conn_make(&message.addresses).await;
        Ok(message.store_data)
    }

    pub async fn get_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> Vec<(String, GetKvResponse)> {
        info!(
            "Initiating GET operation for key: {} in the cluster",
            operation.key
        );
        let cluster = Arc::clone(&self.cluster);

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };
        let cluster_guard = cluster.read().await;
        debug!("Needed quorum votes: {}", needed_quorum_votes);
        let mut futures_set = JoinSet::new();
        for (ip, channel) in cluster_guard.iter() {
            let request = Request::new(kv_operation.clone());
            let ip = ip.clone();
            let channel = channel.clone();
            futures_set.spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.get(request).await {
                    Ok(response) => {
                        info!("Successfully retrieved key from {}: {:?}", ip, response);
                        Ok((ip, response.into_inner()))
                    }
                    Err(e) => {
                        error!("Error retrieving key from {}: {}", ip, e);
                        Err(e.to_string())
                    }
                }
            });
        }
        let mut responses = Vec::new();
        while let Some(result) = futures_set.join_next().await {
            match result {
                Ok(Ok(response)) => {
                    responses.push(response);
                    if responses.len() == needed_quorum_votes {
                        debug!("Reached quorum with {} votes", responses.len());
                        return responses;
                    }
                }
                Ok(Err(e)) => {
                    error!("Failed request: {}", e);
                }
                Err(e) => {
                    error!("Task panicked: {:?}", e);
                }
            }
        }
        responses
    }

    pub async fn remove_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> Vec<RemoveKvResponse> {
        info!(
            "Initiating REMOVE operation for key: {} in the cluster",
            operation.key
        );

        let cluster = Arc::clone(&self.cluster);

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let cluster_guard = cluster.read().await;

        debug!("Needed quorum votes: {}", needed_quorum_votes);

        let mut futures_set = JoinSet::new();
        for channel in cluster_guard.values() {
            let request = Request::new(kv_operation.clone());
            let channel = channel.clone();
            futures_set.spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.remove(request).await {
                    Ok(response) => Ok(response.into_inner()),
                    Err(e) => Err(e.to_string()),
                }
            });
        }
        let mut responses = Vec::new();
        while let Some(result) = futures_set.join_next().await {
            match result {
                Ok(Ok(response)) => {
                    responses.push(response);
                    if responses.len() == needed_quorum_votes {
                        debug!("Quorum reached with {} responses.", responses.len());
                        return responses;
                    }
                }
                Ok(Err(e)) => {
                    error!("Error during REMOVE request: {}", e);
                }
                Err(e) => {
                    // task panic maybe?
                    error!("Error in task execution: {:?}", e);
                }
            }
        }
        responses
    }

    pub async fn solo_add_kv(&self, operation: &Operation, ip: &String) {
        let request = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };
        match self.conn_make(ip).await {
            Ok(channel) => {
                let mut conn = KvStoreClient::new(channel);
                match conn.add(Request::new(request)).await {
                    Ok(response) => {
                        info!(
                            "Successfully added key: {} with response: {:?}",
                            operation.key, response
                        );
                    }
                    Err(e) => {
                        error!("Error adding key {}: {}", operation.key, e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to create connection to {}: {}", ip, e);
            }
        }
    }

    pub async fn solo_remove_kv(&self, operation: &Operation, ip: &String) {
        let request = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };
        match self.conn_make(ip).await {
            Ok(channel) => {
                let mut conn = KvStoreClient::new(channel);
                match conn.remove(Request::new(request)).await {
                    Ok(response) => {
                        info!(
                            "Successfully removed key: {} with response: {:?}",
                            operation.key, response
                        );
                    }
                    Err(e) => {
                        error!("Error removing key {}: {}", operation.key, e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to create connection to {}: {}", ip, e);
            }
        }
    }

    pub async fn add_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> Vec<AddKvResponse> {
        info!(
            "Initiating ADD operation for key: {} in the cluster",
            operation.key
        );
        let cluster = Arc::clone(&self.cluster);

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let cluster_guard = cluster.read().await;
        debug!("Needed quorum votes: {}", needed_quorum_votes);
        let mut futures_set = JoinSet::new();
        for (ip, channel) in cluster_guard.iter() {
            let request = Request::new(kv_operation.clone());
            let channel = channel.clone();
            let ip = ip.clone();
            futures_set.spawn(async move {
                info!("Sending ADD request to IP: {}", ip);
                let mut conn = KvStoreClient::new(channel);
                match conn.add(request).await {
                    Ok(response) => {
                        info!("Successfully added key to {}: {:?}", ip, response);
                        Ok(response.into_inner())
                    }
                    Err(e) => {
                        error!("Error adding key to {}: {}", ip, e);
                        Err(e.to_string())
                    }
                }
            });
        }
        let mut responses = Vec::new();
        while let Some(result) = futures_set.join_next().await {
            match result {
                Ok(Ok(response)) => {
                    responses.push(response);
                    if responses.len() == needed_quorum_votes {
                        debug!("Quorum reachd with {} votes", responses.len());
                        return responses;
                    }
                }
                Ok(Err(e)) => {
                    error!("Failed request: {}", e);
                }
                Err(e) => {
                    error!("Task panicked: {:?}", e);
                }
            }
        }
        responses
    }
}
