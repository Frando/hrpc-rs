fn main() {
    prost_build::compile_protos(&["src/hrpc.proto"], &["src"]).unwrap();
}
