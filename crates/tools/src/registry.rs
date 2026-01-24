use super::Tool;
use super::builtin::{
    EchoTool, EditTool, GlobTool, GrepTool, MultiEditTool, NoopTool, PatchTool, ReadTool, ShellTool, WriteTool,
};
use super::skill_tool::SkillTool;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thunderus_core::config::PathAccessResult;
use thunderus_core::{ApprovalGate, ApprovalMode, Profile, Result};
use thunderus_providers::{ToolResult, ToolSpec};
use thunderus_skills::SkillLoader;

/// Registry that holds all available tools
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Box<dyn Tool>>>>,
    /// Optional approval gate for controlling tool execution
    approval_gate: Option<ApprovalGate>,
    /// Optional profile for sandbox policy enforcement
    profile: Option<Profile>,
    /// Workspace root directories for edit tool validation (legacy, kept for compatibility)
    workspace_roots: Vec<PathBuf>,
}

impl ToolRegistry {
    /// Creates a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            approval_gate: None,
            profile: None,
            workspace_roots: Vec::new(),
        }
    }

    /// Creates a new tool registry with approval control
    pub fn with_approval(approval_gate: ApprovalGate, workspace_roots: Vec<PathBuf>) -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            approval_gate: Some(approval_gate),
            profile: None,
            workspace_roots,
        }
    }

    /// Creates a new tool registry with profile-based sandbox policy
    pub fn with_profile(profile: Profile) -> Self {
        let workspace_roots = profile.workspace.all_roots();
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            approval_gate: None,
            profile: Some(profile),
            workspace_roots,
        }
    }

    /// Creates a tool registry with all built-in tools registered
    pub fn with_builtin_tools() -> Self {
        let registry = Self::new();
        registry.register(NoopTool).unwrap();
        registry.register(EchoTool).unwrap();
        registry.register(GrepTool).unwrap();
        registry.register(GlobTool).unwrap();
        registry.register(ReadTool).unwrap();
        registry.register(ShellTool).unwrap();
        registry.register(PatchTool).unwrap();
        registry.register(WriteTool).unwrap();
        registry.register(EditTool).unwrap();
        registry.register(MultiEditTool).unwrap();
        registry
    }

    /// Sets the approval gate for this registry
    pub fn set_approval_gate(&mut self, gate: ApprovalGate) {
        self.approval_gate = Some(gate);
    }

    /// Sets the profile for sandbox policy enforcement
    pub fn set_profile(&mut self, profile: Profile) {
        let workspace_roots = profile.workspace.all_roots();
        self.profile = Some(profile);
        self.workspace_roots = workspace_roots;
    }

    /// Gets the approval gate
    pub fn approval_gate(&self) -> Option<&ApprovalGate> {
        self.approval_gate.as_ref()
    }

    /// Gets the profile
    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// Sets the workspace roots for edit tool validation (legacy, kept for compatibility)
    pub fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.workspace_roots = roots;
    }

    /// Gets the workspace roots (legacy, kept for compatibility)
    pub fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_roots
    }

    /// Get the risk level for a tool by name
    pub fn tool_risk(&self, tool_name: &str) -> Option<thunderus_core::ToolRisk> {
        let tools = self.tools.read().ok()?;
        tools.get(tool_name).map(|tool| tool.risk_level())
    }

    /// Check if a tool is read-only by name
    pub fn tool_is_read_only(&self, tool_name: &str) -> Option<bool> {
        let tools = self.tools.read().ok()?;
        tools.get(tool_name).map(|tool| tool.is_read_only())
    }

    /// Check if a path is within workspace boundaries
    pub fn is_within_workspace(&self, path: &Path) -> bool {
        match self.workspace_roots.is_empty() {
            true => true,
            false => self.workspace_roots.iter().any(|root| path.starts_with(root)),
        }
    }

    /// Check if tool execution requires approval
    fn check_approval_required(&self, tool: &dyn Tool, arguments: &serde_json::Value) -> Result<bool> {
        if tool.is_read_only() {
            return Ok(true);
        }

        let mode = match &self.approval_gate {
            Some(gate) => gate.mode(),
            None => ApprovalMode::Auto,
        };

        if let Some(profile) = &self.profile {
            if tool.name() == "shell"
                && let Some(cmd) = arguments.get("command").and_then(|v| v.as_str())
                && Self::is_network_command(cmd)
            {
                return self.check_network_access(profile, mode, cmd);
            }

            if let Some(path) = self.extract_target_path(tool, arguments) {
                let path_buf = PathBuf::from(&path);
                return self.check_path_access(profile, &path_buf, mode, tool);
            }
        }

        match mode {
            ApprovalMode::ReadOnly => Err(thunderus_core::Error::Approval(
                "Tool execution rejected: approval mode is read-only".to_string(),
            )),
            ApprovalMode::FullAccess => Ok(true),
            ApprovalMode::Auto => match self.extract_target_path(tool, arguments) {
                Some(path) => {
                    let path_buf = PathBuf::from(&path);
                    if self.is_within_workspace(&path_buf) {
                        Ok(true)
                    } else {
                        Err(thunderus_core::Error::Approval(format!(
                            "Tool execution requires approval: path '{}' is outside workspace",
                            path
                        )))
                    }
                }
                None => Ok(true),
            },
        }
    }

    /// Check if a command is a network command
    fn is_network_command(cmd: &str) -> bool {
        let cmd_lower = cmd.to_lowercase();
        cmd_lower.contains("curl ")
            || cmd_lower.contains("wget ")
            || cmd_lower.starts_with("curl")
            || cmd_lower.starts_with("wget")
            || cmd_lower.contains("ssh ")
            || cmd_lower.starts_with("ssh")
            || cmd_lower.contains("http://")
            || cmd_lower.contains("https://")
    }

    /// Check network access based on profile and mode
    fn check_network_access(&self, profile: &Profile, mode: ApprovalMode, cmd: &str) -> Result<bool> {
        match mode {
            ApprovalMode::FullAccess => Ok(true),
            ApprovalMode::ReadOnly => Err(thunderus_core::Error::Approval(format!(
                "Network command '{}' blocked: read-only mode",
                cmd
            ))),
            ApprovalMode::Auto => {
                if profile.is_network_allowed() {
                    Ok(true)
                } else {
                    Err(thunderus_core::Error::Approval(format!(
                        "Network command '{}' blocked: network access disabled",
                        cmd
                    )))
                }
            }
        }
    }

    /// Check path access based on profile, mode, and tool type
    fn check_path_access(&self, profile: &Profile, path: &Path, mode: ApprovalMode, tool: &dyn Tool) -> Result<bool> {
        match profile.check_path_access(path, mode) {
            PathAccessResult::Allowed => Ok(true),
            PathAccessResult::ReadOnly => {
                if tool.is_read_only() {
                    Ok(true)
                } else {
                    Err(thunderus_core::Error::Approval(format!(
                        "Write access to '{}' denied: read-only mode",
                        path.display()
                    )))
                }
            }
            PathAccessResult::Denied(reason) => Err(thunderus_core::Error::Approval(format!(
                "Access to '{}' denied: {}",
                path.display(),
                reason
            ))),
            PathAccessResult::NeedsApproval(reason) => Err(thunderus_core::Error::Approval(format!(
                "Access to '{}' requires approval: {}",
                path.display(),
                reason
            ))),
        }
    }

    /// Extract the target file path from tool arguments
    fn extract_target_path(&self, tool: &dyn Tool, arguments: &serde_json::Value) -> Option<String> {
        let tool_name = tool.name();

        match tool_name {
            "edit" | "read" => arguments
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            "multiedit" => arguments
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        }
    }

    /// Registers a new tool in the registry
    ///
    /// Returns error if a tool with the same name already exists
    pub fn register<T: Tool + 'static>(&self, tool: T) -> Result<()> {
        let name = tool.name().to_string();
        let mut tools = self.tools.write().unwrap();

        if tools.contains_key(&name) {
            return Err(thunderus_core::Error::Validation(format!(
                "Tool '{}' already registered",
                name
            )));
        }

        tools.insert(name.clone(), Box::new(tool));
        Ok(())
    }

    /// Gets a tool by name
    /// Note: Due to trait object limitations, this returns None for now
    /// Use the execute() method which works correctly
    pub fn get(&self, _name: &str) -> Option<Arc<dyn Tool>> {
        None
    }

    /// Checks if a tool exists
    pub fn has(&self, name: &str) -> bool {
        let tools = self.tools.read().unwrap();
        tools.contains_key(name)
    }

    /// Returns names of all registered tools
    pub fn list(&self) -> Vec<String> {
        let tools = self.tools.read().unwrap();
        tools.keys().cloned().collect()
    }

    /// Returns all tool specs (for sending to providers)
    pub fn specs(&self) -> Vec<ToolSpec> {
        let tools = self.tools.read().unwrap();
        tools.values().map(|tool| tool.spec()).collect()
    }

    /// Returns the number of registered tools
    pub fn count(&self) -> usize {
        let tools = self.tools.read().unwrap();
        tools.len()
    }

    /// Load and register skills from the default skills directories.
    ///
    /// This discovers skills from:
    /// - `.thunderus/skills/` (project-local, higher priority)
    /// - `~/.thunderus/skills/` (global, lower priority)
    ///
    /// Returns the number of skills successfully loaded.
    pub fn load_skills(&self) -> Result<usize> {
        let mut skill_loader = SkillLoader::new(thunderus_skills::SkillsConfig::default())?;
        self.load_skills_from_loader(&mut skill_loader)
    }

    /// Load and register skills from a custom SkillLoader.
    ///
    /// Returns the number of skills successfully loaded.
    pub fn load_skills_from_loader(&self, skill_loader: &mut SkillLoader) -> Result<usize> {
        let skills = skill_loader.discover()?;
        let mut loaded = 0;

        for skill_meta in skills {
            if let Ok(skill) = skill_loader.load(&skill_meta.name) {
                let tool = SkillTool::new((*skill).clone());

                if self.register(tool).is_ok() {
                    loaded += 1;
                }
            }
        }

        Ok(loaded)
    }

    /// Load and register a specific skill by name.
    ///
    /// Returns error if the skill is not found.
    pub fn load_skill(&self, name: &str) -> Result<()> {
        let mut skill_loader = SkillLoader::new(thunderus_skills::SkillsConfig::default())?;
        let skill = skill_loader.load(name)?;

        let tool = SkillTool::new((*skill).clone());
        self.register(tool)?;

        Ok(())
    }

    /// Create a registry with built-in tools and skills loaded.
    pub fn with_builtin_tools_and_skills() -> Result<Self> {
        let registry = Self::with_builtin_tools();
        registry.load_skills()?;
        Ok(registry)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to execute a tool from the registry directly
impl ToolRegistry {
    /// Executes a tool by name with given arguments
    ///
    /// This is a convenience method that combines getting a tool and executing it.
    /// Classification reasoning from the tool is included in the result for
    /// pedagogical value (teaching users the safety model).
    ///
    /// Uses dynamic classification based on the specific execution arguments
    /// (e.g., ShellTool classifies specific commands).
    ///
    /// Approval checks are performed before execution:
    /// - Read-only tools (grep, glob, read) always bypass approval
    /// - Edit tools check approval mode and workspace boundaries
    pub fn execute(&self, tool_name: &str, tool_call_id: String, arguments: &serde_json::Value) -> Result<ToolResult> {
        let tools = self.tools.read().unwrap();

        match tools.get(tool_name) {
            Some(tool) => {
                self.check_approval_required(tool.as_ref(), arguments)?;

                let classification = tool.classify_execution(arguments);

                let mut result = tool.execute(tool_call_id.clone(), arguments)?;
                if let Some(classification) = classification {
                    result = result.with_classification(classification);
                }
                Ok(result)
            }
            None => Err(thunderus_core::Error::Tool(format!(
                "Tool '{}' not found in registry",
                tool_name
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::EchoTool;
    use crate::builtin::NoopTool;

    #[test]
    fn test_new_registry() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count(), 0);
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_register_tool() {
        let registry = ToolRegistry::new();
        let noop = NoopTool;

        let result = registry.register(noop);
        assert!(result.is_ok());
        assert_eq!(registry.count(), 1);
        assert!(registry.has("noop"));
    }

    #[test]
    fn test_duplicate_tool() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let result = registry.register(NoopTool);
        assert!(result.is_err());
        assert!(matches!(result, Err(thunderus_core::Error::Validation(_))));
    }

    #[test]
    fn test_list_tools() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 1);
        assert!(tools.contains(&"noop".to_string()));
    }

    #[test]
    fn test_get_specs() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let specs = registry.specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name(), "noop");
    }

    #[test]
    fn test_execute_tool() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let args = serde_json::json!({});
        let result = registry.execute("noop", "call_123".to_string(), &args);
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_123");
        assert!(tool_result.is_success());
    }

    #[test]
    fn test_execute_tool_with_classification() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let args = serde_json::json!({});
        let result = registry.execute("noop", "call_456".to_string(), &args);
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_456");
        assert!(tool_result.is_success());

        assert!(tool_result.risk_level.is_some());
        assert_eq!(tool_result.risk_level.unwrap(), thunderus_core::ToolRisk::Safe);
        assert!(tool_result.classification_reasoning.is_some());
        assert!(
            tool_result
                .classification_reasoning
                .as_ref()
                .unwrap()
                .contains("no side effects")
        );
    }

    #[test]
    fn test_execute_echo_with_classification() {
        let registry = ToolRegistry::new();
        registry.register(EchoTool).unwrap();

        let args = serde_json::json!({"message": "test"});
        let result = registry.execute("echo", "call_789".to_string(), &args);
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_789");
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "test");

        assert!(tool_result.risk_level.is_some());
        assert_eq!(tool_result.risk_level.unwrap(), thunderus_core::ToolRisk::Safe);
        assert!(tool_result.classification_reasoning.is_some());
        assert!(
            tool_result
                .classification_reasoning
                .as_ref()
                .unwrap()
                .contains("no side effects")
        );
    }

    #[test]
    fn test_execute_nonexistent_tool() {
        let registry = ToolRegistry::new();

        let args = serde_json::json!({});
        let result = registry.execute("nonexistent", "call_123".to_string(), &args);
        assert!(result.is_err());
        assert!(matches!(result, Err(thunderus_core::Error::Tool(_))));
    }

    #[test]
    fn test_readonly_tools_bypass_approval_in_readonly_mode() {
        let mut registry = ToolRegistry::new();
        let gate = ApprovalGate::new(ApprovalMode::ReadOnly, false);
        registry.set_approval_gate(gate);
        registry.register(GrepTool).unwrap();
        registry.register(GlobTool).unwrap();
        registry.register(ReadTool).unwrap();

        let grep_args = serde_json::json!({"pattern": "test"});
        let result = registry.execute("grep", "call_1".to_string(), &grep_args);
        assert!(!matches!(result, Err(thunderus_core::Error::Approval(_))));

        let glob_args = serde_json::json!({"pattern": "*.rs"});
        let result = registry.execute("glob", "call_2".to_string(), &glob_args);
        assert!(!matches!(result, Err(thunderus_core::Error::Approval(_))));
    }

    #[test]
    fn test_edit_tools_rejected_in_readonly_mode() {
        let mut registry = ToolRegistry::new();
        let gate = ApprovalGate::new(ApprovalMode::ReadOnly, false);
        registry.set_approval_gate(gate);
        registry.register(EditTool).unwrap();

        let edit_args = serde_json::json!({
            "file_path": "/tmp/test.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let result = registry.execute("edit", "call_1".to_string(), &edit_args);
        assert!(matches!(result, Err(thunderus_core::Error::Approval(_))));
    }

    #[test]
    fn test_edit_tools_allowed_in_full_access_mode() {
        let mut registry = ToolRegistry::new();
        let gate = ApprovalGate::new(ApprovalMode::FullAccess, false);
        registry.set_approval_gate(gate);
        registry.register(EditTool).unwrap();

        let edit_args = serde_json::json!({
            "file_path": "/tmp/test.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let result = registry.execute("edit", "call_1".to_string(), &edit_args);
        assert!(!matches!(result, Err(thunderus_core::Error::Approval(_))));
    }

    #[test]
    fn test_workspace_boundary_checking() {
        let mut registry = ToolRegistry::new();
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        registry.set_approval_gate(gate);
        registry.set_workspace_roots(vec![PathBuf::from("/workspace")]);
        registry.register(EditTool).unwrap();

        let workspace_args = serde_json::json!({
            "file_path": "/workspace/src/main.rs",
            "old_string": "old",
            "new_string": "new"
        });
        let result = registry.execute("edit", "call_1".to_string(), &workspace_args);
        assert!(!matches!(result, Err(thunderus_core::Error::Approval(_))));

        let external_args = serde_json::json!({
            "file_path": "/etc/passwd",
            "old_string": "old",
            "new_string": "new"
        });
        let result = registry.execute("edit", "call_2".to_string(), &external_args);
        assert!(matches!(result, Err(thunderus_core::Error::Approval(_))));
        if let Err(thunderus_core::Error::Approval(msg)) = result {
            assert!(msg.contains("outside workspace"));
        }
    }

    #[test]
    fn test_no_approval_gate_allows_all() {
        let registry = ToolRegistry::new();
        registry.register(EditTool).unwrap();

        let edit_args = serde_json::json!({
            "file_path": "/any/path/test.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let result = registry.execute("edit", "call_1".to_string(), &edit_args);
        assert!(!matches!(result, Err(thunderus_core::Error::Approval(_))));
    }

    #[test]
    fn test_is_within_workspace() {
        let registry = ToolRegistry::new();
        assert!(registry.is_within_workspace(Path::new("/any/path")));

        let mut registry = ToolRegistry::new();
        registry.set_workspace_roots(vec![PathBuf::from("/workspace"), PathBuf::from("/data")]);
        assert!(registry.is_within_workspace(Path::new("/workspace/src/main.rs")));
        assert!(registry.is_within_workspace(Path::new("/data/file.txt")));
        assert!(!registry.is_within_workspace(Path::new("/etc/passwd")));
        assert!(!registry.is_within_workspace(Path::new("/tmp/file.txt")));
    }

    #[test]
    fn test_registry_with_approval() {
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let roots = vec![PathBuf::from("/workspace")];
        let registry = ToolRegistry::with_approval(gate, roots);

        assert!(registry.approval_gate().is_some());
        assert_eq!(registry.workspace_roots(), &[PathBuf::from("/workspace")]);
    }

    #[test]
    fn test_setters_getters() {
        let mut registry = ToolRegistry::new();

        let gate = ApprovalGate::new(ApprovalMode::FullAccess, true);
        registry.set_approval_gate(gate);
        assert!(registry.approval_gate().is_some());
        assert_eq!(registry.approval_gate().unwrap().mode(), ApprovalMode::FullAccess);
        assert!(registry.approval_gate().unwrap().allow_network());

        let roots = vec![PathBuf::from("/workspace"), PathBuf::from("/data")];
        registry.set_workspace_roots(roots.clone());
        assert_eq!(registry.workspace_roots(), &roots[..]);
    }

    #[test]
    fn test_all_builtin_tools_registered() {
        let registry = ToolRegistry::with_builtin_tools();
        let tools = registry.list();

        assert!(tools.contains(&"noop".to_string()));
        assert!(tools.contains(&"echo".to_string()));
        assert!(tools.contains(&"grep".to_string()));
        assert!(tools.contains(&"glob".to_string()));
        assert!(tools.contains(&"read".to_string()));
        assert!(tools.contains(&"shell".to_string()));
        assert!(tools.contains(&"patch".to_string()));
        assert!(tools.contains(&"write".to_string()));
        assert!(tools.contains(&"edit".to_string()));
        assert!(tools.contains(&"multiedit".to_string()));
        assert_eq!(tools.len(), 10);
    }
}
