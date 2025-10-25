//! Integration Test Helpers
//!
//! Common utilities for integration testing Ferrix server and client functionality.

pub mod server;
pub mod client;
pub mod assertions;

pub use server::TestServer;
pub use client::TestClient;
pub use assertions::*;

use std::path::PathBuf;
use tempfile::TempDir;

/// Test fixture providing isolated environment for each test
pub struct TestFixture {
    pub temp_dir: TempDir,
    pub socket_path: PathBuf,
}

impl TestFixture {
    /// Create a new test fixture with isolated temp directory and socket path
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let socket_path = temp_dir.path().join("ferrix.sock");

        Self {
            temp_dir,
            socket_path,
        }
    }

    /// Get the socket path for this fixture
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Get a path within the temp directory
    pub fn path(&self, relative: &str) -> PathBuf {
        self.temp_dir.path().join(relative)
    }
}

impl Default for TestFixture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_creation() {
        let fixture = TestFixture::new();
        assert!(fixture.temp_dir.path().exists());
        assert!(fixture.socket_path.starts_with(fixture.temp_dir.path()));
    }
}
