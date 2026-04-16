//! Build orchestration: take a set of `.fl` source files, compile each
//! one to TypeScript, and report what succeeded.
//!
//! Mirrors Gleam's `build/package_compiler.rs`. Today it drives the CLI's
//! `floe build` and `floe check` commands; tomorrow the LSP will reuse the
//! same compiler with an incremental cache so every keystroke doesn't
//! re-parse every module.

pub mod package_compiler;

pub use package_compiler::{BuildReport, CompiledFile, PackageCompiler};
