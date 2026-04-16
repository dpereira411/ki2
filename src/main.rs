use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ki2::core::SchematicProject;
use ki2::diagnostic::Diagnostic;
use ki2::diagnostic::Severity;
use ki2::erc;
use ki2::loader::load_schematic_tree;
use ki2::model::SchItem;
use ki2::netlist::{render_reduced_kicad_netlist, render_reduced_xml_netlist};
use ki2::parser::parse_schematic_file;
use serde_json::json;

fn main() {
    raise_process_stack_limit_best_effort();
    let exit_code = run_cli_command();

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

#[derive(Debug)]
struct ErcCommandConfig {
    path: String,
    output: Option<String>,
    format: ErcOutputFormat,
    units: ErcReportUnits,
    severity_mask: ErcSeverityMask,
    exit_code_violations: bool,
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

// upstream: none
// parity_status: local-only
// local_kind: local-only-transitional
// divergence: KiCad's CLI does not need an explicit worker thread stack shim, but the local Rust
// upstream: none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: KiCad's job entrypoints run directly on the command thread; the local CLI now does
// the same again and relies only on narrower inner workers where exercised recursion still needs
// them
// local_only_reason: keeps the outer command path shallow and avoids an additional oversized
// worker boundary while deeper graph-owned work is still being flattened
// replaced_by: none
// remove_when: none
fn run_cli_command() -> i32 {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage_and_exit();
    };

    match command.as_str() {
        "validate" => run_validate_command(args.collect()),
        "erc" => run_erc_command(args.collect()),
        "netlist" => run_netlist_command(args.collect()),
        _ => {
            eprintln!("unknown command: {command}");
            print_usage_and_exit();
        }
    }
}

// upstream: none
// parity_status: local-only
// local_kind: local-only-transitional
// divergence: KiCad does not raise the process stack limit in-process; this best-effort Unix shim
// exists only to let the requested CLI worker stack budget take effect reliably on current large
// hierarchy fixtures
// local_only_reason: some exercised CLI runs still hit platform stack caps before the Rust worker
// thread can use the oversized stack requested by the compatibility shim
// replaced_by: none
// remove_when: the CLI worker no longer needs oversized stack requests and large hierarchy
// fixtures are stable without raising process stack limits
fn raise_process_stack_limit_best_effort() {
    #[cfg(unix)]
    unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };

        if libc::getrlimit(libc::RLIMIT_STACK, &mut limit) != 0 {
            return;
        }

        let target = limit.rlim_max;

        if target == libc::RLIM_INFINITY || target > limit.rlim_cur {
            let updated = libc::rlimit {
                rlim_cur: target,
                rlim_max: limit.rlim_max,
            };
            let _ = libc::setrlimit(libc::RLIMIT_STACK, &updated);
        }
    }
}

// upstream: none
// parity_status: local-only
// local_kind: local-only-transitional
// divergence: KiCad does not size worker stacks explicitly; this picks a bounded worker-stack
// request that fits within real host stack caps instead of requesting an oversized budget that can
// become unstable on hosts whose hard stack limit is much lower than the requested value
// local_only_reason: keeps compatibility-lane ERC worker threads reliable on exercised hierarchy
// fixtures while lower-layer recursion still needs an oversized stack relative to the normal CLI
// thread
// replaced_by: none
// remove_when: ERC no longer needs dedicated oversized worker threads on large hierarchy fixtures
fn erc_worker_stack_size(target_bytes: usize) -> usize {
    const CONSTRAINED_HOST_THRESHOLD_BYTES: usize = 96 * 1024 * 1024;
    const CONSTRAINED_HOST_HEADROOM_BYTES: usize = 512 * 1024;

    #[cfg(unix)]
    unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };

        if libc::getrlimit(libc::RLIMIT_STACK, &mut limit) == 0 {
            let hard_limit = limit.rlim_max;

            if hard_limit != libc::RLIM_INFINITY {
                let hard_limit = hard_limit as usize;

                if hard_limit <= CONSTRAINED_HOST_THRESHOLD_BYTES {
                    return target_bytes.min(hard_limit.saturating_sub(CONSTRAINED_HOST_HEADROOM_BYTES));
                }

                return target_bytes;
            }
        }
    }

    target_bytes
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

    execute_erc_command(ErcCommandConfig {
        path,
        output,
        format,
        units,
        severity_mask: ErcSeverityMask::from_flags(
            severity_all,
            severity_error,
            severity_warning,
            severity_exclusions,
        ),
        exit_code_violations,
    })
}

// upstream: SCH_ERC_COMMAND::doPerform or none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: still routes through the reduced local CLI/report stack instead of KiCad's job and
// marker pipeline, but this helper narrows the heavy execution frame so the exercised ERC command
// path can stay within current stack limits while lower-layer recursion is still being flattened
// local_only_reason: keeps the compatibility-lane `erc` command executable without adding another
// owner boundary; it only splits the command frame around the existing load/project/ERC/report flow
// replaced_by: none
// remove_when: the command can return to a flatter single-frame flow without reintroducing stack
// instability on large hierarchy fixtures
fn execute_erc_command(config: ErcCommandConfig) -> i32 {
    execute_erc_command_on_worker_stack(config)
}

