//! Application-owned tool policy and execution adapters.

use std::fmt;
use std::sync::Arc;

use crate::{CancelToken, ToolPermissionDecision, ToolUseRequest};

type ToolPermissionCallback<Context> =
    dyn Fn(&ToolUseRequest, &Context, &CancelToken) -> ToolPermissionDecision + Send + Sync;
type ToolExecutionCallback<Context, Output> = dyn Fn(&ToolUseRequest, &Context, &CancelToken) -> Output + Send + Sync;

/// Typed application adapter for a sensitive tool permission decision.
///
/// `Context` is owned by the application. It may contain local policy such as
/// workspace containment or a remote permission bridge; neither becomes a
/// dependency of this crate.
#[derive(Clone)]
pub struct ToolPermissionHook<Context>(Arc<ToolPermissionCallback<Context>>);

impl<Context> ToolPermissionHook<Context> {
    /// Create a permission hook from an application callback.
    pub fn new(
        callback: impl Fn(&ToolUseRequest, &Context, &CancelToken) -> ToolPermissionDecision + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(callback))
    }

    /// Ask the application whether a sensitive tool call may execute.
    pub fn decide(&self, request: &ToolUseRequest, context: &Context, cancel: &CancelToken) -> ToolPermissionDecision {
        (self.0)(request, context, cancel)
    }
}

impl<Context> fmt::Debug for ToolPermissionHook<Context> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ToolPermissionHook(..)")
    }
}

/// Typed application adapter for a tool execution override.
///
/// The output remains application-defined so local filesystem, process, and
/// transport audit details do not leak into the provider-neutral contract.
#[derive(Clone)]
pub struct ToolExecutionHook<Context, Output>(Arc<ToolExecutionCallback<Context, Output>>);

impl<Context, Output> ToolExecutionHook<Context, Output> {
    /// Create a tool execution hook from an application callback.
    pub fn new(callback: impl Fn(&ToolUseRequest, &Context, &CancelToken) -> Output + Send + Sync + 'static) -> Self {
        Self(Arc::new(callback))
    }

    /// Execute the application-owned override for one tool request.
    pub fn execute(&self, request: &ToolUseRequest, context: &Context, cancel: &CancelToken) -> Output {
        (self.0)(request, context, cancel)
    }
}

impl<Context, Output> fmt::Debug for ToolExecutionHook<Context, Output> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ToolExecutionHook(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_hook_receives_only_typed_application_context() {
        let hook = ToolPermissionHook::new(|request, prefix: &String, _| {
            if request.name.starts_with(prefix) {
                ToolPermissionDecision::Allow
            } else {
                ToolPermissionDecision::Reject
            }
        });
        let request = ToolUseRequest::new("read_file", "{}", "call_1");

        assert_eq!(
            hook.decide(&request, &"read".to_string(), &CancelToken::new()),
            ToolPermissionDecision::Allow
        );
    }

    #[test]
    fn execution_hook_preserves_application_defined_output() {
        let hook = ToolExecutionHook::new(|request, suffix: &String, _| format!("{}-{suffix}", request.name));
        let request = ToolUseRequest::new("search", "{}", "call_1");

        assert_eq!(
            hook.execute(&request, &"local".to_string(), &CancelToken::new()),
            "search-local"
        );
    }
}
