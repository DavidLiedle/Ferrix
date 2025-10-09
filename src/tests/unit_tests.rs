#[cfg(test)]
mod window_tests {
    use crate::server::window::Window;
    use crate::server::layout::NavigationDirection;
    use crate::protocol::{WindowId, SplitDirection};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_window_creation() {
        let window_id = WindowId(Uuid::new_v4());
        let window = Window::new(window_id.clone(), "test-window".to_string());

        assert_eq!(window.id, window_id);
        assert_eq!(window.name, "test-window");
        assert_eq!(window.panes.len(), 1);
        assert!(window.current_pane.is_some());
    }

    #[tokio::test]
    async fn test_pane_split() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test-window".to_string());

        let initial_pane = window.current_pane.clone().unwrap();

        // Split horizontally
        let new_pane_id = window.split_pane(&initial_pane, SplitDirection::Horizontal)
            .await
            .unwrap();

        assert_eq!(window.panes.len(), 2);
        assert!(window.panes.contains_key(&new_pane_id));
    }

    #[tokio::test]
    async fn test_pane_navigation() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test-window".to_string());

        let initial_pane = window.current_pane.clone().unwrap();

        // Split to create multiple panes
        let _pane2 = window.split_pane(&initial_pane, SplitDirection::Horizontal)
            .await
            .unwrap();

        // Navigate down (should switch panes)
        window.navigate_pane(NavigationDirection::Down).await.unwrap();
        assert_ne!(window.current_pane, Some(initial_pane.clone()));
    }

    #[tokio::test]
    async fn test_pane_close() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test-window".to_string());

        let initial_pane = window.current_pane.clone().unwrap();

        // Create second pane
        let pane2 = window.split_pane(&initial_pane, SplitDirection::Vertical)
            .await
            .unwrap();

        assert_eq!(window.panes.len(), 2);

        // Close second pane
        window.close_pane(&pane2).await.unwrap();
        assert_eq!(window.panes.len(), 1);
        assert!(!window.panes.contains_key(&pane2));
    }

    #[tokio::test]
    async fn test_window_resize() {
        let window_id = WindowId(Uuid::new_v4());
        let mut window = Window::new(window_id, "test-window".to_string());

        // Initial size
        assert_eq!(window.width, 80);
        assert_eq!(window.height, 24);

        // Resize
        window.resize(120, 40).await.unwrap();
        assert_eq!(window.width, 120);
        assert_eq!(window.height, 40);
    }
}

#[cfg(test)]
mod layout_tests {
    use crate::server::layout::{Layout, NavigationDirection};
    use crate::protocol::{PaneId, SplitDirection};
    use uuid::Uuid;

    #[test]
    fn test_layout_creation() {
        let pane_id = PaneId(Uuid::new_v4());
        let layout = Layout::new(pane_id.clone());

        match layout {
            Layout::Leaf(id) => assert_eq!(id, pane_id),
            _ => panic!("Expected Leaf layout"),
        }
    }

    #[test]
    fn test_layout_split() {
        let pane1 = PaneId(Uuid::new_v4());
        let pane2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane1.clone());

        // Split the layout
        let success = layout.split(&pane1, SplitDirection::Horizontal, pane2.clone());
        assert!(success);

        // Verify split structure
        match layout {
            Layout::Split { direction, first, second, .. } => {
                assert_eq!(direction, SplitDirection::Horizontal);
                assert!(matches!(first.as_ref(), Layout::Leaf(_)));
                assert!(matches!(second.as_ref(), Layout::Leaf(_)));
            }
            _ => panic!("Expected Split layout"),
        }
    }

    #[test]
    fn test_layout_navigation() {
        let pane1 = PaneId(Uuid::new_v4());
        let pane2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane1.clone());

        layout.split(&pane1, SplitDirection::Vertical, pane2.clone());

        // Navigate right from pane1 should go to pane2
        let next = layout.navigate(&pane1, NavigationDirection::Right);
        assert_eq!(next, Some(pane2.clone()));

        // Navigate left from pane2 should go to pane1
        let next = layout.navigate(&pane2, NavigationDirection::Left);
        assert_eq!(next, Some(pane1));
    }

    #[test]
    fn test_layout_dimensions() {
        let pane1 = PaneId(Uuid::new_v4());
        let pane2 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane1.clone());

        layout.split(&pane1, SplitDirection::Horizontal, pane2.clone());

        let dimensions = layout.get_dimensions(100, 50);
        assert_eq!(dimensions.len(), 2);

        // Check that panes split the space correctly
        let (_, _, _, _, h1) = dimensions.iter().find(|(id, _, _, _, _)| *id == pane1).unwrap();
        let (_, _, _, _, h2) = dimensions.iter().find(|(id, _, _, _, _)| *id == pane2).unwrap();

        assert_eq!(h1 + h2, 50); // Total height should equal container height
    }

    #[test]
    fn test_layout_remove_pane() {
        let pane1 = PaneId(Uuid::new_v4());
        let pane2 = PaneId(Uuid::new_v4());
        let pane3 = PaneId(Uuid::new_v4());
        let mut layout = Layout::new(pane1.clone());

        // Create a more complex layout
        layout.split(&pane1, SplitDirection::Horizontal, pane2.clone());
        layout.split(&pane2, SplitDirection::Vertical, pane3.clone());

        assert_eq!(layout.get_all_panes().len(), 3);

        // Remove middle pane
        let removed = layout.remove_pane(&pane2);
        assert!(removed);

        // Should have 2 panes left
        let remaining = layout.get_all_panes();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.contains(&pane1));
        assert!(remaining.contains(&pane3));
    }
}

