use std::collections::VecDeque;
use crate::client::ansi_parser::Cell;

/// Search mode for scrollback buffer
#[derive(Debug, Clone, PartialEq)]
pub enum SearchMode {
    Forward,
    Backward,
    Regex,
    CaseSensitive,
    WholeWord,
}

/// Search result with context
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub line_index: usize,
    pub char_position: usize,
    pub line_content: String,
    pub match_length: usize,
    pub context_before: Option<String>,
    pub context_after: Option<String>,
}

/// Advanced search engine for terminal scrollback
pub struct ScrollbackSearch {
    query: String,
    mode: SearchMode,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
    results: Vec<SearchResult>,
    current_match: Option<usize>,
    history: VecDeque<String>,
    max_history: usize,
}

impl Default for ScrollbackSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollbackSearch {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchMode::Forward,
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            results: Vec::new(),
            current_match: None,
            history: VecDeque::with_capacity(50),
            max_history: 50,
        }
    }

    /// Set the search query
    pub fn set_query(&mut self, query: String) {
        if self.query != query {
            self.query = query;
            self.results.clear();
            self.current_match = None;
        }
    }

    /// Get the current search query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Toggle case sensitivity
    pub fn toggle_case_sensitive(&mut self) {
        self.case_sensitive = !self.case_sensitive;
        self.results.clear();
    }

    /// Toggle whole word matching
    pub fn toggle_whole_word(&mut self) {
        self.whole_word = !self.whole_word;
        self.results.clear();
    }

    /// Toggle regex mode
    pub fn toggle_regex(&mut self) {
        self.use_regex = !self.use_regex;
        self.results.clear();
    }

    /// Set search mode
    pub fn set_mode(&mut self, mode: SearchMode) {
        self.mode = mode;
    }

    /// Search through a vector of strings
    pub fn search_lines(&mut self, lines: &[String]) -> Vec<SearchResult> {
        self.results.clear();

        if self.query.is_empty() {
            return Vec::new();
        }

        let search_pattern = if self.use_regex {
            self.query.clone()
        } else if self.whole_word {
            format!(r"\b{}\b", regex::escape(&self.query))
        } else {
            regex::escape(&self.query)
        };

        let regex_result = if self.case_sensitive {
            regex::Regex::new(&search_pattern)
        } else {
            regex::Regex::new(&format!("(?i){}", search_pattern))
        };

        let regex = match regex_result {
            Ok(re) => re,
            Err(_) => return Vec::new(),
        };

        for (line_idx, line) in lines.iter().enumerate() {
            for mat in regex.find_iter(line) {
                let result = SearchResult {
                    line_index: line_idx,
                    char_position: mat.start(),
                    line_content: line.clone(),
                    match_length: mat.end() - mat.start(),
                    context_before: if line_idx > 0 {
                        lines.get(line_idx - 1).cloned()
                    } else {
                        None
                    },
                    context_after: lines.get(line_idx + 1).cloned(),
                };
                self.results.push(result);
            }
        }

        // Add to history if not empty
        if !self.query.is_empty() && !self.history.contains(&self.query) {
            self.history.push_back(self.query.clone());
            if self.history.len() > self.max_history {
                self.history.pop_front();
            }
        }

        self.results.clone()
    }

    /// Search through cell-based scrollback
    pub fn search_cells(&mut self, cell_lines: &[Vec<Cell>]) -> Vec<SearchResult> {
        let text_lines: Vec<String> = cell_lines.iter()
            .map(|cells| cells.iter().map(|cell| cell.ch).collect())
            .collect();

        self.search_lines(&text_lines)
    }

    /// Navigate to the next match
    pub fn next_match(&mut self) -> Option<&SearchResult> {
        if self.results.is_empty() {
            return None;
        }

        match self.mode {
            SearchMode::Forward => {
                self.current_match = Some(
                    self.current_match.map(|i| (i + 1) % self.results.len()).unwrap_or(0)
                );
            }
            SearchMode::Backward => {
                self.current_match = Some(
                    self.current_match
                        .map(|i| if i == 0 { self.results.len() - 1 } else { i - 1 })
                        .unwrap_or(self.results.len() - 1)
                );
            }
            _ => {
                self.current_match = Some(
                    self.current_match.map(|i| (i + 1) % self.results.len()).unwrap_or(0)
                );
            }
        }

        self.current_match.and_then(|i| self.results.get(i))
    }

    /// Navigate to the previous match
    pub fn previous_match(&mut self) -> Option<&SearchResult> {
        if self.results.is_empty() {
            return None;
        }

        self.current_match = Some(
            self.current_match
                .map(|i| if i == 0 { self.results.len() - 1 } else { i - 1 })
                .unwrap_or(self.results.len() - 1)
        );

        self.current_match.and_then(|i| self.results.get(i))
    }

    /// Jump to a specific match by index
    pub fn jump_to_match(&mut self, index: usize) -> Option<&SearchResult> {
        if index < self.results.len() {
            self.current_match = Some(index);
            self.results.get(index)
        } else {
            None
        }
    }

    /// Get all search results
    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    /// Get the current match
    pub fn current_result(&self) -> Option<&SearchResult> {
        self.current_match.and_then(|i| self.results.get(i))
    }

    /// Get the current match index
    pub fn current_index(&self) -> Option<usize> {
        self.current_match
    }

    /// Get total number of matches
    pub fn match_count(&self) -> usize {
        self.results.len()
    }

    /// Clear search results
    pub fn clear(&mut self) {
        self.query.clear();
        self.results.clear();
        self.current_match = None;
    }

    /// Get search history
    pub fn history(&self) -> &VecDeque<String> {
        &self.history
    }

    /// Select from history
    pub fn select_from_history(&mut self, index: usize) {
        if let Some(query) = self.history.get(index) {
            self.set_query(query.clone());
        }
    }

    /// Clear search history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Check if search is active
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// Get search status string for UI
    pub fn status_string(&self) -> String {
        if self.results.is_empty() {
            if self.query.is_empty() {
                String::from("Type to search")
            } else {
                format!("No matches for '{}'", self.query)
            }
        } else {
            let current = self.current_match.map(|i| i + 1).unwrap_or(0);
            format!(
                "{}/{} matches for '{}' {}{}{}",
                current,
                self.results.len(),
                self.query,
                if self.case_sensitive { "[Case]" } else { "" },
                if self.whole_word { "[Word]" } else { "" },
                if self.use_regex { "[Regex]" } else { "" }
            )
        }
    }

    /// Highlight matches in a line of text
    pub fn highlight_line(&self, line: &str, highlight_color: &str, reset_color: &str) -> String {
        if self.query.is_empty() {
            return line.to_string();
        }

        let search_pattern = if self.use_regex {
            self.query.clone()
        } else if self.whole_word {
            format!(r"\b{}\b", regex::escape(&self.query))
        } else {
            regex::escape(&self.query)
        };

        let regex_result = if self.case_sensitive {
            regex::Regex::new(&search_pattern)
        } else {
            regex::Regex::new(&format!("(?i){}", search_pattern))
        };

        let regex = match regex_result {
            Ok(re) => re,
            Err(_) => return line.to_string(),
        };

        let mut result = String::new();
        let mut last_end = 0;

        for mat in regex.find_iter(line) {
            result.push_str(&line[last_end..mat.start()]);
            result.push_str(highlight_color);
            result.push_str(&line[mat.start()..mat.end()]);
            result.push_str(reset_color);
            last_end = mat.end();
        }
        result.push_str(&line[last_end..]);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_basic() {
        let mut search = ScrollbackSearch::new();
        let lines = vec![
            "Hello world".to_string(),
            "This is a test".to_string(),
            "Hello again".to_string(),
        ];

        search.set_query("Hello".to_string());
        let results = search.search_lines(&lines);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line_index, 0);
        assert_eq!(results[1].line_index, 2);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut search = ScrollbackSearch::new();
        let lines = vec![
            "Hello world".to_string(),
            "HELLO world".to_string(),
            "hello world".to_string(),
        ];

        search.set_query("hello".to_string());
        let results = search.search_lines(&lines);

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_whole_word() {
        let mut search = ScrollbackSearch::new();
        search.toggle_whole_word();

        let lines = vec![
            "test testing".to_string(),
            "test".to_string(),
            "atest test testa".to_string(),
        ];

        search.set_query("test".to_string());
        let results = search.search_lines(&lines);

        assert_eq!(results.len(), 3); // "test" appears as a whole word 3 times
    }

    #[test]
    fn test_search_navigation() {
        let mut search = ScrollbackSearch::new();
        let lines = vec![
            "match 1".to_string(),
            "match 2".to_string(),
            "match 3".to_string(),
        ];

        search.set_query("match".to_string());
        search.search_lines(&lines);

        assert_eq!(search.match_count(), 3);

        let first = search.next_match().unwrap();
        assert_eq!(first.line_index, 0);

        let second = search.next_match().unwrap();
        assert_eq!(second.line_index, 1);

        let third = search.next_match().unwrap();
        assert_eq!(third.line_index, 2);

        // Wrap around
        let wrapped = search.next_match().unwrap();
        assert_eq!(wrapped.line_index, 0);
    }

    #[test]
    fn test_search_history() {
        let mut search = ScrollbackSearch::new();
        let lines = vec!["test".to_string()];

        search.set_query("query1".to_string());
        search.search_lines(&lines);

        search.set_query("query2".to_string());
        search.search_lines(&lines);

        assert_eq!(search.history().len(), 2);
        assert!(search.history().contains(&"query1".to_string()));
        assert!(search.history().contains(&"query2".to_string()));
    }
}