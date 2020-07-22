# hrpc-rs

Simple RPC with Protobuf Services

Rust port of [hrpc for Node.js](https://github.com/mafintosh/hrpc)

HRPC is a binary, bidirectional RPC protocol. The top-level crate contains the protocol parser and high-level RPC interface. Services, methods, requests and responses are defined in Protocol Buffer files. `hrpc-build` can create a full client from this schema file. It also creates traits that makes it straightforward to implement RPC services. 