#[cfg(test)]
mod copy_mode_tests {
    use crate::ui::copymode::{CopyMode, SearchDirection};
    use crate::config::CopyModeStyle;

    #[test]
    fn test_copy_mode_creation() {
        let copy_mode = CopyMode::new(CopyModeStyle::Vi);
        assert!(!copy_mode.is_active());
    }

    #[test]
    fn test_copy_mode_activation() {
        let mut copy_mode = CopyMode::new(CopyModeStyle::Vi);

        let buffer = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
        ];

        copy_mode.enter(buffer.clone());
        assert!(copy_mode.is_active());

        copy_mode.exit();
        assert!(!copy_mode.is_active());
    }

    #[test]
    fn test_vim_motions() {
        let mut copy_mode = CopyMode::new(CopyModeStyle::Vi);

        let buffer = vec![
            "Hello world".to_string(),
            "This is a test".to_string(),
            "Final line".to_string(),
        ];

        copy_mode.enter(buffer);

        // Test basic movements
        copy_mode.move_cursor_down();
        copy_mode.move_cursor_right();
        copy_mode.move_cursor_right();

        // Test word movement
        copy_mode.move_word_forward();
        copy_mode.move_word_backward();

        // Test line movements
        copy_mode.move_to_line_end();
        copy_mode.move_to_line_start();

        // Test page movements
        copy_mode.move_half_page_down();
        copy_mode.move_half_page_up();
    }

    #[test]
    fn test_visual_mode_selection() {
        let mut copy_mode = CopyMode::new(CopyModeStyle::Vi);

        let buffer = vec![
            "Select this text".to_string(),
            "And this line".to_string(),
        ];

        copy_mode.enter(buffer);

        // Enter visual mode
        copy_mode.enter_visual_mode();

        // Move to select text
        copy_mode.move_cursor_right();
        copy_mode.move_cursor_right();
        copy_mode.move_cursor_right();
        copy_mode.update_selection();

        // Get selected text
        let selected = copy_mode.get_selected_text();
        assert!(selected.is_some());
    }

    #[test]
    fn test_search_functionality() {
        let mut copy_mode = CopyMode::new(CopyModeStyle::Vi);

        let buffer = vec![
            "Search for this".to_string(),
            "Find this text".to_string(),
            "Another this here".to_string(),
        ];

        copy_mode.enter(buffer);

        // Start search
        copy_mode.start_search(SearchDirection::Forward);
        copy_mode.update_search("this".to_string());

        // Jump to matches
        copy_mode.jump_to_next_match();
        copy_mode.jump_to_previous_match();
    }

    #[test]
    fn test_yank_operation() {
        let mut copy_mode = CopyMode::new(CopyModeStyle::Vi);

        let buffer = vec![
            "Yank this text".to_string(),
        ];

        copy_mode.enter(buffer);

        // Select and yank
        copy_mode.enter_visual_mode();
        copy_mode.move_to_line_end();
        copy_mode.update_selection();
        copy_mode.yank_selection();

        let yanked = copy_mode.get_yanked_text();
        assert!(yanked.is_some());
    }
}

#[cfg(test)]
mod error_tests {
    use crate::error::{FerrixError, Result};

    #[test]
    fn test_error_creation() {
        let io_err = FerrixError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        assert!(format!("{}", io_err).contains("IO error"));

        let session_err = FerrixError::SessionNotFound("test-session".to_string());
        assert!(format!("{}", session_err).contains("test-session"));
    }

    #[test]
    fn test_error_conversion() {
        fn returns_result() -> Result<()> {
            Err(FerrixError::Other("test error".to_string()))
        }

        let result = returns_result();
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod protocol_tests {
    use crate::protocol::{ClientMessage, SessionId, ClientId, AuthCredentials};
    use uuid::Uuid;

    #[test]
    fn test_message_serialization() {
        let msg = ClientMessage::CreateSession {
            name: Some("test".to_string()),
            working_dir: None,
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: ClientMessage = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            ClientMessage::CreateSession { name, working_dir: _ } => {
                assert_eq!(name, Some("test".to_string()));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_auth_credentials() {
        let creds = AuthCredentials {
            username: "user".to_string(),
            password: Some("pass".to_string()),
            token: None,
            certificate: None,
        };

        let msg = ClientMessage::Authenticate(creds);
        let serialized = serde_json::to_string(&msg).unwrap();
        assert!(serialized.contains("username"));
        assert!(serialized.contains("user"));
    }

    #[test]
    fn test_id_types() {
        let session_id = SessionId(Uuid::new_v4());
        let _client_id = ClientId(Uuid::new_v4());

        // IDs should be unique
        let session_id2 = SessionId(Uuid::new_v4());
        assert_ne!(session_id, session_id2);
    }
}