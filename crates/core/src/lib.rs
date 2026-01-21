pub mod approval;
pub mod classification;
pub mod config;
pub mod context;
pub mod error;
pub mod layout;
pub mod memory;
pub mod patch;
pub mod patch_queue_manager;
pub mod search;
pub mod session;
pub mod task_context;
pub mod teaching;
pub mod views;

pub use approval::{
    ActionType, ApprovalContext, ApprovalDecision, ApprovalGate, ApprovalId, ApprovalProtocol, ApprovalRecord,
    ApprovalRequest, ApprovalResponse, ApprovalStats, AutoApprove, AutoReject, Interactive,
};
pub use classification::{Classification, ToolRisk};
pub use config::{ApprovalMode, Config, Profile, ProviderConfig, SandboxMode};
pub use context::{CONTEXT_FILES, ContextLoader, LOCAL_CONTEXT_PATTERN, LoadedContext};
pub use error::{BlockedCommandError, Error, Result};
pub use layout::{AgentDir, SessionId, SessionIdError, ViewFile};
pub use memory::{
    CORE_MEMORY_DIR, CORE_MEMORY_FILE, CORE_MEMORY_HARD_LIMIT, CORE_MEMORY_SOFT_LIMIT, CoreMemory, CoreMemoryLint,
    CoreMemorySource, DECISIONS_DIR, EPISODIC_MEMORY_DIR, FACTS_DIR, INDEXES_DIR, LintSeverity, MANIFEST_FILE,
    MEMORY_DIR, MEMORY_VERSION, MemoryDoc, MemoryFrontmatter, MemoryKind, MemoryPaths, PLAYBOOKS_DIR,
    PROCEDURAL_MEMORY_DIR, Provenance, SEMANTIC_MEMORY_DIR, SessionMeta, TAGS_FILE, THUNDERUS_DIR_NAME, Verification,
    VerificationStatus,
};
pub use patch::{Hunk, Patch, PatchId, PatchQueue};
pub use patch_queue_manager::PatchQueueManager;
pub use search::{SearchHit, SearchScope, search_session};
pub use session::{Event, LoggedEvent, PatchStatus, Seq, Session, TokensUsed};
pub use task_context::{TaskContext, TaskContextTracker};
pub use teaching::{TeachingState, get_hint_for_concept, suggest_concept};
pub use views::{MaterializedViews, ViewKind, ViewMaterializer};
