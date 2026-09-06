//! A lexical-only fixture must not inherit this developer machine's bridge.

use std::path::Path;

use kmp_mcp::KernelMcpServer;
use kmp_proto_mapping::v1beta1::LexicalBridge;

pub(super) fn open(data_dir: &Path) -> KernelMcpServer {
    // Version 1, one vector dimension, zero words, no provenance and the
    // required zero word offset. Validate the fixture with the real reader.
    let mut table = b"KMPBRIDG".to_vec();
    table.extend_from_slice(&1u16.to_le_bytes());
    table.extend_from_slice(&1u16.to_le_bytes());
    table.extend_from_slice(&0u32.to_le_bytes());
    table.extend_from_slice(&0u16.to_le_bytes());
    table.extend_from_slice(&0u32.to_le_bytes());
    assert!(
        LexicalBridge::from_bytes(&table)
            .expect("valid empty table")
            .is_silent()
    );
    std::fs::write(data_dir.join("lexical-bridge.kmpb"), table).expect("store-local empty bridge");
    KernelMcpServer::embedded(data_dir).expect("embedded server opens without machine vocabulary")
}
