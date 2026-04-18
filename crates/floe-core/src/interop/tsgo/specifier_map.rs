//! Specifier mapping — maps probe export names back to import specifiers.

use std::collections::HashMap;
use std::path::PathBuf;

use super::probe_gen::{expr_to_callee_name, unwrap_try_await_expr};

use crate::parser::ast::*;

use super::probe_gen::collect_all_consts;
use super::{DtsExport, TsType};

/// Build a specifier-to-exports map from the resolved probe exports.
///
/// The probe uses `_r0`, `_r1`, etc. as export names. We map these back
/// to the original import specifiers by replaying the same probe generation
/// logic to know which index corresponds to which import.
pub(super) fn build_specifier_map(
    program: &Program,
    probe_exports: &[DtsExport],
    ts_imports: &HashMap<String, PathBuf>,
) -> HashMap<String, Vec<DtsExport>> {
    let mut result: HashMap<String, Vec<DtsExport>> = HashMap::new();
    let mut probe_index = 0usize;

    // Collect external imports (npm + relative TS)
    let mut imported_names: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
            let is_ts_import = ts_imports.contains_key(&decl.source);
            if !is_relative || is_ts_import {
                for spec in &decl.specifiers {
                    let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
                    imported_names.insert(effective_name.to_string(), decl.source.clone());
                }
            }
        }
    }

    // Use the same recursive const collection as generate_probe
    let all_consts = collect_all_consts(program);

    // Map call probe results (including Construct nodes)
    for decl in &all_consts {
        let inner_value = unwrap_try_await_expr(&decl.value);

        // Handle Construct nodes (uppercase calls like QueryClient({...}))
        if let ExprKind::Construct { type_name, .. } = &inner_value.kind
            && imported_names.contains_key(type_name)
        {
            let specifier = &imported_names[type_name];
            let binding_name = decl.binding.binding_name();
            let probe_name = format!("_r{probe_index}");
            if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
                result
                    .entry(specifier.clone())
                    .or_default()
                    .push(DtsExport {
                        name: format!("__probe_{}", binding_name),
                        ts_type: export.ts_type.clone(),
                    });
            }
            probe_index += 1;
            continue;
        }

        if let ExprKind::Call { callee, .. } = &inner_value.kind {
            let callee_name = expr_to_callee_name(callee);
            if let Some(name) = &callee_name {
                let is_imported = imported_names.contains_key(name);
                let root_name = name.split('.').next().unwrap_or("");
                let is_member_of_import =
                    name.contains('.') && imported_names.contains_key(root_name);

                if !is_imported && !is_member_of_import {
                    continue;
                }

                let specifier = if is_imported {
                    &imported_names[name]
                } else {
                    &imported_names[root_name]
                };
                let binding_name = decl.binding.binding_name();

                // For array bindings, collect individual element types
                if let ConstBinding::Array(names) = &decl.binding {
                    let elem_types: Vec<TsType> = names
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            let elem_name = format!("_r{}_{i}", probe_index);
                            probe_exports
                                .iter()
                                .find(|e| e.name == elem_name)
                                .map(|e| e.ts_type.clone())
                                .unwrap_or(TsType::Unknown)
                        })
                        .collect();
                    result
                        .entry(specifier.clone())
                        .or_default()
                        .push(DtsExport {
                            name: format!("__probe_{}", binding_name),
                            ts_type: TsType::Tuple(elem_types),
                        });
                } else if let ConstBinding::Object(fields) = &decl.binding {
                    // For object destructuring: const { data } = f(...)
                    // Probe names use __probe_{field}_rN_I format
                    let elem_types: Vec<TsType> = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            let elem_name = format!("__probe_{}_r{}_{i}", f.field, probe_index);
                            probe_exports
                                .iter()
                                .find(|e| e.name == elem_name)
                                .map(|e| e.ts_type.clone())
                                .unwrap_or(TsType::Unknown)
                        })
                        .collect();
                    // Create individual probes for each destructured field
                    // Use probe_index to disambiguate when same field name appears multiple times
                    for (i, f) in fields.iter().enumerate() {
                        if i < elem_types.len() {
                            // Probes use the original field name for lookup
                            let field = &f.field;
                            result
                                .entry(specifier.clone())
                                .or_default()
                                .push(DtsExport {
                                    name: format!("__probe_{field}_{probe_index}"),
                                    ts_type: elem_types[i].clone(),
                                });
                        }
                    }
                } else {
                    let probe_name = format!("_r{probe_index}");
                    if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
                        result
                            .entry(specifier.clone())
                            .or_default()
                            .push(DtsExport {
                                name: format!("__probe_{}", binding_name),
                                ts_type: export.ts_type.clone(),
                            });
                    }
                }
                probe_index += 1;
                continue;
            }
        }
    }

    // Map re-export probe results — ALL imported names (sorted for deterministic order)
    let mut sorted_import_names: Vec<_> = imported_names.iter().collect();
    sorted_import_names.sort_by_key(|(name, _)| (*name).clone());
    for (name, specifier) in sorted_import_names {
        let probe_name = format!("_r{probe_index}");
        if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
            result
                .entry(specifier.clone())
                .or_default()
                .push(DtsExport {
                    name: name.clone(),
                    ts_type: export.ts_type.clone(),
                });
        }
        probe_index += 1;
    }

    // Map member access probe results (__member_X_field exports)
    // and inlined const call probe results (__probe_X_N exports)
    // and type alias probe results (__tprobe_X exports)
    for export in probe_exports {
        if let Some(rest) = export.name.strip_prefix("__member_") {
            // Find which specifier this belongs to
            if let Some(underscore_pos) = rest.find('_') {
                let obj_name = &rest[..underscore_pos];
                if let Some(specifier) = imported_names.get(obj_name) {
                    result
                        .entry(specifier.clone())
                        .or_default()
                        .push(export.clone());
                }
            }
        }
        // Route chain probe results (__chain_X$Y$Z exports)
        if let Some(rest) = export.name.strip_prefix("__chain_")
            && let Some(dollar_pos) = rest.find('$')
        {
            let obj_name = &rest[..dollar_pos];
            if let Some(specifier) = imported_names.get(obj_name) {
                result
                    .entry(specifier.clone())
                    .or_default()
                    .push(export.clone());
            } else if let Some(first_specifier) = result.keys().next().cloned() {
                // Type-name-rooted chain (e.g. __chain_Database$insert$values$returning)
                // where the root is a Floe structural alias, not a direct npm import.
                // Route to any available specifier so lookup_dts_probe can find it.
                result
                    .entry(first_specifier)
                    .or_default()
                    .push(export.clone());
            }
        }
        // Route type/JSX probes to any specifier so the checker can find them
        for prefix in ["__tprobe_", "__jsx_", "__jsxc_"] {
            if export.name.starts_with(prefix)
                && let Some(first_specifier) = result.keys().next().cloned()
            {
                result
                    .entry(first_specifier)
                    .or_default()
                    .push(export.clone());
            }
        }
        // Inlined const call probes (__probe_user_5, etc.) — same routing
        // but with dedup guard since __probe_ entries can also come from the
        // index-based mapping above. Skip raw per-field probes from object
        // destructuring (e.g. __probe_data_r4_0) since those are already
        // mapped by the index-based section as __probe_data_4.
        if export.name.starts_with("__probe_")
            && !is_raw_per_field_probe(&export.name)
            && !result.values().flatten().any(|e| e.name == export.name)
            && let Some(first_specifier) = result.keys().next().cloned()
        {
            result
                .entry(first_specifier)
                .or_default()
                .push(export.clone());
        }
    }

    result
}

/// Check if a probe name is a raw per-field probe from object destructuring
/// (e.g. `__probe_data_r4_0`). These are already mapped by the index-based
/// section and should not be duplicated via the catch-all routing.
fn is_raw_per_field_probe(name: &str) -> bool {
    // Pattern: __probe_{field}_r{digit}_{digit}
    if let Some(rest) = name.strip_prefix("__probe_") {
        rest.contains("_r")
            && rest
                .find("_r")
                .and_then(|pos| rest.as_bytes().get(pos + 2))
                .is_some_and(|b| b.is_ascii_digit())
    } else {
        false
    }
}
