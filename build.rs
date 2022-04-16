const RLW_PROTO_PATH: &str = "./src/proto/service.proto";

fn main() {
    tonic_build::compile_protos(RLW_PROTO_PATH).unwrap_or_else(|e| {
        panic!(
            "Failed to compile proto with path {}: {:?}",
            RLW_PROTO_PATH, e
        )
    });
}
