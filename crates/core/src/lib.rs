pub mod approval;
pub mod classification;
pub mod config;
pub mod context;
pub mod error;
pub mod layout;
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
pub use patch::{Hunk, Patch, PatchId, PatchQueue};
pub use patch_queue_manager::PatchQueueManager;
pub use search::{SearchHit, SearchScope, search_session};
pub use session::{Event, LoggedEvent, PatchStatus, Seq, Session, TokensUsed};
pub use task_context::{TaskContext, TaskContextTracker};
pub use teaching::{TeachingState, get_hint_for_concept, suggest_concept};
pub use views::{MaterializedViews, ViewKind, ViewMaterializer};
