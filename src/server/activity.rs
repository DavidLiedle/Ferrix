use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::protocol::{WindowId, PaneId, SessionId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivityType {
    Output,      // New output received
    Bell,        // Terminal bell triggered
    Silence,     // No activity for a period
    Finished,    // Process completed
}

#[derive(Debug, Clone)]
pub struct ActivityEvent {
    pub session_id: SessionId,
    pub window_id: WindowId,
    pub pane_id: PaneId,
    pub activity_type: ActivityType,
    pub timestamp: Instant,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActivityMonitor {
    // Track last activity time for each pane
    last_activity: HashMap<PaneId, Instant>,

    // Track if pane has unseen activity
    unseen_activity: HashMap<PaneId, bool>,

    // Track if monitoring is enabled for each pane
    monitoring_enabled: HashMap<PaneId, bool>,

    // Silence detection threshold
    silence_threshold: Duration,

    // Track if a bell was triggered
    bell_triggered: HashMap<PaneId, bool>,
}

impl ActivityMonitor {
    pub fn new() -> Self {
        Self {
            last_activity: HashMap::new(),
            unseen_activity: HashMap::new(),
            monitoring_enabled: HashMap::new(),
            silence_threshold: Duration::from_secs(30),
            bell_triggered: HashMap::new(),
        }
    }

    pub fn enable_monitoring(&mut self, pane_id: &PaneId) {
        self.monitoring_enabled.insert(pane_id.clone(), true);
        self.last_activity.insert(pane_id.clone(), Instant::now());
    }

    pub fn disable_monitoring(&mut self, pane_id: &PaneId) {
        self.monitoring_enabled.insert(pane_id.clone(), false);
    }

    pub fn is_monitoring_enabled(&self, pane_id: &PaneId) -> bool {
        self.monitoring_enabled.get(pane_id).copied().unwrap_or(false)
    }

    pub fn record_activity(&mut self, pane_id: &PaneId, activity_type: ActivityType) {
        if !self.is_monitoring_enabled(pane_id) {
            return;
        }

        self.last_activity.insert(pane_id.clone(), Instant::now());

        match activity_type {
            ActivityType::Bell => {
                self.bell_triggered.insert(pane_id.clone(), true);
                self.unseen_activity.insert(pane_id.clone(), true);
            }
            ActivityType::Output | ActivityType::Finished => {
                self.unseen_activity.insert(pane_id.clone(), true);
            }
            _ => {}
        }
    }

    pub fn mark_as_seen(&mut self, pane_id: &PaneId) {
        self.unseen_activity.insert(pane_id.clone(), false);
        self.bell_triggered.insert(pane_id.clone(), false);
    }

    pub fn has_unseen_activity(&self, pane_id: &PaneId) -> bool {
        self.unseen_activity.get(pane_id).copied().unwrap_or(false)
    }

    pub fn has_bell(&self, pane_id: &PaneId) -> bool {
        self.bell_triggered.get(pane_id).copied().unwrap_or(false)
    }

    pub fn check_for_silence(&self, pane_id: &PaneId) -> bool {
        if !self.is_monitoring_enabled(pane_id) {
            return false;
        }

        if let Some(last_time) = self.last_activity.get(pane_id) {
            last_time.elapsed() > self.silence_threshold
        } else {
            false
        }
    }

    pub fn set_silence_threshold(&mut self, duration: Duration) {
        self.silence_threshold = duration;
    }

    pub fn get_activity_status(&self, pane_id: &PaneId) -> Option<String> {
        if !self.is_monitoring_enabled(pane_id) {
            return None;
        }

        if self.has_bell(pane_id) {
            return Some("🔔 BELL".to_string());
        }

        if self.has_unseen_activity(pane_id) {
            return Some("● ACTIVITY".to_string());
        }

        if self.check_for_silence(pane_id) {
            return Some("○ SILENCE".to_string());
        }

        None
    }

    pub fn cleanup_pane(&mut self, pane_id: &PaneId) {
        self.last_activity.remove(pane_id);
        self.unseen_activity.remove(pane_id);
        self.monitoring_enabled.remove(pane_id);
        self.bell_triggered.remove(pane_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_activity_monitor_creation() {
        let monitor = ActivityMonitor::new();
        assert_eq!(monitor.silence_threshold, Duration::from_secs(30));
    }

    #[test]
    fn test_enable_disable_monitoring() {
        let mut monitor = ActivityMonitor::new();
        let pane_id = PaneId(Uuid::new_v4());

        assert!(!monitor.is_monitoring_enabled(&pane_id));

        monitor.enable_monitoring(&pane_id);
        assert!(monitor.is_monitoring_enabled(&pane_id));

        monitor.disable_monitoring(&pane_id);
        assert!(!monitor.is_monitoring_enabled(&pane_id));
    }

    #[test]
    fn test_record_activity() {
        let mut monitor = ActivityMonitor::new();
        let pane_id = PaneId(Uuid::new_v4());

        monitor.enable_monitoring(&pane_id);
        monitor.record_activity(&pane_id, ActivityType::Output);

        assert!(monitor.has_unseen_activity(&pane_id));
        assert!(!monitor.has_bell(&pane_id));

        monitor.record_activity(&pane_id, ActivityType::Bell);
        assert!(monitor.has_bell(&pane_id));
    }

    #[test]
    fn test_mark_as_seen() {
        let mut monitor = ActivityMonitor::new();
        let pane_id = PaneId(Uuid::new_v4());

        monitor.enable_monitoring(&pane_id);
        monitor.record_activity(&pane_id, ActivityType::Output);
        monitor.record_activity(&pane_id, ActivityType::Bell);

        assert!(monitor.has_unseen_activity(&pane_id));
        assert!(monitor.has_bell(&pane_id));

        monitor.mark_as_seen(&pane_id);

        assert!(!monitor.has_unseen_activity(&pane_id));
        assert!(!monitor.has_bell(&pane_id));
    }

    #[test]
    fn test_activity_status() {
        let mut monitor = ActivityMonitor::new();
        let pane_id = PaneId(Uuid::new_v4());

        assert_eq!(monitor.get_activity_status(&pane_id), None);

        monitor.enable_monitoring(&pane_id);
        monitor.record_activity(&pane_id, ActivityType::Bell);
        assert_eq!(monitor.get_activity_status(&pane_id), Some("🔔 BELL".to_string()));

        monitor.mark_as_seen(&pane_id);
        monitor.record_activity(&pane_id, ActivityType::Output);
        assert_eq!(monitor.get_activity_status(&pane_id), Some("● ACTIVITY".to_string()));
    }

    #[test]
    fn test_cleanup_pane() {
        let mut monitor = ActivityMonitor::new();
        let pane_id = PaneId(Uuid::new_v4());

        monitor.enable_monitoring(&pane_id);
        monitor.record_activity(&pane_id, ActivityType::Output);

        assert!(monitor.is_monitoring_enabled(&pane_id));
        assert!(monitor.has_unseen_activity(&pane_id));

        monitor.cleanup_pane(&pane_id);

        assert!(!monitor.is_monitoring_enabled(&pane_id));
        assert!(!monitor.has_unseen_activity(&pane_id));
    }
}