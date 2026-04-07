use crate::core::SchematicProject;
use crate::diagnostic::{Diagnostic, Severity};
use crate::model::SchItem;

// Upstream parity: local entrypoint for the first implemented `ERC_TESTER` slice. This is not a
// 1:1 KiCad ERC runner because the current tree still lacks markers, rule matrices, and connection
// graph ownership, but it exists so ERC work can start from real upstream routines instead of
// ad-hoc checks. Remaining divergence is the broader unported `ERC_TESTER` surface beyond the
// implemented field-name-whitespace pass.
pub fn run(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_duplicate_sheet_names(project));
    diagnostics.extend(check_field_name_whitespace(project));
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
