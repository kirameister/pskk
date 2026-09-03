use pskk::grpc::server::run_all_servers;
use pskk::util::{get_config_data, init_logging};

fn parse_addr(env_key: &str, default: &str) -> std::net::SocketAddr {
    std::env::var(env_key)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .unwrap_or_else(|e| {
            eprintln!(
                "Invalid {} value, falling back to {}: {}",
                env_key, default, e
            );
            default.parse().unwrap()
        })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    let config = get_config_data().ok().map(|(config, _)| config);

    init_logging(config.as_ref())?;

    println!("Starting PSKK Server...");

    // gRPC endpoint (used by the IBus Python client).
    let grpc_addr = parse_addr("PSKK_GRPC_ADDR", "127.0.0.1:50051");
    // JSON/TCP endpoint (used by the Fcitx 5 C++ addon).
    let json_addr = parse_addr("PSKK_JSON_ADDR", "127.0.0.1:50052");

    println!("gRPC server listening on {}", grpc_addr);
    println!("JSON server listening on {}", json_addr);

    run_all_servers(grpc_addr, json_addr).await?;

    Ok(())
}
