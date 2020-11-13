//! This modules implements a line parser for FTP control channel commands
//!
//! Use the parse method. It takes a FTP line and returns an instance of the Command enum.
//!
pub mod error;
mod parser;
#[cfg(test)]
mod tests;

pub use parser::parse;
