fn main() {
    hrpc_build::compile_protos(&["src/example-rpc.proto"], &["src"]).unwrap();
}
