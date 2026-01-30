//! Crash Pattern Analysis
//!
//! Analyzes crash reports to detect recurring issues and patterns.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::error::Result;
use super::storage::{CrashStorage, CrashReport};

/// A detected crash pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashPattern {
    /// Pattern identifier
    pub id: String,

    /// Pattern type
    pub pattern_type: PatternType,

    /// Number of occurrences
    pub occurrence_count: usize,

    /// First occurrence timestamp
    pub first_seen: chrono::DateTime<chrono::Utc>,

    /// Last occurrence timestamp
    pub last_seen: chrono::DateTime<chrono::Utc>,

    /// Crash IDs that match this pattern
    pub crash_ids: Vec<uuid::Uuid>,

    /// Pattern description
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatternType {
    /// Same file and line number
    SameLocation,

    /// Same panic message
    SameMessage,

    /// Similar backtrace
    SimilarBacktrace,

    /// Memory-related crashes
    MemoryRelated,

    /// PTY-related crashes
    PtyRelated,

    /// Protocol-related crashes
    ProtocolRelated,
}

/// Crash analyzer
pub struct CrashAnalyzer {
    storage: CrashStorage,
}

impl CrashAnalyzer {
    /// Create a new crash analyzer
    pub fn new() -> Result<Self> {
        Ok(Self {
            storage: CrashStorage::new()?,
        })
    }

    /// Analyze all crash reports and detect patterns
    pub fn analyze(&self) -> Result<Vec<CrashPattern>> {
        let crashes = self.storage.list_crashes()?;

        let mut patterns = Vec::new();

        // Group by location
        patterns.extend(self.analyze_by_location(&crashes));

        // Group by message
        patterns.extend(self.analyze_by_message(&crashes));

        // Detect memory-related patterns
        patterns.extend(self.analyze_memory_related(&crashes));

        // Detect PTY-related patterns
        patterns.extend(self.analyze_pty_related(&crashes));

        // Detect protocol-related patterns
        patterns.extend(self.analyze_protocol_related(&crashes));

        // Sort patterns by occurrence count (descending)
        patterns.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

        Ok(patterns)
    }

    /// Analyze crashes grouped by file location
    fn analyze_by_location(&self, crashes: &[CrashReport]) -> Vec<CrashPattern> {
        let mut location_map: HashMap<String, Vec<&CrashReport>> = HashMap::new();

        for crash in crashes {
            if let Some(ref location) = crash.metadata.location {
                let key = format!("{}:{}", location.file, location.line);
                location_map.entry(key).or_default().push(crash);
            }
        }

        location_map
            .into_iter()
            .filter(|(_, reports)| reports.len() > 1) // Only patterns with multiple occurrences
            .map(|(location, reports)| {
                let first_seen = reports.iter().map(|r| r.metadata.timestamp).min().expect("filtered for len > 1");
                let last_seen = reports.iter().map(|r| r.metadata.timestamp).max().expect("filtered for len > 1");

                CrashPattern {
                    id: format!("location-{}", md5::compute(location.as_bytes()).0.iter().take(8).map(|b| format!("{:02x}", b)).collect::<String>()),
                    pattern_type: PatternType::SameLocation,
                    occurrence_count: reports.len(),
                    first_seen,
                    last_seen,
                    crash_ids: reports.iter().map(|r| r.metadata.id).collect(),
                    description: format!("Recurring crash at {}", location),
                }
            })
            .collect()
    }

    /// Analyze crashes grouped by panic message
    fn analyze_by_message(&self, crashes: &[CrashReport]) -> Vec<CrashPattern> {
        let mut message_map: HashMap<String, Vec<&CrashReport>> = HashMap::new();

        for crash in crashes {
            let key = crash.metadata.message.clone();
            message_map.entry(key).or_default().push(crash);
        }

        message_map
            .into_iter()
            .filter(|(_, reports)| reports.len() > 1)
            .map(|(message, reports)| {
                let first_seen = reports.iter().map(|r| r.metadata.timestamp).min().expect("filtered for len > 1");
                let last_seen = reports.iter().map(|r| r.metadata.timestamp).max().expect("filtered for len > 1");

                CrashPattern {
                    id: format!("message-{}", md5::compute(message.as_bytes()).0.iter().take(8).map(|b| format!("{:02x}", b)).collect::<String>()),
                    pattern_type: PatternType::SameMessage,
                    occurrence_count: reports.len(),
                    first_seen,
                    last_seen,
                    crash_ids: reports.iter().map(|r| r.metadata.id).collect(),
                    description: format!("Recurring crash: {}", message),
                }
            })
            .collect()
    }

