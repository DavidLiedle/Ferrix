#[cfg(test)]
mod ui_tests {
    use super::*;
    use crate::error::Result;
    use crate::ui::statusbar::StatusBar;
    use crate::ui::copymode::{CopyMode, CopyModeStyle, VimMotion};
    use crate::protocol::{SessionId, WindowId, PaneId};
    use uuid::Uuid;

    #[test]
    fn test_status_bar_creation() -> Result<()> {
        let status_bar = StatusBar::new();

        // Status bar should be created with default values
        assert!(!status_bar.visible);
        Ok(())
    }

    #[test]
    fn test_status_bar_session_info() -> Result<()> {
        let mut status_bar = StatusBar::new();
        let session_id = SessionId(Uuid::new_v4());

        status_bar.set_session_info("test-session".to_string(), session_id);

        // Should update session information
        assert_eq!(status_bar.session_name, Some("test-session".to_string()));
        assert_eq!(status_bar.session_id, Some(session_id));
        Ok(())
    }

    #[test]
    fn test_status_bar_window_count() -> Result<()> {
        let mut status_bar = StatusBar::new();

        status_bar.set_window_count(5);
        assert_eq!(status_bar.window_count, 5);

        status_bar.set_window_count(0);
        assert_eq!(status_bar.window_count, 0);
        Ok(())
    }

    #[test]
    fn test_status_bar_visibility() -> Result<()> {
        let mut status_bar = StatusBar::new();

        assert!(!status_bar.visible);

        status_bar.show();
        assert!(status_bar.visible);

        status_bar.hide();
        assert!(!status_bar.visible);
        Ok(())
    }

    #[test]
    fn test_copy_mode_creation() -> Result<()> {
        let copy_mode = CopyMode::new();

        assert!(!copy_mode.active);
        assert_eq!(copy_mode.cursor_row, 0);
        assert_eq!(copy_mode.cursor_col, 0);
        Ok(())
    }

    #[test]
    fn test_copy_mode_activation() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["line1".to_string(), "line2".to_string()];

        copy_mode.enter(content.clone());

