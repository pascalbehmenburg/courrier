//! Mail parsing and forwarded-mail analysis.

pub mod forwarding;
pub mod parser;
pub mod unsubscribe;

pub use parser::ParsedMail;
