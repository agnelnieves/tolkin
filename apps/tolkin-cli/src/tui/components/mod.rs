//! Component library for the dashboard. Every component takes data plus a
//! `&Theme`; none reads the environment, the clock, or the filesystem.

pub mod bars;
pub mod card;
pub mod chrome;
pub mod empty;
pub mod gauge;
pub mod list;
pub mod modal;
pub mod spinner;
