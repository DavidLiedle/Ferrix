use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::protocol::{PaneId, SplitDirection};
use super::layout::Layout;

/// Predefined layout presets that users can quickly apply
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutPreset {
    /// Single pane (default)
    Single,

    /// Two equal vertical panes (side by side)
    EvenVertical,

    /// Two equal horizontal panes (top and bottom)
    EvenHorizontal,

    /// Main pane on left (70%), secondary on right (30%)
    MainLeft,

    /// Main pane on right (70%), secondary on left (30%)
    MainRight,

    /// Main pane on top (70%), secondary on bottom (30%)
    MainTop,

    /// Main pane on bottom (70%), secondary on top (30%)
    MainBottom,

    /// Three equal vertical panes
    TripleVertical,

    /// Three equal horizontal panes
    TripleHorizontal,

    /// Four panes in a 2x2 grid
    Grid2x2,

    /// IDE-like: Main center pane with sidebars
    IDE,

    /// Tiled: 6 equal panes in a 3x2 grid
    Tiled3x2,

    /// Custom preset with a saved layout
    Custom(String, Box<Layout>),
}

impl LayoutPreset {
    /// Convert a preset into a Layout structure
    pub fn to_layout(&self) -> Layout {
        match self {
            LayoutPreset::Single => {
                Layout::Leaf(PaneId(Uuid::new_v4()))
            }

            LayoutPreset::EvenVertical => {
                Layout::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                }
            }

            LayoutPreset::EvenHorizontal => {
                Layout::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                }
            }

            LayoutPreset::MainLeft => {
                Layout::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.7,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                }
            }

            LayoutPreset::MainRight => {
                Layout::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.3,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                }
            }

            LayoutPreset::MainTop => {
                Layout::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.7,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                }
            }

            LayoutPreset::MainBottom => {
                Layout::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.3,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                }
            }

            LayoutPreset::TripleVertical => {
                Layout::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.333,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    }),
                }
            }

            LayoutPreset::TripleHorizontal => {
                Layout::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.333,
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    second: Box::new(Layout::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.5,
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    }),
                }
            }

            LayoutPreset::Grid2x2 => {
                Layout::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(Layout::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    }),
                    second: Box::new(Layout::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                    }),
                }
            }

            LayoutPreset::IDE => {
                // Main editor in center (60%), sidebar left (20%), terminal bottom (20%)
                Layout::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.2, // Left sidebar 20%
                    first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))), // Sidebar
                    second: Box::new(Layout::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.75, // Main editor 75% of remaining
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))), // Editor
                        second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))), // Terminal
                    }),
                }
            }

            LayoutPreset::Tiled3x2 => {
                Layout::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                    first: Box::new(Layout::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.333,
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        second: Box::new(Layout::Split {
                            direction: SplitDirection::Vertical,
                            ratio: 0.5,
                            first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                            second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        }),
                    }),
                    second: Box::new(Layout::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.333,
                        first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        second: Box::new(Layout::Split {
                            direction: SplitDirection::Vertical,
                            ratio: 0.5,
                            first: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                            second: Box::new(Layout::Leaf(PaneId(Uuid::new_v4()))),
                        }),
                    }),
                }
            }

            LayoutPreset::Custom(_name, layout) => {
                *layout.clone()
            }
        }
    }

    /// Get the name of the preset
    pub fn name(&self) -> &str {
        match self {
            LayoutPreset::Single => "single",
            LayoutPreset::EvenVertical => "even-vertical",
            LayoutPreset::EvenHorizontal => "even-horizontal",
            LayoutPreset::MainLeft => "main-left",
            LayoutPreset::MainRight => "main-right",
            LayoutPreset::MainTop => "main-top",
            LayoutPreset::MainBottom => "main-bottom",
            LayoutPreset::TripleVertical => "triple-vertical",
            LayoutPreset::TripleHorizontal => "triple-horizontal",
            LayoutPreset::Grid2x2 => "grid-2x2",
            LayoutPreset::IDE => "ide",
            LayoutPreset::Tiled3x2 => "tiled-3x2",
            LayoutPreset::Custom(name, _) => name,
        }
    }

    /// Get a description of the preset
    pub fn description(&self) -> &str {
        match self {
            LayoutPreset::Single => "Single pane",
            LayoutPreset::EvenVertical => "Two equal vertical panes",
            LayoutPreset::EvenHorizontal => "Two equal horizontal panes",
            LayoutPreset::MainLeft => "Main pane on left (70%), secondary on right",
            LayoutPreset::MainRight => "Main pane on right (70%), secondary on left",
            LayoutPreset::MainTop => "Main pane on top (70%), secondary on bottom",
            LayoutPreset::MainBottom => "Main pane on bottom (70%), secondary on top",
            LayoutPreset::TripleVertical => "Three equal vertical panes",
            LayoutPreset::TripleHorizontal => "Three equal horizontal panes",
            LayoutPreset::Grid2x2 => "Four panes in a 2x2 grid",
            LayoutPreset::IDE => "IDE layout with sidebar and terminal",
            LayoutPreset::Tiled3x2 => "Six panes in a 3x2 grid",
            LayoutPreset::Custom(name, _) => name,
        }
    }

    /// Parse a preset from a string name
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "single" => Some(LayoutPreset::Single),
            "even-vertical" | "vsplit" => Some(LayoutPreset::EvenVertical),
            "even-horizontal" | "hsplit" => Some(LayoutPreset::EvenHorizontal),
            "main-left" => Some(LayoutPreset::MainLeft),
            "main-right" => Some(LayoutPreset::MainRight),
            "main-top" => Some(LayoutPreset::MainTop),
            "main-bottom" => Some(LayoutPreset::MainBottom),
            "triple-vertical" | "3v" => Some(LayoutPreset::TripleVertical),
            "triple-horizontal" | "3h" => Some(LayoutPreset::TripleHorizontal),
            "grid" | "2x2" => Some(LayoutPreset::Grid2x2),
            "ide" => Some(LayoutPreset::IDE),
            "tiled" | "3x2" => Some(LayoutPreset::Tiled3x2),
            _ => None,
        }
    }

    /// Get all available presets
    pub fn all_presets() -> Vec<LayoutPreset> {
        vec![
            LayoutPreset::Single,
            LayoutPreset::EvenVertical,
            LayoutPreset::EvenHorizontal,
            LayoutPreset::MainLeft,
            LayoutPreset::MainRight,
            LayoutPreset::MainTop,
            LayoutPreset::MainBottom,
            LayoutPreset::TripleVertical,
            LayoutPreset::TripleHorizontal,
            LayoutPreset::Grid2x2,
            LayoutPreset::IDE,
            LayoutPreset::Tiled3x2,
        ]
    }
}

