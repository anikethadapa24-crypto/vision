//! Core in-daemon logic shared across the API surface.
//!
//! `service` implements the M1 stub `VisionApi` gRPC service — fixed
//! responses, no persistence. Real logic replaces it milestone by
//! milestone starting with Permissions/Audit in M2 (`docs/TASKS.md`).

pub mod paths;
pub mod service;

pub use service::VisionApiService;
