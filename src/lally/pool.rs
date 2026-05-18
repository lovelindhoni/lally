use crate::cluster::services::cluster_management_client::ClusterManagementClient;
use crate::cluster::services::kv_store_client::KvStoreClient;
use crate::cluster::services::{
    AddKvResponse, AddNodeRequest, GetKvResponse, KvData, KvOperation, NoContentRequest,
    RemoveKvResponse,
};
use crate::utils::Operation;
use anyhow::{anyhow, Context, Result};
use papaya::HashMap;
use rapidhash::fast::RandomState;
use tokio::task::JoinSet;
use tonic::transport::{Channel, Uri};
use tonic::Request;
use tracing::{debug, error, info, span, Level};

type PoolMap = HashMap<String, Channel, RandomState>;

pub struct Pool {
    pool: PoolMap,
}

impl Default for Pool {
    fn default() -> Self {
        Pool {
            pool: HashMap::builder().hasher(RandomState::default()).build(),
        }
    }
}

impl Pool {
    pub fn get_addrs(&self) -> Vec<String> {
        self.pool.pin().iter().map(|(k, _)| k.clone()).collect()
    }

    pub fn remove(&self, ip: &String) -> Result<String> {
        match self.pool.pin().remove(ip) {
            Some(_) => {
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
        info!("Starting to leave the cluster");
        let entries: Vec<(String, Channel)> = self
            .pool
            .pin()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let mut futures_set = JoinSet::new();
        for (ip, channel) in entries {
            let trace_span = span!(Level::INFO, "remove_node", ip = %ip);
            let _enter = trace_span.enter();
            let request = Request::new(NoContentRequest {});
            futures_set.spawn(async move {
                let mut conn = ClusterManagementClient::new(channel);
                match conn.remove_node(request).await {
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
        let trace_span = span!(Level::DEBUG, "conn_make", addr = %addr);
        let _enter = trace_span.enter();

        if let Some(channel) = self.pool.pin().get(addr) {
            return Ok(channel.clone());
        }

        info!(
            "Attempting to make a connection to node at address: {}",
            addr
        );

        let client_uri = format!("http://{}", addr)
            .parse::<Uri>()
            .context("Failed to parse the client URI")?;

        match Channel::builder(client_uri).connect().await {
            Ok(channel) => {
                info!("Successfully connected to {}", addr);
                // If a concurrent caller raced us, keep whichever channel landed first.
                let pin = self.pool.pin();
                let stored = pin.get_or_insert_with(addr.clone(), || channel.clone());
                Ok(stored.clone())
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", addr, e);
                Err(anyhow::anyhow!("Failed to connect to the channel").context(e))
            }
        }
    }

    pub async fn bulk_conn_make(&self, ip_addrs: &[String]) {
        let trace_span = span!(Level::INFO, "bulk_conn_make", num_ips = ip_addrs.len());
        let _enter = trace_span.enter();

        info!(
            "Starting bulk connection setup for {} IP addresses.",
            ip_addrs.len()
        );
        let mut futures_set = JoinSet::new();
        for ip in ip_addrs.iter() {
            let ip = ip.clone();
            let client_uri = format!("http://{}", ip);
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
        let pin = self.pool.pin();
        for (ip, channel) in results.into_iter().flatten() {
            pin.insert(ip, channel);
        }
        info!("Finished processing bulk connection setup.");
    }

    pub async fn gossip(&self, addr: String) {
        let trace_span = span!(Level::INFO, "gossip", addr = addr.clone());
        let _enter = trace_span.enter();

        info!(
            "Starting gossip to add node with address: {} to cluster",
            addr
        );

        let channels: Vec<Channel> = self.pool.pin().iter().map(|(_, v)| v.clone()).collect();
        let mut futures_set = JoinSet::new();
        for channel in channels {
            let request = Request::new(AddNodeRequest { ip: addr.clone() });
            let addr = addr.clone();
            futures_set.spawn(async move {
                debug!("Gossiping to node with address: {}", addr);
                let mut conn = ClusterManagementClient::new(channel);
                match conn.add_node(request).await {
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
        let trace_span = span!(Level::INFO, "join", addr = addr.clone());
        let _enter = trace_span.enter();

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
                return Err(anyhow!("Failed to join cluster through seed node"));
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
        debug!(
            "Initiating GET operation for key: {} in the cluster",
            operation.key
        );

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let entries: Vec<(String, Channel)> = self
            .pool
            .pin()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        debug!("Needed quorum votes: {}", needed_quorum_votes);
        let mut futures_set = JoinSet::new();
        for (ip, channel) in entries {
            let request = Request::new(kv_operation.clone());
            futures_set.spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.get_kv(request).await {
                    Ok(response) => {
                        debug!("Successfully retrieved key from {}: {:?}", ip, response);
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
        debug!(
            "Initiating REMOVE operation for key: {} in the cluster",
            operation.key
        );

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let channels: Vec<Channel> = self.pool.pin().iter().map(|(_, v)| v.clone()).collect();

        debug!("Needed quorum votes: {}", needed_quorum_votes);

        let mut futures_set = JoinSet::new();
        for channel in channels {
            let request = Request::new(kv_operation.clone());
            futures_set.spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.remove_kv(request).await {
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
                    error!("Error in task execution: {:?}", e);
                }
            }
        }
        responses
    }

    pub async fn add_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> Vec<AddKvResponse> {
        debug!(
            "Initiating ADD operation for key: {} in the cluster",
            operation.key
        );

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let entries: Vec<(String, Channel)> = self
            .pool
            .pin()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        debug!("Needed quorum votes: {}", needed_quorum_votes);
        let mut futures_set = JoinSet::new();
        for (ip, channel) in entries {
            let request = Request::new(kv_operation.clone());
            futures_set.spawn(async move {
                debug!("Sending ADD request to IP: {}", ip);
                let mut conn = KvStoreClient::new(channel);
                match conn.add_kv(request).await {
                    Ok(response) => {
                        debug!("Successfully added key to {}: {:?}", ip, response);
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
                match conn.add_kv(Request::new(request)).await {
                    Ok(response) => {
                        debug!(
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
                match conn.remove_kv(Request::new(request)).await {
                    Ok(response) => {
                        debug!(
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
}