// upstream: SCH_ERC_COMMAND::doPerform or none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: KiCad executes the command on the caller thread; this shim runs the full local
// load/project/ERC/report flow on one bounded worker stack because the current Rust path still
// needs an oversized CLI stack on exercised hierarchy fixtures
// local_only_reason: keeps the compatibility-lane `erc` command executable on constrained hosts
// without splitting the same deep flow across separate near-ceiling load/run workers
// replaced_by: flatter upstream-shaped parser/loader/connectivity/ERC ownership that no longer
// needs a dedicated oversized command worker
// remove_when: the ERC command is stable on the normal CLI thread for the large hierarchy
// fixtures
fn execute_erc_command_on_worker_stack(config: ErcCommandConfig) -> i32 {
    let stack_size = erc_worker_stack_size(64 * 1024 * 1024);

    std::thread::Builder::new()
        .name("ki2-erc-command".to_string())
        .stack_size(stack_size)
        .spawn(move || execute_erc_command_inner(config))
        .expect("spawn erc command worker")
        .join()
        .expect("erc command worker")
}

// upstream: SCH_ERC_COMMAND::doPerform or none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: still routes through the reduced local CLI/report stack instead of KiCad's job and
// marker pipeline, but this helper narrows the heavy execution frame under one worker-owned ERC
// command body while lower-layer recursion is still being flattened
// local_only_reason: keeps the compatibility-lane `erc` command executable without adding another
// owner boundary; it only splits the command frame around the existing load/project/ERC/report flow
// replaced_by: none
// remove_when: the command can return to a flatter single-frame flow without reintroducing stack
// instability on large hierarchy fixtures
fn execute_erc_command_inner(config: ErcCommandConfig) -> i32 {
    let loaded = match load_schematic_tree(Path::new(&config.path)) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };

    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);
    let filtered_diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|diagnostic| config.severity_mask.includes(diagnostic.severity))
        .collect();
    let output_path = erc_output_path(&config.path, config.output.as_deref(), config.format);
    let report = match config.format {
        ErcOutputFormat::Text => render_erc_text_report(
            Path::new(&config.path),
            &filtered_diagnostics,
            config.units,
            config.severity_mask,
        ),
        ErcOutputFormat::Json => render_erc_json_report(
            Path::new(&config.path),
            &filtered_diagnostics,
            config.units,
            config.severity_mask,
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

    if config.exit_code_violations && !filtered_diagnostics.is_empty() {
        1
    } else {
        0
    }
}

// Upstream parity: reduced local analogue for `EESCHEMA_JOBS_HANDLER::JobExportNetlist()`. This
// is not a 1:1 KiCad jobs/exporter path because the local CLI still exposes only reduced XML and
// reduced KiCad-format netlist slices instead of the full common exporter base and all exporter
// backends, but it now follows KiCad's default `KICADSEXPR` format/output-path branch, accepts
// KiCad job-format aliases (`kicadsexpr`, `kicadxml`), applies one selected current variant
// before export through the existing `SchematicProject` owner, and emits the exercised duplicate-
// sheet-name and annotation-warning branches before writing the netlist instead of keeping the
// command path silent.
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

    if has_netlist_annotation_errors(&project) {
        eprintln!(
            "Warning: schematic has annotation errors, please use the schematic editor to fix them"
        );
    }

    if !erc::check_duplicate_sheet_names(&project).is_empty() {
        eprintln!("Warning: duplicate sheet names.");
    }

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

// Upstream parity: reduced local analogue for the `SCH_REFERENCE_LIST::CheckAnnotation()` warning
// branch used by KiCad's netlist jobs handler. This is not a 1:1 reference-list owner because the
// Rust tree still lacks `SCH_REFERENCE_LIST` and its exact split/sort semantics, but it preserves
// the exercised export-time warning trigger for unannotated references, invalid selected units,
// duplicate same-unit references, differing unit-count references, and same-reference
// different-value mismatches across the loaded hierarchy. It now also resolves `Reference` /
// `Value` through the same occurrence-aware project/loader owner as netlist export instead of
// re-reading raw parser-owned symbol properties, so selected variants and reused-sheet occurrences
// participate in the warning path like the exporter output they precede.
fn has_netlist_annotation_errors(project: &SchematicProject) -> bool {
    #[derive(Clone)]
    struct AnnotationEntry {
        reference: String,
        unit: i32,
        value: String,
        unit_count: usize,
    }

    let mut by_reference = BTreeMap::<String, Vec<AnnotationEntry>>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            if !symbol.in_netlist {
                continue;
            }

            let Some(reference) =
                project.resolved_symbol_property_value(sheet_path, symbol, "Reference")
            else {
                continue;
            };

            if reference.contains('?') {
                return true;
            }

            let unit = symbol.unit.unwrap_or(1);
            let unit_count = symbol
                .lib_symbol
                .as_ref()
                .map(|lib_symbol| lib_symbol.units.len().max(1))
                .unwrap_or(1);

            if unit as usize > unit_count {
                return true;
            }

            let value = project
                .resolved_symbol_property_value(sheet_path, symbol, "Value")
                .unwrap_or_default();

            by_reference
                .entry(reference.to_ascii_uppercase())
                .or_default()
                .push(AnnotationEntry {
                    reference,
                    unit,
                    value,
                    unit_count,
                });
        }
    }

    for entries in by_reference.into_values() {
        for (index, first) in entries.iter().enumerate() {
            for second in entries.iter().skip(index + 1) {
                if !first.reference.eq_ignore_ascii_case(&second.reference) {
                    continue;
                }

                if first.unit == second.unit {
                    return true;
                }

                if first.unit_count != second.unit_count {
                    return true;
                }

                if first.value != second.value {
                    return true;
                }
            }
        }
    }

    false
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
