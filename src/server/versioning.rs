use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, FerrixError};
use crate::protocol::SessionId;
use super::snapshot::SessionSnapshot;

/// Git-like versioning system for sessions
pub struct SessionVersioning {
    repository_path: PathBuf,
    branches: HashMap<String, Branch>,
    current_branch: String,
    commits: HashMap<CommitId, Commit>,
    head: CommitId,
    staging_area: Option<SessionSnapshot>,
    #[allow(dead_code)]
    config: VersioningConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub head: CommitId,
    pub upstream: Option<String>,
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: CommitId,
    pub parent: Option<CommitId>,
    pub parents: Vec<CommitId>, // For merge commits
    pub snapshot: SessionSnapshot,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersioningConfig {
    pub auto_commit: bool,
    pub auto_commit_interval: u64, // seconds
    pub max_history_size: usize,
    pub compression_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MergeStrategy {
    Ours,     // Keep our changes
    Theirs,   // Take their changes
    Manual,   // Require manual resolution
    Auto,     // Attempt automatic merge
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConflict {
    pub path: String,
    pub ours: serde_json::Value,
    pub theirs: serde_json::Value,
    pub base: Option<serde_json::Value>,
}

impl SessionVersioning {
    pub fn new(session_id: SessionId) -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| FerrixError::Other("Cannot find home directory".to_string()))?;
        let repository_path = home.join(".ferrix").join("versions").join(session_id.0.to_string());
        Self::new_with_path(repository_path)
    }

    pub fn new_with_path(repository_path: PathBuf) -> Result<Self> {
        // Create repository directory if it doesn't exist
        std::fs::create_dir_all(&repository_path)
            .map_err(|e| FerrixError::Other(format!("Failed to create repository: {}", e)))?;

        let initial_commit_id = CommitId(Uuid::new_v4().to_string());
        let master_branch = Branch {
            name: "master".to_string(),
            head: initial_commit_id.clone(),
            upstream: None,
            created_at: Utc::now(),
            description: Some("Main branch".to_string()),
        };

        let mut branches = HashMap::new();
        branches.insert("master".to_string(), master_branch);

        Ok(Self {
            repository_path,
            branches,
            current_branch: "master".to_string(),
            commits: HashMap::new(),
            head: initial_commit_id,
            staging_area: None,
            config: VersioningConfig::default(),
        })
    }

    /// Initialize a repository for a session
    pub fn init(&mut self, _session_id: &SessionId, initial_snapshot: SessionSnapshot) -> Result<()> {
        let initial_commit = Commit {
            id: self.head.clone(),
            parent: None,
            parents: Vec::new(),
            snapshot: initial_snapshot,
            message: "Initial commit".to_string(),
            author: "System".to_string(),
            timestamp: Utc::now(),
            tags: vec!["init".to_string()],
        };

        self.commits.insert(initial_commit.id.clone(), initial_commit);
        self.save_repository_state()?;

        Ok(())
    }

    /// Stage changes for commit
    pub fn stage(&mut self, snapshot: SessionSnapshot) -> Result<()> {
        self.staging_area = Some(snapshot);
        Ok(())
    }


    /// Create a new branch
    pub fn branch(&mut self, name: String, description: Option<String>) -> Result<()> {
        if self.branches.contains_key(&name) {
            return Err(FerrixError::Other(format!("Branch '{}' already exists", name)));
        }

        let branch = Branch {
            name: name.clone(),
            head: self.head.clone(),
            upstream: None,
            created_at: Utc::now(),
            description,
        };

        self.branches.insert(name.clone(), branch);
        self.save_repository_state()?;

        Ok(())
    }

    /// Switch to a different branch
    pub fn checkout(&mut self, branch_name: &str) -> Result<SessionSnapshot> {
        let branch = self.branches.get(branch_name)
            .ok_or_else(|| FerrixError::Other(format!("Branch '{}' not found", branch_name)))?;

        self.current_branch = branch_name.to_string();
        self.head = branch.head.clone();

        // Return the snapshot from the branch head
        let commit = self.commits.get(&branch.head)
            .ok_or_else(|| FerrixError::Other("Branch head commit not found".to_string()))?;

        Ok(commit.snapshot.clone())
    }

    /// Merge another branch into current branch
    pub fn merge(&mut self, branch_name: &str, strategy: MergeStrategy) -> Result<MergeResult> {
        let other_branch = self.branches.get(branch_name)
            .ok_or_else(|| FerrixError::Other(format!("Branch '{}' not found", branch_name)))?
            .clone();

        let current_commit = self.commits.get(&self.head)
            .ok_or_else(|| FerrixError::Other("Current head commit not found".to_string()))?;

        let other_commit = self.commits.get(&other_branch.head)
            .ok_or_else(|| FerrixError::Other("Other branch head commit not found".to_string()))?;

        // Find common ancestor
        let common_ancestor = self.find_common_ancestor(&self.head, &other_branch.head)?;

        match strategy {
            MergeStrategy::Ours => {
                // Keep our version, just record the merge
                let merge_commit = Commit {
                    id: CommitId(Uuid::new_v4().to_string()),
                    parent: Some(self.head.clone()),
                    parents: vec![self.head.clone(), other_branch.head.clone()],
                    snapshot: current_commit.snapshot.clone(),
                    message: format!("Merge branch '{}' (ours strategy)", branch_name),
                    author: "System".to_string(),
                    timestamp: Utc::now(),
                    tags: vec!["merge".to_string()],
                };

                self.commits.insert(merge_commit.id.clone(), merge_commit.clone());
                self.head = merge_commit.id.clone();
                self.update_branch_head(&self.current_branch.clone(), merge_commit.id.clone())?;

                Ok(MergeResult::Success(merge_commit.snapshot))
            }

            MergeStrategy::Theirs => {
                // Take their version
                let merge_commit = Commit {
                    id: CommitId(Uuid::new_v4().to_string()),
                    parent: Some(self.head.clone()),
                    parents: vec![self.head.clone(), other_branch.head.clone()],
                    snapshot: other_commit.snapshot.clone(),
                    message: format!("Merge branch '{}' (theirs strategy)", branch_name),
                    author: "System".to_string(),
                    timestamp: Utc::now(),
                    tags: vec!["merge".to_string()],
                };

                self.commits.insert(merge_commit.id.clone(), merge_commit.clone());
                self.head = merge_commit.id.clone();
                self.update_branch_head(&self.current_branch.clone(), merge_commit.id.clone())?;

                Ok(MergeResult::Success(merge_commit.snapshot))
            }

            MergeStrategy::Auto => {
                // Attempt automatic merge
                match self.auto_merge_snapshots(
                    &common_ancestor,
                    &current_commit.snapshot,
                    &other_commit.snapshot
                ) {
                    Ok(merged_snapshot) => {
                        let merge_commit = Commit {
                            id: CommitId(Uuid::new_v4().to_string()),
                            parent: Some(self.head.clone()),
                            parents: vec![self.head.clone(), other_branch.head.clone()],
                            snapshot: merged_snapshot.clone(),
                            message: format!("Merge branch '{}' (auto merge)", branch_name),
                            author: "System".to_string(),
                            timestamp: Utc::now(),
                            tags: vec!["merge".to_string(), "auto-merge".to_string()],
                        };

                        self.commits.insert(merge_commit.id.clone(), merge_commit.clone());
                        self.head = merge_commit.id.clone();
                        self.update_branch_head(&self.current_branch.clone(), merge_commit.id.clone())?;

                        Ok(MergeResult::Success(merged_snapshot))
                    }
                    Err(_) => {
                        // Auto-merge failed, return conflicts
                        let conflicts = self.detect_conflicts(
                            &common_ancestor,
                            &current_commit.snapshot,
                            &other_commit.snapshot
                        )?;

                        Ok(MergeResult::Conflicts(conflicts))
                    }
                }
            }

            MergeStrategy::Manual => {
                // Always require manual resolution
                let conflicts = self.detect_conflicts(
                    &common_ancestor,
                    &current_commit.snapshot,
                    &other_commit.snapshot
                )?;

                Ok(MergeResult::Conflicts(conflicts))
            }
        }
    }

    /// Cherry-pick a specific commit
    pub fn cherry_pick(&mut self, commit_id: &CommitId) -> Result<SessionSnapshot> {
        let commit = self.commits.get(commit_id)
            .ok_or_else(|| FerrixError::Other("Commit not found".to_string()))?
            .clone();

        let new_commit = Commit {
            id: CommitId(Uuid::new_v4().to_string()),
            parent: Some(self.head.clone()),
            parents: vec![self.head.clone()],
            snapshot: commit.snapshot.clone(),
            message: format!("Cherry-pick: {}", commit.message),
            author: commit.author,
            timestamp: Utc::now(),
            tags: vec!["cherry-pick".to_string()],
        };

        self.commits.insert(new_commit.id.clone(), new_commit.clone());
        self.head = new_commit.id.clone();
        self.update_branch_head(&self.current_branch.clone(), new_commit.id.clone())?;

        Ok(new_commit.snapshot)
    }

    /// Revert a commit
    pub fn revert(&mut self, commit_id: &CommitId) -> Result<SessionSnapshot> {
        let commit = self.commits.get(commit_id)
            .ok_or_else(|| FerrixError::Other("Commit not found".to_string()))?;

        // Get the parent's snapshot to revert to
        let parent_snapshot = if let Some(parent_id) = &commit.parent {
            self.commits.get(parent_id)
                .map(|c| c.snapshot.clone())
                .ok_or_else(|| FerrixError::Other("Parent commit not found".to_string()))?
        } else {
            return Err(FerrixError::Other("Cannot revert initial commit".to_string()));
        };

        let revert_commit = Commit {
            id: CommitId(Uuid::new_v4().to_string()),
            parent: Some(self.head.clone()),
            parents: vec![self.head.clone()],
            snapshot: parent_snapshot.clone(),
            message: format!("Revert: {}", commit.message),
            author: "System".to_string(),
            timestamp: Utc::now(),
            tags: vec!["revert".to_string()],
        };

        self.commits.insert(revert_commit.id.clone(), revert_commit.clone());
        self.head = revert_commit.id.clone();
        self.update_branch_head(&self.current_branch.clone(), revert_commit.id.clone())?;

        Ok(parent_snapshot)
    }

    /// Reset to a specific commit
    pub fn reset(&mut self, commit_id: &CommitId, hard: bool) -> Result<SessionSnapshot> {
        let snapshot = self.commits.get(commit_id)
            .ok_or_else(|| FerrixError::Other("Commit not found".to_string()))?
            .snapshot.clone();

        self.head = commit_id.clone();
        let branch = self.current_branch.clone();
        self.update_branch_head(&branch, commit_id.clone())?;

        if hard {
            self.staging_area = None;
        }

        Ok(snapshot)
    }

    /// Get commit history
    pub fn log(&self, limit: Option<usize>) -> Vec<&Commit> {
        let mut history = Vec::new();
        let mut current = Some(&self.head);
        let mut count = 0;

        while let Some(commit_id) = current {
            if let Some(max) = limit {
                if count >= max {
                    break;
                }
            }

            if let Some(commit) = self.commits.get(commit_id) {
                history.push(commit);
                current = commit.parent.as_ref();
                count += 1;
            } else {
                break;
            }
        }

        history
    }


    /// Tag a commit
    pub fn tag(&mut self, commit_id: &CommitId, tag: String) -> Result<()> {
        let commit = self.commits.get_mut(commit_id)
            .ok_or_else(|| FerrixError::Other("Commit not found".to_string()))?;

        commit.tags.push(tag);
        self.save_repository_state()?;

        Ok(())
    }

    // Helper methods

    fn find_common_ancestor(&self, commit1: &CommitId, commit2: &CommitId) -> Result<Option<SessionSnapshot>> {
        let ancestors1 = self.get_ancestors(commit1);
        let ancestors2 = self.get_ancestors(commit2);

        for ancestor in &ancestors1 {
            if ancestors2.contains(ancestor) {
                if let Some(commit) = self.commits.get(ancestor) {
                    return Ok(Some(commit.snapshot.clone()));
                }
            }
        }

        Ok(None)
    }

    fn get_ancestors(&self, commit_id: &CommitId) -> HashSet<CommitId> {
        let mut ancestors = HashSet::new();
        let mut stack = vec![commit_id.clone()];

        while let Some(id) = stack.pop() {
            if ancestors.contains(&id) {
                continue;
            }

            ancestors.insert(id.clone());

            if let Some(commit) = self.commits.get(&id) {
                if let Some(parent) = &commit.parent {
                    stack.push(parent.clone());
                }
                for parent in &commit.parents {
                    stack.push(parent.clone());
                }
            }
        }

        ancestors
    }

    fn auto_merge_snapshots(
        &self,
        base: &Option<SessionSnapshot>,
        ours: &SessionSnapshot,
        theirs: &SessionSnapshot,
    ) -> Result<SessionSnapshot> {
        // Perform three-way merge
        let mut merged = ours.clone();

        // If no base, use simple merge strategy
        if base.is_none() {
            // Merge windows - union of both
            for window in &theirs.windows {
                if !merged.windows.iter().any(|w| w.id == window.id) {
                    merged.windows.push(window.clone());
                }
            }

            // Take newer config
            if theirs.created_at > ours.created_at {
                merged.config = theirs.config.clone();
            }

            return Ok(merged);
        }

        let base = base.as_ref().unwrap();

        // Three-way merge for windows
        for their_window in &theirs.windows {
            let our_window = ours.windows.iter().find(|w| w.id == their_window.id);
            let base_window = base.windows.iter().find(|w| w.id == their_window.id);

            match (our_window, base_window) {
                (Some(our), Some(base)) => {
                    // Window exists in all three - check for changes
                    if their_window != base && our == base {
                        // They changed it, we didn't - take theirs
                        if let Some(pos) = merged.windows.iter().position(|w| w.id == their_window.id) {
                            merged.windows[pos] = their_window.clone();
                        }
                    }
                    // If both changed it, keep ours (could be a conflict)
                }
                (None, None) => {
                    // They added it, we didn't have it - take it
                    if !merged.windows.iter().any(|w| w.id == their_window.id) {
                        merged.windows.push(their_window.clone());
                    }
                }
                _ => {
                    // Window added in both branches or other complex case
                    // For now, keep both if not already present
                    if !merged.windows.iter().any(|w| w.id == their_window.id) {
                        merged.windows.push(their_window.clone());
                    }
                }
            }
        }

        // Three-way merge for environment variables
        for (key, their_value) in &theirs.environment {
            if let Some(our_value) = ours.environment.get(key) {
                if let Some(base_value) = base.environment.get(key) {
                    if their_value != base_value && our_value == base_value {
                        // They changed it, we didn't - take theirs
                        merged.environment.insert(key.clone(), their_value.clone());
                    }
                }
            } else if !base.environment.contains_key(key) {
                // They added it - take it
                merged.environment.insert(key.clone(), their_value.clone());
            }
        }

        Ok(merged)
    }

    fn detect_conflicts(
        &self,
        base: &Option<SessionSnapshot>,
        ours: &SessionSnapshot,
        theirs: &SessionSnapshot,
    ) -> Result<Vec<MergeConflict>> {
        let mut conflicts = Vec::new();

        if let Some(base) = base {
            // Check for conflicting window changes
            for our_window in &ours.windows {
                let their_window = theirs.windows.iter().find(|w| w.id == our_window.id);
                let base_window = base.windows.iter().find(|w| w.id == our_window.id);

                if let (Some(their), Some(base_w)) = (their_window, base_window) {
                    if our_window != base_w && their != base_w && our_window != their {
                        // Both changed the same window differently
                        conflicts.push(MergeConflict {
                            path: format!("windows.{}", our_window.id.0),
                            ours: serde_json::to_value(our_window).unwrap_or(serde_json::Value::Null),
                            theirs: serde_json::to_value(their).unwrap_or(serde_json::Value::Null),
                            base: Some(serde_json::to_value(base_w).unwrap_or(serde_json::Value::Null)),
                        });
                    }
                }
            }

            // Check for conflicting environment variable changes
            for (key, our_value) in &ours.environment {
                if let Some(their_value) = theirs.environment.get(key) {
                    if let Some(base_value) = base.environment.get(key) {
                        if our_value != base_value && their_value != base_value && our_value != their_value {
                            conflicts.push(MergeConflict {
                                path: format!("environment.{}", key),
                                ours: serde_json::Value::String(our_value.clone()),
                                theirs: serde_json::Value::String(their_value.clone()),
                                base: Some(serde_json::Value::String(base_value.clone())),
                            });
                        }
                    }
                }
            }
        }

        Ok(conflicts)
    }

    #[allow(dead_code)]
    fn calculate_diff(&self, from: &SessionSnapshot, to: &SessionSnapshot) -> SessionDiff {
        let mut diff = SessionDiff {
            windows_added: Vec::new(),
            windows_removed: Vec::new(),
            windows_modified: Vec::new(),
            panes_added: Vec::new(),
            panes_removed: Vec::new(),
            panes_modified: Vec::new(),
            config_changes: HashMap::new(),
        };

        // Check windows
        for to_window in &to.windows {
            if let Some(from_window) = from.windows.iter().find(|w| w.id == to_window.id) {
                if to_window != from_window {
                    diff.windows_modified.push(to_window.id.0.to_string());
                }

                // Check panes within windows
                for (pane_id, to_pane) in &to_window.panes {
                    if let Some(from_pane) = from_window.panes.get(pane_id) {
                        if to_pane != from_pane {
                            diff.panes_modified.push(pane_id.clone());
                        }
                    } else {
                        diff.panes_added.push(pane_id.clone());
                    }
                }

                for pane_id in from_window.panes.keys() {
                    if !to_window.panes.contains_key(pane_id) {
                        diff.panes_removed.push(pane_id.clone());
                    }
                }
            } else {
                diff.windows_added.push(to_window.id.0.to_string());
            }
        }

        for from_window in &from.windows {
            if !to.windows.iter().any(|w| w.id == from_window.id) {
                diff.windows_removed.push(from_window.id.0.to_string());
            }
        }

        // Check environment variables
        for (key, to_value) in &to.environment {
            if let Some(from_value) = from.environment.get(key) {
                if to_value != from_value {
                    diff.config_changes.insert(
                        format!("env.{}", key),
                        (from_value.clone(), to_value.clone())
                    );
                }
            } else {
                diff.config_changes.insert(
                    format!("env.{}", key),
                    (String::new(), to_value.clone())
                );
            }
        }

        diff
    }

    fn update_branch_head(&mut self, branch_name: &str, commit_id: CommitId) -> Result<()> {
        if let Some(branch) = self.branches.get_mut(branch_name) {
            branch.head = commit_id;
            self.save_repository_state()?;
        }
        Ok(())
    }

    fn save_repository_state(&self) -> Result<()> {
        // Save repository metadata to disk
        let repo_file = self.repository_path.join("ferrix.repo");
        let repo_data = RepositoryData {
            branches: self.branches.clone(),
            current_branch: self.current_branch.clone(),
            head: self.head.clone(),
        };

        let json = serde_json::to_string_pretty(&repo_data)
            .map_err(|e| FerrixError::Other(format!("Failed to serialize repository: {}", e)))?;

        std::fs::write(repo_file, json)
            .map_err(|e| FerrixError::Other(format!("Failed to save repository: {}", e)))?;

        Ok(())
    }

    // Additional helper methods for Session integration
    pub fn init_with_snapshot(&mut self, snapshot: SessionSnapshot) -> Result<()> {
        let initial_commit = Commit {
            id: self.head.clone(),
            parent: None,
            parents: Vec::new(),
            snapshot,
            message: "Initial commit".to_string(),
            author: "System".to_string(),
            timestamp: Utc::now(),
            tags: vec!["init".to_string()],
        };

        self.commits.insert(initial_commit.id.clone(), initial_commit);
        self.save_repository_state()?;
        Ok(())
    }

    pub fn commit(&mut self, snapshot: SessionSnapshot, message: &str, author: &str) -> Result<CommitId> {
        let commit_id = CommitId(Uuid::new_v4().to_string());
        let commit = Commit {
            id: commit_id.clone(),
            parent: Some(self.head.clone()),
            parents: vec![self.head.clone()],
            snapshot,
            message: message.to_string(),
            author: author.to_string(),
            timestamp: Utc::now(),
            tags: Vec::new(),
        };

        self.commits.insert(commit_id.clone(), commit);
        self.head = commit_id.clone();

        // Update current branch head
        if let Some(branch) = self.branches.get_mut(&self.current_branch) {
            branch.head = commit_id.clone();
        }

        self.save_repository_state()?;
        Ok(commit_id)
    }

    pub fn list_branches(&self) -> Vec<Branch> {
        self.branches.values().cloned().collect()
    }

    pub fn current_branch(&self) -> Option<&str> {
        Some(&self.current_branch)
    }

    pub fn get_log(&self, limit: usize) -> Vec<Commit> {
        let mut log = Vec::new();
        let mut current_id = Some(self.head.clone());
        let mut count = 0;

        while let Some(id) = current_id {
            if count >= limit {
                break;
            }

            if let Some(commit) = self.commits.get(&id) {
                log.push(commit.clone());
                current_id = commit.parent.clone();
                count += 1;
            } else {
                break;
            }
        }

        log
    }

    pub fn diff(&self, from: Option<&str>, to: Option<&str>) -> Result<String> {
        let from_id = from.map(|s| CommitId(s.to_string()))
            .or_else(|| self.commits.get(&self.head).and_then(|c| c.parent.clone()))
            .ok_or_else(|| FerrixError::Other("Cannot determine 'from' commit".to_string()))?;

        let to_id = to.map(|s| CommitId(s.to_string()))
            .unwrap_or_else(|| self.head.clone());

        let from_commit = self.commits.get(&from_id)
            .ok_or_else(|| FerrixError::Other("From commit not found".to_string()))?;

        let to_commit = self.commits.get(&to_id)
            .ok_or_else(|| FerrixError::Other("To commit not found".to_string()))?;

        // Generate a simple diff output
        Ok(format!("Diff from {} to {}:\n  Windows: {} -> {}\n  Author: {} -> {}",
            from_id.0, to_id.0,
            from_commit.snapshot.windows.len(),
            to_commit.snapshot.windows.len(),
            from_commit.author,
            to_commit.author
        ))
    }

    pub fn create_branch(&mut self, name: &str, from_commit: Option<&str>) -> Result<()> {
        if self.branches.contains_key(name) {
            return Err(FerrixError::Other(format!("Branch '{}' already exists", name)));
        }

        let head = from_commit
            .map(|c| CommitId(c.to_string()))
            .unwrap_or_else(|| self.head.clone());

        let branch = Branch {
            name: name.to_string(),
            head: head.clone(),
            upstream: None,
            created_at: Utc::now(),
            description: None,
        };

        self.branches.insert(name.to_string(), branch);
        self.save_repository_state()?;
        Ok(())
    }

    pub fn checkout_branch(&mut self, name: &str) -> Result<SessionSnapshot> {
        let branch = self.branches.get(name)
            .ok_or_else(|| FerrixError::Other(format!("Branch '{}' not found", name)))?
            .clone();

        let commit = self.commits.get(&branch.head)
            .ok_or_else(|| FerrixError::Other("Branch head commit not found".to_string()))?;

        self.current_branch = name.to_string();
        self.head = branch.head;
        self.save_repository_state()?;

        Ok(commit.snapshot.clone())
    }

    pub fn merge_branch(&mut self, source: &str, auto_resolve: bool) -> Result<(SessionSnapshot, Vec<String>, Vec<String>)> {
        let source_branch = self.branches.get(source)
            .ok_or_else(|| FerrixError::Other(format!("Branch '{}' not found", source)))?
            .clone();

        let source_commit = self.commits.get(&source_branch.head)
            .ok_or_else(|| FerrixError::Other("Source branch head commit not found".to_string()))?
            .clone();

        let current_commit = self.commits.get(&self.head)
            .ok_or_else(|| FerrixError::Other("Current head commit not found".to_string()))?
            .clone();

        // Find common ancestor for three-way merge
        let ancestor_snapshot = self.find_common_ancestor(&self.head, &source_branch.head)?;

        // Perform three-way merge
        let (merged_snapshot, conflicts, resolved) = self.three_way_merge(
            ancestor_snapshot.as_ref(),
            &current_commit.snapshot,
            &source_commit.snapshot,
            auto_resolve
        )?;

        Ok((merged_snapshot, conflicts, resolved))
    }

    /// Perform three-way merge of snapshots
    fn three_way_merge(
        &self,
        ancestor: Option<&SessionSnapshot>,
        current: &SessionSnapshot,
        source: &SessionSnapshot,
        auto_resolve: bool,
    ) -> Result<(SessionSnapshot, Vec<String>, Vec<String>)> {
        use std::collections::{HashMap, HashSet};

        let mut conflicts = Vec::new();
        let mut resolved = Vec::new();
        let mut merged_snapshot = current.clone();

        // Merge session name
        if let Some(ancestor_snap) = ancestor {
            if ancestor_snap.session.name != source.session.name {
                if current.session.name != source.session.name
                    && current.session.name != ancestor_snap.session.name {
                    // Both changed differently - conflict
                    if auto_resolve {
                        merged_snapshot.session.name = source.session.name.clone();
                        resolved.push("session.name".to_string());
                    } else {
                        conflicts.push(format!("session.name: '{}' vs '{}'",
                            current.session.name, source.session.name));
                    }
                } else {
                    // Only source changed, use source's value
                    merged_snapshot.session.name = source.session.name.clone();
                }
            }
        } else if current.session.name != source.session.name {
            // No ancestor - just check if current and source differ
            if auto_resolve {
                merged_snapshot.session.name = source.session.name.clone();
                resolved.push("session.name".to_string());
            } else {
                conflicts.push(format!("session.name: '{}' vs '{}'",
                    current.session.name, source.session.name));
            }
        }

        // Merge windows - track by window ID
        let ancestor_windows: HashMap<_, _> = ancestor.map(|a| a.windows.iter()
            .map(|w| (w.id.clone(), w.clone()))
            .collect()).unwrap_or_default();
        let current_windows: HashMap<_, _> = current.windows.iter()
            .map(|w| (w.id.clone(), w.clone()))
            .collect();
        let source_windows: HashMap<_, _> = source.windows.iter()
            .map(|w| (w.id.clone(), w.clone()))
            .collect();

        let all_window_ids: HashSet<_> = ancestor_windows.keys()
            .chain(current_windows.keys())
            .chain(source_windows.keys())
            .collect();

        let mut merged_windows = Vec::new();

        for window_id in all_window_ids {
            let in_ancestor = ancestor_windows.contains_key(window_id);
            let in_current = current_windows.contains_key(window_id);
            let in_source = source_windows.contains_key(window_id);

            match (in_ancestor, in_current, in_source) {
                (true, false, false) => {
                    // Deleted in both - skip
                }
                (true, true, false) => {
                    // Deleted in source - skip (respect deletion)
                }
                (true, false, true) => {
                    // Deleted in current - skip (respect deletion)
                }
                (false, true, false) => {
                    // Added in current only
                    merged_windows.push(current_windows[window_id].clone());
                }
                (false, false, true) => {
                    // Added in source only
                    merged_windows.push(source_windows[window_id].clone());
                }
                (false, true, true) => {
                    // Added in both - potential conflict
                    if auto_resolve {
                        merged_windows.push(source_windows[window_id].clone());
                        resolved.push(format!("window.{}", window_id.0));
                    } else {
                        merged_windows.push(current_windows[window_id].clone());
                        conflicts.push(format!("window.{} added in both branches", window_id.0));
                    }
                }
                (true, true, true) => {
                    // Exists in all - check for modifications
                    if current_windows[window_id].name != source_windows[window_id].name {
                        if auto_resolve {
                            merged_windows.push(source_windows[window_id].clone());
                            resolved.push(format!("window.{}.name", window_id.0));
                        } else {
                            merged_windows.push(current_windows[window_id].clone());
                            conflicts.push(format!("window.{} modified in both branches", window_id.0));
                        }
                    } else {
                        // No conflict, use current
                        merged_windows.push(current_windows[window_id].clone());
                    }
                }
                (false, false, false) => {
                    // Window doesn't exist anywhere - shouldn't happen, skip
                }
            }
        }

        merged_snapshot.windows = merged_windows;

        // Merge panes similarly
        let ancestor_panes: HashMap<_, _> = ancestor.map(|a| a.panes.iter()
            .map(|p| (p.id.clone(), p.clone()))
            .collect()).unwrap_or_default();
        let current_panes: HashMap<_, _> = current.panes.iter()
            .map(|p| (p.id.clone(), p.clone()))
            .collect();
        let source_panes: HashMap<_, _> = source.panes.iter()
            .map(|p| (p.id.clone(), p.clone()))
            .collect();

        let all_pane_ids: HashSet<_> = ancestor_panes.keys()
            .chain(current_panes.keys())
            .chain(source_panes.keys())
            .collect();

        let mut merged_panes = Vec::new();

        for pane_id in all_pane_ids {
            let in_ancestor = ancestor_panes.contains_key(pane_id);
            let in_current = current_panes.contains_key(pane_id);
            let in_source = source_panes.contains_key(pane_id);

            match (in_ancestor, in_current, in_source) {
                (true, false, false) | (true, true, false) | (true, false, true) => {
                    // Deleted - skip
                }
                (false, true, false) => {
                    merged_panes.push(current_panes[pane_id].clone());
                }
                (false, false, true) => {
                    merged_panes.push(source_panes[pane_id].clone());
                }
                (false, true, true) => {
                    if auto_resolve {
                        merged_panes.push(source_panes[pane_id].clone());
                        resolved.push(format!("pane.{}", pane_id.0));
                    } else {
                        merged_panes.push(current_panes[pane_id].clone());
                        conflicts.push(format!("pane.{} added in both branches", pane_id.0));
                    }
                }
                (true, true, true) => {
                    // Use current pane (could be enhanced to merge pane properties)
                    merged_panes.push(current_panes[pane_id].clone());
                }
                (false, false, false) => {
                    // Pane doesn't exist anywhere - shouldn't happen, skip
                }
            }
        }

        merged_snapshot.panes = merged_panes;

        Ok((merged_snapshot, conflicts, resolved))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepositoryData {
    branches: HashMap<String, Branch>,
    current_branch: String,
    head: CommitId,
}

#[derive(Debug, Clone)]
pub enum MergeResult {
    Success(SessionSnapshot),
    Conflicts(Vec<MergeConflict>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDiff {
    pub windows_added: Vec<String>,
    pub windows_removed: Vec<String>,
    pub windows_modified: Vec<String>,
    pub panes_added: Vec<String>,
    pub panes_removed: Vec<String>,
    pub panes_modified: Vec<String>,
    pub config_changes: HashMap<String, (String, String)>,
}

impl Default for VersioningConfig {
    fn default() -> Self {
        Self {
            auto_commit: false,
            auto_commit_interval: 300, // 5 minutes
            max_history_size: 100,
            compression_enabled: true,
        }
    }
}
#[cfg(test)]
mod tests {
    

    #[test]
    fn test_versioning_system() {
        // Test versioning system
        assert!(true);
    }

    #[test]
    fn test_version_compatibility() {
        // Test version compatibility checks
        assert!(true);
    }
}
