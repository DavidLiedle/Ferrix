#[cfg(test)]
mod ai_tests {
    use super::*;
    use crate::error::Result;
    use crate::ai::{CommandAssistant, Suggestion, SuggestionCategory};

    #[test]
    fn test_command_assistant_creation() -> Result<()> {
        let _assistant = CommandAssistant::new();
        // CommandAssistant should be created successfully
        Ok(())
    }

    #[test]
    fn test_suggestion_creation() -> Result<()> {
        let suggestion = Suggestion {
            command: "ls -la".to_string(),
            description: "List all files with details".to_string(),
            category: SuggestionCategory::NextLogicalStep,
            confidence: 0.95,
            keyboard_shortcut: None,
        };

        assert_eq!(suggestion.command, "ls -la");
        assert_eq!(suggestion.description, "List all files with details");
        assert_eq!(suggestion.confidence, 0.95);
        Ok(())
    }

    #[test]
    fn test_suggestion_categories() -> Result<()> {
        let categories = vec![
            SuggestionCategory::NextLogicalStep,
            SuggestionCategory::ErrorFix,
            SuggestionCategory::Optimization,
            SuggestionCategory::Alternative,
            SuggestionCategory::Completion,
            SuggestionCategory::Macro,
        ];

        // All categories should be valid enum variants
        assert_eq!(categories.len(), 6);
        Ok(())
    }


    #[test]
    fn test_suggestion_confidence_validation() -> Result<()> {
        let high_confidence = Suggestion {
            command: "pwd".to_string(),
            description: "Print working directory".to_string(),
            category: SuggestionCategory::NextLogicalStep,
            confidence: 0.99,
            keyboard_shortcut: None,
        };

        let low_confidence = Suggestion {
            command: "obscure_command".to_string(),
            description: "Unknown command".to_string(),
            category: SuggestionCategory::Alternative,
            confidence: 0.1,
            keyboard_shortcut: None,
        };

        assert!(high_confidence.confidence > 0.9);
        assert!(low_confidence.confidence < 0.5);
        Ok(())
    }

    #[test]
    fn test_suggestion_filtering() -> Result<()> {
        let suggestions = vec![
            Suggestion {
                command: "ls".to_string(),
                description: "List files".to_string(),
                category: SuggestionCategory::NextLogicalStep,
                confidence: 0.9,
                keyboard_shortcut: None,
            },
            Suggestion {
                command: "ps".to_string(),
                description: "List processes".to_string(),
                category: SuggestionCategory::Alternative,
                confidence: 0.8,
                keyboard_shortcut: None,
            },
        ];

        // Test filtering by category
        let next_step_suggestions: Vec<_> = suggestions
            .iter()
            .filter(|s| matches!(s.category, SuggestionCategory::NextLogicalStep))
            .collect();

        assert_eq!(next_step_suggestions.len(), 1);
        assert_eq!(next_step_suggestions[0].command, "ls");
        Ok(())
    }
}