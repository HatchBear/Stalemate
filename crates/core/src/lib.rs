//! Stalemate core library (crate `stalemate-core`). Module list: 10-technical-architecture.md.

pub mod account;
pub mod api_client;
pub mod broadcast;
pub mod build_check;
pub mod build_matcher;
pub mod builds;
pub mod counter;
pub mod game_state;
pub mod ipc;
pub mod live_safe;
pub mod match_watcher;
pub mod pace;
pub mod projector;
pub mod purchase_log;
pub mod review;
pub mod scouting;
pub mod settings;
pub mod storage;
pub mod types;

pub use types::*;
