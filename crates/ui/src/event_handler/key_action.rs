/// Actions that can be triggered by key events
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    /// User wants to send a message
    SendMessage { message: String },
    /// User wants to execute a shell command
    ExecuteShellCommand { command: String },
    /// User approves an action
    Approve { action: String, risk: String },
    /// User rejects an action
    Reject { action: String, risk: String },
    /// User cancels an action
    Cancel { action: String, risk: String },
    /// User wants to cancel generation
    CancelGeneration,
    /// Toggle sidebar
    ToggleSidebar,
    /// Toggle verbosity level
    ToggleVerbosity,
    /// Toggle sidebar section collapse
    /// TODO: individual section control
    ToggleSidebarSection,
    /// Toggle theme variant
    ToggleTheme,
    /// Toggle Advisor mode (read-only/suggestions only)
    ToggleAdvisorMode,
    /// Toggle Inspector view
    ToggleInspector,
    /// Inspect specific memory document
    InspectMemory { path: String },
    /// Navigate within inspector
    InspectorNavigate,
    /// Open external editor for current input
    OpenExternalEditor,
    /// Navigate message history (handled internally by InputState)
    NavigateHistory,
    /// Activate fuzzy finder
    ActivateFuzzyFinder,
    /// Select file in fuzzy finder
    SelectFileInFinder { path: String },
    /// Navigate fuzzy finder up
    NavigateFinderUp,
    /// Navigate fuzzy finder down
    NavigateFinderDown,
    /// Toggle fuzzy finder sort mode
    ToggleFinderSort,
    /// Cancel fuzzy finder
    CancelFuzzyFinder,
    /// Slash command: switch provider/model
    SlashCommandModel { model: String },
    /// Slash command: change approval mode
    SlashCommandApprovals { mode: String },
    /// Slash command: change verbosity level
    SlashCommandVerbosity { level: String },
    /// Slash command: show session stats
    SlashCommandStatus,
    /// Slash command: display PLAN.md content
    SlashCommandPlan,
    /// Slash command: add item to plan
    SlashCommandPlanAdd { item: String },
    /// Slash command: mark plan item as done
    SlashCommandPlanDone { index: usize },
    /// Slash command: trigger review pass
    SlashCommandReview,
    /// Slash command: display MEMORY.md content
    SlashCommandMemory,
    /// Slash command: add fact to memory
    SlashCommandMemoryAdd { fact: String },
    /// Slash command: search memory store
    SlashCommandMemorySearch { query: String },
    /// Slash command: pin memory document
    SlashCommandMemoryPin { id: String },
    /// Slash command: clear transcript (keep session history)
    SlashCommandClear,
    /// Slash command: garden consolidate session
    SlashCommandGardenConsolidate { session_id: String },
    /// Slash command: garden hygiene check
    SlashCommandGardenHygiene,
    /// Slash command: garden drift detection
    SlashCommandGardenDrift,
    /// Slash command: garden verify document
    SlashCommandGardenVerify { doc_id: String },
    /// Slash command: garden statistics
    SlashCommandGardenStats,
    /// Slash command: search session with ripgrep
    SlashCommandSearch {
        query: String,
        scope: thunderus_core::SearchScope,
    },
    /// Memory hits panel navigation
    MemoryHitsNavigate,
    /// Open a document from the memory hits panel
    MemoryHitsOpen { path: String },
    /// Pin/unpin a document from the memory hits panel
    MemoryHitsPin { id: String },
    /// Close the memory hits panel
    MemoryHitsClose,
    /// Navigate to next action card
    NavigateCardNext,
    /// Navigate to previous action card
    NavigateCardPrev,
    /// Toggle expand/collapse on focused card
    ToggleCardExpand,
    /// Toggle verbose mode on focused card
    ToggleCardVerbose,
    /// Scroll transcript up by one line
    ScrollUp,
    /// Scroll transcript down by one line
    ScrollDown,
    /// Page up in transcript
    PageUp,
    /// Page down in transcript
    PageDown,
    /// Jump to top of transcript
    ScrollToTop,
    /// Jump to bottom of transcript
    ScrollToBottom,
    /// Collapse previous sidebar section
    CollapseSidebarSection,
    /// Expand next sidebar section
    ExpandSidebarSection,
    /// Retry last failed action
    RetryLastFailedAction,
    /// Focus slash command input
    FocusSlashCommand,
    /// Clear transcript view (keep history)
    ClearTranscriptView,
    /// Exit the TUI application
    Exit,
    /// Navigate to next patch in diff queue
    NavigateNextPatch,
    /// Navigate to previous patch in diff queue
    NavigatePrevPatch,
    /// Navigate to next hunk in current patch
    NavigateNextHunk,
    /// Navigate to previous hunk in current patch
    NavigatePrevHunk,
    /// Approve currently selected hunk
    ApproveHunk,
    /// Reject currently selected hunk
    RejectHunk,
    /// Toggle hunk details view
    ToggleHunkDetails,
    /// No action (e.g., navigation in input)
    NoOp,
    /// Open a file from the inspector
    InspectorOpenFile { path: String },
    /// Start the reconcile ritual after drift/interruption
    StartReconcileRitual,
    /// Continue after reconciliation (accept changes)
    ReconcileContinue,
    /// Discard user changes during reconciliation
    ReconcileDiscard,
    /// Stop/reset agent during reconciliation
    ReconcileStop,
    /// Rewind to previous message (undo last sent message)
    RewindLastMessage,
}
