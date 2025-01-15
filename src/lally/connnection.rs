use crate::cluster::services::cluster_management_client::ClusterManagementClient;
use crate::cluster::services::kv_store_client::KvStoreClient;
use crate::cluster::services::{
    AddKvResponse, AddNodeRequest, GetKvResponse, KvData, KvOperation, NoContentRequest,
    RemoveKvResponse,
};
use crate::utils::Operation;
use anyhow::{anyhow, Context, Result};
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tonic::transport::{Channel, Uri};
use tonic::Request;

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
            Some(_channel) => Ok("Removed Node".to_string()),
            None => Err(anyhow!("Node doesnt't exists")),
        }
    }

    pub async fn leave(&self) {
        let mut cluster = self.cluster.write().await;
        println!("leave cluster function statrted");
        let leave_cluster_futures = cluster
            .values_mut()
            .map(|channel| {
                let request = Request::new(NoContentRequest {});
                let channel = channel.clone();
                async move {
                    let mut conn = ClusterManagementClient::new(channel);
                    let response = conn.remove(request).await;
                    match response {
                        Ok(msg) => println!("{}", msg.into_inner().message),
                        Err(e) => eprintln!("{}", e),
                    }
                }
            })
            .collect::<Vec<_>>();

        let _ = join_all(leave_cluster_futures).await;
    }

    pub async fn conn_make(&self, addr: &String) -> Result<Channel> {
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

        let client_uri = format!("http://{}", addr)
            .parse::<Uri>()
            .context("Failed to parse the client URI")?; // use anyhow to provide a better error message
        println!("{}", client_uri);

        let channel = Channel::builder(client_uri)
            .connect()
            .await
            .context("Failed to connect to the channel")?; // additional error context

        cluster_guard.insert(addr.clone(), channel.clone());
        Ok(channel)
    }

    pub async fn bulk_conn_make(&self, ip_addrs: &[String]) {
        let cluster = Arc::clone(&self.cluster);
        let conn_futures = ip_addrs.iter().map(|ip| {
            let client_uri = format!("https://{}", ip);
            let ip = ip.clone();
            async move {
                let client_uri = match client_uri.parse::<Uri>() {
                    Ok(uri) => uri,
                    Err(err) => {
                        eprintln!("Failed to parse URI for IP {}: {}", ip, err);
                        return None; // Skip this IP and move to the next one
                    }
                };

                match Channel::builder(client_uri).connect().await {
                    Ok(channel) => Some((ip, channel)),
                    Err(err) => {
                        eprintln!("Failed to connect to {}: {}", ip, err);
                        None
                    }
                }
            }
        });
        let results = join_all(conn_futures).await;

        for result in results.into_iter().flatten() {
            println!("Connected to {}: {:?}", result.0, result.1);
            let mut cluster_guard = cluster.write().await;
            cluster_guard.insert(result.0, result.1);
        }
    }

    pub async fn gossip(&self, addr: String) {
        let cluster = Arc::clone(&self.cluster);

        let gossip_results = {
            let cluster_guard = cluster.read().await;
            cluster_guard
                .values()
                .map(|channel| {
                    let request = Request::new(AddNodeRequest { ip: addr.clone() });
                    let channel = channel.clone();
                    async move {
                        let mut conn = ClusterManagementClient::new(channel);
                        let response = conn.add(request).await;
                        match response {
                            Ok(msg) => println!("{}", msg.into_inner().message),
                            Err(e) => eprintln!("{}", e),
                        }
                    }
                })
                .collect::<Vec<_>>()
        };

        let _ = join_all(gossip_results).await;
    }

    pub async fn join(&self, addr: String) -> Result<Vec<KvData>> {
        let seed_node_channel = self
            .conn_make(&addr)
            .await
            .context("failed to make connection to seed node")?;
        let request = Request::new(NoContentRequest {});
        let mut seed_node = ClusterManagementClient::new(seed_node_channel);
        let response = seed_node
            .join(request)
            .await
            .context("failed to join cluster through seed node")?;
        let message = response.into_inner();
        self.bulk_conn_make(&message.addresses).await;
        Ok(message.store_data)
    }

    pub async fn get_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> (Vec<(String, GetKvResponse)>, bool) {
        let cluster = Arc::clone(&self.cluster);
        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };
        let cluster_guard = cluster.read().await;
        println!("{}", cluster_guard.len());
        let (tx, mut rx) = mpsc::channel(cluster_guard.len() + 1);
        println!("{}", needed_quorum_votes);
        for (ip, channel) in cluster_guard.iter() {
            let tx = tx.clone();
            let request = Request::new(kv_operation.clone());
            let ip = ip.clone();
            let channel = channel.clone();
            tokio::spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.get(request).await {
                    Ok(response) => {
                        let _ = tx.send(Ok((ip, response.into_inner()))).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string())).await;
                    }
                }
            });
        }
        drop(tx);
        let mut responses = Vec::new();
        while let Some(result) = rx.recv().await {
            match result {
                Ok(response) => {
                    responses.push(response);
                    if responses.len() == needed_quorum_votes {
                        return (responses, true);
                    }
                }
                Err(response) => {
                    println!("{:?}", response);
                    continue;
                }
            }
        }
        let responses_len = responses.len();
        (responses, responses_len == needed_quorum_votes)
    }

    pub async fn remove_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> (Vec<RemoveKvResponse>, bool) {
        let cluster = Arc::clone(&self.cluster);

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let cluster_guard = cluster.read().await;
        println!("{}", cluster_guard.len());
        let (tx, mut rx) = mpsc::channel(cluster_guard.len() + 1);

        println!("{}", needed_quorum_votes);

        for channel in cluster_guard.values() {
            let tx = tx.clone();
            let request = Request::new(kv_operation.clone());
            let channel = channel.clone();
            tokio::spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.remove(request).await {
                    Ok(response) => {
                        let _ = tx.send(Ok(response.into_inner())).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string())).await;
                    }
                }
            });
        }
        drop(tx);
        let mut responses = Vec::new();
        while let Some(result) = rx.recv().await {
            match result {
                Ok(response) => {
                    println!("{:?}", response);
                    responses.push(response);
                    if responses.len() == needed_quorum_votes {
                        return (responses, true);
                    }
                }
                Err(response) => {
                    println!("{:?}", response);
                    continue;
                }
            }
        }
        let responses_len = responses.len();
        (responses, responses_len == needed_quorum_votes)
    }

    pub async fn solo_add_kv(&self, operation: &Operation, ip: &String) {
        let request = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };
        let channel = self.conn_make(ip).await.unwrap();
        let mut conn = KvStoreClient::new(channel);
        match conn.add(request).await {
            Ok(response) => println!("{:?}", response),
            Err(e) => eprintln!("{}", e),
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
        let channel = self.conn_make(ip).await.unwrap();
        let mut conn = KvStoreClient::new(channel);
        match conn.remove(request).await {
            Ok(response) => println!("{:?}", response),
            Err(e) => eprintln!("{}", e),
        }
    }

    pub async fn add_kv(
        &self,
        operation: &Operation,
        needed_quorum_votes: usize,
    ) -> (Vec<AddKvResponse>, bool) {
        let cluster = Arc::clone(&self.cluster);

        let kv_operation = KvOperation {
            name: operation.name.clone(),
            level: operation.level.clone(),
            value: operation.value.clone(),
            timestamp: Some(operation.timestamp),
            key: operation.key.clone(),
        };

        let cluster_guard = cluster.read().await;
        println!("{}", cluster_guard.len());
        let (tx, mut rx) = mpsc::channel(cluster_guard.len() + 1);

        println!("{}", needed_quorum_votes);

        for channel in cluster_guard.values() {
            let tx = tx.clone();
            let request = Request::new(kv_operation.clone());
            let channel = channel.clone();
            tokio::spawn(async move {
                let mut conn = KvStoreClient::new(channel);
                match conn.add(request).await {
                    Ok(response) => {
                        let _ = tx.send(Ok(response.into_inner())).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string())).await;
                    }
                }
            });
        }
        drop(tx);
        let mut responses = Vec::new();
        while let Some(result) = rx.recv().await {
            match result {
                Ok(response) => {
                    println!("{:?}", response);
                    responses.push(response);
                    if responses.len() == needed_quorum_votes {
                        return (responses, true);
                    }
                }
                Err(response) => {
                    println!("{:?}", response);
                    continue;
                }
            }
        }
        let responses_len = responses.len();
        (responses, responses_len == needed_quorum_votes)
    }
}
