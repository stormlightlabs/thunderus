pub mod backup;
pub mod builtin;
pub mod classification;
pub mod dispatcher;
pub mod read_history;
pub mod registry;
pub mod result_formatting;
pub mod session_dispatcher;
pub mod teaching_errors;
pub mod tool;

pub use backup::{BackupManager, BackupMetadata, BackupMode, command_requires_backup};
pub use builtin::{
    EchoTool, EditTool, GlobTool, GrepTool, MultiEditOperation, MultiEditTool, NoopTool, ReadTool, ShellTool,
};
pub use classification::{CommandClassifier, Pattern};
pub use dispatcher::ToolDispatcher;
pub use read_history::{ReadHistory, validate_read_before_edit};
pub use registry::ToolRegistry;
pub use result_formatting::{
    EditFormatter, FormattedResult, GlobFormatter, GrepFormatter, MultiEditFormatter, ReadFormatter,
};
pub use session_dispatcher::{SessionToolDispatcher, validate_read_before_edit as validate_session_read_before_edit};
pub use teaching_errors::{
    EditErrors, ErrorCategory, GlobErrors, GrepErrors, MultiEditErrors, ReadErrors, TeachingError,
};
pub use thunderus_core::ToolRisk;
pub use tool::Tool;

#[cfg(test)]
pub use builtin::{echo_tool_call, glob_tool_call, grep_tool_call, noop_tool_call, read_tool_call, shell_tool_call};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_framework_integration() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count(), 0);

        registry.register(NoopTool).unwrap();
        registry.register(EchoTool).unwrap();
        assert_eq!(registry.count(), 2);

        let specs = registry.specs();
        assert_eq!(specs.len(), 2);
        let spec_names: Vec<_> = specs.iter().map(|s| s.name()).collect();
        assert!(spec_names.contains(&"noop"));
        assert!(spec_names.contains(&"echo"));

        let dispatcher = ToolDispatcher::new(registry);

        let tool_call = builtin::noop_tool_call("call_test");
        let result = dispatcher.execute(&tool_call);
        assert!(result.is_ok());
        assert!(result.unwrap().is_success());

        let echo_call = builtin::echo_tool_call("echo_test", "Integration test");
        let result = dispatcher.execute(&echo_call);
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.content, "Integration test");
    }

    #[test]
    fn test_tool_properties() {
        let noop = NoopTool;
        assert_eq!(noop.name(), "noop");
        assert_eq!(noop.spec().name(), "noop");
        assert!(noop.risk_level().is_safe());

        let echo = EchoTool;
        assert_eq!(echo.name(), "echo");
        assert_eq!(echo.spec().name(), "echo");
        assert!(echo.risk_level().is_safe());
    }

    #[test]
    fn test_batch_execution() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();
        registry.register(EchoTool).unwrap();

        let dispatcher = ToolDispatcher::new(registry);

        let calls = vec![
            builtin::noop_tool_call("call_1"),
            builtin::echo_tool_call("call_2", "First"),
            builtin::noop_tool_call("call_3"),
            builtin::echo_tool_call("call_4", "Second"),
        ];

        let results = dispatcher.execute_batch(&calls).unwrap();
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|r| r.is_success()));
        assert_eq!(results[1].content, "First");
        assert_eq!(results[3].content, "Second");
    }
}
