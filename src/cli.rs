use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ferrix")]
#[command(author, version, about, long_about = None)]
#[command(after_help = "For more information, see: https://github.com/davidliedle/Ferrix")]
pub struct Cli {
    #[arg(short, long, default_value = "/tmp/ferrix.sock", help = "Path to Unix socket for IPC")]
    pub socket: String,

    #[arg(short, long, help = "Enable debug logging")]
    pub debug: bool,

    #[cfg(feature = "gpu")]
    #[arg(long, help = "Use GPU-accelerated rendering (experimental)")]
    pub gpu: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Server {
        #[arg(short, long)]
        foreground: bool,

        #[arg(long, help = "Enable automatic session recovery on startup (experimental)")]
        recover: bool,

        #[arg(long, help = "Enable remote TCP/TLS access")]
        remote: bool,

        #[arg(long, default_value = "8080", help = "Port for remote connections")]
        port: u16,

        #[arg(long, help = "TLS certificate file for secure connections")]
        tls_cert: Option<String>,

        #[arg(long, help = "TLS private key file for secure connections")]
        tls_key: Option<String>,

        #[arg(long, default_value = "0.0.0.0", help = "Bind address for remote connections")]
        bind: String,
    },

    #[command(visible_alias = "n")]
    New {
        #[arg(short, long)]
        session: Option<String>,

        #[arg(short, long)]
        command: Option<String>,

        #[arg(short, long)]
        detached: bool,
    },

    #[command(visible_alias = "a")]
    Attach {
        target: Option<String>,
    },

    #[command(about = "Connect to a remote Ferrix server")]
    Connect {
        #[arg(help = "Remote server address (host:port)")]
        address: String,

        #[arg(short, long, help = "Username for authentication")]
        username: String,

        #[arg(short, long, help = "Password for authentication (will prompt if not provided)")]
        password: Option<String>,

        #[arg(long, help = "CA certificate file for TLS verification")]
        tls_ca: Option<String>,

        #[arg(long, help = "Enable TLS (auto-detected if certificates provided)")]
        tls: bool,
    },

    #[command(visible_alias = "ls")]
    List,

    #[command(visible_alias = "k")]
    Kill {
        target: String,
    },

    #[command(visible_alias = "d")]
    Detach,

    // Snapshot commands
    #[command(about = "Save a session snapshot")]
    SaveSnapshot {
        #[arg(help = "Session ID or name to snapshot")]
        session: String,

        #[arg(short, long, help = "Name for the snapshot")]
        name: Option<String>,

        #[arg(short, long, help = "Description for the snapshot")]
        description: Option<String>,
    },

    #[command(about = "Load a session from snapshot")]
    LoadSnapshot {
        #[arg(help = "Path to snapshot file")]
        path: String,
    },

    #[command(about = "Restore snapshot into existing session")]
    RestoreSnapshot {
        #[arg(help = "Session ID or name to restore into")]
        session: String,

        #[arg(help = "Path to snapshot file")]
        path: String,
    },

    #[command(about = "List available snapshots")]
    ListSnapshots,

    #[command(about = "Delete a snapshot")]
    DeleteSnapshot {
        #[arg(help = "Path to snapshot file to delete")]
        path: String,
    },

    #[command(about = "Export snapshot to compressed archive")]
    ExportSnapshot {
        #[arg(help = "Path to snapshot file")]
        snapshot: String,

        #[arg(help = "Path for exported archive")]
        output: String,
    },

    #[command(about = "Import snapshot from compressed archive")]
    ImportSnapshot {
        #[arg(help = "Path to compressed archive")]
        archive: String,
    },

    SendKeys {
        target: String,
        keys: Vec<String>,
    },

    #[command(visible_alias = "config")]
    ReloadConfig,

    #[command(about = "Generate a default configuration file")]
    GenerateConfig {
        #[arg(short, long, help = "Force overwrite existing config")]
        force: bool,

        #[arg(short, long, help = "Output path for config file")]
        output: Option<String>,
    },

