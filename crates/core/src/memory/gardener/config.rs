//! Gardener configuration

use serde::{Deserialize, Serialize};

/// Main gardener configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GardenerConfig {
    /// Enable automatic consolidation after session ends
    pub auto_consolidate: bool,

    /// Run hygiene checks on memory changes
    pub hygiene_on_change: bool,

    /// Run drift detection before agent actions
    pub drift_check_on_start: bool,

    /// Entity extraction configuration
    pub extraction: ExtractionConfig,

    /// Hygiene configuration
    pub hygiene: HygieneConfig,

    /// Drift detection configuration
    pub drift: DriftConfig,

    /// Recap generation configuration
    pub recap: RecapConfig,
}

impl Default for GardenerConfig {
    fn default() -> Self {
        Self {
            auto_consolidate: true,
            hygiene_on_change: true,
            drift_check_on_start: true,
            extraction: ExtractionConfig::default(),
            hygiene: HygieneConfig::default(),
            drift: DriftConfig::default(),
            recap: RecapConfig::default(),
        }
    }
}

/// Entity extraction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractionConfig {
    /// Minimum confidence for fact extraction (0.0 - 1.0)
    pub fact_confidence_threshold: f64,

    /// Keywords that signal decisions
    pub decision_keywords: Vec<String>,

    /// Minimum steps for workflow detection
    pub min_workflow_steps: usize,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            fact_confidence_threshold: 0.7,
            decision_keywords: vec![
                "decided".to_string(),
                "chose".to_string(),
                "selected".to_string(),
                "picked".to_string(),
                "went with".to_string(),
                "opted for".to_string(),
            ],
            min_workflow_steps: 3,
        }
    }
}

/// Hygiene configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HygieneConfig {
    /// Core memory soft token limit (warning)
    pub core_soft_limit: usize,

    /// Core memory hard token limit (error)
    pub core_hard_limit: usize,

    /// Individual document soft token limit
    pub doc_soft_limit: usize,

    /// Individual document hard token limit
    pub doc_hard_limit: usize,

    /// Deduplication strategy
    pub dedup_strategy: DeduplicationStrategy,

    /// Require provenance links on all durable documents
    pub require_provenance: bool,
}

impl Default for HygieneConfig {
    fn default() -> Self {
        Self {
            core_soft_limit: 4000,
            core_hard_limit: 8000,
            doc_soft_limit: 2000,
            doc_hard_limit: 4000,
            dedup_strategy: DeduplicationStrategy::MergeToFirst,
            require_provenance: true,
        }
    }
}

/// Deduplication strategy for facts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
pub enum DeduplicationStrategy {
    /// Merge duplicates into first occurrence
    MergeToFirst,
    /// Keep newest, remove older
    KeepNewest,
    /// Flag for manual review
    FlagForReview,
}

/// Drift detection configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriftConfig {
    /// Mark docs stale after this many commits without verification
    pub stale_after_commits: usize,

    /// Auto-mark verified on consolidation approval
    pub auto_verify_on_approve: bool,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self { stale_after_commits: 10, auto_verify_on_approve: true }
    }
}

/// Recap generation configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecapConfig {
    /// Include file change summary in recaps
    pub include_file_changes: bool,

    /// Maximum files to list in recap (truncate beyond)
    pub max_files_listed: usize,
}

impl Default for RecapConfig {
    fn default() -> Self {
        Self { include_file_changes: true, max_files_listed: 20 }
    }
}

/// Size limits configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SizeLimits {
    /// Soft limit (warning)
    pub soft: usize,
    /// Hard limit (error)
    pub hard: usize,
}

impl SizeLimits {
    /// Create new size limits
    pub fn new(soft: usize, hard: usize) -> Self {
        Self { soft, hard }
    }

    /// Check if a value exceeds soft limit
    pub fn exceeds_soft(&self, value: usize) -> bool {
        value > self.soft
    }

    /// Check if a value exceeds hard limit
    pub fn exceeds_hard(&self, value: usize) -> bool {
        value > self.hard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gardener_config_default() {
        let config = GardenerConfig::default();
        assert!(config.auto_consolidate);
        assert!(config.hygiene_on_change);
        assert!(config.drift_check_on_start);
    }

    #[test]
    fn test_extraction_config_default() {
        let config = ExtractionConfig::default();
        assert_eq!(config.fact_confidence_threshold, 0.7);
        assert_eq!(config.min_workflow_steps, 3);
        assert!(!config.decision_keywords.is_empty());
    }

    #[test]
    fn test_hygiene_config_default() {
        let config = HygieneConfig::default();
        assert_eq!(config.core_soft_limit, 4000);
        assert_eq!(config.core_hard_limit, 8000);
        assert_eq!(config.dedup_strategy, DeduplicationStrategy::MergeToFirst);
    }

    #[test]
    fn test_drift_config_default() {
        let config = DriftConfig::default();
        assert_eq!(config.stale_after_commits, 10);
        assert!(config.auto_verify_on_approve);
    }

    #[test]
    fn test_recap_config_default() {
        let config = RecapConfig::default();
        assert!(config.include_file_changes);
        assert_eq!(config.max_files_listed, 20);
    }

    #[test]
    fn test_size_limits() {
        let limits = SizeLimits::new(100, 200);

        assert!(!limits.exceeds_soft(90));
        assert!(limits.exceeds_soft(150));
        assert!(!limits.exceeds_hard(150));
        assert!(limits.exceeds_hard(250));
    }
}
