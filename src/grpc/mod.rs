pub mod server;
pub mod client;
pub mod conversion;

// Include the generated protobuf code
pub mod proto {
    tonic::include_proto!("pskk");
}
