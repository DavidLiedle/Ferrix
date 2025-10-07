//! Format variable expansion system
//!
//! Implements tmux-style format string expansion with #{variable} syntax.
//! Supports session, window, pane, and client variables with modifiers and conditionals.
//!
//! # Examples
//!
//! ```
//! # use ferrix::format::FormatExpander;
//! let expander = FormatExpander::new();
//! let result = expander.expand("Session: #{session_name}");
//! ```

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::error::{Result, FerrixError};

/// Format variable value
#[derive(Debug, Clone)]
pub enum FormatValue {
    String(String),
    Number(i64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    None,
}

impl FormatValue {
    /// Convert to string representation
    pub fn as_string(&self) -> String {
        match self {
            FormatValue::String(s) => s.clone(),
            FormatValue::Number(n) => n.to_string(),
            FormatValue::Boolean(b) => if *b { "1" } else { "0" }.to_string(),
            FormatValue::Timestamp(t) => t.to_rfc3339(),
            FormatValue::None => String::new(),
        }
    }

    /// Convert to boolean
    pub fn as_bool(&self) -> bool {
        match self {
            FormatValue::String(s) => !s.is_empty(),
            FormatValue::Number(n) => *n != 0,
            FormatValue::Boolean(b) => *b,
            FormatValue::Timestamp(_) => true,
            FormatValue::None => false,
        }
    }

    /// Convert to number
    pub fn as_number(&self) -> i64 {
        match self {
            FormatValue::String(s) => s.parse().unwrap_or(0),
            FormatValue::Number(n) => *n,
            FormatValue::Boolean(b) => if *b { 1 } else { 0 },
            FormatValue::Timestamp(t) => t.timestamp(),
            FormatValue::None => 0,
        }
    }
}

impl From<String> for FormatValue {
    fn from(s: String) -> Self {
        FormatValue::String(s)
    }
}

impl From<&str> for FormatValue {
    fn from(s: &str) -> Self {
        FormatValue::String(s.to_string())
    }
}

impl From<i64> for FormatValue {
    fn from(n: i64) -> Self {
        FormatValue::Number(n)
    }
}

impl From<bool> for FormatValue {
    fn from(b: bool) -> Self {
        FormatValue::Boolean(b)
    }
}

/// Format variable provider trait
pub trait FormatProvider {
    /// Get a format variable value
    fn get_variable(&self, name: &str) -> Option<FormatValue>;
}

/// Format string expander
pub struct FormatExpander {
    variables: HashMap<String, FormatValue>,
}

impl FormatExpander {
    /// Create a new format expander
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    /// Set a variable
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<FormatValue>) {
        self.variables.insert(name.into(), value.into());
    }

    /// Get a variable
    pub fn get_variable(&self, name: &str) -> Option<&FormatValue> {
        self.variables.get(name)
    }

