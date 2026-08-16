//! Core in-daemon logic shared across the API surface.
//!
//! `service` implements the real `VisionApi` gRPC service — every RPC is
//! backed by the modules below. `stores::graph`/`stores::vectors` are
//! explicit interim stand-ins for Kùzu/LanceDB (`docs/ARCHITECTURE.md`
//! §9.1); `embed` is a stand-in for the local embedding model; `llm` uses
//! `candle` rather than literal llama.cpp bindings (§9.1 again). See each
//! module's doc comment and `docs/TASKS.md`'s Parking Lot.

pub mod blob;
pub mod chunk;
pub mod embed;
pub mod engine;
pub mod error;
pub mod extract;
pub mod graph_query;
pub mod ids;
pub mod ingest;
pub mod llm;
pub mod paths;
pub mod query;
pub mod service;
pub mod stores;
pub mod synthesize;

pub use engine::Engine;
pub use service::VisionApiService;
