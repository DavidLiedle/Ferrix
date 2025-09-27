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
    pub fn new(repository_path: PathBuf) -> Result<Self> {
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
    pub fn init(&mut self, session_id: &SessionId, initial_snapshot: SessionSnapshot) -> Result<()> {
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

    /// Commit staged changes
    pub fn commit(&mut self, message: String, author: String) -> Result<CommitId> {
        let snapshot = self.staging_area.take()
            .ok_or_else(|| FerrixError::Other("No changes staged for commit".to_string()))?;

        let commit_id = CommitId(Uuid::new_v4().to_string());
        let commit = Commit {
            id: commit_id.clone(),
            parent: Some(self.head.clone()),
            parents: vec![self.head.clone()],
            snapshot,
            message,
            author,
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

    /// Get diff between two commits
    pub fn diff(&self, from: &CommitId, to: &CommitId) -> Result<SessionDiff> {
        let from_commit = self.commits.get(from)
            .ok_or_else(|| FerrixError::Other("From commit not found".to_string()))?;

        let to_commit = self.commits.get(to)
            .ok_or_else(|| FerrixError::Other("To commit not found".to_string()))?;

        Ok(self.calculate_diff(&from_commit.snapshot, &to_commit.snapshot))
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
        _base: &Option<SessionSnapshot>,
        ours: &SessionSnapshot,
        _theirs: &SessionSnapshot,
    ) -> Result<SessionSnapshot> {
        // Simplified auto-merge: for now, just use ours
        // A real implementation would compare and merge non-conflicting changes
        Ok(ours.clone())
    }

    fn detect_conflicts(
        &self,
        _base: &Option<SessionSnapshot>,
        _ours: &SessionSnapshot,
        _theirs: &SessionSnapshot,
    ) -> Result<Vec<MergeConflict>> {
        // Simplified conflict detection
        // A real implementation would compare fields and detect actual conflicts
        Ok(Vec::new())
    }

    fn calculate_diff(&self, _from: &SessionSnapshot, _to: &SessionSnapshot) -> SessionDiff {
        // Simplified diff calculation
        SessionDiff {
            windows_added: Vec::new(),
            windows_removed: Vec::new(),
            windows_modified: Vec::new(),
            panes_added: Vec::new(),
            panes_removed: Vec::new(),
            panes_modified: Vec::new(),
            config_changes: HashMap::new(),
        }
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