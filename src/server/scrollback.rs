use std::collections::VecDeque;
use crate::client::ansi_parser::Cell;

/// Optimized scrollback buffer for terminal output
/// Uses a circular buffer (VecDeque) for efficient insertion/removal
#[derive(Debug, Clone)]
pub struct ScrollbackBuffer<T> {
    buffer: VecDeque<T>,
    max_lines: usize,
}

impl<T: Clone> ScrollbackBuffer<T> {
    /// Create a new scrollback buffer with the specified maximum lines
    pub fn new(max_lines: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_lines.min(1000)), // Reasonable initial capacity
            max_lines,
        }
    }

    /// Add a new line to the scrollback buffer
    /// If the buffer is full, removes the oldest line
    pub fn push(&mut self, line: T) {
        if self.buffer.len() >= self.max_lines {
            self.buffer.pop_front();
        }
        self.buffer.push_back(line);
    }

    /// Get a line from the scrollback buffer by index (0 = oldest)
    pub fn get(&self, index: usize) -> Option<&T> {
        self.buffer.get(index)
    }

    /// Get the number of lines in the buffer
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear all lines from the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get an iterator over all lines (oldest to newest)
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buffer.iter()
    }

    /// Get the last N lines from the buffer
    pub fn last_n(&self, n: usize) -> impl Iterator<Item = &T> {
        let start = self.buffer.len().saturating_sub(n);
        self.buffer.range(start..)
    }

    /// Get a range of lines from the buffer
    pub fn range(&self, start: usize, end: usize) -> impl Iterator<Item = &T> {
        let end = end.min(self.buffer.len());
        let start = start.min(end);
        self.buffer.range(start..end)
    }

    /// Resize the buffer to a new maximum size
    /// If the new size is smaller, removes oldest lines
    pub fn resize(&mut self, new_max_lines: usize) {
        self.max_lines = new_max_lines;
        while self.buffer.len() > self.max_lines {
            self.buffer.pop_front();
        }
        // Shrink capacity if significantly oversized
        if self.buffer.capacity() > self.max_lines * 2 {
            self.buffer.shrink_to_fit();
        }
    }

    /// Get all lines as a vector (for serialization/snapshots)
    pub fn to_vec(&self) -> Vec<T> {
        self.buffer.iter().cloned().collect()
    }

    /// Load lines from a vector (for deserialization/restoration)
    pub fn from_vec(&mut self, lines: Vec<T>) {
        self.buffer.clear();
        for line in lines.into_iter().take(self.max_lines) {
            self.buffer.push_back(line);
        }
    }

    /// Get the maximum number of lines this buffer can hold
    pub fn max_lines(&self) -> usize {
        self.max_lines
    }

    /// Get memory usage estimation in bytes
    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>() +
        self.buffer.capacity() * std::mem::size_of::<T>()
    }
}

/// Specialized scrollback buffer for terminal lines (strings)
pub type LineScrollback = ScrollbackBuffer<String>;

/// Specialized scrollback buffer for terminal cells (with formatting)
pub type CellScrollback = ScrollbackBuffer<Vec<Cell>>;

impl LineScrollback {
    /// Search for lines containing the specified text
    pub fn search(&self, needle: &str, case_sensitive: bool) -> Vec<(usize, &String)> {
        let mut results = Vec::new();
        for (index, line) in self.buffer.iter().enumerate() {
            let found = if case_sensitive {
                line.contains(needle)
            } else {
                line.to_lowercase().contains(&needle.to_lowercase())
            };
            if found {
                results.push((index, line));
            }
        }
        results
    }

    /// Get the total character count of all lines
    pub fn char_count(&self) -> usize {
        self.buffer.iter().map(|line| line.len()).sum()
    }

    /// Estimate memory usage for string content
    pub fn content_memory_usage(&self) -> usize {
        self.memory_usage() +
        self.buffer.iter().map(|s| s.capacity()).sum::<usize>()
    }
}

impl CellScrollback {
    /// Get the total cell count across all lines
    pub fn cell_count(&self) -> usize {
        self.buffer.iter().map(|line| line.len()).sum()
    }

    /// Convert to plain text (strips formatting)
    pub fn to_plain_text(&self) -> Vec<String> {
        self.buffer.iter().map(|cells| {
            cells.iter().map(|cell| cell.ch).collect()
        }).collect()
    }

    /// Search for text in the scrollback buffer
    pub fn search_text(&self, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
        let mut results = Vec::new();
        for (line_idx, cells) in self.buffer.iter().enumerate() {
            let line_text: String = cells.iter().map(|cell| cell.ch).collect();
            let found = if case_sensitive {
                line_text.contains(needle)
            } else {
                line_text.to_lowercase().contains(&needle.to_lowercase())
            };
            if found {
                // Find character position within the line
                if let Some(char_pos) = if case_sensitive {
                    line_text.find(needle)
                } else {
                    line_text.to_lowercase().find(&needle.to_lowercase())
                } {
                    results.push((line_idx, char_pos));
                }
            }
        }
        results
    }

    /// Estimate memory usage including cell content
    pub fn content_memory_usage(&self) -> usize {
        self.memory_usage() +
        self.buffer.iter().map(|line| {
            line.capacity() * std::mem::size_of::<Cell>()
        }).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrollback_basic_operations() {
        let mut buffer = LineScrollback::new(3);

        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);

        buffer.push("line1".to_string());
        buffer.push("line2".to_string());
        buffer.push("line3".to_string());

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.get(0), Some(&"line1".to_string()));
        assert_eq!(buffer.get(2), Some(&"line3".to_string()));
    }

    #[test]
    fn test_scrollback_overflow() {
        let mut buffer = LineScrollback::new(2);

        buffer.push("line1".to_string());
        buffer.push("line2".to_string());
        buffer.push("line3".to_string()); // Should remove line1

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.get(0), Some(&"line2".to_string()));
        assert_eq!(buffer.get(1), Some(&"line3".to_string()));
    }

    #[test]
    fn test_scrollback_resize() {
        let mut buffer = LineScrollback::new(5);
        for i in 1..=5 {
            buffer.push(format!("line{}", i));
        }

        assert_eq!(buffer.len(), 5);

        // Resize down
        buffer.resize(3);
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.get(0), Some(&"line3".to_string()));
        assert_eq!(buffer.get(2), Some(&"line5".to_string()));
    }

    #[test]
    fn test_scrollback_search() {
        let mut buffer = LineScrollback::new(5);
        buffer.push("Hello world".to_string());
        buffer.push("Goodbye world".to_string());
        buffer.push("Hello again".to_string());

        let results = buffer.search("Hello", true);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[1].0, 2);

        let results_case_insensitive = buffer.search("WORLD", false);
        assert_eq!(results_case_insensitive.len(), 2);
    }

    #[test]
    fn test_last_n_lines() {
        let mut buffer = LineScrollback::new(5);
        for i in 1..=5 {
            buffer.push(format!("line{}", i));
        }

        let last_3: Vec<_> = buffer.last_n(3).cloned().collect();
        assert_eq!(last_3, vec!["line3", "line4", "line5"]);

        let last_10: Vec<_> = buffer.last_n(10).cloned().collect();
        assert_eq!(last_10.len(), 5); // Only 5 lines available
    }
}