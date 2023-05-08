use std::env::var;
use std::io::Result;

fn main() -> Result<()> {
    prost_build::Config::new()
        .protoc_arg("--experimental_allow_proto3_optional")
        .out_dir(var("OUT").or(var("OUT_DIR")).unwrap())
        .compile_protos(
            &[
                "proto/cbmix/graph.proto",
                "proto/cbmix/message/message.proto",
            ],
            &["proto/"],
        )
}
