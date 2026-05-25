use pskk::grpc::server::run_server;
use pskk::util::{get_config_data, init_logging};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    let config = get_config_data()
        .ok()
        .map(|(config, _)| config);
    
    init_logging(config.as_ref())?;
    
    println!("Starting PSKK gRPC Server...");
    
    let addr = "127.0.0.1:50051".parse()?;
    println!("Server listening on {}", addr);
    
    run_server(addr).await?;
    
    Ok(())
}
