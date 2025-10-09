//! Resilience Patterns
//!
//! Provides fault tolerance mechanisms for production reliability:
//! - Retry logic with exponential backoff
//! - Circuit breakers for failing components
//! - Graceful degradation strategies

pub mod retry;
pub mod circuit_breaker;
