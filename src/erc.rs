use crate::core::SchematicProject;
use crate::diagnostic::{Diagnostic, Severity};
use crate::loader::{
    resolve_cross_reference_text_var, resolve_label_connectivity_text_var,
    resolve_label_text_token_without_connectivity, resolve_sheet_text_var, resolve_text_variables,
    resolved_sheet_text_state, resolved_symbol_text_state,
};
use crate::model::{Property, SchItem};

// Upstream parity: local entrypoint for the first implemented `ERC_TESTER` slice. This is not a
// 1:1 KiCad ERC runner because the current tree still lacks markers, rule matrices, and connection
// graph ownership, but it exists so ERC work can start from real upstream routines instead of
// ad-hoc checks. Remaining divergence is the broader unported `ERC_TESTER` surface beyond the
// implemented field-name-whitespace pass.
pub fn run(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_duplicate_sheet_names(project));
    diagnostics.extend(check_unresolved_text_variables(project));
    diagnostics.extend(check_field_name_whitespace(project));
    diagnostics
}

fn resolved_property_value(properties: &[Property], token: &str) -> Option<String> {
    let canonical = token.to_ascii_uppercase();
    properties
        .iter()
        .find(|property| {
            let property_key = if property.kind.is_mandatory() {
                property.kind.canonical_key().to_ascii_uppercase()
            } else {
                property.key.to_ascii_uppercase()
            };
            property_key == canonical
        })
        .map(|property| property.value.clone())
}

fn unresolved_variable_diagnostic(path: &std::path::Path, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "erc-unresolved-variable",
        kind: crate::diagnostic::DiagnosticKind::Validation,
        message,
        path: Some(path.to_path_buf()),
        span: None,
        line: None,
        column: None,
    }
}

fn child_sheet_path_for_sheet<'a>(
    project: &'a SchematicProject,
    parent_path: &crate::loader::LoadedSheetPath,
    sheet: &crate::model::Sheet,
) -> Option<&'a crate::loader::LoadedSheetPath> {
    project
        .child_sheet_paths(&parent_path.instance_path)
        .into_iter()
        .find(|child| child.sheet_uuid == sheet.uuid)
}

fn shown_symbol_property_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &crate::model::Symbol,
    property: &Property,
) -> String {
    let state =
        resolved_symbol_text_state(symbol, &sheet_path.instance_path, project.current_variant());

    resolve_text_variables(
        &property.value,
        &|token| {
            if token.contains(':') {
                if let Some(value) = resolve_cross_reference_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.current_variant(),
                    token,
                ) {
                    return Some(value);
                }
            }

            resolved_property_value(&state.properties, token).or_else(|| {
                resolve_sheet_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                    1,
                )
            })
        },
        0,
    )
}

fn shown_sheet_property_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    property: &Property,
) -> String {
    let Some(state) = resolved_sheet_text_state(
        &project.schematics,
        &project.sheet_paths,
        sheet_path,
        project.current_variant(),
    ) else {
        return property.value.clone();
    };

    resolve_text_variables(
        &property.value,
        &|token| {
            resolved_property_value(&state.properties, token).or_else(|| {
                resolve_sheet_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                    1,
                )
            })
        },
        0,
    )
}

fn shown_label_property_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    label: &crate::model::Label,
    property: &Property,
) -> String {
    resolve_text_variables(
        &property.value,
        &|token| {
            resolve_label_connectivity_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                label,
                token,
            )
            .or_else(|| {
                if token.contains(':') {
                    resolve_cross_reference_text_var(
                        &project.schematics,
                        &project.sheet_paths,
                        sheet_path,
                        project.current_variant(),
                        token,
                    )
                } else {
                    None
                }
            })
            .or_else(|| {
                resolve_label_text_token_without_connectivity(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    label,
                    token,
                )
            })
        },
        0,
    )
}

fn shown_text_item_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    text: &str,
) -> String {
    resolve_text_variables(
        text,
        &|token| {
            resolve_sheet_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                token,
                1,
            )
        },
        0,
    )
}

