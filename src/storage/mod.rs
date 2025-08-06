/// Storage module with refactored database operations
/// 
/// This module provides generic database operation patterns and helper functions
/// to reduce code duplication across storage operations.

pub mod core;
pub mod traits;
pub mod mappers; 
pub mod query_builder;
pub mod utils;

// Re-export commonly used items
pub use core::*;
pub use traits::*;
pub use mappers::*;
pub use query_builder::*;
pub use utils::*;