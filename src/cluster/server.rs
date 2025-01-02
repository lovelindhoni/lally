use super::membership::cluster_management_server::{ClusterManagement, ClusterManagementServer};
use super::membership::{AddNodeRequest, AddNodeResponse, Empty, JoinResponse, RemoveNodeResponse};
use crate::lally::Lally;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

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
                .add_service(ClusterManagementServer::new(my_cluster))
                .serve(addr),
        );
    }
}
#[tonic::async_trait]
impl ClusterManagement for GrpcServer {
    async fn join(&self, request: Request<Empty>) -> Result<Response<JoinResponse>, Status> {
        let mut client_addr = request.remote_addr().unwrap();
        client_addr.set_port(50071);
        self.lally.cluster.gossip(client_addr.to_string()).await;
        let server_nodes_ip: Vec<String> = self.lally.cluster.get_ips().await;
        self.lally
            .cluster
            .make(client_addr.to_string())
            .await
            .unwrap();
        println!("{}", client_addr);
        Ok(Response::new(JoinResponse {
            message: "Yoooo".to_string(),
            addresses: server_nodes_ip,
        }))
    }
    async fn remove(
        &self,
        request: Request<Empty>,
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
        if let Err(e) = self.lally.cluster.make(new_commer).await {
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
