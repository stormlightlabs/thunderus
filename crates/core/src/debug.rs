use std::collections::HashSet;
use std::env;
use std::sync::OnceLock;

static DEBUG_CATEGORIES: OnceLock<HashSet<DebugCategory>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugCategory {
    Provider,
    Tools,
    Approval,
    Events,
    Ui,
    Memory,
    Store,
    All,
}

impl DebugCategory {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "provider" | "providers" => Some(DebugCategory::Provider),
            "tools" => Some(DebugCategory::Tools),
            "approval" | "approvals" => Some(DebugCategory::Approval),
            "events" | "event" => Some(DebugCategory::Events),
            "ui" => Some(DebugCategory::Ui),
            "memory" => Some(DebugCategory::Memory),
            "store" => Some(DebugCategory::Store),
            "all" => Some(DebugCategory::All),
            _ => None,
        }
    }
}

pub fn init_debug() {
    let debug_str = env::var("THUNDERUS_DEBUG").unwrap_or_default();

    if debug_str.is_empty() {
        DEBUG_CATEGORIES.get_or_init(HashSet::new);
        return;
    }

    let mut categories = HashSet::new();

    for part in debug_str.split(',') {
        if let Some(cat) = DebugCategory::parse_str(part.trim()) {
            if cat == DebugCategory::All {
                categories.insert(DebugCategory::Provider);
                categories.insert(DebugCategory::Tools);
                categories.insert(DebugCategory::Approval);
                categories.insert(DebugCategory::Events);
                categories.insert(DebugCategory::Ui);
                categories.insert(DebugCategory::Memory);
                categories.insert(DebugCategory::Store);
            } else {
                categories.insert(cat);
            }
        }
    }

    DEBUG_CATEGORIES.get_or_init(|| categories);
}

pub fn is_debug_enabled(category: DebugCategory) -> bool {
    DEBUG_CATEGORIES
        .get()
        .map(|cats| cats.contains(&category))
        .unwrap_or(false)
}

#[macro_export]
macro_rules! debug_log {
    ($category:expr, $($arg:tt)*) => {
        if $crate::debug::is_debug_enabled($category) {
            eprintln!("[DEBUG:{:?}] {}", $category, format!($($arg)*));
        }
    };
}
