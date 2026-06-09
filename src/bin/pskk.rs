use glib::MainLoop;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::runtime::Runtime;
use pskk::grpc::client::PSKKClient;
use pskk::util::{get_config_data, init_logging};
use tracing::{info, error, warn};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let config = get_config_data()
        .ok()
        .map(|(config, _)| config);
    
    init_logging(config.as_ref())?;
    
    info!("Starting PSKK IBus Engine...");
    info!("This is a minimal IBus engine stub - full implementation coming next");

    // Create tokio runtime for async operations
    let rt = Arc::new(Runtime::new()?);
    
    info!("Connecting to PSKK gRPC server...");
    
    // Connect to gRPC server
    let client = rt.block_on(async {
        match PSKKClient::connect("http://127.0.0.1:50051".to_string()).await {
            Ok(client) => {
                info!("✓ Connected to PSKK gRPC server");
                Some(Arc::new(Mutex::new(client)))
            }
            Err(e) => {
                error!("✗ Failed to connect to PSKK gRPC server: {}", e);
                error!("  Make sure pskk-server is running: cargo run --bin pskk-server");
                None
            }
        }
    });
    
    if client.is_none() {
        return Err("Failed to connect to PSKK gRPC server".into());
    }
    
    info!("PSKK IBus engine initialized successfully");
    warn!("Note: Full IBus protocol implementation is not yet complete");
    warn!("      This is a minimal stub for testing the build system");
    info!("Entering main loop...");
    
    // Run the main loop
    // TODO: Implement actual IBus engine protocol using D-Bus
    let main_loop = MainLoop::new(None, false);
    main_loop.run();

    Ok(())
}
