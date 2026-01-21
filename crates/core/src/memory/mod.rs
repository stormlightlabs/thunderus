//! Tiered memory system for durable external memory
//!
//! The memory system provides persistent, reviewable, and git-friendly storage
//! for agent knowledge across multiple tiers:
//!
//! - **Core**: Always-loaded project knowledge
//! - **Semantic**: Curated facts and architectural decisions (ADR-lite)
//! - **Procedural**: Reusable playbooks and workflows
//! - **Episodic**: Session recaps and historical context
//!
//! # Directory Layout
//!
//! ```text
//! .thunderus/memory/
//! ├── core/           # Always-loaded project memory
//! ├── semantic/       # Facts + ADR-lite decisions
//! ├── procedural/     # Playbooks and workflows
//! ├── episodic/       # Session recaps
//! └── indexes/        # Manifests and search indexes
//! ```
//!
//! # Example
//!
//! ```ignore
//! use thunderus_core::memory::{MemoryPaths, CoreMemory};
//!
//! // Create paths from repository root
//! let paths = MemoryPaths::from_thunderus_root(repo_root);
//!
//! // Ensure directories exist
//! paths.ensure()?;
//!
//! // Load core memory with hierarchical precedence
//! let core = CoreMemory::load(&paths, &std::env::current_dir()?)?;
//!
//! // Access merged content
//! println!("Core memory: {} tokens", core.token_count);
//! ```

mod core;
mod document;
mod kinds;
mod paths;

pub use core::{
    CORE_MEMORY_HARD_LIMIT, CORE_MEMORY_SOFT_LIMIT, CoreMemory, CoreMemoryLint, CoreMemorySource, LintSeverity,
};
pub use document::{MemoryDoc, MemoryFrontmatter, ValidationError};
pub use kinds::{MemoryKind, Provenance, SessionMeta, Verification, VerificationStatus};
pub use paths::{
    CORE_LOCAL_MEMORY_FILE, CORE_MEMORY_DIR, CORE_MEMORY_FILE, DECISIONS_DIR, EPISODIC_MEMORY_DIR, FACTS_DIR,
    INDEXES_DIR, MANIFEST_FILE, MEMORY_DIR, MemoryPaths, PLAYBOOKS_DIR, PROCEDURAL_MEMORY_DIR, SEMANTIC_MEMORY_DIR,
    TAGS_FILE, THUNDERUS_DIR_NAME,
};

/// Memory system version for compatibility tracking
pub const MEMORY_VERSION: &str = "1.0.0";

/// Result type for memory operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors specific to memory operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Document not found
    #[error("Document not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_version() {
        assert_eq!(MEMORY_VERSION, "1.0.0");
    }

    #[test]
    fn test_error_display() {
        let err = Error::Parse("test error".to_string());
        assert!(err.to_string().contains("Parse error"));
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let mem_err: Error = io_err.into();
        assert!(matches!(mem_err, Error::Io(_)));
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_ok() -> Result<String> {
            Ok("success".to_string())
        }

        fn returns_err() -> Result<String> {
            Err(Error::NotFound("test".to_string()))
        }

        assert!(returns_ok().is_ok());
        assert!(returns_err().is_err());
    }
}
