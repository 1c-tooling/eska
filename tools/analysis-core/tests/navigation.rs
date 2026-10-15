#![cfg(feature = "analyzer-cli")]

pub use eska_analysis::{AnalysisScope, AnalysisSession, SessionOptions};

// Exercise the real declaration helper without duplicating the SDK library's playground guard.
#[path = "../src/analyzer_cli/navigation/tests.rs"]
mod cases;
#[path = "../src/analyzer_cli/navigation.rs"]
mod navigation;