// Upstream parity: reduced local analogue for the exercised unresolved-variable half of
// `ERC_TESTER::TestTextVars()`. This is not a 1:1 KiCad marker pass because the current tree still
// reports plain diagnostics, still lacks drawing-sheet text coverage, and does not yet walk
// library-child text/textboxes under symbols. It exists so ERC now checks the real loaded
// symbol/sheet/label/text/textbox/sheet-pin shown-text paths that this tree already exercises.
// Remaining divergence is the broader unported lib-child and drawing-sheet resolver surface.
pub fn check_unresolved_text_variables(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for item in &schematic.screen.items {
            match item {
                SchItem::Symbol(symbol) => {
                    for property in &symbol.properties {
                        let shown =
                            shown_symbol_property_text(project, sheet_path, symbol, property);

                        if shown.contains("${") {
                            diagnostics.push(unresolved_variable_diagnostic(
                                &schematic.path,
                                format!(
                                    "Unresolved text variable in symbol field '{}'",
                                    property.key
                                ),
                            ));
                        }
                    }
                }
                SchItem::Label(label) => {
                    for property in &label.properties {
                        let shown = shown_label_property_text(project, sheet_path, label, property);

                        if shown.contains("${") {
                            diagnostics.push(unresolved_variable_diagnostic(
                                &schematic.path,
                                format!(
                                    "Unresolved text variable in label field '{}'",
                                    property.key
                                ),
                            ));
                        }
                    }
                }
                SchItem::Sheet(sheet) => {
                    for property in &sheet.properties {
                        let shown = shown_sheet_property_text(project, sheet_path, property);

                        if shown.contains("${") {
                            diagnostics.push(unresolved_variable_diagnostic(
                                &schematic.path,
                                format!(
                                    "Unresolved text variable in sheet field '{}'",
                                    property.key
                                ),
                            ));
                        }
                    }

                    if let Some(child_sheet_path) =
                        child_sheet_path_for_sheet(project, sheet_path, sheet)
                    {
                        for pin in &sheet.pins {
                            let shown = shown_text_item_text(project, child_sheet_path, &pin.name);

                            if shown.contains("${") {
                                diagnostics.push(unresolved_variable_diagnostic(
                                    &schematic.path,
                                    format!("Unresolved text variable in sheet pin '{}'", pin.name),
                                ));
                            }
                        }
                    }
                }
                SchItem::Text(text) => {
                    let shown = shown_text_item_text(project, sheet_path, &text.text);

                    if shown.contains("${") {
                        diagnostics.push(unresolved_variable_diagnostic(
                            &schematic.path,
                            "Unresolved text variable in schematic text".to_string(),
                        ));
                    }
                }
                SchItem::TextBox(text_box) => {
                    let shown = shown_text_item_text(project, sheet_path, &text_box.text);

                    if shown.contains("${") {
                        diagnostics.push(unresolved_variable_diagnostic(
                            &schematic.path,
                            "Unresolved text variable in schematic text box".to_string(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics
}

fn shown_sheet_name(sheet: &crate::model::Sheet) -> Option<&str> {
    sheet.properties.iter().find_map(|property| {
        (property.kind == crate::model::PropertyKind::SheetName)
            .then_some(property.value.as_str())
            .or_else(|| {
                property
                    .key
                    .eq_ignore_ascii_case("Sheetname")
                    .then_some(property.value.as_str())
            })
    })
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestDuplicateSheetNames()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still reports plain diagnostics and still
// reads the already-loaded sheet-name field text instead of the full `GetShownName()` display
// stack, but it preserves the exercised same-screen duplicate-name comparison and case-insensitive
// matching. Remaining divergence is path-sensitive shown-name exactness.
pub fn check_duplicate_sheet_names(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for schematic in &project.schematics {
        let sheets = schematic
            .screen
            .items
            .iter()
            .filter_map(|item| match item {
                SchItem::Sheet(sheet) => Some(sheet),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (index, sheet) in sheets.iter().enumerate() {
            let Some(name) = shown_sheet_name(sheet) else {
                continue;
            };

            for other in sheets.iter().skip(index + 1) {
                let Some(other_name) = shown_sheet_name(other) else {
                    continue;
                };

                if !name.eq_ignore_ascii_case(other_name) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-duplicate-sheet-name",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!("Duplicate sheet name: '{name}'"),
                    path: Some(schematic.path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestFieldNameWhitespace()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still reports plain diagnostics instead of
// `SCH_MARKER` / `ERC_ITEM`, but it preserves the same exercised symbol/sheet field-name
// whitespace rule and message text. Remaining divergence is richer sheet-path marker context.
pub fn check_field_name_whitespace(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for schematic in &project.schematics {
        for item in &schematic.screen.items {
            match item {
                SchItem::Symbol(symbol) => {
                    for property in &symbol.properties {
                        let trimmed = property.key.trim();

                        if property.key != trimmed {
                            diagnostics.push(Diagnostic {
                                severity: Severity::Warning,
                                code: "erc-field-name-whitespace",
                                kind: crate::diagnostic::DiagnosticKind::Validation,
                                message: format!(
                                    "Field name has leading or trailing whitespace: '{}'",
                                    property.key
                                ),
                                path: Some(schematic.path.clone()),
                                span: None,
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
                SchItem::Sheet(sheet) => {
                    for property in &sheet.properties {
                        let trimmed = property.key.trim();

                        if property.key != trimmed {
                            diagnostics.push(Diagnostic {
                                severity: Severity::Warning,
                                code: "erc-field-name-whitespace",
                                kind: crate::diagnostic::DiagnosticKind::Validation,
                                message: format!(
                                    "Field name has leading or trailing whitespace: '{}'",
                                    property.key
                                ),
                                path: Some(schematic.path.clone()),
                                span: None,
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics
}
