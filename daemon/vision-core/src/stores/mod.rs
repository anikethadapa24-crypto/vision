//! SQLite-backed stores implementing `docs/ARCHITECTURE.md` §5.2's storage
//! rows. `graph` and `vectors` are explicit interim stand-ins for Kùzu and
//! LanceDB (see the module doc comments on each) — real embedded-database
//! integration is tracked in `docs/TASKS.md`'s Parking Lot, not silently
//! treated as done.

pub mod audit;
pub mod config;
pub mod graph;
pub mod vectors;