    #[command(about = "Validate configuration file")]
    ValidateConfig {
        #[arg(help = "Path to config file to validate")]
        path: Option<String>,
    },

    #[command(about = "Toggle pane synchronization (broadcast input to all panes)")]
    TogglePaneSync,

    #[command(about = "Set pane synchronization state")]
    SetPaneSync {
        #[arg(help = "Enable (true) or disable (false) pane synchronization")]
        enabled: bool,
    },

    #[command(about = "Lock session (read-only mode)")]
    LockSession,

    #[command(about = "Unlock session")]
    UnlockSession,

    #[command(about = "Set session lock state")]
    SetSessionLock {
        #[arg(help = "Lock (true) or unlock (false) the session")]
        locked: bool,
    },

    #[command(about = "Toggle pane zoom (expand current pane to full window)")]
    ToggleZoom,

    #[command(about = "Manage remote users and authentication")]
    UserManagement {
        #[command(subcommand)]
        action: UserAction,
    },

    #[command(about = "Rename a window")]
    RenameWindow {
        #[arg(help = "New name for the window")]
        new_name: String,

        #[arg(help = "Window ID (if not provided, renames current window)")]
        window_id: Option<String>,
    },

    #[command(about = "Toggle activity monitoring for a pane")]
    ToggleActivityMonitoring {
        #[arg(help = "Pane ID (optional, defaults to current pane)")]
        pane_id: Option<String>,
    },

    #[command(about = "Set activity monitoring state")]
    SetActivityMonitoring {
        #[arg(help = "Pane ID (optional, defaults to current pane)")]
        pane_id: Option<String>,

        #[arg(help = "Enable or disable activity monitoring")]
        enabled: bool,
    },

    #[command(about = "List all keybindings")]
    ListKeys,

    #[command(about = "Bind a key to an action")]
    BindKey {
        #[arg(help = "Key combination (e.g., 'x' for prefix+x, 'ctrl-x' for prefix+ctrl-x)")]
        key: String,

        #[arg(help = "Action to bind (e.g., 'kill-pane', 'new-window')")]
        action: String,
    },

    #[command(about = "Unbind a key")]
    UnbindKey {
        #[arg(help = "Key combination to unbind")]
        key: String,
    },

    #[command(about = "Reset keybindings to defaults")]
    ResetKeys,

    #[command(about = "Reload keybindings from config")]
    ReloadKeys,

    #[command(about = "Export keybindings to file")]
    ExportKeys {
        #[arg(help = "Path to export keybindings to")]
        path: String,
    },

    #[command(about = "Import keybindings from file")]
    ImportKeys {
        #[arg(help = "Path to import keybindings from")]
        path: String,
    },

    #[command(about = "Enable auto-save for a session")]
    EnableAutoSave {
        #[arg(help = "Session ID or name")]
        session: Option<String>,

        #[arg(short, long, default_value = "300", help = "Auto-save interval in seconds")]
        interval: u64,
    },

    #[command(about = "Disable auto-save for a session")]
    DisableAutoSave {
        #[arg(help = "Session ID or name")]
        session: Option<String>,
    },

    #[command(about = "Get auto-save status for a session")]
    AutoSaveStatus {
        #[arg(help = "Session ID or name")]
        session: Option<String>,
    },

    // Layout management commands
    #[command(about = "Apply a preset layout to the current window")]
    ApplyLayout {
        #[arg(help = "Layout preset name (single, vsplit, hsplit, main-left, main-right, main-top, main-bottom, 3v, 3h, 2x2, ide, 3x2)")]
        preset: String,
    },

    #[command(about = "Cycle through available layouts")]
    CycleLayout {
        #[arg(short, long, help = "Cycle backwards through layouts")]
        reverse: bool,
    },

    #[command(about = "Save current layout as a template")]
    SaveLayout {
        #[arg(help = "Name for the layout template")]
        name: String,

        #[arg(short, long, help = "Description of the layout")]
        description: Option<String>,
    },

    #[command(about = "List available layouts")]
    ListLayouts,

