//! Thunderus Skills System
//!
//! This crate provides on-demand capability loading through the Skills system.
//! Skills are discovered from `.thunderus/skills/` directories and can be
//! loaded and executed as tools.

mod loader;
mod parser;
mod types;

pub use loader::SkillLoader;
pub use parser::parse_skill;
pub use types::{Result, ScriptType, Skill, SkillError, SkillMatch, SkillMeta, SkillRisk, SkillScript, SkillsConfig};
