use std::fs;
use std::path::{Path, PathBuf};

use ki2::core::SchematicProject;
use ki2::diagnostic::Diagnostic;
use ki2::erc;
use ki2::loader::load_schematic_tree;
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

// Upstream parity: reduced local analogue for the current `kicad-cli sch` command dispatch. This
// is not a 1:1 command tree yet because the local binary still exposes flat subcommands instead of
// KiCad's full job/config layer, but it keeps the exercised validate and ERC paths on explicit
// command-owned argument parsing instead of ad-hoc test helpers.
fn print_usage_and_exit() -> ! {
    eprintln!("usage: ki2 validate <path> [--tree]");
    eprintln!("       ki2 erc <path> [--output <path>] [--format <text|json>]");
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
// report writer, severity filters, and unit options, but it now drives the live hierarchy loader
// and ERC engine through a real command boundary and emits reduced text/JSON reports instead of
// staying stdout-only.
fn run_erc_command(args: Vec<String>) -> i32 {
    let mut path = None;
    let mut output = None;
    let mut format = ErcOutputFormat::Text;
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
                "text" => ErcOutputFormat::Text,
                "json" => ErcOutputFormat::Json,
                _ => print_usage_and_exit(),
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

    let loaded = match load_schematic_tree(Path::new(&path)) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };

    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);
    let output_path = erc_output_path(&path, output.as_deref(), format);
    let report = match format {
        ErcOutputFormat::Text => render_erc_text_report(&diagnostics),
        ErcOutputFormat::Json => render_erc_json_report(&diagnostics),
    };

    for diagnostic in &diagnostics {
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

    println!("found {} violations", diagnostics.len());
    println!("saved ERC report to {}", output_path.display());

    if let Err(err) = fs::write(&output_path, report) {
        eprintln!("failed to write ERC report: {err}");
        return 1;
    }

    if diagnostics.is_empty() { 0 } else { 1 }
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

// Upstream parity: reduced local analogue for KiCad's ERC text-report writer. This is not 1:1
// with `ERC_REPORT` because the local command still lacks KiCad's units/settings/report sections,
// but it now writes a stable text artifact instead of leaving ERC results ephemeral on stdout.
fn render_erc_text_report(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();

    for diagnostic in diagnostics {
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

    out.push_str(&format!("found {} violations\n", diagnostics.len()));
    out
}

// Upstream parity: reduced local analogue for KiCad's JSON ERC report path. This is not 1:1 with
// KiCad's JSON schema because the local command still emits only the exercised diagnostic fields,
// but it now supports the same broad text-vs-JSON report split as the upstream ERC job.
fn render_erc_json_report(diagnostics: &[Diagnostic]) -> String {
    serde_json::to_string_pretty(&json!({
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
        "violation_count": diagnostics.len(),
    }))
    .expect("erc diagnostics json")
}
