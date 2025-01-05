use super::lally_services::cluster_management_server::{
    ClusterManagement, ClusterManagementServer,
};
use super::lally_services::kv_store_server::{KvStore, KvStoreServer};
use super::lally_services::{
    AddKvResponse, AddNodeRequest, AddNodeResponse, GetKvResponse, JoinResponse, KvOperation,
    NoContentRequest, RemoveKvResponse, RemoveNodeResponse,
};
use crate::lally::Lally;
use crate::types::Operation;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

fn convert_to_operation(request: KvOperation) -> Operation {
    Operation {
        name: request.name,
        level: request.level,
        value: request.value,
        timestamp: request.timestamp.expect("Timestamp should be present"),
        key: request.key,
    }
}

// this module needs a whole shit of error handling to be done

#[derive(Clone)]
pub struct GrpcServer {
    lally: Arc<Lally>,
}
impl GrpcServer {
    pub async fn run(lally: Arc<Lally>) {
        let addr = "0.0.0.0:50071".parse().unwrap();
        let my_cluster = GrpcServer { lally };
        println!("cluster server listening on {}", addr);
        tokio::spawn(
            Server::builder()
                .add_service(ClusterManagementServer::new(my_cluster.clone()))
                .add_service(KvStoreServer::new(my_cluster))
                .serve(addr),
        );
    }
}

#[tonic::async_trait]
impl KvStore for GrpcServer {
    async fn get(&self, request: Request<KvOperation>) -> Result<Response<GetKvResponse>, Status> {
        let operation = convert_to_operation(request.into_inner());
        let temp = self.lally.store.get(&operation);
        Ok(Response::new(GetKvResponse {
            message: temp.message,
            timestamp: temp.timestamp,
        }))
    }
    async fn add(&self, request: Request<KvOperation>) -> Result<Response<AddKvResponse>, Status> {
        let operation = convert_to_operation(request.into_inner());
        self.lally.hooks.invoke_all(&operation).await;
        let _ = self.lally.store.add(&operation);
        Ok(Response::new(AddKvResponse {
            message: "Yooooo".to_string(),
        }))
    }
    async fn remove(
        &self,
        request: Request<KvOperation>,
    ) -> Result<Response<RemoveKvResponse>, Status> {
        let operation = convert_to_operation(request.into_inner());
        self.lally.hooks.invoke_all(&operation).await;
        let is_removed = match self.lally.store.remove(&operation) {
            Ok(_response) => true,
            Err(_e) => false,
        };
        Ok(Response::new(RemoveKvResponse {
            message: "Yooooo".to_string(),
            is_removed,
        }))
    }
}

#[tonic::async_trait]
impl ClusterManagement for GrpcServer {
    async fn join(
        &self,
        request: Request<NoContentRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        let mut client_addr = request.remote_addr().unwrap();
        client_addr.set_port(50071);
        println!("{}", client_addr.to_string());
        self.lally.cluster.gossip(client_addr.to_string()).await;
        let server_nodes_ip: Vec<String> = self.lally.cluster.get_ips().await;
        self.lally
            .cluster
            .make(&client_addr.to_string())
            .await
            .unwrap();
        println!("{}", client_addr);
        Ok(Response::new(JoinResponse {
            message: "Joined".to_string(),
            addresses: server_nodes_ip,
        }))
    }
    async fn remove(
        &self,
        request: Request<NoContentRequest>,
    ) -> Result<Response<RemoveNodeResponse>, Status> {
        let mut client_ip = request.remote_addr().unwrap();
        client_ip.set_port(50071);
        let client_ip = client_ip.to_string();
        self.lally.cluster.remove(&client_ip).await;
        Ok(Response::new(RemoveNodeResponse {
            message: "Removed Node".to_string(),
        }))
    }
    async fn add(
        &self,
        request: Request<AddNodeRequest>,
    ) -> Result<Response<AddNodeResponse>, Status> {
        let new_commer = request.into_inner().ip;
        if let Err(e) = self.lally.cluster.make(&new_commer).await {
            Ok(Response::new(AddNodeResponse {
                message: e.to_string(),
            }))
        } else {
            Ok(Response::new(AddNodeResponse {
                message: "added".to_string(),
            }))
        }
    }
}
