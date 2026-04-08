use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ki2::core::SchematicProject;
use ki2::diagnostic::Diagnostic;
use ki2::diagnostic::Severity;
use ki2::erc;
use ki2::loader::load_schematic_tree;
use ki2::netlist::{render_reduced_kicad_netlist, render_reduced_xml_netlist};
use ki2::parser::parse_schematic_file;
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage_and_exit();
    };

    let exit_code = match command.as_str() {
        "validate" => run_validate_command(args.collect()),
        "erc" => run_erc_command(args.collect()),
        "netlist" => run_netlist_command(args.collect()),
        _ => {
            eprintln!("unknown command: {command}");
            print_usage_and_exit();
        }
    };

    std::process::exit(exit_code);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErcOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErcReportUnits {
    Millimeters,
    Inches,
    Mils,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetlistOutputFormat {
    Xml,
    Kicad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ErcSeverityMask {
    errors: bool,
    warnings: bool,
    exclusions: bool,
}

impl ErcSeverityMask {
    // Upstream parity: reduced local analogue for `JOB_SCH_ERC` default severity selection. This
    // is not 1:1 because the local CLI still has no exclusion marker stream, but it preserves the
    // same default error+warning report set instead of treating every available diagnostic as
    // implicitly selected configuration.
    fn default_reported() -> Self {
        Self {
            errors: true,
            warnings: true,
            exclusions: false,
        }
    }

    // Upstream parity: reduced local analogue for `SCH_ERC_COMMAND::doPerform()` severity-flag
    // folding. This is not 1:1 because local diagnostics still lack exclusion items, but it keeps
    // the same `--severity-all` versus explicit-flag override shape instead of a repo-local
    // hand-picked filter policy.
    fn from_flags(
        severity_all: bool,
        severity_error: bool,
        severity_warning: bool,
        severity_exclusions: bool,
    ) -> Self {
        if severity_all {
            return Self {
                errors: true,
                warnings: true,
                exclusions: true,
            };
        }

        if severity_error || severity_warning || severity_exclusions {
            return Self {
                errors: severity_error,
                warnings: severity_warning,
                exclusions: severity_exclusions,
            };
        }

        Self::default_reported()
    }

    // Upstream parity: reduced local analogue for `SHEETLIST_ERC_ITEMS_PROVIDER::SetSeverities()`
    // filtering. This is not a 1:1 provider-backed marker path because the local CLI still
    // filters plain diagnostics after ERC, but it preserves the exercised error-vs-warning report
    // selection semantics needed by the job entrypoint.
    fn includes(self, severity: Severity) -> bool {
        match severity {
            Severity::Error => self.errors,
            Severity::Warning => self.warnings,
        }
    }

    // Upstream parity: reduced local analogue for `ERC_REPORT` included-severity metadata. This is
    // not 1:1 because the local CLI still lacks KiCad's full severity inventory, but it keeps the
    // report metadata keyed to the actual selected severity mask instead of emitting unlabeled
    // reduced reports.
    fn included_labels(self) -> Vec<&'static str> {
        let mut labels = Vec::new();

        if self.errors {
            labels.push("error");
        }

        if self.warnings {
            labels.push("warning");
        }

        if self.exclusions {
            labels.push("exclusion");
        }

        labels
    }
}

// Upstream parity: reduced local analogue for the current `kicad-cli sch` command dispatch. This
// is not a 1:1 command tree yet because the local binary still exposes flat subcommands instead of
// KiCad's full job/config layer, but it keeps the exercised validate and ERC paths on explicit
// command-owned argument parsing instead of ad-hoc test helpers.
fn print_usage_and_exit() -> ! {
    eprintln!("usage: ki2 validate <path> [--tree]");
    eprintln!(
        "       ki2 erc <path> [--output <path>] [--format <report|json>] [--units <in|mm|mils>]"
    );
    eprintln!(
        "               [--severity-all] [--severity-error] [--severity-warning] [--severity-exclusions]"
    );
    eprintln!("               [--exit-code-violations]");
    eprintln!(
        "       ki2 netlist <path> [--output <path>] [--format <kicad|kicadsexpr|xml|kicadxml>] [--variant <name>]"
    );
    std::process::exit(2);
}

// Upstream parity: reduced local analogue for the schematic-validate CLI entrypoint. This is not
// 1:1 with KiCad's jobs handler because the local binary still lacks the full `sch` command tree
// and report/output options, but it preserves the current parser-vs-loader split through `--tree`.
fn run_validate_command(args: Vec<String>) -> i32 {
    let mut tree = false;
    let mut path = None;

    for arg in args {
        if arg == "--tree" {
            tree = true;
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("unexpected argument: {arg}");
            print_usage_and_exit();
        }
    }

    let Some(path) = path else {
        print_usage_and_exit();
    };

    let result = if tree {
        load_schematic_tree(Path::new(&path)).map(|loaded| loaded.schematics.len())
    } else {
        parse_schematic_file(Path::new(&path)).map(|_| 1usize)
    };

    match result {
        Ok(count) => {
            println!("validated {count} schematic(s)");
            0
        }
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

// Upstream parity: reduced local analogue for KiCad's schematic ERC CLI entrypoint. This is not
// 1:1 with `EESCHEMA_JOBS_HANDLER::JobSchErc()` because the local binary still lacks KiCad's
// full settings layer and exclusion markers, but it now follows the same command-owned flag flow
// for format, units, reported severities, and exit-code behavior while still emitting reduced
// text/JSON reports instead of KiCad's full report schemas.
fn run_erc_command(args: Vec<String>) -> i32 {
    let mut path = None;
    let mut output = None;
    let mut format = ErcOutputFormat::Text;
    let mut units = ErcReportUnits::Millimeters;
    let mut severity_all = false;
    let mut severity_error = false;
    let mut severity_warning = false;
    let mut severity_exclusions = false;
    let mut exit_code_violations = false;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        if arg == "--output" {
            let Some(value) = args.next() else {
                print_usage_and_exit();
            };
            output = Some(value);
        } else if arg == "--format" {
            let Some(value) = args.next() else {
                print_usage_and_exit();
            };
            format = match value.as_str() {
                "report" | "text" => ErcOutputFormat::Text,
                "json" => ErcOutputFormat::Json,
                _ => {
                    eprintln!("invalid report format");
                    return 2;
                }
            };
        } else if arg == "--units" {
            let Some(value) = args.next() else {
                print_usage_and_exit();
            };
            units = match value.as_str() {
                "mm" => ErcReportUnits::Millimeters,
                "in" => ErcReportUnits::Inches,
                "mils" => ErcReportUnits::Mils,
                _ => {
                    eprintln!("invalid units specified");
                    return 2;
                }
            };
        } else if arg == "--severity-all" {
            severity_all = true;
        } else if arg == "--severity-error" {
            severity_error = true;
        } else if arg == "--severity-warning" {
            severity_warning = true;
        } else if arg == "--severity-exclusions" {
            severity_exclusions = true;
        } else if arg == "--exit-code-violations" {
            exit_code_violations = true;
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("unexpected argument: {arg}");
            print_usage_and_exit();
        }
    }

    let Some(path) = path else {
        print_usage_and_exit();
    };

    let loaded = match load_schematic_tree(Path::new(&path)) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };

    let severity_mask = ErcSeverityMask::from_flags(
        severity_all,
        severity_error,
        severity_warning,
        severity_exclusions,
    );

    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);
    let filtered_diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|diagnostic| severity_mask.includes(diagnostic.severity))
        .collect();
    let output_path = erc_output_path(&path, output.as_deref(), format);
    let report = match format {
        ErcOutputFormat::Text => render_erc_text_report(
            Path::new(&path),
            &filtered_diagnostics,
            units,
            severity_mask,
        ),
        ErcOutputFormat::Json => render_erc_json_report(
            Path::new(&path),
            &filtered_diagnostics,
            units,
            severity_mask,
        ),
    };

    for diagnostic in &filtered_diagnostics {
        println!(
            "{}:{}:{}: {} [{}]",
            diagnostic
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string()),
            diagnostic.line.unwrap_or(0),
            diagnostic.column.unwrap_or(0),
            diagnostic.message,
            diagnostic.code
        );
    }

    println!("found {} violations", filtered_diagnostics.len());
    println!("saved ERC report to {}", output_path.display());

    if let Err(err) = fs::write(&output_path, report) {
        eprintln!("failed to write ERC report: {err}");
        return 1;
    }

    if exit_code_violations && !filtered_diagnostics.is_empty() {
        1
    } else {
        0
    }
}