    /// Detect memory-related crash patterns
    fn analyze_memory_related(&self, crashes: &[CrashReport]) -> Vec<CrashPattern> {
        let memory_keywords = ["out of memory", "allocation", "malloc", "heap", "memory"];

        let memory_crashes: Vec<&CrashReport> = crashes
            .iter()
            .filter(|crash| {
                memory_keywords.iter().any(|keyword| {
                    crash.metadata.message.to_lowercase().contains(keyword) ||
                    crash.metadata.backtrace.as_ref()
                        .map(|bt| bt.to_lowercase().contains(keyword))
                        .unwrap_or(false)
                })
            })
            .collect();

        if memory_crashes.len() > 1 {
            let first_seen = memory_crashes.iter().map(|r| r.metadata.timestamp).min().expect("len > 1 checked");
            let last_seen = memory_crashes.iter().map(|r| r.metadata.timestamp).max().expect("len > 1 checked");

            vec![CrashPattern {
                id: "memory-related".to_string(),
                pattern_type: PatternType::MemoryRelated,
                occurrence_count: memory_crashes.len(),
                first_seen,
                last_seen,
                crash_ids: memory_crashes.iter().map(|r| r.metadata.id).collect(),
                description: "Memory-related crashes detected".to_string(),
            }]
        } else {
            vec![]
        }
    }

    /// Detect PTY-related crash patterns
    fn analyze_pty_related(&self, crashes: &[CrashReport]) -> Vec<CrashPattern> {
        let pty_keywords = ["pty", "spawn", "pseudo", "terminal"];

        let pty_crashes: Vec<&CrashReport> = crashes
            .iter()
            .filter(|crash| {
                pty_keywords.iter().any(|keyword| {
                    crash.metadata.message.to_lowercase().contains(keyword) ||
                    crash.metadata.location.as_ref()
                        .map(|loc| loc.file.to_lowercase().contains(keyword))
                        .unwrap_or(false)
                })
            })
            .collect();

        if pty_crashes.len() > 1 {
            let first_seen = pty_crashes.iter().map(|r| r.metadata.timestamp).min().expect("len > 1 checked");
            let last_seen = pty_crashes.iter().map(|r| r.metadata.timestamp).max().expect("len > 1 checked");

            vec![CrashPattern {
                id: "pty-related".to_string(),
                pattern_type: PatternType::PtyRelated,
                occurrence_count: pty_crashes.len(),
                first_seen,
                last_seen,
                crash_ids: pty_crashes.iter().map(|r| r.metadata.id).collect(),
                description: "PTY-related crashes detected".to_string(),
            }]
        } else {
            vec![]
        }
    }

    /// Detect protocol-related crash patterns
    fn analyze_protocol_related(&self, crashes: &[CrashReport]) -> Vec<CrashPattern> {
        let protocol_keywords = ["protocol", "codec", "message", "serializ"];

        let protocol_crashes: Vec<&CrashReport> = crashes
            .iter()
            .filter(|crash| {
                protocol_keywords.iter().any(|keyword| {
                    crash.metadata.message.to_lowercase().contains(keyword) ||
                    crash.metadata.location.as_ref()
                        .map(|loc| loc.file.to_lowercase().contains(keyword))
                        .unwrap_or(false)
                })
            })
            .collect();

        if protocol_crashes.len() > 1 {
            let first_seen = protocol_crashes.iter().map(|r| r.metadata.timestamp).min().expect("len > 1 checked");
            let last_seen = protocol_crashes.iter().map(|r| r.metadata.timestamp).max().expect("len > 1 checked");

            vec![CrashPattern {
                id: "protocol-related".to_string(),
                pattern_type: PatternType::ProtocolRelated,
                occurrence_count: protocol_crashes.len(),
                first_seen,
                last_seen,
                crash_ids: protocol_crashes.iter().map(|r| r.metadata.id).collect(),
                description: "Protocol-related crashes detected".to_string(),
            }]
        } else {
            vec![]
        }
    }