    /// Expand a format string
    ///
    /// Supports:
    /// - `#{variable_name}` - Simple variable expansion
    /// - `##` - Literal '#' character
    /// - `#{?condition,true_value,false_value}` - Conditional
    /// - `#{variable:modifier}` - Modifiers (padding, trimming, etc.)
    pub fn expand(&self, format: &str) -> Result<String> {
        let mut result = String::new();
        let mut chars = format.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '#' {
                // Check for escape sequence ##
                if chars.peek() == Some(&'#') {
                    chars.next();
                    result.push('#');
                    continue;
                }

                // Check for variable expansion #{...}
                if chars.peek() == Some(&'{') {
                    chars.next(); // consume '{'

                    // Check for conditional format
                    if chars.peek() == Some(&'?') {
                        chars.next(); // consume '?'
                        let expanded = self.expand_conditional(&mut chars)?;
                        result.push_str(&expanded);
                    } else {
                        // Regular variable expansion with optional modifiers
                        let expanded = self.expand_variable(&mut chars)?;
                        result.push_str(&expanded);
                    }
                } else {
                    // Not a format variable, keep the '#'
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    /// Expand a variable with optional modifiers
    fn expand_variable(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String> {
        let mut name = String::new();
        let mut modifier = String::new();
        let mut has_modifier = false;

        // Extract variable name and modifier
        while let Some(&ch) = chars.peek() {
            if ch == '}' {
                chars.next(); // consume '}'
                break;
            } else if ch == ':' {
                chars.next(); // consume ':'
                has_modifier = true;
                // Extract modifier
                while let Some(&ch) = chars.peek() {
                    if ch == '}' {
                        chars.next(); // consume '}'
                        break;
                    }
                    modifier.push(ch);
                    chars.next();
                }
                break;
            } else {
                name.push(ch);
                chars.next();
            }
        }

        if name.is_empty() {
            return Err(FerrixError::Other("Empty variable name".to_string()));
        }

        // Get variable value
        let mut value = self.variables.get(&name)
            .map(|v| v.as_string())
            .unwrap_or_default();

        // Apply modifier if present
        if has_modifier && !modifier.is_empty() {
            value = self.apply_modifier(&value, &modifier)?;
        }

        Ok(value)
    }

    /// Expand a conditional format: #{?condition,true_value,false_value}
    fn expand_conditional(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String> {
        // Extract condition, true_value, and false_value
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut depth = 0;

        while let Some(&ch) = chars.peek() {
            chars.next();

            match ch {
                '{' => {
                    depth += 1;
                    current.push(ch);
                }
                '}' if depth > 0 => {
                    depth -= 1;
                    current.push(ch);
                }
                '}' => {
                    // End of conditional
                    parts.push(current.clone());
                    break;
                }
                ',' if depth == 0 => {
                    // Part separator
                    parts.push(current.clone());
                    current.clear();
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        if parts.len() < 2 {
            return Err(FerrixError::Other(
                "Conditional format requires at least condition and true value".to_string()
            ));
        }

        let condition = &parts[0];
        let true_value = &parts[1];
        let false_value = parts.get(2).map(|s| s.as_str()).unwrap_or("");

        // Evaluate condition
        let condition_result = self.evaluate_condition(condition)?;

        // Expand the appropriate value
        let result_template = if condition_result { true_value } else { false_value };
        self.expand(result_template)
    }

    /// Evaluate a condition (supports variable checks and comparisons)
    fn evaluate_condition(&self, condition: &str) -> Result<bool> {
        let condition = condition.trim();

        // Check for comparison operators
        if let Some(pos) = condition.find("==") {
            let left = self.expand(&format!("#{{{}}}", condition[..pos].trim()))?;
            let right = condition[pos + 2..].trim().trim_matches('"');
            return Ok(left == right);
        }
        if let Some(pos) = condition.find("!=") {
            let left = self.expand(&format!("#{{{}}}", condition[..pos].trim()))?;
            let right = condition[pos + 2..].trim().trim_matches('"');
            return Ok(left != right);
        }
        if let Some(pos) = condition.find(">=") {
            let left = self.expand(&format!("#{{{}}}", condition[..pos].trim()))?;
            let right = condition[pos + 2..].trim();
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                return Ok(l >= r);
            }
        }
        if let Some(pos) = condition.find("<=") {
            let left = self.expand(&format!("#{{{}}}", condition[..pos].trim()))?;
            let right = condition[pos + 2..].trim();
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                return Ok(l <= r);
            }
        }
        if let Some(pos) = condition.find('>') {
            let left = self.expand(&format!("#{{{}}}", condition[..pos].trim()))?;
            let right = condition[pos + 1..].trim();
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                return Ok(l > r);
            }
        }
        if let Some(pos) = condition.find('<') {
            let left = self.expand(&format!("#{{{}}}", condition[..pos].trim()))?;
            let right = condition[pos + 1..].trim();
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                return Ok(l < r);
            }
        }

        // Simple variable truthiness check
        if let Some(value) = self.variables.get(condition) {
            return Ok(value.as_bool());
        }

        // Try expanding as a nested format
        let expanded = self.expand(&format!("#{{{}}}", condition))?;
        Ok(!expanded.is_empty() && expanded != "0")
    }

    /// Apply a modifier to a value
    fn apply_modifier(&self, value: &str, modifier: &str) -> Result<String> {
        // Padding modifier: p<width>
        if let Some(width_str) = modifier.strip_prefix('p') {
            if let Ok(width) = width_str.parse::<usize>() {
                return Ok(format!("{:>width$}", value, width = width));
            }
        }

        // Trimming modifier: =<length>
        if let Some(length_str) = modifier.strip_prefix('=') {
            if let Ok(length) = length_str.parse::<usize>() {
                let trimmed: String = value.chars().take(length).collect();
                return Ok(trimmed);
            }
        }

        // Substitution modifier: s/old/new/ or s|old|new|
        if modifier.starts_with('s') {
            if let Some(rest) = modifier.strip_prefix('s') {
                if rest.len() >= 3 {
                    if let Some(delimiter) = rest.chars().next() {
                        let parts: Vec<&str> = rest[1..].split(delimiter).collect();
                        if parts.len() >= 2 {
                            return Ok(value.replace(parts[0], parts[1]));
                        }
                    }
                }
            }
        }

        // Left-pad modifier: l<width>
        if let Some(width_str) = modifier.strip_prefix('l') {
            if let Ok(width) = width_str.parse::<usize>() {
                return Ok(format!("{:<width$}", value, width = width));
            }
        }

        // Uppercase modifier: u
        if modifier == "u" {
            return Ok(value.to_uppercase());
        }

        // Lowercase modifier: d
        if modifier == "d" {
            return Ok(value.to_lowercase());
        }

        // Unknown modifier - return value unchanged
        Ok(value.to_string())
    }

    /// Extract variable name from format string (legacy method, kept for compatibility)
    #[allow(dead_code)]
    fn extract_variable_name(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String> {
        let mut name = String::new();

        while let Some(&ch) = chars.peek() {
            if ch == '}' {
                chars.next(); // consume '}'
                break;
            } else if ch == ':' {
                // Modifier syntax
                chars.next();
                // Skip to closing brace
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '}' {
                        break;
                    }
                }
                break;
            } else {
                name.push(ch);
                chars.next();
            }
        }

        if name.is_empty() {
            return Err(FerrixError::Other("Empty variable name".to_string()));
        }

        Ok(name)
    }
}

impl Default for FormatExpander {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_expansion() {
        let mut expander = FormatExpander::new();
        expander.set_variable("session_name", "test-session");
        expander.set_variable("window_index", 1i64);

        let result = expander.expand("Session: #{session_name}").unwrap();
        assert_eq!(result, "Session: test-session");
    }

    #[test]
    fn test_multiple_variables() {
        let mut expander = FormatExpander::new();
        expander.set_variable("session_name", "test");
        expander.set_variable("window_name", "main");

        let result = expander.expand("#{session_name}:#{window_name}").unwrap();
        assert_eq!(result, "test:main");
    }

    #[test]
    fn test_escape_hash() {
        let expander = FormatExpander::new();
        let result = expander.expand("##").unwrap();
        assert_eq!(result, "#");

        let result = expander.expand("Color: ##FF0000").unwrap();
        assert_eq!(result, "Color: #FF0000");
    }

    #[test]
    fn test_missing_variable() {
        let expander = FormatExpander::new();
        let result = expander.expand("#{missing}").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_number_variable() {
        let mut expander = FormatExpander::new();
        expander.set_variable("count", 42i64);

        let result = expander.expand("Count: #{count}").unwrap();
        assert_eq!(result, "Count: 42");
    }

    #[test]
    fn test_boolean_variable() {
        let mut expander = FormatExpander::new();
        expander.set_variable("attached", true);
        expander.set_variable("zoomed", false);

        let result = expander.expand("#{attached}/#{zoomed}").unwrap();
        assert_eq!(result, "1/0");
    }

    #[test]
    fn test_format_value_conversions() {
        let str_val = FormatValue::from("test");
        assert_eq!(str_val.as_string(), "test");
        assert!(str_val.as_bool());

        let num_val = FormatValue::from(42i64);
        assert_eq!(num_val.as_number(), 42);
        assert_eq!(num_val.as_string(), "42");
        assert!(num_val.as_bool());

        let zero_val = FormatValue::from(0i64);
        assert!(!zero_val.as_bool());

        let bool_val = FormatValue::from(true);
        assert_eq!(bool_val.as_number(), 1);
        assert_eq!(bool_val.as_string(), "1");
    }

    #[test]
    fn test_conditional_formats() {
        let mut expander = FormatExpander::new();
        expander.set_variable("active", true);
        expander.set_variable("inactive", false);
        expander.set_variable("count", 5i64);

        // Simple boolean conditional
        let result = expander.expand("#{?active,yes,no}").unwrap();
        assert_eq!(result, "yes");

        let result = expander.expand("#{?inactive,yes,no}").unwrap();
        assert_eq!(result, "no");

        // Comparison conditionals
        let result = expander.expand("#{?count>3,high,low}").unwrap();
        assert_eq!(result, "high");

        let result = expander.expand("#{?count<3,low,high}").unwrap();
        assert_eq!(result, "high");

        // String comparison
        expander.set_variable("status", "ready");
        let result = expander.expand("#{?status==\"ready\",✓,✗}").unwrap();
        assert_eq!(result, "✓");
    }

    #[test]
    fn test_format_modifiers_padding() {
        let mut expander = FormatExpander::new();
        expander.set_variable("name", "test");
        expander.set_variable("num", 42i64);

        // Right padding
        let result = expander.expand("#{name:p10}").unwrap();
        assert_eq!(result, "      test");

        // Left padding
        let result = expander.expand("#{name:l10}").unwrap();
        assert_eq!(result, "test      ");
    }

    #[test]
    fn test_format_modifiers_trimming() {
        let mut expander = FormatExpander::new();
        expander.set_variable("long_name", "verylongname");

        // Trim to 5 characters
        let result = expander.expand("#{long_name:=5}").unwrap();
        assert_eq!(result, "veryl");
    }

    #[test]
    fn test_format_modifiers_substitution() {
        let mut expander = FormatExpander::new();
        expander.set_variable("path", "/home/user/documents");

        // String substitution
        let result = expander.expand("#{path:s/home/usr/}").unwrap();
        assert_eq!(result, "/usr/user/documents");

        // Alternative delimiter
        let result = expander.expand("#{path:s|/home|~|}").unwrap();
        assert_eq!(result, "~/user/documents");
    }

    #[test]
    fn test_format_modifiers_case() {
        let mut expander = FormatExpander::new();
        expander.set_variable("text", "Hello World");

        // Uppercase
        let result = expander.expand("#{text:u}").unwrap();
        assert_eq!(result, "HELLO WORLD");

        // Lowercase
        let result = expander.expand("#{text:d}").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_nested_conditionals() {
        let mut expander = FormatExpander::new();
        expander.set_variable("level", 2i64);

        // Nested conditional
        let result = expander.expand("#{?level>2,high,#{?level>1,medium,low}}").unwrap();
        assert_eq!(result, "medium");

        expander.set_variable("level", 3i64);
        let result = expander.expand("#{?level>2,high,#{?level>1,medium,low}}").unwrap();
        assert_eq!(result, "high");
    }

    #[test]
    fn test_conditional_with_variables() {
        let mut expander = FormatExpander::new();
        expander.set_variable("window_active", true);
        expander.set_variable("window_name", "editor");

        // Conditional with variable expansion
        let result = expander.expand("#{?window_active,#{window_name}*,#{window_name}}").unwrap();
        assert_eq!(result, "editor*");

        expander.set_variable("window_active", false);
        let result = expander.expand("#{?window_active,#{window_name}*,#{window_name}}").unwrap();
        assert_eq!(result, "editor");
    }

    #[test]
    fn test_complex_format() {
        let mut expander = FormatExpander::new();
        expander.set_variable("session_name", "dev-session");
        expander.set_variable("window_count", 3i64);
        expander.set_variable("cpu_usage", 75i64);

        // Complex format with multiple features
        let result = expander.expand(
            "[#{session_name:=10}] #{window_count} windows | CPU: #{?cpu_usage>80,🔴,#{?cpu_usage>50,🟡,🟢}}"
        ).unwrap();
        assert_eq!(result, "[dev-sessio] 3 windows | CPU: 🟡");
    }
}
