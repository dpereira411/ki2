use std::path::Path;

use ki2::core::SchematicProject;
use ki2::erc;
use ki2::loader::load_schematic_tree;
use ki2::parser::parse_schematic_file;

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

// Upstream parity: reduced local analogue for the current `kicad-cli sch` command dispatch. This
// is not a 1:1 command tree yet because the local binary still exposes flat subcommands instead of
// KiCad's full job/config layer, but it keeps the exercised validate and ERC paths on explicit
// command-owned argument parsing instead of ad-hoc test helpers.
fn print_usage_and_exit() -> ! {
    eprintln!("usage: ki2 <validate|erc> <path> [--tree]");
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
// report writer, severity filters, unit options, and JSON/text job outputs, but it now drives the
// live hierarchy loader and ERC engine through a real command boundary instead of test-only calls.
fn run_erc_command(args: Vec<String>) -> i32 {
    let mut path = None;

    for arg in args {
        if path.is_none() {
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

    if diagnostics.is_empty() { 0 } else { 1 }
}