// Upstream parity: reduced local analogue for `EESCHEMA_JOBS_HANDLER::JobExportNetlist()`. This
// is not a 1:1 KiCad jobs/exporter path because the local CLI still exposes only reduced XML and
// reduced KiCad-format netlist slices instead of the full common exporter base and all exporter
// backends, but it now follows KiCad's default `KICADSEXPR` format/output-path branch, accepts
// KiCad job-format aliases (`kicadsexpr`, `kicadxml`), and applies one selected current variant
// before export through the existing `SchematicProject` owner instead of hard-coding one reduced
// unparameterized export mode.
fn run_netlist_command(args: Vec<String>) -> i32 {
    let mut path = None;
    let mut output = None;
    let mut format = NetlistOutputFormat::Kicad;
    let mut variant = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        if arg == "--output" {
            let Some(value) = args.next() else {
                print_usage_and_exit();
            };
            output = Some(value);
        } else if arg == "--variant" {
            let Some(value) = args.next() else {
                print_usage_and_exit();
            };
            variant = Some(value);
        } else if arg == "--format" {
            let Some(value) = args.next() else {
                print_usage_and_exit();
            };
            format = match value.as_str() {
                "xml" | "kicadxml" => NetlistOutputFormat::Xml,
                "kicad" | "kicadsexpr" => NetlistOutputFormat::Kicad,
                _ => {
                    eprintln!("invalid netlist format");
                    return 2;
                }
            };
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("unexpected argument: {arg}");
            print_usage_and_exit();
        }
    }

    let Some(path) = path else {
        print_usage_and_exit();
    };

    let path = PathBuf::from(path);
    let output_path = output.map(PathBuf::from).unwrap_or_else(|| match format {
        NetlistOutputFormat::Xml => path.with_extension("xml"),
        NetlistOutputFormat::Kicad => path.with_extension("net"),
    });
    let loaded = match load_schematic_tree(&path) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    let mut project = SchematicProject::from_load_result(loaded);
    let selected_variant = variant
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("all"));
    project.set_current_variant(selected_variant);
    let content = match format {
        NetlistOutputFormat::Xml => render_reduced_xml_netlist(&project),
        NetlistOutputFormat::Kicad => render_reduced_kicad_netlist(&project),
    };

    if let Err(err) = fs::write(&output_path, content) {
        eprintln!(
            "failed to write netlist '{}': {}",
            output_path.display(),
            err
        );
        return 1;
    }

    println!("wrote netlist {}", output_path.display());
    0
}

