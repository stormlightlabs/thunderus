pub mod approval;
pub mod classification;
pub mod config;
pub mod context;
pub mod drift;
pub mod error;
pub mod layout;
pub mod memory;
pub mod patch;
pub mod patch_queue_manager;
pub mod provenance;
pub mod search;
pub mod session;
pub mod task_context;
pub mod teaching;
pub mod trajectory;
pub mod views;

pub use trajectory::{TrajectoryNode, TrajectoryWalker};

pub use approval::{
    ActionType, ApprovalContext, ApprovalDecision, ApprovalGate, ApprovalId, ApprovalProtocol, ApprovalRecord,
    ApprovalRequest, ApprovalResponse, ApprovalStats, AutoApprove, AutoReject,
};
pub use classification::{Classification, ToolRisk};
pub use config::{ApprovalMode, Config, Profile, ProviderConfig, SandboxMode};
pub use context::{CONTEXT_FILES, ContextLoader, LOCAL_CONTEXT_PATTERN, LoadedContext};
pub use drift::{DriftEvent, DriftMonitor, SnapshotManager};
pub use error::{BlockedCommandError, Error, Result};
pub use layout::{AgentDir, SessionId, SessionIdError, ViewFile};
pub use memory::{
    CORE_MEMORY_DIR, CORE_MEMORY_FILE, CORE_MEMORY_HARD_LIMIT, CORE_MEMORY_SOFT_LIMIT, CoreMemory, CoreMemoryLint,
    CoreMemorySource, DECISIONS_DIR, EPISODIC_MEMORY_DIR, FACTS_DIR, INDEXES_DIR, LintSeverity, MANIFEST_FILE,
    MEMORY_DIR, MEMORY_VERSION, ManifestEntry, ManifestStats, MemoryDoc, MemoryFrontmatter, MemoryKind, MemoryManifest,
    MemoryPaths, PLAYBOOKS_DIR, PROCEDURAL_MEMORY_DIR, ProceduralMemory, Provenance, ProvenanceInfo,
    SEMANTIC_MEMORY_DIR, SemanticMemory, SessionMeta, TAGS_FILE, THUNDERUS_DIR_NAME, Verification, VerificationInfo,
    VerificationStatus,
};
pub use patch::{Hunk, MemoryPatch, MemoryPatchParams, Patch, PatchId, PatchQueue};
pub use patch_queue_manager::PatchQueueManager;
pub use provenance::{ProvenanceValidator, ValidationMode};
pub use search::{SearchHit, SearchScope, search_session};
pub use session::{Event, LoggedEvent, PatchStatus, Seq, Session, TokensUsed};
pub use task_context::{TaskContext, TaskContextTracker};
pub use teaching::{TeachingState, get_hint_for_concept, suggest_concept};
pub use views::{MaterializedViews, ViewKind, ViewMaterializer};
