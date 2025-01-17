pub mod services {
    tonic::include_proto!("lally");
}

use crate::lally::Lally;
use crate::utils::Operation;
use anyhow::{Context, Result};
use services::cluster_management_server::{ClusterManagement, ClusterManagementServer};
use services::kv_store_server::{KvStore, KvStoreServer};
use services::{
    AddKvResponse, AddNodeRequest, AddNodeResponse, GetKvResponse, JoinResponse, KvOperation,
    NoContentRequest, RemoveKvResponse, RemoveNodeResponse,
};
use std::sync::Arc;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::{error, info};

fn convert_to_operation(request: KvOperation) -> Operation {
    // this of a fn would convert the grpc kvOperation to Operation struct which is widely
    // used in lally, my retardness...
    Operation {
        name: request.name,
        level: request.level,
        value: request.value,
        timestamp: request.timestamp.expect("Timestamp should be present"),
        key: request.key,
    }
}

#[derive(Clone)]
pub struct GrpcServer {
    lally: Arc<Lally>,
}

impl GrpcServer {
    pub async fn run(lally: Arc<Lally>) -> Result<()> {
        let addr = "0.0.0.0:50071" // yes, i have hardcoded the port now, for good reasons
            .parse()
            .context("Failed to parse addr for the GRPC server")?;
        let my_cluster = GrpcServer { lally };

        info!("GRPC server listening on {}", addr);

        tokio::spawn(
            Server::builder()
                .add_service(ClusterManagementServer::new(my_cluster.clone()))
                .add_service(KvStoreServer::new(my_cluster))
                .serve(addr),
        );

        Ok(())
    }
}

#[tonic::async_trait]
impl KvStore for GrpcServer {
    async fn get_kv(
        &self,
        request: Request<KvOperation>,
    ) -> Result<Response<GetKvResponse>, Status> {
        let operation = convert_to_operation(request.into_inner());

        let get_response = self.lally.store.get(&operation);

        Ok(Response::new(GetKvResponse {
            value: get_response.value,
            timestamp: get_response.timestamp,
        }))
    }

    async fn add_kv(
        &self,
        request: Request<KvOperation>,
    ) -> Result<Response<AddKvResponse>, Status> {
        let operation = convert_to_operation(request.into_inner());

        self.lally.hooks.invoke_all(&operation).await;

        let _add_response = self.lally.store.add(&operation);

        Ok(Response::new(AddKvResponse {
            message: "key-value pair added".to_string(),
        }))
    }

    async fn remove_kv(
        &self,
        request: Request<KvOperation>,
    ) -> Result<Response<RemoveKvResponse>, Status> {
        let operation = convert_to_operation(request.into_inner());

        self.lally.hooks.invoke_all(&operation).await;

        let remove_response = self.lally.store.remove(&operation);

        if remove_response.success {
            Ok(Response::new(RemoveKvResponse {
                message: "key-value pair removed".to_string(),
                is_removed: remove_response.success,
            }))
        } else {
            Err(Status::invalid_argument(
                "Failed to remove kv because it doesn't exist",
            ))
        }
    }
}

#[tonic::async_trait]
impl ClusterManagement for GrpcServer {
    async fn join(
        &self,
        request: Request<NoContentRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        // getting the addr of the client node that is trying to join the cluster
        let mut client_addr = request.remote_addr().unwrap();
        client_addr.set_port(50071);

        let client_addr_str = client_addr.to_string();
        info!("Client {} attempting to join cluster", client_addr_str);

        // gossiping the client node addr
        self.lally.pool.gossip(client_addr_str.clone()).await;

        self.lally
            .pool
            .conn_make(&client_addr_str)
            .await
            .map_err(|e| {
                error!("Failed to connect to {}: {}", client_addr_str, e);
                Status::invalid_argument(format!("Failed to connect to client: {}", e))
            })?;

        info!("Client {} successfully joined the cluster", client_addr_str);

        // we are packing up the store data and the nodes connected in the cluster rn and send it
        // to the client node so that it could also replicate
        let nodes_addrs: Vec<String> = self.lally.pool.get_addrs().await;
        let store_data = self.lally.store.export_store();

        Ok(Response::new(JoinResponse {
            message: "Joined successfully".to_string(),
            addresses: nodes_addrs,
            store_data,
        }))
    }

    async fn remove_node(
        &self,
        request: Request<NoContentRequest>,
    ) -> Result<Response<RemoveNodeResponse>, Status> {
        let mut client_ip = request.remote_addr().unwrap();
        client_ip.set_port(50071);
        let client_ip = client_ip.to_string();

        info!("Attempting to remove node {}", client_ip);

        self.lally.pool.remove(&client_ip).await.map_err(|err| {
            error!("Failed to remove node {}: {}", client_ip, err);
            Status::internal(format!("Failed to remove node: {}", err))
        })?;

        info!("Node {} removed successfully", client_ip);

        Ok(Response::new(RemoveNodeResponse {
            message: "Node removed successfully".to_string(),
        }))
    }

    async fn add_node(
        &self,
        request: Request<AddNodeRequest>,
    ) -> Result<Response<AddNodeResponse>, Status> {
        let new_commer = request.into_inner().ip;

        info!("Attempting to add node {}", new_commer);

        self.lally.pool.conn_make(&new_commer).await.map_err(|e| {
            error!("Failed to add node {}: {}", new_commer, e);
            Status::invalid_argument(format!("Failed to add node: {}", e))
        })?;

        info!("Node {} added successfully", new_commer);

        Ok(Response::new(AddNodeResponse {
            message: "Node added successfully".to_string(),
        }))
    }
}