    // Session versioning commands (Git-like)
    #[command(about = "Initialize version control for the current session")]
    InitVersioning,

    #[command(about = "Commit current session state")]
    CommitSession {
        #[arg(short, long, help = "Commit message")]
        message: String,

        #[arg(short, long, help = "Author name")]
        author: Option<String>,
    },

    #[command(about = "Create a new branch from current session state")]
    Branch {
        #[arg(help = "Name for the new branch")]
        name: Option<String>,

        #[arg(short, long, help = "List all branches")]
        list: bool,

        #[arg(short, long, help = "Delete a branch")]
        delete: Option<String>,
    },

    #[command(about = "Switch to a different branch")]
    Checkout {
        #[arg(help = "Branch name or commit hash")]
        target: String,

        #[arg(short, long, help = "Create new branch if it doesn't exist")]
        create: bool,
    },

    #[command(about = "Merge another branch into current branch")]
    Merge {
        #[arg(help = "Branch to merge")]
        branch: String,

        #[arg(long, help = "Automatically resolve conflicts")]
        auto: bool,
    },

    #[command(about = "Show session history")]
    Log {
        #[arg(short, long, default_value = "10", help = "Number of commits to show")]
        limit: usize,

        #[arg(long, help = "Show full commit details")]
        verbose: bool,
    },

    #[command(about = "Show differences between session states")]
    Diff {
        #[arg(help = "First commit/branch to compare")]
        from: Option<String>,

        #[arg(help = "Second commit/branch to compare")]
        to: Option<String>,
    },

    // Session configuration commands
    #[command(about = "Load session-specific configuration")]
    LoadSessionConfig {
        #[arg(help = "Path to session config file")]
        path: String,

        #[arg(help = "Session ID or name (defaults to current)")]
        session: Option<String>,
    },

    #[command(about = "Save current session configuration")]
    SaveSessionConfig {
        #[arg(help = "Path to save config file")]
        path: String,

        #[arg(help = "Session ID or name (defaults to current)")]
        session: Option<String>,
    },

    #[command(about = "Apply a session template")]
    ApplySessionTemplate {
        #[arg(help = "Template name (development, remote, monitoring)")]
        template: String,

        #[arg(help = "Session ID or name (defaults to current)")]
        session: Option<String>,
    },

    #[command(about = "List available session templates")]
    ListSessionTemplates,

    // Input mode commands
    #[command(about = "Set input mode (vim or emacs)")]
    SetInputMode {
        #[arg(help = "Input mode (vim, emacs, or default)")]
        mode: String,
    },

    #[command(about = "Show current input mode")]
    GetInputMode,

    #[command(about = "Enter copy mode")]
    EnterCopyMode,

    #[command(about = "Exit copy mode")]
    ExitCopyMode,

    // Plugin marketplace commands
    #[command(about = "Manage plugins")]
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    // Window and pane management commands
    #[command(about = "Create a new window")]
    NewWindow {
        #[arg(short, long, help = "Name for the new window")]
        name: Option<String>,

        #[arg(short, long, help = "Command to run in the new window")]
        command: Option<String>,
    },

    #[command(about = "Select a window")]
    SelectWindow {
        #[arg(help = "Window ID or index")]
        target: String,
    },

    #[command(about = "Kill a window")]
    KillWindow {
        #[arg(help = "Window ID or index (defaults to current)")]
        target: Option<String>,
    },

    #[command(about = "List all windows")]
    ListWindows,

    #[command(about = "Split the current pane")]
    SplitPane {
        #[arg(short = 'v', long, help = "Split vertically")]
        vertical: bool,

        #[arg(short = 'h', long, help = "Split horizontally")]
        horizontal: bool,

        #[arg(short, long, help = "Percentage of space for new pane")]
        percentage: Option<u8>,
    },

    #[command(about = "Select a pane")]
    SelectPane {
        #[arg(help = "Pane direction (up, down, left, right) or ID")]
        target: String,
    },

    #[command(about = "Kill a pane")]
    KillPane {
        #[arg(help = "Pane ID (defaults to current)")]
        target: Option<String>,
    },