    /// Generate a summary report of crash patterns
    pub fn summary_report(&self) -> Result<String> {
        let crashes = self.storage.list_crashes()?;
        let patterns = self.analyze()?;

        let mut report = String::new();
        report.push_str("Crash Analysis Summary\n");
        report.push_str(&"=".repeat(50));
        report.push_str("\n\n");

        report.push_str(&format!("Total crashes: {}\n", crashes.len()));
        report.push_str(&format!("Detected patterns: {}\n\n", patterns.len()));

        if !patterns.is_empty() {
            report.push_str("Top Patterns:\n");
            report.push_str(&"-".repeat(50));
            report.push_str("\n\n");

            for (i, pattern) in patterns.iter().take(10).enumerate() {
                report.push_str(&format!("{}. {} ({})\n", i + 1, pattern.description, pattern.pattern_type_str()));
                report.push_str(&format!("   Occurrences: {}\n", pattern.occurrence_count));
                report.push_str(&format!("   First seen: {}\n", pattern.first_seen.format("%Y-%m-%d %H:%M:%S")));
                report.push_str(&format!("   Last seen: {}\n", pattern.last_seen.format("%Y-%m-%d %H:%M:%S")));
                report.push('\n');
            }
        }

        Ok(report)
    }
}

impl CrashPattern {
    fn pattern_type_str(&self) -> &str {
        match self.pattern_type {
            PatternType::SameLocation => "Same Location",
            PatternType::SameMessage => "Same Message",
            PatternType::SimilarBacktrace => "Similar Backtrace",
            PatternType::MemoryRelated => "Memory Related",
            PatternType::PtyRelated => "PTY Related",
            PatternType::ProtocolRelated => "Protocol Related",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash::capture::{CrashLocation, CrashMetadata, SystemInfo};
    use chrono::Utc;

    fn create_test_crash_with_location(file: &str, line: u32, message: &str) -> CrashMetadata {
        CrashMetadata {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            message: message.to_string(),
            location: Some(CrashLocation {
                file: file.to_string(),
                line,
            }),
            backtrace: Some("test backtrace".to_string()),
            system_info: SystemInfo::capture(),
            metrics: None,
            version: "0.1.0".to_string(),
        }
    }

    #[test]
    fn test_crash_analyzer_creation() {
        let analyzer = CrashAnalyzer::new();
        assert!(analyzer.is_ok());
    }

    #[test]
    fn test_analyze_by_location() {
        let analyzer = CrashAnalyzer::new().unwrap();

        // Create test crashes with same location
        let storage = CrashStorage::new().unwrap();
        let crash1 = create_test_crash_with_location("test.rs", 42, "Error 1");
        let crash2 = create_test_crash_with_location("test.rs", 42, "Error 2");
        let crash3 = create_test_crash_with_location("other.rs", 10, "Other error");

        storage.store_crash(&crash1).unwrap();
        storage.store_crash(&crash2).unwrap();
        storage.store_crash(&crash3).unwrap();

        let crashes = storage.list_crashes().unwrap();
        let patterns = analyzer.analyze_by_location(&crashes);

        // Should detect a pattern for test.rs:42
        assert!(!patterns.is_empty());
        assert!(patterns.iter().any(|p| p.occurrence_count >= 2));

        // Cleanup
        let _ = storage.delete_crash(crash1.id);
        let _ = storage.delete_crash(crash2.id);
        let _ = storage.delete_crash(crash3.id);
    }

    #[test]
    fn test_pattern_detection_memory() {
        let analyzer = CrashAnalyzer::new().unwrap();
        let storage = CrashStorage::new().unwrap();

        let crash1 = create_test_crash_with_location("memory.rs", 10, "out of memory error");
        let crash2 = create_test_crash_with_location("heap.rs", 20, "allocation failed");

        storage.store_crash(&crash1).unwrap();
        storage.store_crash(&crash2).unwrap();

        let crashes = storage.list_crashes().unwrap();
        let patterns = analyzer.analyze_memory_related(&crashes);

        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::MemoryRelated);

        // Cleanup
        let _ = storage.delete_crash(crash1.id);
        let _ = storage.delete_crash(crash2.id);
    }

    #[test]
    fn test_summary_report() {
        let analyzer = CrashAnalyzer::new().unwrap();
        let report = analyzer.summary_report();
        assert!(report.is_ok());

        let report_text = report.unwrap();
        assert!(report_text.contains("Crash Analysis Summary"));
        assert!(report_text.contains("Total crashes:"));
    }
}
