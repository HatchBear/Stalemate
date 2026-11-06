//! M-14 Post-match review engine (PRG-15, PRG-16, PRG-17). See 07-post-match-review-engine.md.
//!
//! Alpha: the stats-only Review built from API-05 match metadata (`stats`), and its storage
//! (`persist`). There is no win-probability model, tier, accuracy or LLM text here (Beta).

mod persist;
mod stats;
#[cfg(test)]
mod tests;

pub use persist::{persist_stats_review, ReviewError};
pub use stats::{
    build_stats_review, LaneLead, LaneSection, LaneUnavailable, R02Finding, ReviewFlags,
    ReviewResult, SoulRow, SoulSource, SoulSources, StatsReview,
};