    #[command(about = "Resize current pane")]
    ResizePane {
        #[arg(help = "Direction (up, down, left, right)")]
        direction: String,

        #[arg(help = "Amount to resize (in cells)", default_value = "5")]
        amount: u16,
    },

    #[command(about = "Generate shell completions")]
    Completions {
        #[arg(help = "Shell type (bash, zsh, fish, powershell, elvish)")]
        shell: String,

        #[arg(short, long, help = "Output file path (defaults to stdout)")]
        output: Option<String>,
    },

    #[command(about = "Check server health status")]
    Health {
        #[arg(short, long, help = "Show detailed component health")]
        detailed: bool,

        #[arg(long, help = "Output format (text, json)")]
        format: Option<String>,
    },

    #[command(about = "Show server metrics")]
    Metrics {
        #[arg(long, help = "Output format (text, json)")]
        format: Option<String>,

        #[arg(short, long, help = "Watch metrics in real-time (refresh every N seconds)")]
        watch: Option<u64>,
    },

    #[command(about = "Inspect session state (read-only)")]
    Inspect {
        #[arg(help = "Session ID or name to inspect")]
        session: String,

        #[arg(long, help = "Output format (text, json)")]
        format: Option<String>,

        #[arg(short, long, help = "Show detailed information")]
        verbose: bool,
    },

    #[command(about = "Export session state for offline analysis")]
    DumpState {
        #[arg(help = "Session ID or name")]
        session: String,

        #[arg(short, long, help = "Output file path (defaults to stdout)")]
        output: Option<String>,

        #[arg(long, help = "Include PTY buffer contents")]
        include_buffers: bool,
    },

    #[command(about = "Profile server performance")]
    Profile {
        #[arg(long, help = "Profile CPU usage")]
        cpu: bool,

        #[arg(long, help = "Profile heap allocations")]
        heap: bool,

        #[arg(short, long, default_value = "30", help = "Duration in seconds")]
        duration: u64,

        #[arg(short, long, help = "Output file path")]
        output: Option<String>,
    },

    #[command(about = "List crash reports")]
    Crashes {
        #[arg(long, help = "Output format (text, json)")]
        format: Option<String>,

        #[arg(short, long, help = "Maximum number of crashes to show")]
        limit: Option<usize>,
    },

    #[command(about = "Show detailed crash information")]
    CrashInfo {
        #[arg(help = "Crash ID (UUID)")]
        crash_id: String,

        #[arg(long, help = "Output format (text, json)")]
        format: Option<String>,

        #[arg(short, long, help = "Show full backtrace")]
        backtrace: bool,
    },

    #[command(about = "Analyze crash patterns")]
    CrashAnalyze {
        #[arg(long, help = "Output format (text, json)")]
        format: Option<String>,
    },

    #[command(about = "Delete crash reports")]
    CrashDelete {
        #[arg(help = "Crash ID (UUID) or 'all' to delete all crashes")]
        crash_id: String,

        #[arg(long, help = "Delete crashes older than N days")]
        older_than: Option<i64>,
    },
}

#[derive(Subcommand)]
pub enum PluginAction {
    #[command(about = "Search for plugins in the marketplace")]
    Search {
        #[arg(help = "Search query")]
        query: String,

        #[arg(short, long, help = "Filter by category")]
        category: Option<String>,
    },

    #[command(about = "Install a plugin")]
    Install {
        #[arg(help = "Plugin name or ID")]
        plugin: String,

        #[arg(short, long, help = "Plugin version (defaults to latest)")]
        version: Option<String>,
    },

    #[command(about = "Update installed plugins")]
    Update {
        #[arg(help = "Plugin name or ID (updates all if not specified)")]
        plugin: Option<String>,
    },

    #[command(about = "Uninstall a plugin")]
    Uninstall {
        #[arg(help = "Plugin name or ID")]
        plugin: String,
    },

    #[command(about = "List installed plugins")]
    List {
        #[arg(short, long, help = "Show detailed information")]
        verbose: bool,
    },

