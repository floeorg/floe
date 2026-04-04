//! Probe execution — creates a temp directory, runs tsgo, and reads output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Create a temporary directory with the probe file and tsconfig.
pub(super) fn create_probe_dir(
    project_dir: &Path,
    probe_content: &str,
    ts_imports: &HashMap<String, PathBuf>,
) -> Result<tempfile::TempDir, String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let probe_dir = tmp.path();

    // Write probe.ts
    std::fs::write(probe_dir.join("probe.ts"), probe_content)
        .map_err(|e| format!("failed to write probe.ts: {e}"))?;

    // Symlink local .ts/.tsx files into the probe directory so tsgo can
    // resolve them without absolute paths (which cause tsgo to emit stray
    // .d.ts files next to the original sources)
    for abs_path in ts_imports.values() {
        if let Some(filename) = abs_path.file_name() {
            let link = probe_dir.join(filename);
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(abs_path, &link).ok();
            }
            #[cfg(not(unix))]
            {
                std::fs::copy(abs_path, &link).ok();
            }
        }
    }

    // Write tsconfig.json, inheriting paths from the project's tsconfig if available
    let paths_config = read_project_tsconfig_paths(project_dir);
    let tsconfig = format!(
        r#"{{
  "compilerOptions": {{
    "moduleResolution": "bundler",
    "strict": false,
    "strictNullChecks": true,
    "noImplicitAny": false,
    "jsx": "react-jsx",
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "./out",
    "rootDir": "/",
    "skipLibCheck": true{paths_config}
  }},
  "include": ["probe.ts"]
}}"#
    );
    std::fs::write(probe_dir.join("tsconfig.json"), &tsconfig)
        .map_err(|e| format!("failed to write tsconfig.json: {e}"))?;

    // Symlink node_modules from the project directory
    let node_modules = project_dir.join("node_modules");
    if node_modules.is_dir() {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&node_modules, probe_dir.join("node_modules"))
                .map_err(|e| format!("failed to symlink node_modules: {e}"))?;
        }
        #[cfg(not(unix))]
        {
            // On Windows, try junction or copy
            let _ = std::fs::create_dir_all(probe_dir.join("node_modules"));
            // Fall through — types may not resolve without node_modules
        }
    }

    Ok(tmp)
}

/// Read `paths` and `baseUrl` from the project's tsconfig.json and format them
/// as JSON properties to include in the probe's tsconfig.
/// Returns an empty string if no paths are configured.
fn read_project_tsconfig_paths(project_dir: &Path) -> String {
    crate::resolve::ParsedTsconfig::from_project_dir(project_dir)
        .map(|p| p.to_probe_json_fragment())
        .unwrap_or_default()
}

/// Find `probe.d.ts` under the output directory.
///
/// With `rootDir: "/"` in the probe tsconfig, tsgo mirrors the full absolute
/// path under `outDir`, so the file ends up at `out/<full-temp-path>/probe.d.ts`
/// instead of `out/probe.d.ts`. We search recursively to handle both cases.
fn find_probe_dts(probe_dir: &Path) -> Option<PathBuf> {
    let out_dir = probe_dir.join("out");
    // Fast path: check the simple location first
    let simple = out_dir.join("probe.d.ts");
    if simple.exists() {
        return Some(simple);
    }
    // Slow path: search recursively under out/
    fn walk(dir: &Path) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|n| n == "probe.d.ts") {
                return Some(path);
            }
            if path.is_dir()
                && let Some(found) = walk(&path)
            {
                return Some(found);
            }
        }
        None
    }
    walk(&out_dir)
}

/// Check whether tsgo (or npx @typescript/native-preview) is available on the system.
/// The result is cached for the lifetime of the process.
pub fn is_tsgo_available() -> bool {
    use std::sync::OnceLock;
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        // Try tsgo directly
        if let Ok(output) = Command::new("tsgo").arg("--version").output()
            && output.status.success()
        {
            return true;
        }

        // Try npx @typescript/native-preview
        if let Ok(output) = Command::new("npx")
            .args(["@typescript/native-preview", "--version"])
            .output()
            && output.status.success()
        {
            return true;
        }

        false
    })
}

/// Run tsgo on the probe directory and return the output `.d.ts` content.
pub(super) fn run_tsgo(probe_dir: &Path) -> Result<String, String> {
    // Try tsgo first, then fall back to npx @typescript/native-preview
    let tsgo_result = Command::new("tsgo")
        .args(["-p", "tsconfig.json"])
        .current_dir(probe_dir)
        .output();

    let output = match tsgo_result {
        Ok(output) if output.status.success() || find_probe_dts(probe_dir).is_some() => output,
        _ => {
            // Fall back to npx @typescript/native-preview
            let npx_result = Command::new("npx")
                .args(["@typescript/native-preview", "-p", "tsconfig.json"])
                .current_dir(probe_dir)
                .output()
                .map_err(|e| format!("failed to run tsgo or npx: {e}"))?;

            if !npx_result.status.success() && find_probe_dts(probe_dir).is_none() {
                let stderr = String::from_utf8_lossy(&npx_result.stderr);
                return Err(format!("tsgo failed: {stderr}"));
            }
            npx_result
        }
    };

    // Even if tsgo reports errors (e.g. for unused variables), check if the .d.ts was emitted
    let _ = output;
    let dts_path =
        find_probe_dts(probe_dir).ok_or_else(|| "tsgo did not emit probe.d.ts".to_string())?;

    std::fs::read_to_string(&dts_path).map_err(|e| format!("failed to read probe.d.ts: {e}"))
}
