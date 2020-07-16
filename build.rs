fn main() {
    prost_build::compile_protos(&["src/error.proto"], &["src"]).unwrap();
}
