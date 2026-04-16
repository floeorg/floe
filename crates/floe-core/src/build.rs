//! Build orchestration: take `.fl` source files, compile each to
//! TypeScript, and report what succeeded. Drives the CLI's `build` and
//! `check` commands.

pub mod package_compiler;

pub use package_compiler::{CompiledFile, PackageCompiler};