/// Manager for custom layout presets
pub struct LayoutPresetsManager {
    custom_presets: Vec<LayoutPreset>,
}

impl Default for LayoutPresetsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutPresetsManager {
    pub fn new() -> Self {
        Self {
            custom_presets: Vec::new(),
        }
    }

    /// Save the current layout as a custom preset
    pub fn save_custom_preset(&mut self, name: String, layout: Layout) {
        let preset = LayoutPreset::Custom(name, Box::new(layout));
        self.custom_presets.push(preset);
    }

    /// Get a custom preset by name
    pub fn get_custom_preset(&self, name: &str) -> Option<&LayoutPreset> {
        self.custom_presets.iter()
            .find(|p| matches!(p, LayoutPreset::Custom(n, _) if n == name))
    }

    /// List all custom presets
    pub fn list_custom_presets(&self) -> Vec<String> {
        self.custom_presets.iter()
            .filter_map(|p| match p {
                LayoutPreset::Custom(name, _) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Delete a custom preset
    pub fn delete_custom_preset(&mut self, name: &str) -> bool {
        if let Some(pos) = self.custom_presets.iter().position(|p| {
            matches!(p, LayoutPreset::Custom(n, _) if n == name)
        }) {
            self.custom_presets.remove(pos);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_to_layout() {
        let preset = LayoutPreset::Single;
        let layout = preset.to_layout();
        assert!(matches!(layout, Layout::Leaf(_)));
    }

    #[test]
    fn test_even_vertical_preset() {
        let preset = LayoutPreset::EvenVertical;
        let layout = preset.to_layout();

        match layout {
            Layout::Split { direction, ratio, .. } => {
                assert!(matches!(direction, SplitDirection::Vertical));
                assert_eq!(ratio, 0.5);
            }
            _ => panic!("Expected Split layout"),
        }
    }

    #[test]
    fn test_ide_preset() {
        let preset = LayoutPreset::IDE;
        let layout = preset.to_layout();

        // Should have a sidebar and main area
        match layout {
            Layout::Split { direction, ratio, .. } => {
                assert!(matches!(direction, SplitDirection::Vertical));
                assert_eq!(ratio, 0.2); // Sidebar is 20%
            }
            _ => panic!("Expected Split layout"),
        }
    }

    #[test]
    fn test_preset_from_name() {
        assert!(matches!(LayoutPreset::from_name("single"), Some(LayoutPreset::Single)));
        assert!(matches!(LayoutPreset::from_name("vsplit"), Some(LayoutPreset::EvenVertical)));
        assert!(matches!(LayoutPreset::from_name("ide"), Some(LayoutPreset::IDE)));
        assert!(LayoutPreset::from_name("invalid").is_none());
    }

    #[test]
    fn test_preset_names() {
        assert_eq!(LayoutPreset::Single.name(), "single");
        assert_eq!(LayoutPreset::IDE.name(), "ide");
        assert_eq!(LayoutPreset::Grid2x2.name(), "grid-2x2");
    }

    #[test]
    fn test_all_presets_count() {
        let presets = LayoutPreset::all_presets();
        assert_eq!(presets.len(), 12); // We have 12 built-in presets
    }

    #[test]
    fn test_custom_preset() {
        let layout = Layout::Leaf(PaneId(Uuid::new_v4()));
        let preset = LayoutPreset::Custom("my-layout".to_string(), Box::new(layout.clone()));

        assert_eq!(preset.name(), "my-layout");
        assert_eq!(preset.description(), "my-layout");
    }

    #[test]
    fn test_presets_manager() {
        let mut manager = LayoutPresetsManager::new();
        let layout = Layout::Leaf(PaneId(Uuid::new_v4()));

        manager.save_custom_preset("test".to_string(), layout.clone());
        assert_eq!(manager.list_custom_presets(), vec!["test"]);

        assert!(manager.get_custom_preset("test").is_some());
        assert!(manager.get_custom_preset("nonexistent").is_none());

        assert!(manager.delete_custom_preset("test"));
        assert!(!manager.delete_custom_preset("test")); // Already deleted
        assert!(manager.list_custom_presets().is_empty());
    }
}