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
mod lint;
mod manifest;
mod paths;
mod procedural;
mod retriever;
mod semantic;

pub use core::{
    CORE_MEMORY_HARD_LIMIT, CORE_MEMORY_SOFT_LIMIT, CoreMemory, CoreMemoryLint, CoreMemorySource, LintSeverity,
};
pub use document::{MemoryDoc, MemoryFrontmatter, ValidationError};
pub use kinds::{MemoryKind, Provenance, SessionMeta, Verification, VerificationStatus};
pub use lint::{LintDiagnostic, LintRule, LintSeverity as MemoryLintSeverity, MemoryLinter};
pub use manifest::{ManifestEntry, ManifestStats, MemoryManifest, ProvenanceInfo, VerificationInfo};
pub use paths::{
    CORE_LOCAL_MEMORY_FILE, CORE_MEMORY_DIR, CORE_MEMORY_FILE, DECISIONS_DIR, EPISODIC_MEMORY_DIR, FACTS_DIR,
    INDEXES_DIR, MANIFEST_FILE, MEMORY_DIR, MemoryPaths, PLAYBOOKS_DIR, PROCEDURAL_MEMORY_DIR, SEMANTIC_MEMORY_DIR,
    TAGS_FILE, THUNDERUS_DIR_NAME,
};
pub use procedural::{
    IssueSeverity, NewPlaybook, PlaybookDoc, PlaybookIssue, PlaybookSections, PlaybookUpdate, ProceduralMemory,
};
pub use retriever::{
    InMemoryRetriever, MemoryRetriever, RetrievalError, RetrievalPolicy, RetrievalResult, RetrievedChunk, STOP_WORDS,
    format_memory_context,
};
pub use semantic::{AdrDoc, AdrUpdate, FactDoc, FactUpdate, NewAdr, NewFact, SemanticMemory};

/// Memory system version for compatibility tracking
pub const MEMORY_VERSION: &str = "1.0.0";