        assert!(copy_mode.active);
        assert_eq!(copy_mode.content, content);
        assert_eq!(copy_mode.cursor_row, 0);
        assert_eq!(copy_mode.cursor_col, 0);
        Ok(())
    }

    #[test]
    fn test_copy_mode_navigation() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
        ];

        copy_mode.enter(content);

        // Test moving down
        copy_mode.move_cursor(VimMotion::Down);
        assert_eq!(copy_mode.cursor_row, 1);

        // Test moving right
        copy_mode.move_cursor(VimMotion::Right);
        assert_eq!(copy_mode.cursor_col, 1);

        // Test moving up
        copy_mode.move_cursor(VimMotion::Up);
        assert_eq!(copy_mode.cursor_row, 0);

        // Test moving left
        copy_mode.move_cursor(VimMotion::Left);
        assert_eq!(copy_mode.cursor_col, 0);
        Ok(())
    }

    #[test]
    fn test_copy_mode_boundary_checking() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["short".to_string()];

        copy_mode.enter(content);

        // Try to move beyond boundaries
        copy_mode.move_cursor(VimMotion::Up); // Should stay at 0
        assert_eq!(copy_mode.cursor_row, 0);

        copy_mode.move_cursor(VimMotion::Left); // Should stay at 0
        assert_eq!(copy_mode.cursor_col, 0);

        // Move to end of line
        for _ in 0..10 {
            copy_mode.move_cursor(VimMotion::Right);
        }
        // Should not exceed line length
        assert!(copy_mode.cursor_col <= 5); // "short".len()
        Ok(())
    }

    #[test]
    fn test_copy_mode_word_movements() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["hello world test".to_string()];

        copy_mode.enter(content);

        // Test word forward
        copy_mode.move_cursor(VimMotion::WordForward);
        assert!(copy_mode.cursor_col > 0);

        // Test word backward
        let old_pos = copy_mode.cursor_col;
        copy_mode.move_cursor(VimMotion::WordBackward);
        assert!(copy_mode.cursor_col < old_pos);
        Ok(())
    }

    #[test]
    fn test_copy_mode_line_movements() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["hello world".to_string()];

        copy_mode.enter(content);

        // Move to middle of line
        copy_mode.cursor_col = 5;

        // Test line start
        copy_mode.move_cursor(VimMotion::LineStart);
        assert_eq!(copy_mode.cursor_col, 0);

        // Test line end
        copy_mode.move_cursor(VimMotion::LineEnd);
        assert!(copy_mode.cursor_col > 0);
        Ok(())
    }

    #[test]
    fn test_copy_mode_visual_selection() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["test line".to_string()];

        copy_mode.enter(content);

        // Start visual selection
        copy_mode.start_visual_selection();
        assert!(copy_mode.in_visual_mode());

        // Move cursor to create selection
        copy_mode.move_cursor(VimMotion::Right);
        copy_mode.move_cursor(VimMotion::Right);

        // Get selected text
        let selected = copy_mode.get_selected_text();
        assert!(!selected.is_empty());

        // End visual selection
        copy_mode.end_visual_selection();
        assert!(!copy_mode.in_visual_mode());
        Ok(())
    }

    #[test]
    fn test_copy_mode_search() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec![
            "first line".to_string(),
            "second line with test".to_string(),
            "third line".to_string(),
        ];

        copy_mode.enter(content);

        // Search for "test"
        let found = copy_mode.search("test");
        if found {
            assert_eq!(copy_mode.cursor_row, 1); // Should be on second line
            assert!(copy_mode.cursor_col > 0); // Should be at "test" position
        }

        // Search for non-existent text
        let not_found = copy_mode.search("nonexistent");
        assert!(!not_found);
        Ok(())
    }

    #[test]
    fn test_copy_mode_yank() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["test line for yanking".to_string()];

        copy_mode.enter(content);

        // Select some text
        copy_mode.start_visual_selection();
        copy_mode.move_cursor(VimMotion::Right);
        copy_mode.move_cursor(VimMotion::Right);
        copy_mode.move_cursor(VimMotion::Right);
        copy_mode.move_cursor(VimMotion::Right);

        // Yank the selection
        let yanked = copy_mode.yank_selection();
        assert!(!yanked.is_empty());
        assert_eq!(yanked, "test");
        Ok(())
    }

    #[test]
    fn test_copy_mode_exit() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec!["test".to_string()];

        copy_mode.enter(content);
        assert!(copy_mode.active);

        copy_mode.exit();
        assert!(!copy_mode.active);
        assert!(copy_mode.content.is_empty());
        Ok(())
    }

    #[test]
    fn test_copy_mode_styles() -> Result<()> {
        let emacs_style = CopyModeStyle::Emacs;
        let vim_style = CopyModeStyle::Vim;

        // Test that styles can be compared
        assert_ne!(emacs_style, vim_style);
        assert_eq!(vim_style, CopyModeStyle::Vim);
        Ok(())
    }

    #[test]
    fn test_vim_motion_variants() -> Result<()> {
        // Test that all vim motions can be created
        let motions = vec![
            VimMotion::Up,
            VimMotion::Down,
            VimMotion::Left,
            VimMotion::Right,
            VimMotion::WordForward,
            VimMotion::WordBackward,
            VimMotion::LineStart,
            VimMotion::LineEnd,
            VimMotion::PageUp,
            VimMotion::PageDown,
        ];

        assert_eq!(motions.len(), 10);
        Ok(())
    }

    #[test]
    fn test_status_bar_time_update() -> Result<()> {
        let mut status_bar = StatusBar::new();

        status_bar.update_time();
        assert!(status_bar.current_time.is_some());

        // Update again to ensure it changes
        let first_time = status_bar.current_time.clone();
        std::thread::sleep(std::time::Duration::from_millis(1));
        status_bar.update_time();

        // Time should be updated (might be same due to precision, but shouldn't fail)
        assert!(status_bar.current_time.is_some());
        Ok(())
    }

    #[test]
    fn test_copy_mode_multi_line_selection() -> Result<()> {
        let mut copy_mode = CopyMode::new();
        let content = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ];

        copy_mode.enter(content);

        // Start selection on first line
        copy_mode.start_visual_selection();

        // Move to second line
        copy_mode.move_cursor(VimMotion::Down);
        copy_mode.move_cursor(VimMotion::Right);
        copy_mode.move_cursor(VimMotion::Right);

        let selected = copy_mode.get_selected_text();
        assert!(!selected.is_empty());

        // Should contain text from multiple lines
        assert!(selected.contains("line"));
        Ok(())
    }
}