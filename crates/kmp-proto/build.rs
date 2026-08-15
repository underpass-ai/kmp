use std::env;
use std::path::PathBuf;

// The kernel contract lives twice on purpose. `api/proto` is where the
// contract is authored, linted and checked for breakage; this crate keeps
// a vendored copy under `proto/` because `cargo publish` only ships files
// inside the package directory, and a crate that compiles its protos from
// `../../api` builds here and nowhere else. `scripts/ci/contract-gate.sh`
// diffs the two so the copy can never quietly become a different wire.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let proto_root = manifest_dir.join("proto");
    let query_proto = proto_root.join("underpass/rehydration/kernel/v1beta1/query.proto");
    let command_proto = proto_root.join("underpass/rehydration/kernel/v1beta1/command.proto");
    let memory_proto = proto_root.join("underpass/rehydration/kernel/v1beta1/memory.proto");
    let common_proto = proto_root.join("underpass/rehydration/kernel/v1beta1/common.proto");
    let descriptor_path =
        PathBuf::from(env::var("OUT_DIR")?).join("rehydration_kernel_v1beta1_descriptor.bin");

    for path in [
        &proto_root,
        &query_proto,
        &command_proto,
        &memory_proto,
        &common_proto,
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .file_descriptor_set_path(descriptor_path)
        .compile_protos(
            &[query_proto, command_proto, memory_proto],
            std::slice::from_ref(&proto_root),
        )?;

    Ok(())
}
