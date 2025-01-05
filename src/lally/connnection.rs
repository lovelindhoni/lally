use crate::cluster::membership::cluster_management_client::ClusterManagementClient;
use crate::cluster::membership::{AddNodeRequest, Empty};
use futures::future::join_all;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tonic::Request;

// this module needs some good refactoring

#[derive(Default)]
pub struct Connections {
    pub cluster: RwLock<HashMap<String, ClusterManagementClient<Channel>>>,
}

impl Connections {
    pub async fn get_ips(&self) -> Vec<String> {
        let cluster = self.cluster.read().await;
        cluster.keys().cloned().collect()
    }
    pub async fn remove(&self, ip: &String) {
        let mut cluster = self.cluster.write().await;
        cluster.remove(ip).unwrap();
    }

    pub async fn leave(&self) {
        let mut pool = self.cluster.write().await;
        let leave_cluster_futures = pool
            .values_mut()
            .map(|conn| {
                let request = Request::new(Empty {});
                async move {
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

    pub async fn make(
        &self,
        addr: String,
    ) -> Result<ClusterManagementClient<Channel>, tonic::transport::Error> {
        let mut cluster = self.cluster.write().await;
        match cluster.get(&addr) {
            Some(node) => Ok(node.clone()),
            None => {
                let client_url = format!("https://{}", addr);
                let node = ClusterManagementClient::connect(client_url).await?;
                cluster.insert(addr, node.clone());
                Ok(node)
            }
        }
    }
    pub async fn bulk_make(&self, ip_addrs: &[String]) {
        let mut cluster = self.cluster.write().await;
        let conn_futures = ip_addrs.iter().map(|ip| {
            let client_url = format!("https://{}", ip);
            async move {
                match ClusterManagementClient::connect(client_url).await {
                    Ok(node_conn) => Some((ip.clone(), node_conn)),
                    Err(err) => {
                        eprintln!("Failed to connect to {}: {}", ip, err);
                        None
                    }
                }
            }
        });
        let results = join_all(conn_futures).await;
        for result in results.into_iter().flatten() {
            println!("{} : {:?}", result.0, result.1);
            cluster.insert(result.0, result.1);
        }
    }
    pub async fn gossip(&self, addr: String) {
        // gossip protocol needs to be implemented
        let mut cluster = self.cluster.write().await;
        let gossip_results = cluster
            .values_mut()
            .map(|conn| {
                let request = Request::new(AddNodeRequest { ip: addr.clone() });
                async move {
                    let response = conn.add(request).await;
                    match response {
                        Ok(msg) => println!("{}", msg.into_inner().message),
                        Err(e) => eprintln!("{}", e),
                    }
                }
            })
            .collect::<Vec<_>>();
        let _ = join_all(gossip_results).await;
    }
    pub async fn join(&self, addr: String) {
        let mut seed_node = self.make(addr).await.unwrap();
        let request = Request::new(Empty {});
        let response = seed_node.join(request).await.unwrap();
        let message = response.into_inner();
        self.bulk_make(&message.addresses).await;
    }
}