// Upstream parity: reduced local analogue for `JOB_SCH_ERC` default output-path handling. This is
// not 1:1 because the local CLI only supports reduced text/JSON report shapes, but it now follows
// the exercised default output naming instead of forcing stdout-only ERC output.
fn erc_output_path(
    input_path: &str,
    override_path: Option<&str>,
    format: ErcOutputFormat,
) -> PathBuf {
    if let Some(path) = override_path {
        return PathBuf::from(path);
    }

    let mut output = PathBuf::from(input_path);
    let stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("erc");
    let extension = match format {
        ErcOutputFormat::Text => "rpt",
        ErcOutputFormat::Json => "json",
    };
    output.set_file_name(format!("{stem}-erc.{extension}"));
    output
}

// Upstream parity: reduced local analogue for `EESCHEMA_JOBS_HANDLER::JobSchErc()` unit handling.
// This is not a 1:1 KiCad units-provider path because the current text diagnostics do not render
// coordinates in report units, but the command now preserves KiCad's report-unit selection in the
// written report metadata instead of dropping the option entirely.
fn erc_units_label(units: ErcReportUnits) -> &'static str {
    match units {
        ErcReportUnits::Millimeters => "mm",
        ErcReportUnits::Inches => "in",
        ErcReportUnits::Mils => "mils",
    }
}

