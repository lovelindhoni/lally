// #![allow(unused)]

mod kv_store;
mod server;

#[tokio::main]
async fn main() {
    server::run().await;
}
