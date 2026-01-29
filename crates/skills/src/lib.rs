//! Thunderus Skills System
//!
//! This crate provides on-demand capability loading through the Skills system.
//! Skills are discovered from `.thunderus/skills/` directories and can be
//! loaded and executed as tools.

mod host_api;
mod loader;
mod parser;
mod types;

pub use host_api::{HostApiError, HostContext, KvStore};
pub use loader::SkillLoader;
pub use parser::parse_skill;
pub use types::{
    FilesystemPermissions, NetworkPermissions, PluginFunction, Result, ScriptType, Skill, SkillDriver, SkillError,
    SkillMatch, SkillMeta, SkillPermissions, SkillRisk, SkillScript, SkillsConfig,
};