// Upstream parity: reduced local analogue for `ERC_REPORT` sheet-path ordering. This is not 1:1
// because the local report still groups by diagnostic source path instead of KiCad sheet objects,
// but it preserves the exercised report-owned sheet bucketing instead of leaving all violations in
// one flat unordered stream.
fn group_diagnostics_by_path<'a>(
    diagnostics: &'a [Diagnostic],
) -> Vec<(String, Vec<&'a Diagnostic>)> {
    let mut groups: BTreeMap<String, Vec<&Diagnostic>> = BTreeMap::new();

    for diagnostic in diagnostics {
        let key = diagnostic
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        groups.entry(key).or_default().push(diagnostic);
    }

    groups.into_iter().collect()
}

// Upstream parity: reduced local analogue for KiCad's ERC text-report writer. This is not 1:1
// with `ERC_REPORT` because the local command still lacks KiCad's ignored-check section and true
// sheet-object ordering, but it now groups violations by sheet path and reports severity totals
// instead of emitting one flat reduced list.
fn render_erc_text_report(
    input_path: &Path,
    diagnostics: &[Diagnostic],
    units: ErcReportUnits,
    severity_mask: ErcSeverityMask,
) -> String {
    let mut out = String::new();
    let grouped = group_diagnostics_by_path(diagnostics);
    let error_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();

    out.push_str(&format!(
        "ERC report ({}, Encoding UTF8)\n",
        input_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "Report includes: {}\n",
        severity_mask.included_labels().join(", ")
    ));
    out.push_str(&format!("Coordinate units: {}\n", erc_units_label(units)));

    for (sheet_path, items) in grouped {
        out.push_str(&format!("\n***** Sheet {sheet_path}\n"));

        for diagnostic in items {
            let path = diagnostic
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            let line = diagnostic.line.unwrap_or(0);
            let column = diagnostic.column.unwrap_or(0);
            out.push_str(&format!(
                "{path}:{line}:{column}: {} [{}]\n",
                diagnostic.message, diagnostic.code
            ));
        }
    }

    out.push_str(&format!(
        "\n ** ERC messages: {}  Errors {}  Warnings {}\n",
        diagnostics.len(),
        error_count,
        warning_count
    ));
    out.push_str(&format!("found {} violations\n", diagnostics.len()));
    out
}

// Upstream parity: reduced local analogue for KiCad's JSON ERC report path. This is not 1:1 with
// KiCad's JSON schema because the local command still lacks UUID sheet paths and ignored-check
// detail, but it now emits sheet-grouped violations and severity totals like the upstream report
// path instead of only a flat reduced list.
fn render_erc_json_report(
    input_path: &Path,
    diagnostics: &[Diagnostic],
    units: ErcReportUnits,
    severity_mask: ErcSeverityMask,
) -> String {
    let grouped = group_diagnostics_by_path(diagnostics);
    let error_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();

    serde_json::to_string_pretty(&json!({
        "source": input_path.file_name().and_then(|name| name.to_str()),
        "coordinate_units": erc_units_label(units),
        "included_severities": severity_mask.included_labels(),
        "sheets": grouped.iter().map(|(sheet_path, items)| {
            json!({
                "path": sheet_path,
                "violations": items.iter().map(|diagnostic| {
                    json!({
                        "path": diagnostic.path.as_ref().map(|path| path.display().to_string()),
                        "line": diagnostic.line,
                        "column": diagnostic.column,
                        "message": diagnostic.message,
                        "code": diagnostic.code,
                        "kind": format!("{:?}", diagnostic.kind),
                        "severity": format!("{:?}", diagnostic.severity),
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "violations": diagnostics.iter().map(|diagnostic| {
            json!({
                "path": diagnostic.path.as_ref().map(|path| path.display().to_string()),
                "line": diagnostic.line,
                "column": diagnostic.column,
                "message": diagnostic.message,
                "code": diagnostic.code,
                "kind": format!("{:?}", diagnostic.kind),
                "severity": format!("{:?}", diagnostic.severity),
            })
        }).collect::<Vec<_>>(),
        "error_count": error_count,
        "warning_count": warning_count,
        "ignored_checks": Vec::<serde_json::Value>::new(),
        "violation_count": diagnostics.len(),
    }))
    .expect("erc diagnostics json")
}
