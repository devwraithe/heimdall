fn main() {
    tonic_prost_build::compile_protos("../../proto/heimdall.proto")
        .expect("Failed to compile proto files");
}
