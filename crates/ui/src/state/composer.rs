use std::path::PathBuf;

use crate::FuzzyFinder;

/// Composer mode for input handling
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ComposerMode {
    /// Normal text input
    #[default]
    Normal,
    /// Fuzzy file finder active
    FuzzyFinder,
}

/// Input composer state
#[derive(Debug, Clone)]
pub struct ComposerState {
    /// Composer mode
    pub composer_mode: ComposerMode,
    /// Active fuzzy finder (if any)
    pub fuzzy_finder: Option<FuzzyFinder>,
}

impl ComposerState {
    pub fn new() -> Self {
        Self { composer_mode: ComposerMode::default(), fuzzy_finder: None }
    }

    /// Enter fuzzy finder mode
    pub fn enter_fuzzy_finder(&mut self, cwd: PathBuf, original_input: String, original_cursor: usize) {
        self.composer_mode = ComposerMode::FuzzyFinder;
        let mut finder = FuzzyFinder::new(cwd, original_input, original_cursor);
        if let Ok(()) = finder.discover_files() {
            self.fuzzy_finder = Some(finder);
        }
    }

    /// Exit fuzzy finder mode
    pub fn exit_fuzzy_finder(&mut self) {
        self.composer_mode = ComposerMode::Normal;
        self.fuzzy_finder = None;
    }

    /// Check if fuzzy finder is active
    pub fn is_fuzzy_finder_active(&self) -> bool {
        matches!(self.composer_mode, ComposerMode::FuzzyFinder)
    }

    /// Get mutable reference to fuzzy finder if active
    pub fn fuzzy_finder_mut(&mut self) -> Option<&mut FuzzyFinder> {
        self.fuzzy_finder.as_mut()
    }

    /// Get reference to fuzzy finder if active
    pub fn fuzzy_finder(&self) -> Option<&FuzzyFinder> {
        self.fuzzy_finder.as_ref()
    }
}

impl Default for ComposerState {
    fn default() -> Self {
        Self::new()
    }
}
