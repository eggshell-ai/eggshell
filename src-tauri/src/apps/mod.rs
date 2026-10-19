//! Agent mode definitions.
//!
//! Each module owns one mode's tool selection and system prompt so that
//! `llm.rs` stays focused on the shared traits and orchestration plumbing.

mod admin_panel;
mod max_planner;
mod planning;
mod qa;

pub use admin_panel::{AdminPanel, AdminPanelApp};
pub use max_planner::MaxPlannerApp;
pub use planning::PlanningApp;
pub use qa::QaApp;