    #[command(about = "Show plugin information")]
    Info {
        #[arg(help = "Plugin name or ID")]
        plugin: String,
    },

    #[command(about = "Enable a plugin")]
    Enable {
        #[arg(help = "Plugin name or ID")]
        plugin: String,
    },

    #[command(about = "Disable a plugin")]
    Disable {
        #[arg(help = "Plugin name or ID")]
        plugin: String,
    },

    #[command(about = "Reload plugin configuration")]
    Reload,
}

#[derive(Subcommand)]
pub enum UserAction {
    #[command(about = "Add a new remote user")]
    Add {
        #[arg(help = "Username")]
        username: String,

        #[arg(short, long, help = "Password (will prompt if not provided)")]
        password: Option<String>,
    },

    #[command(about = "Remove a remote user")]
    Remove {
        #[arg(help = "Username to remove")]
        username: String,
    },

    #[command(about = "List all remote users")]
    List,

    #[command(about = "Change user password")]
    ChangePassword {
        #[arg(help = "Username")]
        username: String,

        #[arg(short, long, help = "New password (will prompt if not provided)")]
        password: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_parsing_server_foreground() {
        let args = vec!["ferrix", "server", "--foreground"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::Server { foreground, .. }) => {
                assert!(foreground);
            }
            _ => panic!("Expected Server command"),
        }
    }

    #[test]
    fn test_cli_parsing_new_session() {
        let args = vec!["ferrix", "new", "--session", "test-session"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::New { session, .. }) => {
                assert_eq!(session, Some("test-session".to_string()));
            }
            _ => panic!("Expected New command"),
        }
    }

    #[test]
    fn test_cli_parsing_attach() {
        let args = vec!["ferrix", "attach", "existing-session"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::Attach { target }) => {
                assert_eq!(target, Some("existing-session".to_string()));
            }
            _ => panic!("Expected Attach command"),
        }
    }

    #[test]
    fn test_cli_parsing_list() {
        let args = vec!["ferrix", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::List) => {
                // Successfully parsed list command
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn test_cli_parsing_save_snapshot() {
        let args = vec!["ferrix", "save-snapshot", "session1", "--name", "backup1"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::SaveSnapshot { session, name, .. }) => {
                assert_eq!(session, "session1");
                assert_eq!(name, Some("backup1".to_string()));
            }
            _ => panic!("Expected SaveSnapshot command"),
        }
    }

    #[test]
    fn test_cli_default_socket() {
        let args = vec!["ferrix", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.socket, "/tmp/ferrix.sock");
    }

    #[test]
    fn test_cli_custom_socket() {
        let args = vec!["ferrix", "--socket", "/custom/path.sock", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.socket, "/custom/path.sock");
    }

    #[test]
    fn test_cli_debug_flag() {
        let args = vec!["ferrix", "--debug", "list"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(cli.debug);
    }

    #[test]
    fn test_cli_user_management_add() {
        let args = vec!["ferrix", "user-management", "add", "username"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::UserManagement { action }) => {
                match action {
                    UserAction::Add { username, .. } => {
                        assert_eq!(username, "username");
                    }
                    _ => panic!("Expected Add action"),
                }
            }
            _ => panic!("Expected UserManagement command"),
        }
    }

    #[test]
    fn test_cli_rename_window() {
        let args = vec!["ferrix", "rename-window", "new-window-name"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::RenameWindow { new_name, window_id }) => {
                assert_eq!(window_id, None);
                assert_eq!(new_name, "new-window-name");
            }
            _ => panic!("Expected RenameWindow command"),
        }
    }

    #[test]
    fn test_cli_rename_window_with_id() {
        let args = vec!["ferrix", "rename-window", "new-window-name", "550e8400-e29b-41d4-a716-446655440000"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Some(Commands::RenameWindow { new_name, window_id }) => {
                assert_eq!(window_id, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));
                assert_eq!(new_name, "new-window-name");
            }
            _ => panic!("Expected RenameWindow command"),
        }
    }
}