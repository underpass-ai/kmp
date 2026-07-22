//! Transport-neutral mapping between the v1beta1 proto contract and the
//! application/domain types. Extracted from `rehydration-transport-grpc` so
//! any composition (gRPC server, embedded MCP backend) can speak the same
//! wire shapes without linking transport infrastructure.

pub mod v1beta1;
