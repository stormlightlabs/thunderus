//! Focused tests for the application state machine.

mod agent_events;
mod commands;
mod compaction;
mod context_tests;
mod helpers;
mod input;
mod input_behavior;
mod labels;
mod lifecycle;
mod movement;
mod prompts;
mod queue;
mod session_startup;
mod setup;
mod slash;

use super::*;
use crate::acp::permissions::{PendingPermission, PermissionDecision, PermissionKindView, PermissionOptionView};
use crate::config::{Config, ConfigOrigin, ConfigSource, LoadedConfigLayer};
use crate::context::{AGENTS_MD_SIZE_CAP, ContextSource, discover_workspace_root};
use crate::harness::HarnessTurn;
use crate::input::PromptInput;
use crate::renderer;
use crate::tools::AgentRunConfig;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helpers::*;
use std::io::Write;
use std::sync::mpsc;
use thndrs_agent::CancelToken;
