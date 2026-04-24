use pskk::grpc::server::run_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting PSKK gRPC Server...");
    
    let addr = "127.0.0.1:50051".parse()?;
    println!("Server listening on {}", addr);
    
    run_server(addr).await?;
    
    Ok(())
}
