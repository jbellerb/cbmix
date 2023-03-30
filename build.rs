use std::io::Result;

fn main() -> Result<()> {
    prost_build::Config::new().compile_protos(
        &[
            "proto/cbmix/control.proto",
            "proto/cbmix/message/message.proto",
        ],
        &["proto/"],
    )
}
