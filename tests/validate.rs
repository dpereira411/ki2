use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use ki2::core::SchematicProject;
use ki2::diagnostic::DiagnosticKind;
use ki2::erc;
use ki2::error::Error;
use ki2::loader::{DrawingSheetSource, load_schematic_tree};
use ki2::model::{
    EmbeddedFileType, FieldAutoplacement, FillType, Group, LabelKind, LabelShape, LabelSpin,
    LineKind, MirrorAxis, PropertyKind, ResolvedSimLibrary, ResolvedSimModelKind, SchItem,
    ShapeKind, SheetPinShape, SheetSide, SimLibraryKind, SimLibrarySource, SimModelOrigin,
    SimValueBinding, StrokeStyle, TextHJustify, TextKind, TextVJustify,
};
use ki2::netlist::render_reduced_xml_netlist;
use ki2::parser::parse_schematic_file;
use ki2::sim::{
    SimLibraryContent, classify_symbol_sim_library_kind, collect_symbol_sim_library_sources,
    load_symbol_sim_library_content, resolve_symbol_sim_library, resolve_symbol_sim_library_source,
    resolve_symbol_sim_model,
};
use uuid::Uuid;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(path)
}

fn temp_schematic(name: &str, src: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = env::temp_dir().join(format!("{name}_{nanos}.kicad_sch"));
    fs::write(&path, src).expect("write temp schematic");
    path
}

fn ki2_binary() -> &'static str {
    env!("CARGO_BIN_EXE_ki2")
}

fn init_git_repo_with_head(dir: &Path) -> (String, String) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run git");
        assert!(status.success(), "git {:?} must succeed", args);
    };

    run(&["init"]);
    run(&["config", "user.name", "ki2"]);
    run(&["config", "user.email", "ki2@example.com"]);
    run(&["add", "."]);
    run(&["commit", "-m", "fixture"]);

    let full_hash = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .stderr(Stdio::null())
        .output()
        .expect("read full hash");
    assert!(
        full_hash.status.success(),
        "git rev-parse HEAD must succeed"
    );
    let short_hash = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--short", "HEAD"])
        .stderr(Stdio::null())
        .output()
        .expect("read short hash");
    assert!(
        short_hash.status.success(),
        "git rev-parse --short HEAD must succeed"
    );

    (
        String::from_utf8(full_hash.stdout)
            .expect("utf8 full hash")
            .trim()
            .to_string(),
        String::from_utf8(short_hash.stdout)
            .expect("utf8 short hash")
            .trim()
            .to_string(),
    )
}

fn env_lock() -> MutexGuard<'static, ()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

fn temp_dir_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    env::temp_dir().join(format!("{name}_{nanos}"))
}

#[test]
fn cli_erc_reports_clean_schematic() {
    let path = temp_schematic(
        "cli_erc_clean",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4"))"#,
    );
    let report_path = path.with_file_name(format!(
        "{}-erc.rpt",
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .expect("report stem")
    ));

    let output = Command::new(ki2_binary())
        .args(["erc", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 erc");

    assert!(
        output.status.success(),
        "erc must succeed on clean schematic"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("found 0 violations"), "{stdout}");
    assert!(
        stdout.contains(report_path.to_str().expect("report path")),
        "{stdout}"
    );
    let report = fs::read_to_string(&report_path).expect("read report");
    assert!(
        report.contains("** ERC messages: 0  Errors 0  Warnings 0"),
        "{report}"
    );
    assert!(report.contains("found 0 violations"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_erc_reports_violations() {
    let path = temp_schematic(
        "cli_erc_violation",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (text "${ERC_ERROR worksheet-like failure}" (at 1 2 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("erc.txt");

    let output = Command::new(ki2_binary())
        .args([
            "erc",
            path.to_str().expect("path string"),
            "--output",
            report_path.to_str().expect("report path"),
        ])
        .output()
        .expect("run ki2 erc");

    assert!(
        output.status.success(),
        "erc must succeed without --exit-code-violations"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("worksheet-like failure"), "{stdout}");
    assert!(stdout.contains("found 2 violations"), "{stdout}");
    assert!(
        stdout.contains(report_path.to_str().expect("report path")),
        "{stdout}"
    );
    let report = fs::read_to_string(&report_path).expect("read report");
    assert!(
        report.contains("Report includes: error, warning"),
        "{report}"
    );
    assert!(report.contains("Coordinate units: mm"), "{report}");
    assert!(report.contains("***** Sheet"), "{report}");
    assert!(
        report.contains("** ERC messages: 2  Errors 2  Warnings 0"),
        "{report}"
    );
    assert!(report.contains("worksheet-like failure"), "{report}");
    assert!(report.contains("found 2 violations"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_erc_writes_json_report() {
    let path = temp_schematic(
        "cli_erc_json",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (text "${ERC_ERROR json failure}" (at 1 2 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("erc.json");

    let output = Command::new(ki2_binary())
        .args([
            "erc",
            path.to_str().expect("path string"),
            "--format",
            "json",
            "--output",
            report_path.to_str().expect("report path"),
        ])
        .output()
        .expect("run ki2 erc");

    assert!(
        output.status.success(),
        "erc must succeed without --exit-code-violations"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("found 2 violations"), "{stdout}");
    let report = fs::read_to_string(&report_path).expect("read report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("json report");
    assert_eq!(
        json["source"],
        path.file_name().and_then(|name| name.to_str()).unwrap()
    );
    assert_eq!(json["coordinate_units"], "mm");
    assert_eq!(
        json["included_severities"],
        serde_json::json!(["error", "warning"])
    );
    assert_eq!(json["error_count"], 2);
    assert_eq!(json["warning_count"], 0);
    assert_eq!(json["ignored_checks"], serde_json::json!([]));
    assert_eq!(json["violation_count"], 2);
    assert_eq!(json["violations"][0]["code"], "erc-generic-error");
    assert_eq!(
        json["sheets"][0]["violations"][0]["code"],
        "erc-generic-error"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_erc_exit_code_violations_matches_flag() {
    let path = temp_schematic(
        "cli_erc_exit_code_violations",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (text "${ERC_ERROR exit code failure}" (at 1 2 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("erc.rpt");

    let output = Command::new(ki2_binary())
        .args([
            "erc",
            path.to_str().expect("path string"),
            "--exit-code-violations",
            "--output",
            report_path.to_str().expect("report path"),
        ])
        .output()
        .expect("run ki2 erc");

    assert!(
        !output.status.success(),
        "erc must exit nonzero when --exit-code-violations is set and violations exist"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_erc_filters_reported_severities() {
    let path = temp_schematic(
        "cli_erc_severity_filter",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (text "${ERC_WARNING warning only}" (at 1 2 0) (effects (font (size 1 1))))
  (text "${ERC_ERROR error only}" (at 3 4 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("erc.rpt");

    let output = Command::new(ki2_binary())
        .args([
            "erc",
            path.to_str().expect("path string"),
            "--severity-warning",
            "--units",
            "in",
            "--format",
            "report",
            "--output",
            report_path.to_str().expect("report path"),
        ])
        .output()
        .expect("run ki2 erc");

    assert!(output.status.success(), "warning-only erc must succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("warning only"), "{stdout}");
    assert!(!stdout.contains("error only"), "{stdout}");
    assert!(stdout.contains("found 1 violations"), "{stdout}");

    let report = fs::read_to_string(&report_path).expect("read report");
    assert!(report.contains("Report includes: warning"), "{report}");
    assert!(report.contains("Coordinate units: in"), "{report}");
    assert!(report.contains("***** Sheet"), "{report}");
    assert!(
        report.contains("** ERC messages: 1  Errors 0  Warnings 1"),
        "{report}"
    );
    assert!(report.contains("warning only"), "{report}");
    assert!(!report.contains("error only"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_writes_reduced_xml_by_default() {
    let path = temp_schematic(
        "cli_netlist_xml",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (title_block
    (title "Main ${PROJECTNAME}")
    (company "OpenAI")
    (rev "A")
    (date "2026-04-07")
    (comment 1 "Build ${PROJECTNAME}"))
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1))))
    (property "Footprint" "Resistor_SMD:R_0603" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_IN" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "NET_OUT" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("xml");
    let project_path = path.with_extension("kicad_pro");
    fs::write(
        &project_path,
        r#"{
  "meta": { "version": 1 },
  "text_variables": { "PROJECTNAME": "CliNetlist" }
}"#,
    )
    .expect("write project");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains(report_path.to_str().expect("report path")));

    let report = fs::read_to_string(&report_path).expect("read netlist");
    let expected_project_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("project stem");
    assert!(report.contains("<export version=\"E\">"), "{report}");
    assert!(report.contains("<design>"), "{report}");
    let expected_source_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("source file name");
    assert!(
        report.contains(&format!("<source>/")) && report.contains(expected_source_name),
        "{report}"
    );
    assert!(report.contains("<tool>Eeschema 0.1.0</tool>"), "{report}");
    assert!(
        report.contains("<textvar name=\"PROJECTNAME\">CliNetlist</textvar>"),
        "{report}"
    );
    assert!(
        report.contains("<sheet number=\"1\" name=\"/\" tstamps=\"/\">"),
        "{report}"
    );
    assert!(
        report.contains(&format!("<title>Main {expected_project_name}</title>")),
        "{report}"
    );
    assert!(report.contains("<company>OpenAI</company>"), "{report}");
    assert!(report.contains("<rev>A</rev>"), "{report}");
    assert!(report.contains("<date>2026-04-07</date>"), "{report}");
    assert!(
        report.contains(&format!(
            "<comment number=\"1\" value=\"Build {expected_project_name}\" />"
        )),
        "{report}"
    );
    assert!(report.contains("<components>"), "{report}");
    assert!(report.contains("<comp ref=\"R1\">"), "{report}");
    assert!(report.contains("<value>10k</value>"), "{report}");
    assert!(
        report.contains("<footprint>Resistor_SMD:R_0603</footprint>"),
        "{report}"
    );
    assert!(report.contains("lib=\"Device\""), "{report}");
    assert!(report.contains("part=\"R\""), "{report}");
    assert!(report.contains("description=\"\""), "{report}");
    assert!(report.contains("<libparts>"), "{report}");
    assert!(report.contains("<libraries>"), "{report}");
    assert!(
        report.contains("<libpart lib=\"Device\" part=\"R\">"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Reference\">R</field>"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Value\">R</field>"),
        "{report}"
    );
    let reference_field = report
        .find("<field name=\"Reference\">R</field>")
        .expect("reference field");
    let value_field = report
        .find("<field name=\"Value\">R</field>")
        .expect("value field");
    let footprint_field = report
        .find("<field name=\"Footprint\"></field>")
        .expect("footprint field");
    assert!(
        reference_field < value_field && value_field < footprint_field,
        "{report}"
    );
    assert!(report.contains("<pins>"), "{report}");
    assert!(
        report.contains("<pin num=\"1\" name=\"~\" type=\"passive\" />"),
        "{report}"
    );
    assert!(
        report.contains("<pin num=\"2\" name=\"~\" type=\"passive\" />"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Footprint\">Resistor_SMD:R_0603</field>"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Datasheet\"></field>"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Description\"></field>"),
        "{report}"
    );
    assert!(report.contains("<nets>"), "{report}");
    assert!(report.contains("name=\"NET_IN\""), "{report}");
    assert!(report.contains("name=\"NET_OUT\""), "{report}");
    assert!(report.contains("<node ref=\"R1\" pin=\"1\""), "{report}");
    assert!(report.contains("<node ref=\"R1\" pin=\"2\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
    let _ = fs::remove_file(project_path);
}

#[test]
fn cli_netlist_uses_schematic_lib_name_in_libsource() {
    let path = temp_schematic(
        "cli_netlist_lib_name_libsource",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "LOCAL"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "LOCAL_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (wire (pts (xy -5 0) (xy 0 0)))
  (wire (pts (xy 10 0) (xy 15 0)))
  (label "NET_IN" (at -5 0 0) (effects (font (size 1 1))))
  (label "NET_OUT" (at 15 0 0) (effects (font (size 1 1))))
  (symbol
    (lib_id "Device:R")
    (lib_name "LOCAL")
    (at 5 0 0)
    (property "Reference" "R1" (at 5 -2 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 5 2 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains("<libsource lib=\"\" part=\"LOCAL\""),
        "{report}"
    );
    assert!(
        report.contains("<libpart lib=\"Device\" part=\"R\">"),
        "{report}"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_sorts_libpart_pins_by_str_num_cmp() {
    let path = temp_schematic(
        "cli_netlist_libpart_pin_order",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:ODD"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "ODD" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "ODD_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "P2" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "P10" (effects (font (size 1 1))))
          (number "10" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:ODD")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "ODD" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    let pin_two = report
        .find("<pin num=\"2\" name=\"P2\" type=\"passive\" />")
        .expect("pin 2");
    let pin_ten = report
        .find("<pin num=\"10\" name=\"P10\" type=\"passive\" />")
        .expect("pin 10");
    assert!(pin_two < pin_ten, "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_prefers_sorted_connected_label_name() {
    let path = temp_schematic(
        "cli_netlist_label_driver_order",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (label "B_NET" (at -5 0 0))
  (label "A_NET" (at -10 0 0))
  (wire (pts (xy 10 0) (xy 20 0)))
  (label "OUT" (at 20 0 0)))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("name=\"/A_NET\""), "{report}");
    assert!(!report.contains("name=\"/B_NET\""), "{report}");
    assert!(report.contains("name=\"/OUT\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_uses_power_symbol_value_as_net_name() {
    let path = temp_schematic(
        "cli_netlist_power_symbol_driver",
        r##"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1)))))))
    (symbol "power:VCC"
      (power)
      (property "Reference" "#PWR" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "VCC" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "VCC_1_1"
        (pin power_in line (at 0 0 180) (length 2.54)
          (name "VCC" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "power:VCC")
    (at -10 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at -10 0 0) (effects (font (size 1 1))))
    (property "Value" "VCC" (at -10 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy -10 0) (xy 0 0)))
  (wire (pts (xy 10 0) (xy 20 0)))
  (label "OUT" (at 20 0 0)))"##,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("name=\"VCC\""), "{report}");
    assert!(report.contains("name=\"/OUT\""), "{report}");
    assert!(report.contains("<node ref=\"R1\" pin=\"1\""), "{report}");
    assert!(!report.contains("<node ref=\"#PWR1\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_skips_power_only_nets_without_renumbering_remaining_codes() {
    let path = temp_schematic(
        "cli_netlist_skip_power_only_net",
        r##"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1)))))))
    (symbol "power:VCC"
      (power)
      (property "Reference" "#PWR" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "VCC" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "VCC_1_1"
        (pin power_in line (at 0 0 180) (length 2.54)
          (name "VCC" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "power:VCC")
    (at -10 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at -10 0 0) (effects (font (size 1 1))))
    (property "Value" "VCC" (at -10 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:R")
    (at 0 20 0)
    (unit 1)
    (property "Reference" "R1" (at 0 20 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 20 0) (effects (font (size 1 1)))))
  (wire (pts (xy -10 0) (xy -5 0)))
  (wire (pts (xy -5 0) (xy 0 0)))
  (wire (pts (xy 0 20) (xy -10 20)))
  (wire (pts (xy 10 20) (xy 20 20)))
  (label "USER_NET" (at -10 20 0))
  (label "OUT" (at 20 20 0)))"##,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(!report.contains("<node ref=\"#PWR1\""), "{report}");
    assert!(!report.contains("<net code=\"1\" name=\"VCC\""), "{report}");
    assert!(
        report.contains("<net code=\"2\" name=\"/USER_NET\""),
        "{report}"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_uses_default_symbol_pin_net_name() {
    let path = temp_schematic(
        "cli_netlist_default_pin_driver",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:R")
    (at 20 0 180)
    (unit 1)
    (property "Reference" "R2" (at 20 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 20 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 10 0) (xy 20 0))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("name=\"Net-(R1-Pad2)\""), "{report}");
    assert!(report.contains("<node ref=\"R1\" pin=\"2\""), "{report}");
    assert!(report.contains("<node ref=\"R2\" pin=\"1\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_prefers_user_net_for_duplicate_pin_numbers() {
    let path = temp_schematic(
        "cli_netlist_duplicate_pin_preference",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DUP"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "DUP" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "DUP_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 0 10 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:DUP")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "DUP" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy -10 0) (xy 0 0)))
  (global_label "USER_NET" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy -10 10) (xy 0 10))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains("<net code=\"1\" name=\"USER_NET\""),
        "{report}"
    );
    assert!(
        report.contains("<node ref=\"U1\" pin=\"1\" pinfunction=\"A_1\""),
        "{report}"
    );
    assert!(!report.contains("Net-(U1-Pad1)"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_expands_stacked_pin_notation() {
    let path = temp_schematic(
        "cli_netlist_stacked_pins",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:STACK"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "STACK" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "STACK_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "IO" (effects (font (size 1 1))))
          (number "[1,2]" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:STACK")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "STACK" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy -10 0) (xy 0 0)))
  (global_label "STACKED" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");

    assert!(
        report.contains("<pin num=\"1\" name=\"IO\" type=\"passive\" />"),
        "{report}"
    );
    assert!(
        report.contains("<pin num=\"2\" name=\"IO\" type=\"passive\" />"),
        "{report}"
    );
    assert!(
        report.contains("<node ref=\"U1\" pin=\"1\" pinfunction=\"IO_1\""),
        "{report}"
    );
    assert!(
        report.contains("<node ref=\"U1\" pin=\"2\" pinfunction=\"IO_2\""),
        "{report}"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_marks_single_no_connect_nodes() {
    let path = temp_schematic(
        "cli_netlist_no_connect",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:IN"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "IN" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "IN_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "IN" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:IN")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "IN" (at 0 0 0) (effects (font (size 1 1)))))
  (no_connect (at 0 0)))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("pintype=\"input+no_connect\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_marks_stacked_no_connect_nodes() {
    let path = temp_schematic(
        "cli_netlist_stacked_no_connect",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:STACK"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "STACK" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "STACK_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "IN" (effects (font (size 1 1))))
          (number "[1,2]" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:STACK")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "STACK" (at 0 0 0) (effects (font (size 1 1)))))
  (no_connect (at 0 0)))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains(
            "<node ref=\"U1\" pin=\"1\" pinfunction=\"IN_1\" pintype=\"input+no_connect\" />"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "<node ref=\"U1\" pin=\"2\" pinfunction=\"IN_2\" pintype=\"input+no_connect\" />"
        ),
        "{report}"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_omits_pinfunction_for_single_unnamed_pins() {
    let path = temp_schematic(
        "cli_netlist_unnamed_pinfunction",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains("<node ref=\"R1\" pin=\"1\" pintype=\"passive\" />"),
        "{report}"
    );
    assert!(!report.contains("pinfunction=\"~\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_sorts_components_and_nets_with_strnumcmp_ordering() {
    let path = temp_schematic(
        "cli_netlist_strnumcmp_ordering",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (property "Reference" "R10" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:R")
    (at 30 0 0)
    (property "Reference" "R2" (at 30 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 30 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy -10 0) (xy 0 0)))
  (global_label "NET10" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (wire (pts (xy 20 0) (xy 30 0)))
  (wire (pts (xy 40 0) (xy 50 0)))
  (global_label "NET2" (shape input) (at 50 0 180) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");

    let r2_component = report
        .find("<comp ref=\"R2\">")
        .expect("R2 component in xml");
    let r10_component = report
        .find("<comp ref=\"R10\">")
        .expect("R10 component in xml");
    assert!(r2_component < r10_component, "{report}");

    let net2 = report
        .find("<net code=\"1\" name=\"NET2\"")
        .expect("NET2 in xml");
    let net10 = report
        .find("<net code=\"2\" name=\"NET10\"")
        .expect("NET10 in xml");
    assert!(net2 < net10, "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_exports_component_metadata_properties() {
    let path = temp_schematic(
        "cli_netlist_component_metadata",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:JUMPER"
      (duplicate_pin_numbers_are_jumpers yes)
      (jumper_pin_groups ("1" "2") ("3" "4"))
      (property "Reference" "JP" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "JUMPER" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (property "ki_keywords" "analog precision")
      (property "ki_fp_filters" "R_* 0603")
      (symbol "JUMPER_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:JUMPER")
    (at 0 0 0)
    (in_bom no)
    (on_board no)
    (dnp yes)
    (property "Reference" "JP1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "JUMPER" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains("<property name=\"exclude_from_bom\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"exclude_from_board\" />"),
        "{report}"
    );
    assert!(report.contains("<property name=\"dnp\" />"), "{report}");
    assert!(
        report.contains("<property name=\"ki_keywords\" value=\"analog precision\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"ki_fp_filters\" value=\"R_* 0603\" />"),
        "{report}"
    );
    assert!(
        report.contains("<duplicate_pin_numbers_are_jumpers>1</duplicate_pin_numbers_are_jumpers>"),
        "{report}"
    );
    assert!(report.contains("<jumper_pin_groups>"), "{report}");
    assert!(report.contains("<group>"), "{report}");
    assert!(report.contains("<pin>1</pin>"), "{report}");
    assert!(report.contains("<pin>4</pin>"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_skips_blank_footprint_filters() {
    let path = temp_schematic(
        "cli_netlist_blank_fp_filters",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:JUMPER"
      (property "Reference" "JP" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "JUMPER" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "JUMPER_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:JUMPER")
    (at 0 0 0)
    (property "Reference" "JP1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "JUMPER" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );
    let loaded = load_schematic_tree(&path).expect("load tree");
    let mut project = SchematicProject::from_load_result(loaded);
    project.schematics[0]
        .screen
        .lib_symbols
        .iter_mut()
        .find(|lib_symbol| lib_symbol.lib_id == "Device:JUMPER")
        .expect("linked lib symbol")
        .fp_filters = vec!["SOIC*".to_string(), String::new()];
    let placed_symbol = project.schematics[0]
        .screen
        .items
        .iter_mut()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    placed_symbol
        .lib_symbol
        .as_mut()
        .expect("placed linked lib symbol")
        .fp_filters = vec!["SOIC*".to_string(), String::new()];

    let report = render_reduced_xml_netlist(&project);
    assert!(
        report.contains("<property name=\"ki_fp_filters\" value=\"SOIC*\" />"),
        "{report}"
    );
    assert!(!report.contains("value=\"SOIC* \""), "{report}");
    assert!(!report.contains("<fp></fp>"), "{report}");

    let _ = fs::remove_file(path);
}

#[test]
fn cli_netlist_sorts_jumper_group_pins() {
    let path = temp_schematic(
        "cli_netlist_jumper_group_pin_order",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:JUMPER"
      (duplicate_pin_numbers_are_jumpers yes)
      (jumper_pin_groups ("B" "A"))
      (property "Reference" "JP" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "JUMPER" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "JUMPER_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:JUMPER")
    (at 0 0 0)
    (property "Reference" "JP1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "JUMPER" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    let pin_a = report.find("<pin>A</pin>").expect("group pin A");
    let pin_b = report.find("<pin>B</pin>").expect("group pin B");
    assert!(pin_a < pin_b, "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_collapses_multi_unit_components() {
    let path = temp_schematic(
        "cli_netlist_multi_unit_component",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DUAL"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "DUAL" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "DUAL_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))
      (symbol "DUAL_2_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:DUAL")
    (uuid "73000000-0000-0000-0000-000000000010")
    (unit 1)
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "DUAL" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:DUAL")
    (uuid "73000000-0000-0000-0000-000000000001")
    (unit 2)
    (at 10 0 0)
    (property "Reference" "U1" (at 10 0 0) (effects (font (size 1 1))))
    (property "Value" "DUAL" (at 10 0 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert_eq!(report.matches("<comp ref=\"U1\">").count(), 1, "{report}");
    assert!(
        report.contains(
            "<tstamps>73000000-0000-0000-0000-000000000010 73000000-0000-0000-0000-000000000001</tstamps>"
        ),
        "{report}"
    );
    assert!(report.contains("<unit name=\"DUAL_1_1\">"), "{report}");
    assert!(report.contains("<unit name=\"DUAL_2_1\">"), "{report}");
    assert!(report.contains("<pin num=\"1\" />"), "{report}");
    assert!(report.contains("<pin num=\"2\" />"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_exports_component_variant_diffs() {
    let dir = env::temp_dir().join(format!(
        "ki2_netlist_component_variants_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let schematic_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");
    let report_path = schematic_path.with_extension("xml");

    fs::write(
        &project_path,
        r#"{
  "meta": { "filename": "root.kicad_pro", "version": 1 },
  "schematic": {
    "variants": [
      { "name": "ALT", "description": "Alt build" }
    ]
  }
}"#,
    )
    .expect("write project");

    fs::write(
        &schematic_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73000000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1))))
    (instances
      (project "root"
        (path ""
          (reference "R1")
          (variant
            (name "ALT")
            (dnp yes)
            (exclude_from_sim yes)
            (in_bom no)
            (in_pos_files no)
            (field (name "Value") (value "22k"))
            (field (name "Tolerance") (value "1%"))))))))"#,
    )
    .expect("write schematic");

    let output = Command::new(ki2_binary())
        .args(["netlist", schematic_path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("<variants>"), "{report}");
    assert!(report.contains("<variant name=\"ALT\">"), "{report}");
    assert!(
        report.contains("<property name=\"dnp\" value=\"1\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"exclude_from_bom\" value=\"1\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"exclude_from_sim\" value=\"1\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"exclude_from_pos_files\" value=\"1\" />"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Tolerance\">1%</field>"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Value\">22k</field>"),
        "{report}"
    );

    let _ = fs::remove_file(schematic_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(report_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_netlist_preserves_library_unit_order_on_components() {
    let path = temp_schematic(
        "cli_netlist_component_unit_order",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DUAL"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "DUAL" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "DUAL_1_1"
        (unit_name "Zeta")
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))
      (symbol "DUAL_2_1"
        (unit_name "Alpha")
        (pin passive line (at 0 0 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:DUAL")
    (uuid "73000000-0000-0000-0000-000000000010")
    (unit 1)
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "DUAL" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:DUAL")
    (uuid "73000000-0000-0000-0000-000000000001")
    (unit 2)
    (at 10 0 0)
    (property "Reference" "U1" (at 10 0 0) (effects (font (size 1 1))))
    (property "Value" "DUAL" (at 10 0 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    let zeta = report.find("<unit name=\"Zeta\">").expect("zeta unit");
    let alpha = report.find("<unit name=\"Alpha\">").expect("alpha unit");
    assert!(zeta < alpha, "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_rejects_unknown_formats() {
    let path = temp_schematic(
        "cli_netlist_bad_format",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4"))"#,
    );

    let output = Command::new(ki2_binary())
        .args([
            "netlist",
            path.to_str().expect("path string"),
            "--format",
            "bogus",
        ])
        .output()
        .expect("run ki2 netlist");

    assert!(
        !output.status.success(),
        "netlist must reject unsupported formats"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("invalid netlist format"), "{stderr}");

    let _ = fs::remove_file(path);
}

#[test]
fn cli_netlist_supports_reduced_kicad_format() {
    let dir = env::temp_dir().join(format!(
        "ki2_netlist_kicad_format_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let schematic_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");
    let report_path = schematic_path.with_extension("net");

    fs::write(
        &project_path,
        r#"{
  "meta": { "filename": "root.kicad_pro", "version": 1 },
  "schematic": {
    "variants": [
      { "name": "ALT", "description": "Alt build" }
    ]
  }
}"#,
    )
    .expect("write project");

    fs::write(
        &schematic_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73000000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (group "DesignBlock" (uuid "74000000-0000-0000-0000-000000000020")
    (members "73000000-0000-0000-0000-000000000010")))"#,
    )
    .expect("write schematic");

    let output = Command::new(ki2_binary())
        .args([
            "netlist",
            schematic_path.to_str().expect("path string"),
            "--format",
            "kicad",
        ])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = fs::read_to_string(&report_path).expect("read netlist");
    let components = report.find("<components>").expect("components");
    let groups = report.find("<groups>").expect("groups");
    let variants = report.find("<variants>").expect("variants");
    let libparts = report.find("<libparts>").expect("libparts");
    let libraries = report.find("<libraries>").expect("libraries");
    assert!(
        components < groups && groups < variants && variants < libparts && libparts < libraries,
        "{report}"
    );
    assert!(
        report.contains(
            "<group name=\"DesignBlock\" uuid=\"74000000-0000-0000-0000-000000000020\" lib_id=\"\">"
        ),
        "{report}"
    );
    assert!(
        report.contains("<member uuid=\"73000000-0000-0000-0000-000000000010\" />"),
        "{report}"
    );
    assert!(
        report.contains("<variant name=\"ALT\" description=\"Alt build\" />"),
        "{report}"
    );

    let _ = fs::remove_file(schematic_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(report_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_netlist_exports_component_classes() {
    let path = temp_schematic(
        "cli_netlist_component_classes",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:U"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "U" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "U_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:U")
    (uuid "73000000-0000-0000-0000-000000000010")
    (at 2 2 0)
    (property "Reference" "U1" (at 2 2 0) (effects (font (size 1 1))))
    (property "Value" "IC" (at 2 2 0) (effects (font (size 1 1))))
    (property "Component Class" "SymbolClass" (at 2 2 0) (effects (font (size 1 1)))))
  (rule_area
    (polyline
      (pts (xy 0 0) (xy 5 0) (xy 5 5) (xy 0 5))
      (stroke (width 0.2))
      (fill (type background))
      (uuid "74000000-0000-0000-0000-000000000020")))
  (directive_label "D" (shape dot) (at 1 1 0)
    (property "Component Class" "RuleAreaClass" (at 1 1 0) (effects (font (size 1 1))))
    (effects (font (size 1 1)))))"#,
    );

    let report_path = path.with_extension("xml");
    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(&report_path).expect("read netlist");
    let start = report
        .find("<component_classes>")
        .expect("component classes start");
    let end = report
        .find("</component_classes>")
        .expect("component classes end");
    let classes = &report[start..end];

    assert!(classes.contains("<class>RuleAreaClass</class>"), "{report}");
    assert!(classes.contains("<class>SymbolClass</class>"), "{report}");
    assert!(
        classes.find("RuleAreaClass") < classes.find("SymbolClass"),
        "{report}"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_kicad_netlist_filters_excluded_board_symbols() {
    let dir = env::temp_dir().join(format!(
        "ki2_netlist_kicad_board_filter_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let schematic_path = dir.join("root.kicad_sch");
    let xml_path = schematic_path.with_extension("xml");
    let kicad_path = schematic_path.with_extension("net");

    fs::write(
        &schematic_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73000000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:R")
    (uuid "73000000-0000-0000-0000-000000000011")
    (at 5 0 0)
    (on_board no)
    (property "Reference" "R2" (at 5 0 0) (effects (font (size 1 1))))
    (property "Value" "22k" (at 5 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 5 0)) (stroke (width 0.2)) (uuid "73000000-0000-0000-0000-000000000020")))"#,
    )
    .expect("write schematic");

    let xml_output = Command::new(ki2_binary())
        .args(["netlist", schematic_path.to_str().expect("path string")])
        .output()
        .expect("run ki2 xml netlist");
    assert!(
        xml_output.status.success(),
        "xml netlist must succeed: {}",
        String::from_utf8_lossy(&xml_output.stderr)
    );

    let kicad_output = Command::new(ki2_binary())
        .args([
            "netlist",
            schematic_path.to_str().expect("path string"),
            "--format",
            "kicad",
        ])
        .output()
        .expect("run ki2 kicad netlist");
    assert!(
        kicad_output.status.success(),
        "kicad netlist must succeed: {}",
        String::from_utf8_lossy(&kicad_output.stderr)
    );

    let xml_report = fs::read_to_string(&xml_path).expect("read xml report");
    let kicad_report = fs::read_to_string(&kicad_path).expect("read kicad report");

    assert!(xml_report.contains("<comp ref=\"R2\">"), "{xml_report}");
    assert!(
        xml_report.contains("<node ref=\"R2\" pin=\"1\""),
        "{xml_report}"
    );
    assert!(
        !kicad_report.contains("<comp ref=\"R2\">"),
        "{kicad_report}"
    );
    assert!(
        !kicad_report.contains("<node ref=\"R2\" pin=\"1\""),
        "{kicad_report}"
    );

    let _ = fs::remove_file(schematic_path);
    let _ = fs::remove_file(xml_path);
    let _ = fs::remove_file(kicad_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_netlist_uses_human_readable_component_sheet_paths() {
    let dir = env::temp_dir().join(format!(
        "ki2_netlist_sheetpath_names_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let report_path = root_path.with_extension("xml");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 20 20)
    (uuid "73000000-0000-0000-0000-000000000010")
    (property "Sheetname" "Child" (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "child.kicad_sch" (at 0 1 0) (effects (font (size 1 1))))))"#,
    )
    .expect("write root schematic");

    fs::write(
        &child_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73100000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73100000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1))))))"#,
    )
    .expect("write child schematic");

    let output = Command::new(ki2_binary())
        .args(["netlist", root_path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains("sheetpath names=\"/Child\"")
            && report.contains(
                "tstamps=\"/73000000-0000-0000-0000-000000000001/73000000-0000-0000-0000-000000000010\""
            ),
        "{report}"
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(report_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_netlist_exports_parent_sheet_properties_on_components() {
    let dir = env::temp_dir().join(format!(
        "ki2_netlist_sheet_properties_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let report_path = root_path.with_extension("xml");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 20 20)
    (uuid "73000000-0000-0000-0000-000000000010")
    (property "Sheetname" "Child" (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "child.kicad_sch" (at 0 1 0) (effects (font (size 1 1))))
    (property "Assembly" "MainBoard" (at 0 2 0) (effects (font (size 1 1))))))"#,
    )
    .expect("write root schematic");

    fs::write(
        &child_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73100000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73100000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1))))))"#,
    )
    .expect("write child schematic");

    let output = Command::new(ki2_binary())
        .args(["netlist", root_path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(
        report.contains("<property name=\"Assembly\" value=\"MainBoard\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"Sheetname\" value=\"Child\" />"),
        "{report}"
    );
    assert!(
        report.contains("<property name=\"Sheetfile\" value=\"child.kicad_sch\" />"),
        "{report}"
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(report_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_netlist_merges_multi_unit_fields_by_unit_order() {
    let path = temp_schematic(
        "cli_netlist_multi_unit_field_merge",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DUAL"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "DUAL" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "DUAL_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))
      (symbol "DUAL_2_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:DUAL")
    (unit 2)
    (uuid "73000000-0000-0000-0000-000000000020")
    (at 5 0 0)
    (property "Reference" "U1" (at 5 0 0) (effects (font (size 1 1))))
    (property "Value" "" (at 5 0 0) (effects (font (size 1 1))))
    (property "Datasheet" "unit2.pdf" (at 5 0 0) (effects (font (size 1 1))))
    (property "Manufacturer" "OtherCorp" (at 5 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:DUAL")
    (unit 1)
    (uuid "73000000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "PrimaryValue" (at 0 0 0) (effects (font (size 1 1))))
    (property "Manufacturer" "Acme" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );

    let report_path = path.with_extension("xml");
    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("<comp ref=\"U1\">"), "{report}");
    assert!(report.contains("<value>PrimaryValue</value>"), "{report}");
    assert!(
        report.contains("<datasheet>unit2.pdf</datasheet>"),
        "{report}"
    );
    assert!(
        report.contains("<field name=\"Manufacturer\">Acme</field>"),
        "{report}"
    );
    assert!(!report.contains("OtherCorp"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_keeps_lowest_unit_empty_user_field() {
    let path = temp_schematic(
        "cli_netlist_multi_unit_empty_field",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DUAL"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "DUAL" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "DUAL_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))
      (symbol "DUAL_2_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:DUAL")
    (unit 1)
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "DUAL" (at 0 0 0) (effects (font (size 1 1))))
    (property "MPN" "" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:DUAL")
    (unit 2)
    (at 10 0 0)
    (property "Reference" "U1" (at 10 0 0) (effects (font (size 1 1))))
    (property "Value" "DUAL" (at 10 0 0) (effects (font (size 1 1))))
    (property "MPN" "SHOULD_NOT_WIN" (at 10 0 0) (effects (font (size 1 1))))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("<field name=\"MPN\"></field>"), "{report}");
    assert!(!report.contains("SHOULD_NOT_WIN"), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_prefers_non_pad_default_driver_names() {
    let path = temp_schematic(
        "cli_netlist_prefers_non_pad_driver",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:A"
      (property "Reference" "A" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "A" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "A_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))))
    (symbol "Device:Z"
      (property "Reference" "Z" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "Z" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "Z_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "OUT" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:A")
    (uuid "73000000-0000-0000-0000-000000000010")
    (at 0 0 0)
    (property "Reference" "A1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "A" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:Z")
    (uuid "73000000-0000-0000-0000-000000000011")
    (at 10 0 0)
    (property "Reference" "Z1" (at 10 0 0) (effects (font (size 1 1))))
    (property "Value" "Z" (at 10 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0)) (stroke (width 0.2)) (uuid "73000000-0000-0000-0000-000000000020")))"#,
    );

    let report_path = path.with_extension("xml");
    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(
        output.status.success(),
        "netlist must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert!(report.contains("name=\"Net-(Z1-OUT)\""), "{report}");
    assert!(!report.contains("name=\"Net-(A1-Pad1)\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn cli_netlist_groups_same_named_subgraphs_under_one_net() {
    let path = temp_schematic(
        "cli_netlist_same_named_subgraphs",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:IN"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "IN" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "IN_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "IN" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:IN")
    (at 0 0 0)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "IN" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:IN")
    (at 50 0 0)
    (property "Reference" "U2" (at 50 0 0) (effects (font (size 1 1))))
    (property "Value" "IN" (at 50 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy -10 0) (xy 0 0)))
  (global_label "SHARED" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 40 0) (xy 50 0)))
  (global_label "SHARED" (shape input) (at 40 0 0) (effects (font (size 1 1)))))"#,
    );
    let report_path = path.with_extension("xml");

    let output = Command::new(ki2_binary())
        .args(["netlist", path.to_str().expect("path string")])
        .output()
        .expect("run ki2 netlist");

    assert!(output.status.success(), "netlist must succeed");
    let report = fs::read_to_string(&report_path).expect("read netlist");
    assert_eq!(report.matches("<net code=").count(), 1, "{report}");
    assert!(
        report.contains("<net code=\"1\" name=\"SHARED\""),
        "{report}"
    );
    assert!(report.contains("<node ref=\"U1\" pin=\"1\""), "{report}");
    assert!(report.contains("<node ref=\"U2\" pin=\"1\""), "{report}");

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(report_path);
}

#[test]
fn erc_reports_symbol_and_sheet_field_name_whitespace() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_field_name_whitespace_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73000000-0000-0000-0000-000000000002")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (property " trailing " "x")))
"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73000000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "73000000-0000-0000-0000-000000000003")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (property " leading " "y"))
  (sheet_instances
    (path "" (page "1"))
    (path "/73000000-0000-0000-0000-000000000003" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let load = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project);

    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().all(|diagnostic| {
        diagnostic.code == "erc-field-name-whitespace"
            && diagnostic.kind == DiagnosticKind::Validation
            && diagnostic.severity == ki2::diagnostic::Severity::Warning
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Field name has leading or trailing whitespace: ' leading '"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Field name has leading or trailing whitespace: ' trailing '"
    }));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_duplicate_sheet_names_case_insensitively() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_duplicate_sheet_names_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("root.kicad_sch");
    let child_a = dir.join("child-a.kicad_sch");
    let child_b = dir.join("child-b.kicad_sch");

    fs::write(
        &path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73100000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "73100000-0000-0000-0000-000000000002")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child-a.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "73100000-0000-0000-0000-000000000003")
    (property "Sheetname" "child")
    (property "Sheetfile" "child-b.kicad_sch"))
)"#,
    )
    .expect("write root");
    fs::write(
        &child_a,
        r#"(kicad_sch (version 20260306) (generator "eeschema") (uuid "73100000-0000-0000-0000-000000000011"))"#,
    )
    .expect("write child a");
    fs::write(
        &child_b,
        r#"(kicad_sch (version 20260306) (generator "eeschema") (uuid "73100000-0000-0000-0000-000000000012"))"#,
    )
    .expect("write child b");

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-duplicate-sheet-name"
            && diagnostic.kind == DiagnosticKind::Validation
            && diagnostic.severity == ki2::diagnostic::Severity::Error
            && diagnostic.message == "Duplicate sheet name: 'Child'"
    }));

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(child_a);
    let _ = fs::remove_file(child_b);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_same_local_and_global_labels() {
    let path = temp_schematic(
        "erc_same_local_global_label",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (label "SIG" (at 0 0 0) (effects (font (size 1 1))))
  (global_label "SIG" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-same-local-global-label"
            && diagnostic
                .message
                .contains("Local and global labels share the same shown text")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_similar_labels_that_differ_only_by_case() {
    let path = temp_schematic(
        "erc_similar_labels",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (label "Net_A" (at 0 0 0) (effects (font (size 1 1))))
  (global_label "net_a" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-similar-labels"
            && diagnostic.severity == ki2::diagnostic::Severity::Warning
            && diagnostic
                .message
                .contains("Similar labels differ only by case")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_connected_driver_conflicts() {
    let path = temp_schematic(
        "erc_driver_conflict",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "NET_B" (shape input) (at 0 0 0) (effects (font (size 1 1))))
  (global_label "NET_A" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-driver-conflict"
            && diagnostic.severity == ki2::diagnostic::Severity::Warning
            && diagnostic
                .message
                .contains("Both NET_A and NET_B are attached to the same items; NET_A will be used in the netlist")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_single_global_labels() {
    let path = temp_schematic(
        "erc_single_global_label",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (global_label "NET_A" (shape input) (at 0 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-single-global-label")
        .collect::<Vec<_>>();

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");

    let _ = fs::remove_file(path);
}

#[test]
fn erc_uses_project_rule_severity_overrides() {
    let dir = temp_dir_path("erc_rule_severity_override");
    fs::create_dir_all(&dir).expect("create temp dir");
    let schematic_path = dir.join("erc_rule_severity_override.kicad_sch");
    let project_path = dir.join("erc_rule_severity_override.kicad_pro");

    fs::write(
        &schematic_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (global_label "NET_A" (shape input) (at 0 0 0) (effects (font (size 1 1)))))"#,
    )
    .expect("write schematic");

    fs::write(
        &project_path,
        r#"{
  "erc": {
    "rule_severities": {
      "single_global_label": "error"
    }
  }
}"#,
    )
    .expect("write project");

    let loaded = load_schematic_tree(&schematic_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-single-global-label")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, ki2::diagnostic::Severity::Error);
    assert_eq!(
        diagnostics[0].message,
        "Global label 'NET_A' appears only once"
    );

    let _ = fs::remove_file(schematic_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_allows_repeated_global_labels() {
    let path = temp_schematic(
        "erc_repeated_global_labels",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "NET_A" (shape input) (at 0 0 0) (effects (font (size 1 1))))
  (global_label "NET_A" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-single-global-label"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_root_hierarchical_labels() {
    let path = temp_schematic(
        "erc_root_hierarchical_label",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (hierarchical_label "NET_A" (shape input) (at 0 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-pin-not-connected")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "Hierarchical label 'NET_A' in root sheet cannot be connected to non-existent parent sheet"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_hierarchical_sheet_name_mismatches() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_hier_sheet_mismatch_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (hierarchical_label "OUT" (shape input) (at 0 5 0) (effects (font (size 1 1)))))"#;
    let root_src = format!(
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -5 5) (xy 0 5)))
  (sheet (at 0 0) (size 20 10)
    (uuid "81111111-1111-1111-1111-111111111111")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "IN" input (at 0 5 180) (uuid "82222222-2222-2222-2222-222222222222"))))"#,
        child_path.display()
    );

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-hierarchical-label-mismatch")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("Sheet pin IN has no matching hierarchical label inside the sheet")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("Hierarchical label OUT has no matching sheet pin in the parent sheet")
    }));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_dangling_sheet_pins() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_dangling_sheet_pin_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (hierarchical_label "IN" (shape input) (at 0 5 0) (effects (font (size 1 1)))))"#;
    let root_src = format!(
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (sheet (at 0 0) (size 20 10)
    (uuid "83333333-3333-3333-3333-333333333333")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "IN" input (at 0 5 180) (uuid "84444444-4444-4444-4444-444444444444"))))"#,
        child_path.display()
    );

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-pin-not-connected")
        .collect::<Vec<_>>();

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "Sheet pin 'IN' is not connected")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_dangling_directive_labels() {
    let path = temp_schematic(
        "erc_dangling_directive_label",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (directive_label "D" (shape dot) (at 5 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-label-not-connected")
        .collect::<Vec<_>>();

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "Directive label is not connected at 5, 0")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_floating_wires_and_dangling_endpoints() {
    let path = temp_schematic(
        "erc_floating_wire",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-wire-dangling"
            && diagnostic.message == "Floating wire component at 0, 0"
    }));
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "erc-unconnected-wire-endpoint")
            .count(),
        2
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_connected_wire_endpoints() {
    let path = temp_schematic(
        "erc_connected_wire",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (global_label "NET" (shape input) (at 0 0 0) (effects (font (size 1 1))))
  (global_label "NET" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 0) (xy 10 0))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "erc-wire-dangling"
                || diagnostic.code == "erc-unconnected-wire-endpoint"
        }),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_floating_bus_entries_and_dangling_bus_entry_endpoints() {
    let path = temp_schematic(
        "erc_floating_bus_entry",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (bus_entry (at 0 0) (size 10 10)))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-wire-dangling"
            && diagnostic.message == "Floating wire component at 0, 0"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-unconnected-wire-endpoint"
            && diagnostic.message == "Unconnected wire to bus entry at 0, 0"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-unconnected-wire-endpoint"
            && diagnostic.message == "Unconnected wire to bus entry at 10, 10"
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_bus_entry_endpoint_connected_to_wire() {
    let path = temp_schematic(
        "erc_connected_bus_entry",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -5 0) (xy 0 0)))
  (bus_entry (at 0 0) (size 10 10)))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "erc-unconnected-wire-endpoint"
                && diagnostic.message == "Unconnected wire to bus entry at 0, 0"
        }),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_bus_to_net_conflicts_from_connected_lines() {
    let path = temp_schematic(
        "erc_bus_to_net_lines",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -10 0) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 10 0))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-bus-to-net-conflict"
            && diagnostic.message == "Bus and net items are graphically connected at -10, 0"
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_bus_to_net_conflicts_from_bus_label_and_wire() {
    let path = temp_schematic(
        "erc_bus_to_net_label",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "DATA[0..7]" (shape input) (at 0 0 0) (effects (font (size 1 1))))
  (global_label "NET_A" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-bus-to-net-conflict"
            && diagnostic.message == "Bus and net items are graphically connected at 0, 0"
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_bus_to_bus_conflicts() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_bus_to_bus_conflict_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (hierarchical_label "ADDR[0..7]" (shape input) (at 0 5 0) (effects (font (size 1 1)))))"#;
    let root_src = format!(
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -5 5) (xy 0 5)))
  (sheet (at 0 0) (size 20 10)
    (uuid "85555555-5555-5555-5555-555555555555")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "ADDR[0..7]" input (at 0 5 180) (uuid "86666666-6666-6666-6666-666666666666")))
  (global_label "DATA[0..7]" (shape input) (at -5 5 0) (effects (font (size 1 1)))))"#,
        child_path.display()
    );

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-bus-to-bus-conflict"
            && diagnostic.message == "Bus label and port do not share members at -5, 5"
    }));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_allows_bus_to_bus_overlap_on_shared_member() {
    let path = temp_schematic(
        "erc_bus_to_bus_overlap",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -5 5) (xy 10 5)))
  (global_label "DATA[0..7]" (shape input) (at -5 5 0) (effects (font (size 1 1))))
  (hierarchical_label "DATA[7..9]" (shape input) (at 10 5 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-bus-to-bus-conflict"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_bus_entry_member_conflicts() {
    let path = temp_schematic(
        "erc_bus_entry_conflict",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -5 5) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 10 0)))
  (bus_entry (at 0 0) (size -5 5))
  (label "ADDR9" (at -5 5 0) (effects (font (size 1 1))))
  (global_label "DATA[0..7]" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-bus-entry-conflict"
            && diagnostic.message
                == "Net ADDR9 is graphically connected to bus DATA[0..7] but is not a member of that bus at -5, 5"
    }));
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-bus-to-net-conflict"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_prefixed_bus_group_member_entries() {
    let path = temp_schematic(
        "erc_bus_entry_prefixed_group_member",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -5 5) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 10 0)))
  (bus_entry (at 0 0) (size -5 5))
  (global_label "USB.DP" (shape input) (at -5 5 0) (effects (font (size 1 1))))
  (global_label "USB{DP DM}" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-bus-entry-conflict"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_bus_alias_member_entries() {
    let path = temp_schematic(
        "erc_bus_entry_alias_member",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (bus_alias "DATA" (members D[0..3]))
  (wire (pts (xy -5 5) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 10 0)))
  (bus_entry (at 0 0) (size -5 5))
  (global_label "D0" (shape input) (at -5 5 0) (effects (font (size 1 1))))
  (global_label "DATA" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-bus-entry-conflict"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_does_not_count_buses_as_four_way_junction_items() {
    let path = temp_schematic(
        "erc_four_way_excludes_bus",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -10 0) (xy 0 0)))
  (wire (pts (xy 0 0) (xy 10 0)))
  (wire (pts (xy 0 -10) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 0 10)))
  (junction (at 0 0)))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-four-way-junction"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_suppresses_bus_entry_conflicts_for_global_label_override() {
    let path = temp_schematic(
        "erc_bus_entry_global_override",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy -10 5) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 10 0)))
  (bus_entry (at 0 0) (size -5 5))
  (label "ADDR9" (at -10 5 0) (effects (font (size 1 1))))
  (global_label "OVERRIDE" (shape input) (at -5 5 0) (effects (font (size 1 1))))
  (global_label "DATA[0..7]" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-bus-entry-conflict"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_connected_directive_labels() {
    let path = temp_schematic(
        "erc_connected_directive_label",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (directive_label "D" (shape dot) (at 5 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "erc-label-not-connected"
                && diagnostic.message.contains("Directive label")
        }),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_similar_local_labels_on_different_sheets() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_similar_local_labels_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (label "net_a" (at 0 0 0) (effects (font (size 1 1)))))"#;
    let root_src = format!(
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (sheet (at 0 0) (size 20 20)
    (uuid 11111111-1111-1111-1111-111111111111)
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1)))))
  (label "Net_A" (at 0 0 0) (effects (font (size 1 1))))
  (sheet_instances
    (path "/"
      (page "1"))
    (path "/11111111-1111-1111-1111-111111111111"
      (page "2"))))"#,
        child_path.display()
    );

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-similar-labels"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_footprint_filter_mismatches() {
    let path = temp_schematic(
        "erc_footprint_filter_mismatch",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (property "ki_fp_filters" "R_* 0603")))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1))))
    (property "Footprint" "Package_SO:SOIC-8" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-footprint-link-issues"
            && diagnostic
                .message
                .contains("Assigned footprint (soic-8) doesn't match footprint filters (R_* 0603)")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_matching_footprint_filters() {
    let path = temp_schematic(
        "erc_footprint_filter_match",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (property "ki_fp_filters" "R_* 0603")))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1))))
    (property "Footprint" "Resistor_SMD:R_0603_1608Metric" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-footprint-link-issues"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_invalid_stacked_pin_notation() {
    let path = temp_schematic(
        "erc_invalid_stacked_pin_notation",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:U"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "U" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "U_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "IN" (effects (font (size 1 1))))
          (number "[1-A]" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:U")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "IC" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-stacked-pin-syntax"
            && diagnostic.severity == ki2::diagnostic::Severity::Warning
            && diagnostic
                .message
                .contains("Pin number resembles stacked pin notation but is invalid")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_valid_stacked_pin_notation() {
    let path = temp_schematic(
        "erc_valid_stacked_pin_notation",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:U"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "U" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "U_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "IN" (effects (font (size 1 1))))
          (number "[1, 2, 3]" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:U")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "IC" (at 0 0 0) (effects (font (size 1 1))))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-stacked-pin-syntax"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_ground_pins_on_non_ground_nets() {
    let path = temp_schematic(
        "erc_ground_pin_not_ground",
        r##"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "power:SplitGround"
      (power)
      (property "Reference" "#PWR" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "SplitGround" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "SplitGround_1_1"
        (pin power_in line (at 0 0 180) (length 2.54)
          (name "GND" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin power_in line (at 10 0 0) (length 2.54)
          (name "AGND" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "power:SplitGround")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitGround" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "VCC" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "GND" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"##,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-ground-pin-not-ground"
            && diagnostic
                .message
                .contains("Pin GND not connected to ground net")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_ground_pins_on_ground_nets() {
    let path = temp_schematic(
        "erc_ground_pin_grounded",
        r##"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "power:G"
      (power)
      (property "Reference" "#PWR" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "G" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "G_1_1"
        (pin power_in line (at 0 0 180) (length 2.54)
          (name "GND" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "power:G")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "G" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "GND" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"##,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-ground-pin-not-ground"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_duplicate_pin_numbers_on_different_nets() {
    let path = temp_schematic(
        "erc_duplicate_pin_nets",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:BadDup"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "BadDup" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "BadDup_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:BadDup")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "BadDup" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "NET_B" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-duplicate-pin-nets"
            && diagnostic
                .message
                .contains("Pin 1 on symbol 'U1' is connected to different nets: NET_A and NET_B")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_ignores_duplicate_jumper_pin_numbers() {
    let path = temp_schematic(
        "erc_duplicate_jumper_pin_nets",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:JumperDup"
      (duplicate_pin_numbers_are_jumpers yes)
      (property "Reference" "JP" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "JumperDup" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "JumperDup_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:JumperDup")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "JP1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "JumperDup" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "NET_B" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-duplicate-pin-nets"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_multiunit_pin_number_on_different_nets() {
    let path = temp_schematic(
        "erc_multiunit_pin_conflict",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:SplitUnit"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "SplitUnit" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "SplitUnit_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))
      (symbol "SplitUnit_2_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:SplitUnit")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitUnit" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:SplitUnit")
    (at 20 0 0)
    (unit 2)
    (property "Reference" "U1" (at 20 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitUnit" (at 20 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 20 0) (xy 30 0)))
  (global_label "NET_B" (shape input) (at 30 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-different-unit-net"
            && diagnostic
                .message
                .contains("Pin 1 is connected to both NET_B and NET_A")
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_allows_multiunit_pin_number_on_same_net() {
    let path = temp_schematic(
        "erc_multiunit_pin_same_net",
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:SplitUnit"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "SplitUnit" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "SplitUnit_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))
      (symbol "SplitUnit_2_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:SplitUnit")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitUnit" (at 0 0 0) (effects (font (size 1 1)))))
  (symbol
    (lib_id "Device:SplitUnit")
    (at 20 0 0)
    (unit 2)
    (property "Reference" "U1" (at 20 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitUnit" (at 20 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (wire (pts (xy 20 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-different-unit-net"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_off_grid_endpoints_from_project_grid() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_erc_off_grid_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:OffGrid"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "OffGrid" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "OffGrid_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:OffGrid")
    (at 1 0 0)
    (unit 1)
    (property "Reference" "U1" (at 1 0 0) (effects (font (size 1 1))))
    (property "Value" "OffGrid" (at 1 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 1 0)))
  (bus_entry (at 1 1) (size 0.5 0.5)))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"meta\": {\n    \"version\": 2\n  },\n  \"schematic\": {\n    \"connection_grid_size\": 50.0\n  }\n}\n",
    )
    .expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "erc-endpoint-off-grid")
            .count(),
        4,
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_allows_on_grid_endpoints_from_project_grid() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_erc_on_grid_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:OnGrid"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "OnGrid" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "OnGrid_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:OnGrid")
    (at 1.27 0 0)
    (unit 1)
    (property "Reference" "U1" (at 1.27 0 0) (effects (font (size 1 1))))
    (property "Value" "OnGrid" (at 1.27 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 1.27 0)))
  (bus_entry (at 1.27 1.27) (size 1.27 1.27)))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"meta\": {\n    \"version\": 2\n  },\n  \"schematic\": {\n    \"connection_grid_size\": 50.0\n  }\n}\n",
    )
    .expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-endpoint-off-grid"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_unresolved_text_variables_on_exercised_items() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_unresolved_text_vars_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73200000-0000-0000-0000-000000000001")
  (paper "A4")
  (text "${UNKNOWN_TEXT}" (at 0 0 0))
  (symbol
    (lib_id "Device:R")
    (at 10 0 0)
    (property "Custom" "${UNKNOWN_FIELD}"))
  (global_label "GL" (shape input) (at 20 0 0)
    (property "Custom" "${UNKNOWN_LABEL_FIELD}"))
  (sheet
    (at 0 20)
    (size 20 10)
    (uuid "73200000-0000-0000-0000-000000000002")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "${UNKNOWN_PIN}" input (at 20 5 180)))
  (sheet_instances
    (path "" (page "1"))
    (path "/73200000-0000-0000-0000-000000000002" (page "2")))
)"#,
    )
    .expect("write root");
    fs::write(
        &child_path,
        r#"(kicad_sch (version 20260306) (generator "eeschema") (uuid "73200000-0000-0000-0000-000000000003"))"#,
    )
    .expect("write child");

    let load = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-unresolved-variable")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 5);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "Unresolved text variable in schematic text"
        })
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Unresolved text variable in symbol field 'Custom'"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Unresolved text variable in label field 'Custom'"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Unresolved text variable in label field 'Intersheet References'"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Unresolved text variable in sheet pin '${UNKNOWN_PIN}'"
    }));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_text_assertions_on_exercised_items() {
    let path = temp_schematic(
        "erc_text_assertions",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73300000-0000-0000-0000-000000000001")
  (paper "A4")
  (text "${ERC_WARNING top level warning}" (at 0 0 0))
  (text_box "${ERC_ERROR textbox error}" (at 0 10 0) (size 5 5))
  (symbol
    (lib_id "Device:R")
    (at 10 0 0)
    (property "Custom" "${ERC_WARNING symbol warning}"))
  (global_label "GL" (shape input) (at 20 0 0)
    (property "Custom" "${ERC_ERROR label error}"))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.code == "erc-generic-warning" || diagnostic.code == "erc-generic-error"
        })
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 4);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-warning" && diagnostic.message == "top level warning"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-error" && diagnostic.message == "textbox error"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-warning" && diagnostic.message == "symbol warning"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-error" && diagnostic.message == "label error"
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_unresolved_text_variables_in_linked_library_items() {
    let path = temp_schematic(
        "erc_unresolved_lib_text",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73400000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R")
      (property "Value" "R")
      (symbol "R_1_1"
        (text "${UNKNOWN_LIB_TEXT}" (at 0 0 0))
        (text_box "${UNKNOWN_LIB_BOX}" (at 0 10 0) (size 5 5)))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-unresolved-variable")
        .collect::<Vec<_>>();

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message == "Unresolved text variable in library text" })
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "Unresolved text variable in library text_box"
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_text_assertions_in_linked_library_items() {
    let path = temp_schematic(
        "erc_lib_text_assertions",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73500000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R")
      (property "Value" "R")
      (symbol "R_1_1"
        (text "${ERC_WARNING lib text warning}" (at 0 0 0))
        (text_box "${ERC_ERROR lib box error}" (at 0 10 0) (size 5 5)))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.code == "erc-generic-warning" || diagnostic.code == "erc-generic-error"
        })
        .collect::<Vec<_>>();

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-warning" && diagnostic.message == "lib text warning"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-error" && diagnostic.message == "lib box error"
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_different_multiunit_footprints() {
    let path = temp_schematic(
        "erc_multiunit_footprints",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73600000-0000-0000-0000-000000000001")
  (paper "A4")
  (symbol
    (lib_id "Device:U")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1")
    (property "Footprint" "Package_SO:SOIC-8"))
  (symbol
    (lib_id "Device:U")
    (at 20 0 0)
    (unit 2)
    (property "Reference" "U1")
    (property "Footprint" "Package_DIP:DIP-8_W7.62mm"))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-different-unit-footprint")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "Different footprints assigned to reference 'U1'"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_missing_units_for_multiunit_symbols() {
    let path = temp_schematic(
        "erc_missing_units",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73700000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:U"
      (property "Reference" "U")
      (property "Value" "U")
      (symbol "U_1_1" (text "A" (at 0 0 0)))
      (symbol "U_2_1" (text "B" (at 0 0 0)))))
  (symbol
    (lib_id "Device:U")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1"))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-missing-units")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "Missing symbol units for reference 'U1'"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_undefined_netclasses_from_project_settings() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_missing_netclasses_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73800000-0000-0000-0000-000000000001")
  (paper "A4")
  (global_label "GL" (shape input) (at 0 0 0)
    (property "Netclass" "Fast"))
)"#,
    )
    .expect("write root");
    fs::write(
        &project_path,
        r#"{
  "meta": { "version": 2 },
  "net_settings": {
    "classes": [
      { "name": "Default" }
    ]
  }
}"#,
    )
    .expect("write project");

    let load = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-undefined-netclass")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "Netclass Fast is not defined");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_parse_filesystem_tbtext() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_filesystem_drawing_sheet_text_items_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "%T" (pos 10 20))
  (tbtext "${REVISION}" (pos 30 40 rbcorner))
  (rect (start 0 0) (end 1 1)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].text, "${TITLE}");
    assert_eq!(items[0].at, [277.0, 180.0]);
    assert_eq!(items[1].text, "${REVISION}");
    assert_eq!(items[1].at, [257.0, 160.0]);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_expand_repeated_tbtext() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_repeated_drawing_sheet_text_items_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "1" (pos 10 20) (repeat 3) (incrx 5) (incry 2)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(
        items,
        vec![
            ki2::worksheet::WorksheetTextItem {
                text: "1".to_string(),
                at: [277.0, 180.0]
            },
            ki2::worksheet::WorksheetTextItem {
                text: "2".to_string(),
                at: [272.0, 178.0]
            },
            ki2::worksheet::WorksheetTextItem {
                text: "3".to_string(),
                at: [267.0, 176.0]
            },
        ]
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_honor_incrlabel_for_letters() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_letter_drawing_sheet_incrlabel_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "A" (pos 1 2) (repeat 3) (incry 10) (incrlabel 2)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(
        items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "C", "E"]
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_honor_page_options() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_drawing_sheet_page_options_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "71000000-0000-0000-0000-000000000001")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000002")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000002" (page "2"))))"#,
    )
    .expect("write root schematic");
    fs::write(
        &child_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "71000000-0000-0000-0000-000000000003"))"#,
    )
    .expect("write child schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "all" (pos 0 0))
  (tbtext "first" (pos 1 0) (option page1only))
  (tbtext "later" (pos 2 0) (option notonpage1)))"#,
    )
    .expect("write worksheet");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded
            .current_drawing_sheet_text_items()
            .expect("root worksheet items")
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["all", "first"]
    );

    let child_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| !sheet_path.instance_path.is_empty())
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child instance path");
    assert!(loaded.set_current_sheet_path(&child_instance_path));
    assert_eq!(
        loaded
            .current_drawing_sheet_text_items()
            .expect("child worksheet items")
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["all", "later"]
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_resolve_corner_anchors_and_margins() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_drawing_sheet_corner_anchors_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2") (paper "A4"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (setup (left_margin 10) (right_margin 20) (top_margin 30) (bottom_margin 40))
  (tbtext "LT" (pos 1 2 ltcorner))
  (tbtext "LB" (pos 3 4 lbcorner))
  (tbtext "RB" (pos 5 6 rbcorner))
  (tbtext "RT" (pos 7 8 rtcorner)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(
        items,
        vec![
            ki2::worksheet::WorksheetTextItem {
                text: "LT".to_string(),
                at: [11.0, 32.0]
            },
            ki2::worksheet::WorksheetTextItem {
                text: "LB".to_string(),
                at: [13.0, 166.0]
            },
            ki2::worksheet::WorksheetTextItem {
                text: "RB".to_string(),
                at: [272.0, 164.0]
            },
            ki2::worksheet::WorksheetTextItem {
                text: "RT".to_string(),
                at: [270.0, 38.0]
            },
        ]
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_clip_repeated_items_to_page() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_drawing_sheet_repeat_clipping_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2") (paper "A4"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (setup (left_margin 10) (right_margin 10) (top_margin 10) (bottom_margin 10))
  (tbtext "1" (pos 25 1 ltcorner) (repeat 100) (incrx 50)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(
        items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "2", "3", "4", "5", "6"]
    );
    assert_eq!(
        items.iter().map(|item| item.at[0]).collect::<Vec<_>>(),
        vec![35.0, 85.0, 135.0, 185.0, 235.0, 285.0]
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_decode_backslash_sequences() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_drawing_sheet_backslash_sequences_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2") (paper "A4"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "Line\\nA" (pos 1 2) (repeat 2) (incrlabel 9))
  (tbtext "A\\\\B" (pos 3 4)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(items[0].text, "Line\nA");
    assert_eq!(items[1].text, "Line\nA");
    assert_eq!(items[2].text, "A\\B");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_text_items_parse_embedded_tbtext() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_embedded_drawing_sheet_text_items_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20250114)
  (generator "ki2")
  (uuid "60000000-0000-0000-0000-000000000100")
  (paper "A4")
  (embedded_files
    (file
      (name "custom.kicad_wks")
      (checksum deadbeef)
      (type worksheet)
      (data |(kicad_wks (version 20210606) (generator pl_editor) (tbtext "${COMMENT1}" (pos 5 6)))|))))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_text_items()
        .expect("worksheet items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].text, "${COMMENT1}");
    assert_eq!(items[0].at, [282.0, 194.0]);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_shown_text_items_resolve_sheet_and_project_vars() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_drawing_sheet_shown_text_items_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20231120)
  (generator "ki2")
  (uuid "60000000-0000-0000-0000-000000000101")
  (paper "A4")
  (title_block (title "Demo Title"))
  (sheet_instances (path "" (page "7"))))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "${TITLE}" (pos 1 2))
  (tbtext "Sheet ${#}/${##}" (pos 3 4))
  (tbtext "${PAPER}" (pos 5 6))
  (tbtext "${PROJECTNAME}" (pos 7 8)))"#,
    )
    .expect("write worksheet");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_shown_text_items()
        .expect("shown worksheet items");
    assert_eq!(
        items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Demo Title", "Sheet 7/1", "A4", "demo"]
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_shown_text_items_use_reduced_default_worksheet() {
    let path = temp_schematic(
        "default_drawing_sheet_shown_text",
        r#"(kicad_sch
  (version 20231120)
  (generator "ki2")
  (uuid "60000000-0000-0000-0000-000000000102")
  (paper "A4")
  (title_block (title "Demo Title") (company "Acme") (comment 1 "First"))
  (sheet_instances (path "" (page "7"))))"#,
    );

    let loaded = load_schematic_tree(&path).expect("load tree");
    let items = loaded
        .current_drawing_sheet_shown_text_items()
        .expect("shown worksheet items");
    let texts = items
        .iter()
        .map(|item| item.text.as_str())
        .collect::<Vec<_>>();
    assert!(texts.contains(&"Title: Demo Title"));
    assert!(texts.contains(&"Size: A4"));
    assert!(texts.contains(&"Id: 7/1"));
    assert!(texts.contains(&"Acme"));
    assert!(texts.contains(&"First"));
    assert!(texts.contains(&"A"));
    assert!(texts.contains(&"1"));

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_unresolved_text_variable_in_drawing_sheet() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_erc_drawing_sheet_unresolved_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "${UNKNOWN_VAR}" (pos 1 2)))"#,
    )
    .expect("write worksheet");

    let load = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-unresolved-variable")
        .collect::<Vec<_>>();

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "Unresolved text variable in drawing sheet"
        })
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_default_drawing_sheet_does_not_report_unresolved_text_variables() {
    let path = temp_schematic(
        "erc_default_drawing_sheet_resolves",
        r#"(kicad_sch
  (version 20231120)
  (generator "ki2")
  (uuid "60000000-0000-0000-0000-000000000103")
  (paper "A4")
  (title_block (title "Demo Title"))
  (sheet_instances (path "" (page "7"))))"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-unresolved-variable")
        .collect::<Vec<_>>();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_text_assertions_in_drawing_sheet() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_erc_drawing_sheet_assertions_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");
    let worksheet_path = dir.join("custom.kicad_wks");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");
    fs::write(
        &worksheet_path,
        r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "${ERC_WARNING worksheet warning}" (pos 1 2))
  (tbtext "${ERC_ERROR worksheet error}" (pos 3 4)))"#,
    )
    .expect("write worksheet");

    let load = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.code == "erc-generic-warning" || diagnostic.code == "erc-generic-error"
        })
        .collect::<Vec<_>>();

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-warning" && diagnostic.message == "worksheet warning"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "erc-generic-error" && diagnostic.message == "worksheet error"
    }));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_file(worksheet_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_labels_touching_multiple_wire_segments() {
    let path = temp_schematic(
        "erc_label_multiple_wires",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73900000-0000-0000-0000-000000000001")
  (paper "A4")
  (label "L" (at 0 0 0))
  (wire (pts (xy -10 0) (xy 10 0)))
  (wire (pts (xy 0 -10) (xy 0 10)))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-label-multiple-wires")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "Label connects more than one wire at 0, 0"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_four_way_junctions() {
    let path = temp_schematic(
        "erc_four_way_junction",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73910000-0000-0000-0000-000000000001")
  (paper "A4")
  (wire (pts (xy -10 0) (xy 0 0)))
  (wire (pts (xy 0 0) (xy 10 0)))
  (wire (pts (xy 0 -10) (xy 0 0)))
  (wire (pts (xy 0 0) (xy 0 10)))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-four-way-junction")
        .collect::<Vec<_>>();

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_connected_no_connect_pins() {
    let path = temp_schematic(
        "erc_no_connect_pin_connected",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73920000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:NC"
      (symbol "NC_1_1"
        (pin no_connect line
          (at 0 0 0)
          (length 2.54)
          (name "NC")
          (number "1")))))
  (symbol
    (lib_id "Device:NC")
    (at 0 0 0)
    (uuid "73920000-0000-0000-0000-000000000002"))
  (wire (pts (xy 0 0) (xy 10 0)))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-nc-pin-connected")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "Pin with 'no connection' type is connected"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_connected_no_connect_markers_across_named_subgraphs() {
    let path = temp_schematic(
        "erc_connected_no_connect_marker_named_subgraphs",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73922000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:IN"
      (symbol "IN_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "IN")
          (number "1")))))
  (symbol
    (lib_id "Device:IN")
    (at 0 0 0)
    (uuid "73922000-0000-0000-0000-000000000002"))
  (symbol
    (lib_id "Device:IN")
    (at 40 0 0)
    (uuid "73922000-0000-0000-0000-000000000003"))
  (wire (pts (xy -10 0) (xy 0 0)))
  (global_label "SHARED" (shape input) (at -10 0 0))
  (no_connect (at 0 0))
  (wire (pts (xy 30 0) (xy 40 0)))
  (global_label "SHARED" (shape input) (at 30 0 0))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-no-connect-connected")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "No-connect marker is attached to a connected net"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_ignores_no_connect_on_hierarchical_label_with_remote_pin() {
    let dir = env::temp_dir().join(format!(
        "ki2_erc_hier_label_no_connect_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (hierarchical_label "SIG" (shape input) (at 0 5 0) (effects (font (size 1 1))))
  (no_connect (at 0 5)))"#;
    let root_src = format!(
        r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:IN"
      (symbol "IN_1_1"
        (pin input line
          (at 0 0 180)
          (length 2.54)
          (name "IN")
          (number "1")))))
  (symbol
    (lib_id "Device:IN")
    (at 30 5 0)
    (uuid "73923000-0000-0000-0000-000000000010"))
  (wire (pts (xy 0 5) (xy 30 5)))
  (sheet (at 0 0) (size 20 10)
    (uuid "73923000-0000-0000-0000-000000000011")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "SIG" input (at 0 5 180) (uuid "73923000-0000-0000-0000-000000000012"))))"#,
        child_path.display()
    );

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = SchematicProject::from_load_result(loaded);
    let diagnostics = erc::run(&project);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-no-connect-connected"),
        "{diagnostics:#?}"
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_labels_connected_to_only_one_pin() {
    let path = temp_schematic(
        "erc_label_single_pin",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73925000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:IN"
      (symbol "IN_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "IN")
          (number "1")))))
  (symbol
    (lib_id "Device:IN")
    (at 0 0 0)
    (uuid "73925000-0000-0000-0000-000000000002"))
  (global_label "SIG" (shape input) (at 0 0 0) (effects (font (size 1 1))))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-label-single-pin")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, ki2::diagnostic::Severity::Warning);
    assert_eq!(
        diagnostics[0].message,
        "Label is connected to only one pin at 0, 0"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_local_labels_not_connected_to_pins() {
    let path = temp_schematic(
        "erc_label_not_connected",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73926000-0000-0000-0000-000000000001")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (label "SIG" (at 0 0 0) (effects (font (size 1 1))))
  (label "SIG" (at 10 0 0) (effects (font (size 1 1))))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-label-not-connected")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 2);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == ki2::diagnostic::Severity::Error)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn erc_reports_conflicting_pin_types_on_same_net() {
    let path = temp_schematic(
        "erc_pin_to_pin_conflict",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73930000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DRV"
      (symbol "DRV_1_1"
        (pin output line
          (at 0 0 0)
          (length 2.54)
          (name "OUT")
          (number "1")))))
  (symbol
    (lib_id "Device:DRV")
    (at 0 0 0)
    (uuid "73930000-0000-0000-0000-000000000002"))
  (symbol
    (lib_id "Device:DRV")
    (at 10 0 0)
    (uuid "73930000-0000-0000-0000-000000000003"))
  (wire (pts (xy 0 0) (xy 10 0)))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-pin-to-pin-error")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "Conflicting pins connected at 0, 0");

    let _ = fs::remove_file(path);
}

#[test]
fn erc_uses_project_pin_map_overrides() {
    let dir = temp_dir_path("erc_pin_map_override");
    fs::create_dir_all(&dir).expect("create temp dir");
    let schematic_path = dir.join("erc_pin_map_override.kicad_sch");
    let project_path = dir.join("erc_pin_map_override.kicad_pro");

    fs::write(
        &schematic_path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73930000-0000-0000-0000-000000000011")
  (paper "A4")
  (lib_symbols
    (symbol "Device:DRV"
      (symbol "DRV_1_1"
        (pin output line
          (at 0 0 0)
          (length 2.54)
          (name "OUT")
          (number "1")))))
  (symbol
    (lib_id "Device:DRV")
    (at 0 0 0)
    (uuid "73930000-0000-0000-0000-000000000012"))
  (symbol
    (lib_id "Device:DRV")
    (at 10 0 0)
    (uuid "73930000-0000-0000-0000-000000000013"))
  (wire (pts (xy 0 0) (xy 10 0)))
)"#,
    )
    .expect("write schematic");

    fs::write(
        &project_path,
        r#"{
  "erc": {
    "pin_map": [
      [0,0,0,0,0,0,1,0,0,0,0,2],
      [0,1,0,1,0,0,1,0,2,2,2,2],
      [0,0,0,0,0,0,1,0,1,0,1,2],
      [0,1,0,0,0,0,1,1,2,1,1,2],
      [0,0,0,0,0,0,1,0,0,0,0,2],
      [0,0,0,0,0,0,0,0,0,0,0,2],
      [1,1,1,1,1,0,1,1,1,1,1,2],
      [0,0,0,1,0,0,1,0,0,0,0,2],
      [0,2,1,2,0,0,1,0,2,2,2,2],
      [0,2,0,1,0,0,1,0,2,0,0,2],
      [0,2,1,1,0,0,1,0,2,0,0,2],
      [2,2,2,2,2,2,2,2,2,2,2,2]
    ]
  }
}"#,
    )
    .expect("write project");

    let load = load_schematic_tree(&schematic_path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-pin-to-pin-warning")
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "erc-pin-to-pin-error")
    );

    let _ = fs::remove_file(schematic_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn erc_reports_input_pins_without_driver() {
    let path = temp_schematic(
        "erc_missing_driver",
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "73940000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:IN"
      (symbol "IN_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "IN")
          (number "1")))))
  (symbol
    (lib_id "Device:IN")
    (at 0 0 0)
    (uuid "73940000-0000-0000-0000-000000000002"))
  (symbol
    (lib_id "Device:IN")
    (at 10 0 0)
    (uuid "73940000-0000-0000-0000-000000000003"))
  (wire (pts (xy 0 0) (xy 10 0)))
)"#,
    );

    let load = load_schematic_tree(&path).expect("load tree");
    let project = SchematicProject::from_load_result(load);
    let diagnostics = erc::run(&project)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "erc-missing-driver")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "Input pin is not driven");

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_core_grammar_keyword_heads() {
    let quoted_root = r#"("kicad_sch"
  (version 20260306)
  (generator "eeschema")
  (uuid "root"))
"#;
    let quoted_root_path = temp_schematic("quoted_root_keyword", quoted_root);
    let err = parse_schematic_file(Path::new(&quoted_root_path))
        .expect_err("must reject quoted root keyword");
    assert!(err.to_string().contains("expecting kicad_sch"));

    let quoted_version = r#"(kicad_sch
  ("version" 20260306)
  (generator "eeschema")
  (uuid "root"))
"#;
    let quoted_version_path = temp_schematic("quoted_version_keyword", quoted_version);
    let err = parse_schematic_file(Path::new(&quoted_version_path))
        .expect_err("must reject quoted version keyword");
    assert!(err.to_string().contains("expecting version"));

    let quoted_bus_alias_members = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root")
  (bus_alias "ADDR" ("members" "A0"))
)"#;
    let quoted_bus_alias_members_path =
        temp_schematic("quoted_bus_alias_members", quoted_bus_alias_members);
    let err = parse_schematic_file(Path::new(&quoted_bus_alias_members_path))
        .expect_err("must reject quoted bus_alias members keyword");
    assert!(err.to_string().contains("expecting members"));

    let quoted_wire_xy = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root")
  (wire (pts ("xy" 0 0) (xy 1 1)))
)"#;
    let quoted_wire_xy_path = temp_schematic("quoted_wire_xy", quoted_wire_xy);
    let err = parse_schematic_file(Path::new(&quoted_wire_xy_path))
        .expect_err("must reject quoted wire xy keyword");
    assert!(err.to_string().contains("expecting xy"));

    let quoted_wire_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root")
  (wire ("pts" (xy 0 0) (xy 1 1)))
)"#;
    let quoted_wire_head_path = temp_schematic("quoted_wire_head", quoted_wire_head);
    let err = parse_schematic_file(Path::new(&quoted_wire_head_path))
        .expect_err("must reject quoted wire head keyword");
    assert!(err.to_string().contains("expecting at, uuid or stroke"));

    let quoted_bezier_xy = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root")
  (bezier (pts ("xy" 0 0) (xy 1 1) (xy 2 2) (xy 3 3)))
)"#;
    let quoted_bezier_xy_path = temp_schematic("quoted_bezier_xy", quoted_bezier_xy);
    let err = parse_schematic_file(Path::new(&quoted_bezier_xy_path))
        .expect_err("must reject quoted bezier xy keyword");
    assert!(err.to_string().contains("expecting xy"));

    let _ = fs::remove_file(quoted_root_path);
    let _ = fs::remove_file(quoted_version_path);
    let _ = fs::remove_file(quoted_bus_alias_members_path);
    let _ = fs::remove_file(quoted_wire_xy_path);
    let _ = fs::remove_file(quoted_wire_head_path);
    let _ = fs::remove_file(quoted_bezier_xy_path);
}

#[test]
fn rejects_true_false_boolean_tokens() {
    let embedded_fonts_true = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000001")
  (embedded_fonts true)
)"#;
    let embedded_fonts_true_path =
        temp_schematic("embedded_fonts_true_keyword", embedded_fonts_true);
    let err = parse_schematic_file(Path::new(&embedded_fonts_true_path))
        .expect_err("must reject true/false for embedded_fonts");
    assert!(err.to_string().contains("expecting yes or no"));

    let symbol_in_bom_false = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000002")
  (symbol (lib_id "Device:R") (in_bom false))
)"#;
    let symbol_in_bom_false_path =
        temp_schematic("symbol_in_bom_false_keyword", symbol_in_bom_false);
    let err = parse_schematic_file(Path::new(&symbol_in_bom_false_path))
        .expect_err("must reject true/false for symbol in_bom");
    assert!(err.to_string().contains("expecting yes or no"));

    let bold_true = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000003")
  (text "note" (effects (font (bold true))))
)"#;
    let bold_true_path = temp_schematic("effects_bold_true_keyword", bold_true);
    let err = parse_schematic_file(Path::new(&bold_true_path))
        .expect_err("must reject true/false for effects bold");
    assert!(err.to_string().contains("expecting yes or no"));

    let _ = fs::remove_file(embedded_fonts_true_path);
    let _ = fs::remove_file(symbol_in_bom_false_path);
    let _ = fs::remove_file(bold_true_path);
}

#[test]
fn rejects_unterminated_quoted_atoms_in_lexer() {
    let unterminated = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema
"#;
    let path = temp_schematic("unterminated_quoted_atom", unterminated);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject unterminated quote");
    assert!(matches!(err, Error::SExpr { .. }));
    assert!(err.to_string().contains(":4:1 (byte 55)"));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_number_tokens_in_numeric_fields() {
    let quoted_comment_number = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000004")
  (title_block (comment "1" "note"))
)"#;
    let quoted_comment_number_path = temp_schematic("quoted_comment_number", quoted_comment_number);
    let err = parse_schematic_file(Path::new(&quoted_comment_number_path))
        .expect_err("must reject quoted comment number");
    assert!(err.to_string().contains("missing comment"));

    let quoted_symbol_angle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000005")
  (symbol (lib_id "Device:R") (at 1 2 "90"))
)"#;
    let quoted_symbol_angle_path = temp_schematic("quoted_symbol_angle", quoted_symbol_angle);
    let err = parse_schematic_file(Path::new(&quoted_symbol_angle_path))
        .expect_err("must reject quoted numeric angle");
    assert!(err.to_string().contains("missing symbol orientation"));

    let quoted_text_angle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000006")
  (text "note" (at 1 2 "90"))
)"#;
    let quoted_text_angle_path = temp_schematic("quoted_text_angle", quoted_text_angle);
    let err = parse_schematic_file(Path::new(&quoted_text_angle_path))
        .expect_err("must reject quoted text angle");
    assert!(err.to_string().contains("missing text angle"));

    let quoted_text_box_size = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000007")
  (text_box "body" (size "3" 4))
)"#;
    let quoted_text_box_size_path =
        temp_schematic("quoted_text_box_size_number", quoted_text_box_size);
    let err = parse_schematic_file(Path::new(&quoted_text_box_size_path))
        .expect_err("must reject quoted textbox size number");
    assert!(err.to_string().contains("missing text_box size x"));

    let quoted_lib_text_angle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000008")
  (lib_symbols
    (symbol "MyLib:U"
      (text "TXT" (at 0 0 "90"))))
)"#;
    let quoted_lib_text_angle_path =
        temp_schematic("quoted_lib_text_angle_number", quoted_lib_text_angle);
    let err = parse_schematic_file(Path::new(&quoted_lib_text_angle_path))
        .expect_err("must reject quoted lib text angle");
    assert!(err.to_string().contains("missing text angle"));

    let quoted_lib_pin_angle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-000000000009")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input line (at 0 0 "90") (length 2.54) (name "P") (number "1"))))
)"#;
    let quoted_lib_pin_angle_path =
        temp_schematic("quoted_lib_pin_angle_number", quoted_lib_pin_angle);
    let err = parse_schematic_file(Path::new(&quoted_lib_pin_angle_path))
        .expect_err("must reject quoted lib pin angle");
    assert!(err.to_string().contains("missing pin orientation"));

    let quoted_lib_property_angle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-00000000000a")
  (lib_symbols
    (symbol "MyLib:U"
      (property "RefDes" "U" (at 0 0 "90"))))
)"#;
    let quoted_lib_property_angle_path = temp_schematic(
        "quoted_lib_property_angle_number",
        quoted_lib_property_angle,
    );
    let err = parse_schematic_file(Path::new(&quoted_lib_property_angle_path))
        .expect_err("must reject quoted lib property angle");
    assert!(err.to_string().contains("missing text angle"));

    let quoted_sheet_width = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-00000000000b")
  (sheet
    (size "20" 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let quoted_sheet_width_path = temp_schematic("quoted_sheet_width_number", quoted_sheet_width);
    let err = parse_schematic_file(Path::new(&quoted_sheet_width_path))
        .expect_err("must reject quoted sheet width");
    assert!(err.to_string().contains("missing sheet width"));

    let quoted_sheet_height = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-00000000000c")
  (sheet
    (size 20 "10")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let quoted_sheet_height_path =
        temp_schematic("quoted_sheet_height_number", quoted_sheet_height);
    let err = parse_schematic_file(Path::new(&quoted_sheet_height_path))
        .expect_err("must reject quoted sheet height");
    assert!(err.to_string().contains("missing sheet height"));

    let _ = fs::remove_file(quoted_comment_number_path);
    let _ = fs::remove_file(quoted_symbol_angle_path);
    let _ = fs::remove_file(quoted_text_angle_path);
    let _ = fs::remove_file(quoted_text_box_size_path);
    let _ = fs::remove_file(quoted_lib_text_angle_path);
    let _ = fs::remove_file(quoted_lib_pin_angle_path);
    let _ = fs::remove_file(quoted_lib_property_angle_path);
    let _ = fs::remove_file(quoted_sheet_width_path);
    let _ = fs::remove_file(quoted_sheet_height_path);
}

#[test]
fn rejects_non_integer_rgb_color_channels() {
    let decimal_junction_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-00000000000d")
  (junction (color 10.5 20 30 0.5))
)"#;
    let decimal_junction_color_path =
        temp_schematic("decimal_junction_color", decimal_junction_color);
    let err = parse_schematic_file(Path::new(&decimal_junction_color_path))
        .expect_err("must reject decimal junction RGB channel");
    assert!(err.to_string().contains("missing red"));

    let decimal_effects_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "61000000-0000-0000-0000-00000000000e")
  (text "note" (effects (font (color 10.5 20 30 0.5))))
)"#;
    let decimal_effects_color_path = temp_schematic("decimal_effects_color", decimal_effects_color);
    let err = parse_schematic_file(Path::new(&decimal_effects_color_path))
        .expect_err("must reject decimal effects RGB channel");
    assert!(err.to_string().contains("missing red"));

    let _ = fs::remove_file(decimal_junction_color_path);
    let _ = fs::remove_file(decimal_effects_color_path);
}

#[test]
fn validates_hierarchical_tree_fixture() {
    let loaded = load_schematic_tree(&fixture("hierarchical.kicad_sch")).expect("tree must load");
    assert_eq!(loaded.schematics.len(), 2);
    assert_eq!(loaded.links.len(), 1);
    assert_eq!(
        loaded.links[0].sheet_uuid.as_deref(),
        Some("73000000-0000-0000-0000-000000000002")
    );
    assert_eq!(loaded.links[0].sheet_name.as_deref(), Some("PowerSection"));

    let root = &loaded.schematics[0];
    assert_eq!(
        root.root_sheet.uuid.as_deref(),
        Some("73000000-0000-0000-0000-000000000001")
    );
    assert_eq!(
        root.screen.uuid.as_deref(),
        Some("73000000-0000-0000-0000-000000000001")
    );
    assert_eq!(root.screen.sheet_instances.len(), 1);
    assert_eq!(root.screen.symbol_instances.len(), 2);
    assert!(
        root.screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Wire(line) if line.kind == LineKind::Wire))
    );
    assert!(root.screen.items.iter().any(
        |item| matches!(item, SchItem::Label(label) if label.kind == LabelKind::Hierarchical)
    ));

    let project = SchematicProject::from_load_result(loaded);
    let root = project.root().expect("root schematic");
    assert_eq!(
        root.root_sheet.uuid.as_deref(),
        Some("73000000-0000-0000-0000-000000000001")
    );
    assert_eq!(project.sheet_paths.len(), 2);
    assert_eq!(
        project
            .root_sheet_path()
            .expect("project root sheet path")
            .instance_path,
        ""
    );
    assert_eq!(
        project
            .current_sheet_path()
            .expect("project current sheet path")
            .instance_path,
        ""
    );
    assert_eq!(
        project
            .current_schematic()
            .expect("project current schematic")
            .path,
        project.root_path
    );
    assert_eq!(
        project
            .sheet_paths_of(&project.root_path)
            .next()
            .expect("root sheet path")
            .instance_path,
        ""
    );
    assert_eq!(project.children_of(&project.root_path).count(), 1);
    let child_link = project
        .children_of(&project.root_path)
        .next()
        .expect("child link");
    assert_eq!(project.parents_of(&child_link.child_path).count(), 1);
    assert_eq!(project.sheet_paths_of(&child_link.child_path).count(), 1);
}

#[test]
fn reuses_previously_loaded_child_schematic() {
    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000005")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "70000000-0000-0000-0000-000000000004")
    (property "Reference" "R?" (at 1 2 0))
    (property "Value" "seed" (at 3 4 0))
    (property "Footprint" "seed-footprint" (at 5 6 0))
    (instances
      (project "demo"
        (path "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
          (reference "R1")
          (unit 1)
          (value "10k")
          (footprint "Resistor_SMD:R_0603"))
        (path "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003"
          (reference "R2")
          (unit 2)
          (value "22k")
          (footprint "Resistor_SMD:R_0402")))))
)"#;
    let root_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet (at 0 0) (size 10 10) (uuid "70000000-0000-0000-0000-000000000002")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet (at 20 0) (size 10 10) (uuid "70000000-0000-0000-0000-000000000003")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "9"))
    (path "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002" (page "1"))
    (path "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003" (page "2")))
)"#;
    let dir = env::temp_dir().join(format!(
        "ki2_reuse_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load reused child");
    assert_eq!(loaded.schematics.len(), 2);
    assert_eq!(loaded.links.len(), 2);
    assert_eq!(
        loaded
            .links
            .iter()
            .filter(|link| link.reused_existing_child)
            .count(),
        1
    );
    assert_eq!(loaded.children_of(&root_path).count(), 2);
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    assert_eq!(child.screen.page_number, None);
    assert_eq!(child.screen.page_count, None);
    assert_eq!(child.screen.virtual_page_number, None);
    let child_symbol = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("child symbol before selection");
    assert_eq!(
        child_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(child_symbol.unit, Some(1));
    assert_eq!(
        child_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        child_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );
    assert_eq!(loaded.sheet_paths_of(&child_path).count(), 2);
    assert_eq!(loaded.parents_of(&child_path).count(), 2);
    assert_eq!(
        loaded
            .root_sheet_path()
            .expect("load result root sheet path")
            .page
            .as_deref(),
        Some("9")
    );
    assert_eq!(
        loaded
            .current_sheet_path()
            .expect("load result current sheet path")
            .instance_path,
        ""
    );
    assert_eq!(loaded.current_page_number(), Some("9"));
    assert_eq!(loaded.current_page_count(), Some(3));
    assert_eq!(loaded.current_virtual_page_number(), Some(3));
    assert_eq!(
        loaded
            .current_schematic()
            .expect("load result current schematic")
            .path,
        root_path.canonicalize().unwrap_or(root_path.clone())
    );
    let mut loaded = loaded;
    assert!(loaded.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003"
    ));
    assert_eq!(
        loaded
            .current_sheet_path()
            .expect("updated load result current sheet path")
            .instance_path,
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003"
    );
    assert_eq!(loaded.current_page_number(), Some("2"));
    assert_eq!(loaded.current_page_count(), Some(3));
    assert_eq!(loaded.current_virtual_page_number(), Some(2));
    assert_eq!(
        loaded
            .current_schematic()
            .expect("updated load result current schematic")
            .path,
        child_path.canonicalize().unwrap_or(child_path.clone())
    );
    assert_eq!(
        loaded
            .current_schematic()
            .expect("updated current load-result schematic")
            .screen
            .page_number
            .as_deref(),
        Some("2")
    );
    assert_eq!(
        loaded
            .current_schematic()
            .expect("updated current load-result schematic")
            .screen
            .page_count,
        Some(3)
    );
    assert_eq!(
        loaded
            .current_schematic()
            .expect("updated current load-result schematic")
            .screen
            .virtual_page_number,
        Some(2)
    );
    let loaded_symbol = loaded
        .current_schematic()
        .expect("updated current load-result schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("updated child symbol");
    assert_eq!(
        loaded_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R2")
    );
    assert_eq!(loaded_symbol.unit, Some(2));
    assert_eq!(
        loaded_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("22k")
    );
    assert_eq!(
        loaded_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0402")
    );
    assert!(!loaded.set_current_sheet_path("/missing"));
    assert_eq!(
        loaded
            .sheet_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
            )
            .expect("sheet path A")
            .page
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        loaded
            .sheet_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003"
            )
            .expect("sheet path B")
            .page
            .as_deref(),
        Some("2")
    );
    assert_eq!(
        loaded
            .sheet_path_for_symbol_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002/70000000-0000-0000-0000-000000000004"
            )
            .expect("load result symbol owner path")
            .instance_path,
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
    );
    assert_eq!(
        loaded
            .parent_sheet_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
            )
            .expect("load result parent path")
            .instance_path,
        ""
    );
    assert!(loaded.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
    ));
    assert_eq!(loaded.current_page_number(), Some("1"));
    assert_eq!(loaded.current_page_count(), Some(3));
    assert_eq!(loaded.current_virtual_page_number(), Some(1));
    let switched_a_symbol = loaded
        .current_schematic()
        .expect("switched load-result child schematic A")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("switched load-result child symbol A");
    assert_eq!(
        switched_a_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert!(loaded.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003"
    ));
    assert_eq!(loaded.current_page_number(), Some("2"));
    assert_eq!(loaded.current_page_count(), Some(3));
    assert_eq!(loaded.current_virtual_page_number(), Some(2));
    let switched_b_symbol = loaded
        .current_schematic()
        .expect("switched load-result child schematic B")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("switched load-result child symbol B");
    assert_eq!(
        switched_b_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R2")
    );
    assert!(loaded.set_current_sheet_path(""));
    let reset_child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("reset child schematic");
    assert_eq!(reset_child.screen.page_number, None);
    assert_eq!(reset_child.screen.page_count, None);
    assert_eq!(reset_child.screen.virtual_page_number, None);
    let reset_child_symbol = reset_child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("reset child symbol");
    assert_eq!(
        reset_child_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(reset_child_symbol.unit, Some(1));
    assert_eq!(
        reset_child_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        reset_child_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );

    let project = SchematicProject::from_load_result(loaded);
    assert_eq!(project.sheet_paths_of(&child_path).count(), 2);
    assert_eq!(
        project
            .sheet_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
            )
            .expect("project sheet path A")
            .page
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        project
            .sheet_path_for_symbol_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002/70000000-0000-0000-0000-000000000004"
            )
            .expect("project symbol owner path")
            .instance_path,
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
    );
    assert_eq!(
        project
            .parent_sheet_path(
                "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
            )
            .expect("project parent path")
            .instance_path,
        ""
    );
    let mut project = project;
    assert!(project.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000002"
    ));
    assert_eq!(project.current_page_number(), Some("1"));
    assert_eq!(project.current_page_count(), Some(3));
    assert_eq!(project.current_virtual_page_number(), Some(1));
    assert_eq!(
        project
            .current_schematic()
            .expect("project current child schematic")
            .screen
            .page_number
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        project
            .current_schematic()
            .expect("project current child schematic")
            .screen
            .page_count,
        Some(3)
    );
    assert_eq!(
        project
            .current_schematic()
            .expect("project current child schematic")
            .screen
            .virtual_page_number,
        Some(1)
    );
    let project_symbol = project
        .current_schematic()
        .expect("project current child schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("project child symbol");
    assert_eq!(
        project_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(project_symbol.unit, Some(1));
    assert_eq!(
        project_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        project_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );
    assert!(project.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000001/70000000-0000-0000-0000-000000000003"
    ));
    assert_eq!(project.current_page_number(), Some("2"));
    assert_eq!(project.current_page_count(), Some(3));
    assert_eq!(project.current_virtual_page_number(), Some(2));
    let project_symbol_b = project
        .current_schematic()
        .expect("project current child schematic B")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("project child symbol B");
    assert_eq!(
        project_symbol_b
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R2")
    );
    assert!(project.set_current_sheet_path(""));
    let reset_project_child = project
        .schematic(&child_path)
        .expect("reset project child schematic");
    assert_eq!(reset_project_child.screen.page_number, None);
    assert_eq!(reset_project_child.screen.page_count, None);
    assert_eq!(reset_project_child.screen.virtual_page_number, None);
    let reset_project_symbol = reset_project_child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("reset project child symbol");
    assert_eq!(
        reset_project_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(reset_project_symbol.unit, Some(1));
    assert_eq!(
        reset_project_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        reset_project_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn builds_sheet_paths_and_updates_legacy_symbol_instance_data_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_post_load_instances_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000012")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "70000000-0000-0000-0000-000000000013")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000011")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "70000000-0000-0000-0000-000000000014")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/70000000-0000-0000-0000-000000000014" (page "1")))
  (symbol_instances
    (path "/70000000-0000-0000-0000-000000000014/70000000-0000-0000-0000-000000000013"
      (reference "R7")
      (unit 2)
      (value "47k")
      (footprint "Resistor_SMD:R_0603")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(loaded.sheet_paths.len(), 2);
    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000011/70000000-0000-0000-0000-000000000014"
    );
    assert_eq!(
        loaded.sheet_paths[0].sheet_uuid.as_deref(),
        Some("70000000-0000-0000-0000-000000000014")
    );
    assert_eq!(loaded.sheet_paths[0].sheet_name.as_deref(), Some("Child"));
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("1"));
    assert_eq!(loaded.sheet_paths[0].sheet_number, 1);
    assert_eq!(loaded.sheet_paths[0].sheet_count, 2);
    assert_eq!(loaded.sheet_paths[1].instance_path, "");
    assert_eq!(
        loaded.sheet_paths[1].sheet_uuid.as_deref(),
        Some("70000000-0000-0000-0000-000000000011")
    );
    assert_eq!(loaded.sheet_paths[1].sheet_name, None);
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("2"));
    assert_eq!(loaded.sheet_paths[1].sheet_number, 2);
    assert_eq!(loaded.sheet_paths[1].sheet_count, 2);

    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    assert_eq!(root.screen.page_number.as_deref(), Some("2"));
    assert_eq!(root.screen.page_count, Some(2));
    assert_eq!(root.screen.virtual_page_number, Some(2));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    assert_eq!(child.screen.page_number.as_deref(), Some("1"));
    assert_eq!(child.screen.page_count, Some(2));
    assert_eq!(child.screen.virtual_page_number, Some(1));
    let symbol = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("child symbol");

    assert_eq!(symbol.unit, Some(2));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R7")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("47k")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );
    assert_eq!(symbol.instances.len(), 1);
    assert_eq!(symbol.instances[0].project, "");
    assert_eq!(
        symbol.instances[0].path,
        "/70000000-0000-0000-0000-000000000011/70000000-0000-0000-0000-000000000014"
    );
    assert_eq!(symbol.instances[0].reference.as_deref(), Some("R7"));
    assert_eq!(symbol.instances[0].unit, Some(2));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn legacy_reused_screens_keep_first_instance_state_until_selected() {
    let dir = env::temp_dir().join(format!(
        "ki2_legacy_reuse_instances_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000101")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "70000000-0000-0000-0000-000000000102")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000111")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "70000000-0000-0000-0000-000000000112")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "70000000-0000-0000-0000-000000000113")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "9"))
    (path "/70000000-0000-0000-0000-000000000112" (page "1"))
    (path "/70000000-0000-0000-0000-000000000113" (page "2")))
  (symbol_instances
    (path "/70000000-0000-0000-0000-000000000112/70000000-0000-0000-0000-000000000102"
      (reference "R1")
      (unit 1)
      (value "10k")
      (footprint "Resistor_SMD:R_0603"))
    (path "/70000000-0000-0000-0000-000000000113/70000000-0000-0000-0000-000000000102"
      (reference "R2")
      (unit 2)
      (value "22k")
      (footprint "Resistor_SMD:R_0402")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let mut loaded = load_schematic_tree(&root_path).expect("load reused child");
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    assert_eq!(child.screen.page_number, None);
    assert_eq!(child.screen.page_count, None);
    assert_eq!(child.screen.virtual_page_number, None);
    let symbol = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("child symbol");
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(symbol.unit, Some(1));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );

    assert!(loaded.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000111/70000000-0000-0000-0000-000000000113"
    ));
    let selected_child = loaded
        .current_schematic()
        .expect("selected child schematic");
    let selected_symbol = selected_child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("selected child symbol");
    assert_eq!(
        selected_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R2")
    );
    assert_eq!(selected_symbol.unit, Some(2));
    assert_eq!(
        selected_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("22k")
    );
    assert_eq!(
        selected_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0402")
    );

    assert!(loaded.set_current_sheet_path(""));
    let reset_child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("reset child schematic");
    let reset_symbol = reset_child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("reset child symbol");
    assert_eq!(
        reset_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(reset_symbol.unit, Some(1));
    assert_eq!(
        reset_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        reset_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_variant_refreshes_reused_symbol_occurrence_state() {
    let dir = env::temp_dir().join(format!(
        "ki2_current_variant_reused_symbol_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000101")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "72000000-0000-0000-0000-000000000102")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (property "MPN" "seed-mpn")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000112"
          (reference "R1")
          (unit 1)
          (value "10k")
          (footprint "Resistor_SMD:R_0603")
          (variant
            (name "ALT")
            (dnp yes)
            (exclude_from_sim yes)
            (in_bom no)
            (on_board no)
            (in_pos_files no)
            (field (name "MPN") (value "ALT-A"))))
        (path "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000113"
          (reference "R2")
          (unit 2)
          (value "22k")
          (footprint "Resistor_SMD:R_0402")
          (variant
            (name "ALT")
            (dnp no)
            (exclude_from_sim no)
            (in_bom yes)
            (on_board yes)
            (in_pos_files yes)
            (field (name "MPN") (value "ALT-B")))))))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000111")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000112")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000113")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "9"))
    (path "/72000000-0000-0000-0000-000000000112" (page "1"))
    (path "/72000000-0000-0000-0000-000000000113" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let mut loaded = load_schematic_tree(&root_path).expect("load reused child");
    let symbol = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .and_then(|schematic| {
            schematic.screen.items.iter().find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
        })
        .expect("child symbol");
    assert!(!symbol.dnp);
    assert!(!symbol.excluded_from_sim);
    assert!(symbol.in_bom);
    assert!(symbol.on_board);
    assert!(symbol.in_pos_files);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("seed-mpn")
    );

    loaded.set_current_variant(Some("ALT"));
    let symbol = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .and_then(|schematic| {
            schematic.screen.items.iter().find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
        })
        .expect("child symbol");
    assert!(symbol.dnp);
    assert!(symbol.excluded_from_sim);
    assert!(!symbol.in_bom);
    assert!(!symbol.on_board);
    assert!(!symbol.in_pos_files);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("ALT-A")
    );

    assert!(loaded.set_current_sheet_path(
        "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000113"
    ));
    let selected_symbol = loaded
        .current_schematic()
        .expect("selected child schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("selected child symbol");
    assert!(!selected_symbol.dnp);
    assert!(!selected_symbol.excluded_from_sim);
    assert!(selected_symbol.in_bom);
    assert!(selected_symbol.on_board);
    assert!(selected_symbol.in_pos_files);
    assert_eq!(
        selected_symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("ALT-B")
    );

    loaded.set_current_variant(None);
    let reset_symbol = loaded
        .current_schematic()
        .expect("reset child schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("reset child symbol");
    assert!(!reset_symbol.dnp);
    assert!(!reset_symbol.excluded_from_sim);
    assert!(reset_symbol.in_bom);
    assert!(reset_symbol.on_board);
    assert!(reset_symbol.in_pos_files);
    assert_eq!(
        reset_symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("seed-mpn")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn project_current_variant_refreshes_reused_symbol_occurrence_state() {
    let dir = env::temp_dir().join(format!(
        "ki2_project_current_variant_reused_symbol_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000101")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "72000000-0000-0000-0000-000000000102")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (property "MPN" "seed-mpn")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000112"
          (reference "R1")
          (unit 1)
          (value "10k")
          (footprint "Resistor_SMD:R_0603")
          (variant
            (name "ALT")
            (dnp yes)
            (exclude_from_sim yes)
            (in_bom no)
            (on_board no)
            (in_pos_files no)
            (field (name "MPN") (value "ALT-A"))))
        (path "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000113"
          (reference "R2")
          (unit 2)
          (value "22k")
          (footprint "Resistor_SMD:R_0402")
          (variant
            (name "ALT")
            (dnp no)
            (exclude_from_sim no)
            (in_bom yes)
            (on_board yes)
            (in_pos_files yes)
            (field (name "MPN") (value "ALT-B")))))))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000111")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000112")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000113")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "9"))
    (path "/72000000-0000-0000-0000-000000000112" (page "1"))
    (path "/72000000-0000-0000-0000-000000000113" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load reused child");
    let mut project = SchematicProject::from_load_result(loaded);

    project.set_current_variant(Some("ALT"));

    assert!(project.set_current_sheet_path(
        "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000112"
    ));
    let symbol = project
        .current_schematic()
        .expect("selected child schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("selected child symbol");
    assert!(symbol.dnp);
    assert!(symbol.excluded_from_sim);
    assert!(!symbol.in_bom);
    assert!(!symbol.on_board);
    assert!(!symbol.in_pos_files);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("ALT-A")
    );

    assert!(project.set_current_sheet_path(
        "/72000000-0000-0000-0000-000000000111/72000000-0000-0000-0000-000000000113"
    ));
    let symbol = project
        .current_schematic()
        .expect("selected child schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("selected child symbol");
    assert!(!symbol.dnp);
    assert!(!symbol.excluded_from_sim);
    assert!(symbol.in_bom);
    assert!(symbol.on_board);
    assert!(symbol.in_pos_files);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("ALT-B")
    );

    project.set_current_variant(None);
    let symbol = project
        .current_schematic()
        .expect("selected child schematic")
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("selected child symbol");
    assert!(!symbol.dnp);
    assert!(!symbol.excluded_from_sim);
    assert!(symbol.in_bom);
    assert!(symbol.on_board);
    assert!(symbol.in_pos_files);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "MPN")
            .map(|property| property.value.as_str()),
        Some("seed-mpn")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_variant_refreshes_live_sheet_variant_state() {
    let dir = env::temp_dir().join(format!(
        "ki2_current_variant_sheet_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000201")
  (paper "A4")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000211")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000212")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch")
    (property "POP" "seed-pop")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000211/72000000-0000-0000-0000-000000000212"
          (page "1")
          (variant
            (name "ALT")
            (dnp yes)
            (exclude_from_sim yes)
            (in_bom no)
            (on_board no)
            (field (name "POP") (value "ALT-POP")))))))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000213")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch")
    (property "POP" "seed-pop-b")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000211/72000000-0000-0000-0000-000000000213"
          (page "3")
          (variant
            (name "ALT")
            (dnp no)
            (exclude_from_sim no)
            (in_bom yes)
            (on_board yes)
            (field (name "POP") (value "ALT-POP-B")))))))
  (sheet_instances
    (path "" (page "2"))
    (path "/72000000-0000-0000-0000-000000000212" (page "1"))
    (path "/72000000-0000-0000-0000-000000000213" (page "3")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let sheet = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("A") => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(!sheet.dnp);
    assert!(!sheet.excluded_from_sim);
    assert!(sheet.in_bom);
    assert!(sheet.on_board);
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("seed-pop")
    );

    loaded.set_current_variant(Some("ALT"));
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let sheet = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("A") => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(sheet.dnp);
    assert!(sheet.excluded_from_sim);
    assert!(!sheet.in_bom);
    assert!(!sheet.on_board);
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("ALT-POP")
    );

    assert!(loaded.set_current_sheet_path(
        "/72000000-0000-0000-0000-000000000211/72000000-0000-0000-0000-000000000213"
    ));
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let sheet_b = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("B") => Some(sheet),
            _ => None,
        })
        .expect("sheet b");
    assert!(!sheet_b.dnp);
    assert!(!sheet_b.excluded_from_sim);
    assert!(sheet_b.in_bom);
    assert!(sheet_b.on_board);
    assert_eq!(
        sheet_b
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("ALT-POP-B")
    );

    loaded.set_current_variant(None);
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let sheet = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("A") => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(!sheet.dnp);
    assert!(!sheet.excluded_from_sim);
    assert!(sheet.in_bom);
    assert!(sheet.on_board);
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("seed-pop")
    );

    let sheet_b = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("B") => Some(sheet),
            _ => None,
        })
        .expect("sheet b");
    assert!(!sheet_b.dnp);
    assert!(!sheet_b.excluded_from_sim);
    assert!(sheet_b.in_bom);
    assert!(sheet_b.on_board);
    assert_eq!(
        sheet_b
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("seed-pop-b")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_variant_recomputes_intersheet_refs_for_variant_text() {
    let dir = env::temp_dir().join(format!(
        "ki2_current_variant_intersheet_text_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000221")
  (paper "A4")
  (global_label "${DNP}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000231")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000232")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000231/72000000-0000-0000-0000-000000000232"
          (page "1")
          (variant
            (name "ALT")
            (dnp yes))))))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000233")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000231/72000000-0000-0000-0000-000000000233"
          (page "3")
          (variant
            (name "ALT")
            (dnp no))))))
  (sheet_instances
    (path "" (page "2"))
    (path "/72000000-0000-0000-0000-000000000232" (page "1"))
    (path "/72000000-0000-0000-0000-000000000233" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(""),
        Some(&BTreeSet::from([1, 3]))
    );
    assert!(!loaded.intersheet_ref_pages_by_label.contains_key("DNP"));

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));
    loaded.set_current_variant(Some("ALT"));

    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("DNP"),
        Some(&BTreeSet::from([1]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(""),
        Some(&BTreeSet::from([3]))
    );

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_variant_recomputes_intersheet_refs_for_variant_token() {
    let dir = env::temp_dir().join(format!(
        "ki2_current_variant_intersheet_variant_token_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000241")
  (paper "A4")
  (global_label "${VARIANT}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000251")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000252")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000253")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/72000000-0000-0000-0000-000000000252" (page "1"))
    (path "/72000000-0000-0000-0000-000000000253" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(""),
        Some(&BTreeSet::from([1, 3]))
    );
    loaded.set_current_variant(Some("ALT"));
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("ALT"),
        Some(&BTreeSet::from([1, 3]))
    );
    assert!(!loaded.intersheet_ref_pages_by_label.contains_key(""));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_variant_recomputes_intersheet_refs_for_variant_description() {
    let dir = env::temp_dir().join(format!(
        "ki2_current_variant_intersheet_variant_desc_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000261")
  (paper "A4")
  (global_label "${VARIANT_DESC}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000271")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000272")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000273")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/72000000-0000-0000-0000-000000000272" (page "1"))
    (path "/72000000-0000-0000-0000-000000000273" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true },
  "schematic": {
    "variants": [
      { "name": "ALT", "description": "Assembly" }
    ]
  }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));
    loaded.set_current_variant(Some("ALT"));

    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("Assembly"),
        Some(&BTreeSet::from([1, 3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${VARIANT_DESC}")
    );

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn project_current_variant_refreshes_live_sheet_variant_state() {
    let dir = env::temp_dir().join(format!(
        "ki2_project_current_variant_sheet_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000201")
  (paper "A4")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000211")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000212")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch")
    (property "POP" "seed-pop")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000211/72000000-0000-0000-0000-000000000212"
          (page "1")
          (variant
            (name "ALT")
            (dnp yes)
            (exclude_from_sim yes)
            (in_bom no)
            (on_board no)
            (field (name "POP") (value "ALT-POP")))))))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "72000000-0000-0000-0000-000000000213")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch")
    (property "POP" "seed-pop-b")
    (instances
      (project "demo"
        (path "/72000000-0000-0000-0000-000000000211/72000000-0000-0000-0000-000000000213"
          (page "3")
          (variant
            (name "ALT")
            (dnp no)
            (exclude_from_sim no)
            (in_bom yes)
            (on_board yes)
            (field (name "POP") (value "ALT-POP-B")))))))
  (sheet_instances
    (path "" (page "2"))
    (path "/72000000-0000-0000-0000-000000000212" (page "1"))
    (path "/72000000-0000-0000-0000-000000000213" (page "3")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let mut project = SchematicProject::from_load_result(loaded);

    project.set_current_variant(Some("ALT"));
    let root = project.root().expect("root schematic");
    let sheet = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("A") => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(sheet.dnp);
    assert!(sheet.excluded_from_sim);
    assert!(!sheet.in_bom);
    assert!(!sheet.on_board);
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("ALT-POP")
    );

    assert!(project.set_current_sheet_path(
        "/72000000-0000-0000-0000-000000000211/72000000-0000-0000-0000-000000000213"
    ));
    let root = project.root().expect("root schematic");
    let sheet = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("B") => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(!sheet.dnp);
    assert!(!sheet.excluded_from_sim);
    assert!(sheet.in_bom);
    assert!(sheet.on_board);
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("ALT-POP-B")
    );

    project.set_current_variant(None);
    let root = project.root().expect("root schematic");
    let sheet = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("B") => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(!sheet.dnp);
    assert!(!sheet.excluded_from_sim);
    assert!(sheet.in_bom);
    assert!(sheet.on_board);
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("seed-pop-b")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn load_tree_discovers_companion_project_settings() {
    let dir = std::env::temp_dir().join(format!("ki2_companion_project_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"meta\": {\n    \"version\": 2\n  },\n  \"drawing\": {\n    \"intersheets_ref_show\": true,\n    \"intersheets_ref_own_page\": false,\n    \"intersheets_ref_short\": true,\n    \"intersheets_ref_prefix\": \"@\",\n    \"intersheets_ref_suffix\": \"!\"\n  },\n  \"schematic\": {\n    \"page_layout_descr_file\": \"${KIPRJMOD}/custom.kicad_wks\",\n    \"connection_grid_size\": 50.0\n  },\n  \"erc\": {\n    \"rule_severities\": {}\n  }\n}\n",
    )
    .expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project = loaded.project().expect("project settings");
    assert_eq!(project.path, project_path);
    assert_eq!(project.meta_version(), Some(2));
    assert_eq!(project.erc_rule_severity("single_global_label"), None);
    assert!(project.intersheet_refs().show);
    assert!(!project.intersheet_refs().own_page);
    assert!(project.intersheet_refs().short);
    assert_eq!(project.intersheet_refs().prefix, "@");
    assert_eq!(project.intersheet_refs().suffix, "!");
    assert_eq!(
        project.schematic.page_layout_descr_file.as_deref(),
        Some("${KIPRJMOD}/custom.kicad_wks")
    );
    assert_eq!(project.schematic.connection_grid_size_mm, 1.27);
    assert_eq!(
        loaded.current_drawing_sheet_source(),
        DrawingSheetSource::Filesystem(dir.join("custom.kicad_wks"))
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_drawing_sheet_source_prefers_matching_embedded_worksheet() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_embedded_drawing_sheet_source_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch
  (version 20250114)
  (generator "ki2")
  (uuid "60000000-0000-0000-0000-000000000099")
  (paper "A4")
  (embedded_files
    (file
      (name "custom.kicad_wks")
      (checksum deadbeef)
      (type worksheet)
      (data |worksheet-body|))))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_path,
        "{\n  \"schematic\": {\n    \"page_layout_descr_file\": \"custom.kicad_wks\"\n  }\n}\n",
    )
    .expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(loaded.schematics[0].screen.embedded_files.len(), 1);
    assert_eq!(
        loaded.schematics[0].screen.embedded_files[0]
            .name
            .as_deref(),
        Some("custom.kicad_wks")
    );
    assert_eq!(
        loaded.schematics[0].screen.embedded_files[0].file_type,
        Some(EmbeddedFileType::Worksheet)
    );
    assert_eq!(
        loaded.schematics[0].screen.embedded_files[0]
            .data
            .as_deref(),
        Some("worksheet-body")
    );
    assert_eq!(
        loaded.current_drawing_sheet_source(),
        DrawingSheetSource::SchematicEmbedded {
            name: "custom.kicad_wks".to_string(),
            text: "worksheet-body".to_string(),
        }
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn load_tree_rejects_invalid_companion_project_settings() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_invalid_companion_project_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_path = dir.join("demo.kicad_pro");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(&project_path, "{ invalid json\n").expect("write project");

    let err = load_schematic_tree(&root_path).expect_err("invalid project must fail");
    assert!(err.to_string().contains("invalid .kicad_pro json"));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn load_tree_discovers_companion_project_local_settings() {
    let dir = std::env::temp_dir().join(format!(
        "ki2_companion_project_local_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("demo.kicad_sch");
    let project_local_path = dir.join("demo.kicad_prl");

    fs::write(
        &root_path,
        r#"(kicad_sch (version 20231120) (generator "ki2"))"#,
    )
    .expect("write schematic");
    fs::write(
        &project_local_path,
        "{\n  \"schematic\": {\n    \"last_open_sheet\": \"/root/child\"\n  }\n}\n",
    )
    .expect("write project local settings");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let project_local = loaded
        .project_local_settings()
        .expect("project local settings");
    assert_eq!(project_local.path, project_local_path);
    assert_eq!(
        project_local
            .json
            .get("schematic")
            .and_then(|value| value.get("last_open_sheet"))
            .and_then(|value| value.as_str()),
        Some("/root/child")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_local_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_sheet_switch_refreshes_reused_child_sheet_variants() {
    let dir = env::temp_dir().join(format!(
        "ki2_current_variant_reused_child_sheet_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let grand_path = dir.join("grand.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20231120)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000211")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "73000000-0000-0000-0000-000000000212")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "73000000-0000-0000-0000-000000000213")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/73000000-0000-0000-0000-000000000212" (page "2"))
    (path "/73000000-0000-0000-0000-000000000213" (page "3")))
)"#;

    let child_src = r#"(kicad_sch
  (version 20231120)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000220")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "73000000-0000-0000-0000-000000000221")
    (property "Sheetname" "Grand")
    (property "Sheetfile" "grand.kicad_sch")
    (property "POP" "seed-pop-grand")
    (instances
      (project "demo"
        (path "/73000000-0000-0000-0000-000000000211/73000000-0000-0000-0000-000000000212/73000000-0000-0000-0000-000000000221"
          (page "10")
          (variant
            (name "ALT")
            (field (name "POP") (value "ALT-POP-A"))))
        (path "/73000000-0000-0000-0000-000000000211/73000000-0000-0000-0000-000000000213/73000000-0000-0000-0000-000000000221"
          (page "11")
          (variant
            (name "ALT")
            (field (name "POP") (value "ALT-POP-B")))))))
  (sheet_instances
    (path "/73000000-0000-0000-0000-000000000212" (page "2"))
    (path "/73000000-0000-0000-0000-000000000213" (page "3")))
)"#;

    let grand_src = r#"(kicad_sch
  (version 20231120)
  (generator "ki2")
  (uuid "73000000-0000-0000-0000-000000000230")
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&grand_path, grand_src).expect("write grand");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    loaded.set_current_variant(Some("ALT"));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let grand_sheet = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("Grand") => Some(sheet),
            _ => None,
        })
        .expect("grand sheet");
    assert_eq!(
        grand_sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("ALT-POP-A")
    );

    let second_child_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.page.as_deref() == Some("3")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("second child path");
    assert!(loaded.set_current_sheet_path(&second_child_path));
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let grand_sheet = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) if sheet.name() == Some("Grand") => Some(sheet),
            _ => None,
        })
        .expect("grand sheet");
    assert_eq!(
        grand_sheet
            .properties
            .iter()
            .find(|property| property.key == "POP")
            .map(|property| property.value.as_str()),
        Some("ALT-POP-B")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(grand_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn modern_symbol_instances_ignore_empty_value_and_footprint() {
    let dir = env::temp_dir().join(format!(
        "ki2_modern_symbol_instances_empty_payloads_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000101")
  (symbol
    (lib_id "Device:R")
    (uuid "74000000-0000-0000-0000-000000000102")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (instances
      (project "demo"
        (path "/74000000-0000-0000-0000-000000000111/74000000-0000-0000-0000-000000000112"
          (reference "R1")
          (unit 2)
          (value "~")
          (footprint "~")))))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000111")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "74000000-0000-0000-0000-000000000112")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/74000000-0000-0000-0000-000000000112" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let symbol = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .and_then(|schematic| {
            schematic.screen.items.iter().find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
        })
        .expect("child symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(symbol.unit, Some(2));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("seed")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("seed-footprint")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn legacy_symbol_instances_apply_explicit_empty_value_and_footprint() {
    let dir = env::temp_dir().join(format!(
        "ki2_legacy_empty_instances_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000201")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "70000000-0000-0000-0000-000000000202")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000211")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "70000000-0000-0000-0000-000000000212")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/70000000-0000-0000-0000-000000000212" (page "2")))
  (symbol_instances
    (path "/70000000-0000-0000-0000-000000000212/70000000-0000-0000-0000-000000000202"
      (reference "R1")
      (unit 1)
      (value "")
      (footprint "")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load legacy child");
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let symbol = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("child symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R1")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("")
    );
    assert_eq!(symbol.instances.len(), 1);
    assert_eq!(symbol.instances[0].value.as_deref(), Some(""));
    assert_eq!(symbol.instances[0].footprint.as_deref(), Some(""));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn placed_symbols_start_with_mandatory_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000001")
  (symbol
    (lib_id "Device:R"))
)"#;
    let path = temp_schematic("placed_symbol_mandatory_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.properties.len(), 5);
    assert_eq!(
        symbol
            .properties
            .iter()
            .map(|property| property.kind)
            .collect::<Vec<_>>(),
        vec![
            PropertyKind::SymbolReference,
            PropertyKind::SymbolValue,
            PropertyKind::SymbolFootprint,
            PropertyKind::SymbolDatasheet,
            PropertyKind::SymbolDescription,
        ]
    );
    assert!(
        symbol
            .properties
            .iter()
            .all(|property| property.value.is_empty())
    );
    assert_eq!(symbol.prefix, "U");
    assert!(symbol.in_netlist);
    assert_eq!(symbol.unit, Some(1));
    assert_eq!(symbol.body_style, Some(1));

    let _ = fs::remove_file(path);
}

#[test]
fn parser_resets_sheet_fields_autoplaced_before_branch_walk() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000002")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let path = temp_schematic("sheet_fields_autoplaced_reset", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    assert_eq!(sheet.fields_autoplaced, FieldAutoplacement::None);

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_reference_property_updates_prefix() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000003")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "J12")))"#;
    let path = temp_schematic("symbol_prefix_from_reference", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.prefix, "J");
    assert!(symbol.in_netlist);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("J12")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn later_empty_symbol_reference_clears_prefix() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000004")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "J12")
    (property "Reference" "")))"#;
    let path = temp_schematic("empty_symbol_prefix_from_reference", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.prefix, "");
    assert!(symbol.in_netlist);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_at_moves_preparsed_properties_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000005")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R1" (at 3 4 0))
    (property "Value" "10k" (at 5 6 0))
    (at 10 20 90)))"#;
    let path = temp_schematic("symbol_at_moves_preparsed_properties", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.at, [10.0, 20.0]);
    assert_eq!(symbol.angle, 90.0);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .and_then(|property| property.at),
        Some([13.0, 24.0])
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .and_then(|property| property.at),
        Some([15.0, 26.0])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn power_style_reference_updates_symbol_netlist_state() {
    let src = r##"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000006")
  (symbol
    (lib_id "power:GND")
    (property "Reference" "#PWR01")))"##;
    let path = temp_schematic("symbol_in_netlist_from_reference", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.prefix, "#PWR");
    assert!(!symbol.in_netlist);

    let _ = fs::remove_file(path);
}

#[test]
fn text_box_and_table_cell_preserve_hidden_effects_state() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000007")
  (text_box "hidden box" (at 0 0 0) (size 5 5) (effects (hide)))
  (table
    (column_count 1)
    (column_widths 5)
    (row_heights 5)
    (cells
      (table_cell "hidden cell" (at 0 0 0) (size 5 5) (effects (hide)))))
)"#;
    let path = temp_schematic("hidden_text_box_and_table_cell", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert!(!text_box.visible);
    assert!(text_box.has_effects);
    assert!(text_box.effects.as_ref().expect("text box effects").hidden);

    let table = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");
    let cell = table.get_cell(0, 0).expect("table cell");
    assert!(!cell.visible);
    assert!(cell.has_effects);
    assert!(cell.effects.as_ref().expect("table cell effects").hidden);

    let _ = fs::remove_file(path);
}

#[test]
fn table_without_border_or_separators_keeps_constructor_defaults() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "62000000-0000-0000-0000-000000000008")
  (table
    (column_count 1)
    (column_widths 5)
    (row_heights 5)
    (cells
      (table_cell "c1" (at 0 0 0) (size 5 5))))
)"#;
    let path = temp_schematic("table_constructor_defaults", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let table = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");

    assert!(table.border_external);
    assert!(table.border_header);
    assert_eq!(table.border_stroke.width, Some(0.1524));
    assert_eq!(table.border_stroke.style, StrokeStyle::Default);
    assert!(table.separators_rows);
    assert!(table.separators_cols);
    assert_eq!(table.separators_stroke.width, Some(0.1524));
    assert_eq!(table.separators_stroke.style, StrokeStyle::Default);

    let _ = fs::remove_file(path);
}

#[test]
fn sorts_loaded_sheet_pages_numerically() {
    let dir = env::temp_dir().join(format!(
        "ki2_sheet_page_sort_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let a_path = dir.join("a.kicad_sch");
    let b_path = dir.join("b.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000044")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000041")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000042")
    (property "Sheetname" "A")
    (property "Sheetfile" "a.kicad_sch"))
  (sheet
    (uuid "70000000-0000-0000-0000-000000000043")
    (property "Sheetname" "B")
    (property "Sheetfile" "b.kicad_sch"))
  (sheet_instances
    (path "" (page "99"))
    (path "/70000000-0000-0000-0000-000000000041/70000000-0000-0000-0000-000000000042" (page "10"))
    (path "/70000000-0000-0000-0000-000000000041/70000000-0000-0000-0000-000000000043" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&a_path, child_src).expect("write child a");
    fs::write(&b_path, child_src).expect("write child b");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000041/70000000-0000-0000-0000-000000000043"
    );
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("2"));
    assert_eq!(
        loaded.sheet_paths[1].instance_path,
        "/70000000-0000-0000-0000-000000000041/70000000-0000-0000-0000-000000000042"
    );
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("10"));
    assert_eq!(loaded.sheet_paths[2].instance_path, "");
    assert_eq!(loaded.sheet_paths[2].page.as_deref(), Some("99"));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(a_path);
    let _ = fs::remove_file(b_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn sorts_loaded_sheet_pages_with_numeric_before_string_pages() {
    let dir = env::temp_dir().join(format!(
        "ki2_sheet_page_numeric_before_string_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let a_path = dir.join("a.kicad_sch");
    let b_path = dir.join("b.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000144")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000141")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000142")
    (property "Sheetname" "A")
    (property "Sheetfile" "a.kicad_sch"))
  (sheet
    (uuid "70000000-0000-0000-0000-000000000143")
    (property "Sheetname" "B")
    (property "Sheetfile" "b.kicad_sch"))
  (sheet_instances
    (path "" (page "Z"))
    (path "/70000000-0000-0000-0000-000000000141/70000000-0000-0000-0000-000000000142" (page "A1"))
    (path "/70000000-0000-0000-0000-000000000141/70000000-0000-0000-0000-000000000143" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&a_path, child_src).expect("write child a");
    fs::write(&b_path, child_src).expect("write child b");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000141/70000000-0000-0000-0000-000000000143"
    );
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("2"));
    assert_eq!(
        loaded.sheet_paths[1].instance_path,
        "/70000000-0000-0000-0000-000000000141/70000000-0000-0000-0000-000000000142"
    );
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("A1"));
    assert_eq!(loaded.sheet_paths[2].instance_path, "");
    assert_eq!(loaded.sheet_paths[2].page.as_deref(), Some("Z"));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(a_path);
    let _ = fs::remove_file(b_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn sorts_loaded_sheet_pages_with_natural_string_ordering() {
    let dir = env::temp_dir().join(format!(
        "ki2_sheet_page_natural_order_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let a_path = dir.join("a.kicad_sch");
    let b_path = dir.join("b.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000154")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000151")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000152")
    (property "Sheetname" "A")
    (property "Sheetfile" "a.kicad_sch"))
  (sheet
    (uuid "70000000-0000-0000-0000-000000000153")
    (property "Sheetname" "B")
    (property "Sheetfile" "b.kicad_sch"))
  (sheet_instances
    (path "" (page "Z"))
    (path "/70000000-0000-0000-0000-000000000151/70000000-0000-0000-0000-000000000152" (page "A10"))
    (path "/70000000-0000-0000-0000-000000000151/70000000-0000-0000-0000-000000000153" (page "A2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&a_path, child_src).expect("write child a");
    fs::write(&b_path, child_src).expect("write child b");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000151/70000000-0000-0000-0000-000000000153"
    );
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("A2"));
    assert_eq!(
        loaded.sheet_paths[1].instance_path,
        "/70000000-0000-0000-0000-000000000151/70000000-0000-0000-0000-000000000152"
    );
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("A10"));
    assert_eq!(loaded.sheet_paths[2].instance_path, "");
    assert_eq!(loaded.sheet_paths[2].page.as_deref(), Some("Z"));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(a_path);
    let _ = fs::remove_file(b_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn sorts_loaded_sheet_pages_with_leading_zero_natural_equality() {
    let dir = env::temp_dir().join(format!(
        "ki2_sheet_page_leading_zero_order_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let a_path = dir.join("a.kicad_sch");
    let b_path = dir.join("b.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000164")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000161")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000162")
    (property "Sheetname" "A")
    (property "Sheetfile" "a.kicad_sch"))
  (sheet
    (uuid "70000000-0000-0000-0000-000000000163")
    (property "Sheetname" "B")
    (property "Sheetfile" "b.kicad_sch"))
  (sheet_instances
    (path "" (page "Z"))
    (path "/70000000-0000-0000-0000-000000000161/70000000-0000-0000-0000-000000000162" (page "A02"))
    (path "/70000000-0000-0000-0000-000000000161/70000000-0000-0000-0000-000000000163" (page "A2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&a_path, child_src).expect("write child a");
    fs::write(&b_path, child_src).expect("write child b");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000161/70000000-0000-0000-0000-000000000162"
    );
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("A02"));
    assert_eq!(
        loaded.sheet_paths[1].instance_path,
        "/70000000-0000-0000-0000-000000000161/70000000-0000-0000-0000-000000000163"
    );
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("A2"));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(a_path);
    let _ = fs::remove_file(b_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn initializes_sheet_pages_when_all_sheet_instance_pages_are_missing() {
    let dir = env::temp_dir().join(format!(
        "ki2_sheet_page_init_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let a_path = dir.join("a.kicad_sch");
    let b_path = dir.join("b.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000024")
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000021")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000022")
    (property "Sheetname" "A")
    (property "Sheetfile" "a.kicad_sch"))
  (sheet
    (uuid "70000000-0000-0000-0000-000000000023")
    (property "Sheetname" "B")
    (property "Sheetfile" "b.kicad_sch"))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&a_path, child_src).expect("write child a");
    fs::write(&b_path, child_src).expect("write child b");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000021/70000000-0000-0000-0000-000000000022"
    );
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("1"));
    assert_eq!(
        loaded.sheet_paths[1].instance_path,
        "/70000000-0000-0000-0000-000000000021/70000000-0000-0000-0000-000000000023"
    );
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("2"));
    assert_eq!(loaded.sheet_paths[2].instance_path, "");
    assert_eq!(loaded.sheet_paths[2].page, None);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(a_path);
    let _ = fs::remove_file(b_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn sorts_loaded_sheet_paths_with_virtual_order_tiebreak() {
    let dir = env::temp_dir().join(format!(
        "ki2_sheet_path_virtual_sort_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let grandchild_path = dir.join("grandchild.kicad_sch");

    let grandchild_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000035")
)"#;
    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000034")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000033")
    (property "Sheetname" "Grandchild")
    (property "Sheetfile" "grandchild.kicad_sch"))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "70000000-0000-0000-0000-000000000031")
  (sheet
    (uuid "70000000-0000-0000-0000-000000000032")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032" (page "1"))
    (path "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&grandchild_path, grandchild_src).expect("write grandchild");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    assert_eq!(loaded.sheet_paths.len(), 3);
    assert_eq!(
        loaded.sheet_paths[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032"
    );
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("1"));
    assert_eq!(
        loaded.sheet_paths[1].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    );
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("1"));
    assert_eq!(loaded.sheet_paths[2].instance_path, "");
    assert_eq!(loaded.sheet_paths[2].page.as_deref(), Some("2"));
    let ancestors = loaded.ancestor_sheet_paths(
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    );
    assert_eq!(ancestors.len(), 2);
    assert_eq!(
        ancestors[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032"
    );
    assert_eq!(ancestors[1].instance_path, "");
    let root_children = loaded.child_sheet_paths("");
    assert_eq!(root_children.len(), 1);
    assert_eq!(
        root_children[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032"
    );
    let child_children = loaded.child_sheet_paths(
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032",
    );
    assert_eq!(child_children.len(), 1);
    assert_eq!(
        child_children[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    );

    let project = SchematicProject::from_load_result(loaded);
    let project_ancestors = project.ancestor_sheet_paths(
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    );
    assert_eq!(project_ancestors.len(), 2);
    assert_eq!(
        project_ancestors[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032"
    );
    assert_eq!(project_ancestors[1].instance_path, "");
    let project_root_children = project.child_sheet_paths("");
    assert_eq!(project_root_children.len(), 1);
    assert_eq!(
        project_root_children[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032"
    );
    let project_child_children = project.child_sheet_paths(
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032",
    );
    assert_eq!(project_child_children.len(), 1);
    assert_eq!(
        project_child_children[0].instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    );

    let mut project = project;
    assert!(project.set_current_sheet_path(
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    ));
    assert_eq!(
        project
            .current_sheet_path()
            .expect("updated current sheet path")
            .instance_path,
        "/70000000-0000-0000-0000-000000000031/70000000-0000-0000-0000-000000000032/70000000-0000-0000-0000-000000000033"
    );
    assert_eq!(
        project
            .current_schematic()
            .expect("updated current schematic")
            .path,
        grandchild_path
            .canonicalize()
            .unwrap_or(grandchild_path.clone())
    );
    assert!(!project.set_current_sheet_path("/missing"));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(grandchild_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn recomputes_intersheet_refs_from_loaded_sheet_paths() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000003")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000001")
  (paper "A4")
  (global_label "VCC"
    (shape input)
    (at 1 2 0)
    (property "Intersheet References" "stale" (id 0) (hide yes)))
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000002")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/71000000-0000-0000-0000-000000000002" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let root_global = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("root global label");
    let root_property = root_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("root intersheet refs");
    assert_eq!(root_property.value, "");
    assert_eq!(root_property.id, Some(6));
    assert_eq!(root_property.key, "Intersheet References");
    assert!(!root_property.visible);

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "${INTERSHEET_REFS}");
    assert_eq!(child_property.id, Some(6));
    assert_eq!(child_property.key, "Intersheet References");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn recomputing_intersheet_refs_preserves_explicit_visible_property_state() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_visible_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000013")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000011")
  (paper "A4")
  (global_label "VCC"
    (shape input)
    (at 1 2 0)
    (property "Intersheet References" "show" (at 8 9 0)))
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000012")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/71000000-0000-0000-0000-000000000012" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let global = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let property = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs");

    assert_eq!(property.value, "");
    assert!(!property.visible);
    assert_eq!(property.at, Some([8.0, 9.0]));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn recomputes_intersheet_refs_across_reused_screens() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_reused_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000103")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000101")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000102")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000104")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "9"))
    (path "/71000000-0000-0000-0000-000000000102" (page "1"))
    (path "/71000000-0000-0000-0000-000000000104" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let property = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs");

    assert_eq!(property.value, "${INTERSHEET_REFS}");

    let child_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && !sheet_path.instance_path.is_empty()
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child instance path");
    assert!(loaded.set_current_sheet_path(&child_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let property = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs");

    assert_eq!(property.value, "${INTERSHEET_REFS}");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn switching_current_sheet_resets_noncurrent_intersheet_refs() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_switch_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000203")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000201")
  (paper "A4")
  (global_label "VCC" (shape input) (at 1 2 0))
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000202")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "9"))
    (path "/71000000-0000-0000-0000-000000000202" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let child_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && !sheet_path.instance_path.is_empty()
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child instance path");
    assert!(loaded.set_current_sheet_path(&child_instance_path));
    assert!(loaded.set_current_sheet_path(""));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let property = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs");

    assert_eq!(property.value, "${INTERSHEET_REFS}");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn project_setting_hides_current_sheet_intersheet_refs() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_project_setting_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000301")
  (paper "A4")
  (global_label "VCC" (shape input) (at 1 2 0))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": false }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let global = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let property = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs");

    assert_eq!(property.value, "${INTERSHEET_REFS}");
    assert!(!property.visible);
    assert_eq!(property.at, Some([1.0, 2.0]));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn project_setting_excludes_current_page_from_intersheet_refs() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_own_page_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000313")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000311")
  (paper "A4")
  (global_label "VCC" (shape input) (at 1 2 0))
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000312")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/71000000-0000-0000-0000-000000000312" (page "1")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": {
    "intersheets_ref_show": true,
    "intersheets_ref_own_page": false
  }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let root_global = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("root global label");
    let root_property = root_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("root intersheet refs");
    assert_eq!(root_property.value, "[1]");

    let child_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && !sheet_path.instance_path.is_empty()
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child instance path");
    assert!(loaded.set_current_sheet_path(&child_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn project_settings_format_current_sheet_intersheet_refs() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_formatting_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_a_path = dir.join("child_a.kicad_sch");
    let child_b_path = dir.join("child_b.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = |uuid: &str| {
        format!(
            r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "{uuid}")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#
        )
    };
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000321")
  (paper "A4")
  (global_label "VCC" (shape input) (at 1 2 0))
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000322")
    (property "Sheetname" "A")
    (property "Sheetfile" "child_a.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000323")
    (property "Sheetname" "B")
    (property "Sheetfile" "child_b.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000322" (page "2"))
    (path "/71000000-0000-0000-0000-000000000323" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": {
    "intersheets_ref_show": true,
    "intersheets_ref_short": true,
    "intersheets_ref_prefix": "@",
    "intersheets_ref_suffix": "!"
  }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(
        &child_a_path,
        child_src("71000000-0000-0000-0000-000000000324"),
    )
    .expect("write child a");
    fs::write(
        &child_b_path,
        child_src("71000000-0000-0000-0000-000000000325"),
    )
    .expect("write child b");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let root_global = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("root global label");
    let root_property = root_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("root intersheet refs");

    assert_eq!(root_property.value, "@1..2!");
    assert!(root_property.visible);
    assert_ne!(root_property.at, Some(root_global.at));

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_a_path);
    let _ = fs::remove_file(child_b_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_shown_sheet_text() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_shown_text_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000333")
  (paper "A4")
  (global_label "${SHEETNAME}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000332" (page "2"))
    (path "/71000000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${SHEETNAME}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    let child_b_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("B")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child B instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    assert!(loaded.set_current_sheet_path(&child_b_instance_path));
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_cross_referenced_symbol_field() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_cross_ref_symbol_field_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71100000-0000-0000-0000-000000000333")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "71100000-0000-0000-0000-000000000336")
    (instances
      (project "demo"
        (path "/71100000-0000-0000-0000-000000000331/71100000-0000-0000-0000-000000000332"
          (reference "R1")
          (value "10k"))
        (path "/71100000-0000-0000-0000-000000000331/71100000-0000-0000-0000-000000000334"
          (reference "R1")
          (value "22k")))))
  (global_label "${R1:VALUE}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71100000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71100000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71100000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71100000-0000-0000-0000-000000000332" (page "2"))
    (path "/71100000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("10k"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("22k"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${R1:VALUE}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    let child_b_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("B")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child B instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    assert!(loaded.set_current_sheet_path(&child_b_instance_path));
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_parent_symbol_reference() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_parent_symbol_ref_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71200000-0000-0000-0000-000000000333")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "71200000-0000-0000-0000-000000000336")
    (instances
      (project "demo"
        (path "/71200000-0000-0000-0000-000000000331/71200000-0000-0000-0000-000000000332"
          (reference "R1A")
          (value "10k"))
        (path "/71200000-0000-0000-0000-000000000331/71200000-0000-0000-0000-000000000334"
          (reference "R1B")
          (value "22k")))))
  (global_label "${R1:VALUE}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71200000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71200000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71200000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71200000-0000-0000-0000-000000000332" (page "2"))
    (path "/71200000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("10k"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("22k"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${R1:VALUE}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_mark_unknown_cross_references() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_unknown_cross_ref_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71300000-0000-0000-0000-000000000333")
  (paper "A4")
  (global_label "${R9:VALUE}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71300000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71300000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71300000-0000-0000-0000-000000000332" (page "2")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded
            .intersheet_ref_pages_by_label
            .get("<Unknown reference: R9>"),
        Some(&BTreeSet::from([2]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${R9:VALUE}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_net_name() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_net_name_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71400000-0000-0000-0000-000000000333")
  (paper "A4")
  (label "${SHEETNAME}" (at 0 0 0))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "${NET_NAME}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71400000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71400000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71400000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71400000-0000-0000-0000-000000000332" (page "2"))
    (path "/71400000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("/A/A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("/B/B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${NET_NAME}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    let child_b_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("B")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child B instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label)
                if label.kind == LabelKind::Global && label.text == "${NET_NAME}" =>
            {
                Some(label)
            }
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    assert!(loaded.set_current_sheet_path(&child_b_instance_path));
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label)
                if label.kind == LabelKind::Global && label.text == "${NET_NAME}" =>
            {
                Some(label)
            }
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_sheet_pin_name() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_sheet_pin_net_name_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71450000-0000-0000-0000-000000000999")
  (paper "A4"))"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71450000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71450000-0000-0000-0000-000000000002")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch")
    (pin "OUT_A" output (at 10 5 0)))
  (wire (pts (xy 10 5) (xy 20 5)))
  (global_label "${NET_NAME}" (shape input) (at 20 5 0))
  (sheet
    (at 0 20)
    (size 10 10)
    (uuid "71450000-0000-0000-0000-000000000003")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch")
    (pin "OUT_B" output (at 10 25 0)))
  (wire (pts (xy 10 25) (xy 20 25)))
  (global_label "${NET_NAME}" (shape input) (at 20 25 0))
  (sheet_instances
    (path "" (page "1"))
    (path "/71450000-0000-0000-0000-000000000002" (page "2"))
    (path "/71450000-0000-0000-0000-000000000003" (page "3"))))"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert!(
        loaded.intersheet_ref_pages_by_label.contains_key("/OUT_A"),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );
    assert!(
        loaded.intersheet_ref_pages_by_label.contains_key("/OUT_B"),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${NET_NAME}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_short_net_name() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_short_net_name_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71500000-0000-0000-0000-000000000333")
  (paper "A4")
  (label "${SHEETNAME}" (at 0 0 0))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "${SHORT_NET_NAME}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71500000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71500000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71500000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71500000-0000-0000-0000-000000000332" (page "2"))
    (path "/71500000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${SHORT_NET_NAME}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_net_class() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_net_class_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71600000-0000-0000-0000-000000000333")
  (paper "A4")
  (label "${SHEETNAME}" (at 0 0 0))
  (wire (pts (xy 0 0) (xy 10 0)))
  (directive_label "D" (shape dot) (at 5 0 0)
    (property "Netclass" "${SHEETNAME}"))
  (global_label "${NET_CLASS}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71600000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71600000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71600000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71600000-0000-0000-0000-000000000332" (page "2"))
    (path "/71600000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${NET_CLASS}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label)
                if label.kind == LabelKind::Global && label.text == "${NET_CLASS}" =>
            {
                Some(label)
            }
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_driver_label_net_class() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_driver_label_net_class_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71610000-0000-0000-0000-000000000333")
  (paper "A4")
  (global_label "${SHEETNAME}" (shape input) (at 0 0 0)
    (property "Netclass" "${SHEETNAME}"))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "${NET_CLASS}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71610000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71610000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71610000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71610000-0000-0000-0000-000000000332" (page "2"))
    (path "/71610000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${NET_CLASS}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label)
                if label.kind == LabelKind::Global && label.text == "${NET_CLASS}" =>
            {
                Some(label)
            }
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_bus_member_net_class() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_bus_member_net_class_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71615000-0000-0000-0000-000000000331")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (global_label "DATA[0..3]" (shape input) (at 10 0 0)
    (property "Netclass" "FAST"))
  (bus_entry (at 0 0) (size -5 5))
  (wire (pts (xy -5 5) (xy -10 5)))
  (global_label "${NET_CLASS}" (shape input) (at -10 5 0))
  (sheet_instances
    (path "" (page "1"))))"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert!(
        loaded.intersheet_ref_pages_by_label.contains_key("FAST"),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${NET_CLASS}"),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_rule_area_net_class() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_rule_area_net_class_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71700000-0000-0000-0000-000000000333")
  (paper "A4")
  (label "${SHEETNAME}" (at 0 0 0))
  (wire (pts (xy 0 0) (xy 10 0)))
  (rule_area
    (polyline (pts (xy -1 -1) (xy 11 -1) (xy 11 2) (xy -1 2))))
  (directive_label "D" (shape dot) (at 5 1 0)
    (property "Netclass" "${SHEETNAME}"))
  (global_label "${NET_CLASS}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71700000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71700000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71700000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71700000-0000-0000-0000-000000000332" (page "2"))
    (path "/71700000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("A"),
        Some(&BTreeSet::from([2])),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("B"),
        Some(&BTreeSet::from([3])),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${NET_CLASS}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_symbol_pin_net_name() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_symbol_pin_net_name_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71800000-0000-0000-0000-000000000333")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R")
      (property "Value" "R")
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54) (name "IN") (number "1"))
        (pin passive line (at 20 0 180) (length 2.54) (name "OUT") (number "2")))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (property "Reference" "R1")
    (property "Value" "10k"))
  (label "${SHEETNAME}" (at 0 0 0))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "${R1:NET_NAME(1)}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71800000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71800000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71800000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71800000-0000-0000-0000-000000000332" (page "2"))
    (path "/71800000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("/A/A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("/B/B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${R1:NET_NAME(1)}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_symbol_pin_name() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_symbol_pin_name_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71900000-0000-0000-0000-000000000333")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R")
      (property "Value" "R")
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54) (name "IN") (number "1"))
        (pin passive line (at 20 0 180) (length 2.54) (name "OUT") (number "2")))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (property "Reference" "R1")
    (property "Value" "10k"))
  (global_label "${R1:PIN_NAME(1)}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71900000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71900000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71900000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71900000-0000-0000-0000-000000000332" (page "2"))
    (path "/71900000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("IN"),
        Some(&BTreeSet::from([2, 3])),
        "{:?}",
        loaded.intersheet_ref_pages_by_label
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${R1:PIN_NAME(1)}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_symbol_pin_net_class() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_symbol_pin_net_class_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71950000-0000-0000-0000-000000000333")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R")
      (property "Value" "R")
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54) (name "IN") (number "1"))
        (pin passive line (at 20 0 180) (length 2.54) (name "OUT") (number "2")))))
  (symbol
    (lib_id "Device:R")
    (at 0 0 0)
    (property "Reference" "R1")
    (property "Value" "10k"))
  (label "${SHEETNAME}" (at 0 0 0))
  (wire (pts (xy 0 0) (xy 10 0)))
  (directive_label "D" (shape dot) (at 5 0 0)
    (property "Netclass" "${SHEETNAME}"))
  (global_label "${R1:NET_CLASS(1)}" (shape input) (at 10 0 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71950000-0000-0000-0000-000000000331")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71950000-0000-0000-0000-000000000332")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71950000-0000-0000-0000-000000000334")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71950000-0000-0000-0000-000000000332" (page "2"))
    (path "/71950000-0000-0000-0000-000000000334" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${R1:NET_CLASS(1)}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_project_name() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_project_name_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000351")
  (paper "A4")
  (global_label "${PROJECTNAME}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000361")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000362")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000363")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000362" (page "2"))
    (path "/71000000-0000-0000-0000-000000000363" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("root"),
        Some(&BTreeSet::from([2, 3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${PROJECTNAME}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_project_text_variable() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_project_var_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000371")
  (paper "A4")
  (global_label "${BUS}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000381")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000382")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000383")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000382" (page "2"))
    (path "/71000000-0000-0000-0000-000000000383" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true },
  "text_variables": { "BUS": "CLK_MAIN" }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("CLK_MAIN"),
        Some(&BTreeSet::from([2, 3]))
    );
    assert!(!loaded.intersheet_ref_pages_by_label.contains_key("${BUS}"));

    let child_b_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("B")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child B instance path");
    assert!(loaded.set_current_sheet_path(&child_b_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_current_date() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_current_date_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000391")
  (paper "A4")
  (global_label "${CURRENT_DATE}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000401")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000402")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000403")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000402" (page "2"))
    (path "/71000000-0000-0000-0000-000000000403" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let shown_date = loaded
        .intersheet_ref_pages_by_label
        .keys()
        .find(|label| *label != "${CURRENT_DATE}")
        .cloned()
        .expect("resolved current date label");
    assert_eq!(shown_date.len(), 10);
    assert_eq!(&shown_date[4..5], "-");
    assert_eq!(&shown_date[7..8], "-");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(&shown_date),
        Some(&BTreeSet::from([2, 3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${CURRENT_DATE}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_vcs_hash() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_vcshash_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000411")
  (paper "A4")
  (global_label "${VCSHASH}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000421")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000422")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000423")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000422" (page "2"))
    (path "/71000000-0000-0000-0000-000000000423" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");
    let (full_hash, _) = init_git_repo_with_head(&dir);

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(&full_hash),
        Some(&BTreeSet::from([2, 3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${VCSHASH}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_vcs_short_hash() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_vcsshorthash_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000431")
  (paper "A4")
  (global_label "${VCSSHORTHASH}" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000441")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000442")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000443")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000442" (page "2"))
    (path "/71000000-0000-0000-0000-000000000443" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");
    let (_, short_hash) = init_git_repo_with_head(&dir);

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(&short_hash),
        Some(&BTreeSet::from([2, 3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${VCSSHORTHASH}")
    );

    let child_b_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("B")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child B instance path");
    assert!(loaded.set_current_sheet_path(&child_b_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_filename() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_filename_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_a_path = dir.join("child_a.kicad_sch");
    let child_b_path = dir.join("child_b.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = |uuid: &str| {
        format!(
            r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "{uuid}")
  (paper "A4")
  (global_label "${{FILENAME}}" (shape input) (at 10 10 0))
)"#
        )
    };
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000451")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000452")
    (property "Sheetname" "A")
    (property "Sheetfile" "child_a.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000453")
    (property "Sheetname" "B")
    (property "Sheetfile" "child_b.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000452" (page "2"))
    (path "/71000000-0000-0000-0000-000000000453" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(
        &child_a_path,
        child_src("71000000-0000-0000-0000-000000000461"),
    )
    .expect("write child a");
    fs::write(
        &child_b_path,
        child_src("71000000-0000-0000-0000-000000000462"),
    )
    .expect("write child b");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded
            .intersheet_ref_pages_by_label
            .get("child_a.kicad_sch"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded
            .intersheet_ref_pages_by_label
            .get("child_b.kicad_sch"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${FILENAME}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child_a.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child_a.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_a_path);
    let _ = fs::remove_file(child_b_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_filepath() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_filepath_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_a_path = dir.join("child_a.kicad_sch");
    let child_b_path = dir.join("child_b.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = |uuid: &str| {
        format!(
            r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "{uuid}")
  (paper "A4")
  (global_label "${{FILEPATH}}" (shape input) (at 10 10 0))
)"#
        )
    };
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000471")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000472")
    (property "Sheetname" "A")
    (property "Sheetfile" "child_a.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000473")
    (property "Sheetname" "B")
    (property "Sheetfile" "child_b.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000472" (page "2"))
    (path "/71000000-0000-0000-0000-000000000473" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(
        &child_a_path,
        child_src("71000000-0000-0000-0000-000000000481"),
    )
    .expect("write child a");
    fs::write(
        &child_b_path,
        child_src("71000000-0000-0000-0000-000000000482"),
    )
    .expect("write child b");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let child_a_key = child_a_path
        .canonicalize()
        .expect("canonical child a path")
        .to_string_lossy()
        .into_owned();
    let child_b_key = child_b_path
        .canonicalize()
        .expect("canonical child b path")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(&child_a_key),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get(&child_b_key),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${FILEPATH}")
    );

    let child_b_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child_b.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("B")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child B instance path");
    assert!(loaded.set_current_sheet_path(&child_b_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child_b.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_a_path);
    let _ = fs::remove_file(child_b_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_connection_type() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_connection_type_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000491")
  (paper "A4")
  (global_label "${CONNECTION_TYPE}" (shape output) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000501")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000502")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000503")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000502" (page "2"))
    (path "/71000000-0000-0000-0000-000000000503" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");
    fs::write(&project_path, project_src).expect("write project");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("Output"),
        Some(&BTreeSet::from([2, 3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${CONNECTION_TYPE}")
    );

    let child_a_instance_path = loaded
        .sheet_paths
        .iter()
        .find(|sheet_path| {
            sheet_path.schematic_path.ends_with("child.kicad_sch")
                && sheet_path.sheet_name.as_deref() == Some("A")
        })
        .map(|sheet_path| sheet_path.instance_path.clone())
        .expect("child A instance path");
    assert!(loaded.set_current_sheet_path(&child_a_instance_path));

    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let child_global = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("child global label");
    let child_property = child_global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("child intersheet refs");
    assert_eq!(child_property.value, "[1,2]");

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn intersheet_refs_group_global_labels_by_title_block_text() {
    let dir = env::temp_dir().join(format!(
        "ki2_intersheet_refs_title_text_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_a_path = dir.join("child_a.kicad_sch");
    let child_b_path = dir.join("child_b.kicad_sch");
    let project_path = dir.join("root.kicad_pro");

    let child_src = |uuid: &str, title: &str| {
        format!(
            r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "{uuid}")
  (paper "A4")
  (title_block (title "{title}"))
  (global_label "${{TITLE}}" (shape input) (at 10 10 0))
)"#
        )
    };
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000341")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000342")
    (property "Sheetname" "A")
    (property "Sheetfile" "child_a.kicad_sch"))
  (sheet
    (at 20 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000343")
    (property "Sheetname" "B")
    (property "Sheetfile" "child_b.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000342" (page "2"))
    (path "/71000000-0000-0000-0000-000000000343" (page "3")))
)"#;
    let project_src = r#"{
  "meta": { "version": 2 },
  "drawing": { "intersheets_ref_show": true }
}"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(
        &child_a_path,
        child_src("71000000-0000-0000-0000-000000000344", "TITLE-A"),
    )
    .expect("write child a");
    fs::write(
        &child_b_path,
        child_src("71000000-0000-0000-0000-000000000345", "TITLE-B"),
    )
    .expect("write child b");
    fs::write(&project_path, project_src).expect("write project");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("TITLE-A"),
        Some(&BTreeSet::from([2]))
    );
    assert_eq!(
        loaded.intersheet_ref_pages_by_label.get("TITLE-B"),
        Some(&BTreeSet::from([3]))
    );
    assert!(
        !loaded
            .intersheet_ref_pages_by_label
            .contains_key("${TITLE}")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_a_path);
    let _ = fs::remove_file(child_b_path);
    let _ = fs::remove_file(project_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn current_sheet_refresh_materializes_shape_hatching() {
    let dir = env::temp_dir().join(format!(
        "ki2_shape_hatching_refresh_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000191")
  (rectangle
    (start 0 0)
    (end 10 6)
    (stroke (width 0.2) (type solid))
    (fill (type hatch))
    (uuid "71000000-0000-0000-0000-000000000192"))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000181")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000182")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "1"))
    (path "/71000000-0000-0000-0000-000000000182" (page "2")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let mut loaded = load_schematic_tree(&root_path).expect("load tree");
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let shape = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Rectangle => Some(shape),
            _ => None,
        })
        .expect("child rectangle");
    assert!(shape.hatch_lines.is_empty());
    assert!(shape.hatch_dirty);

    assert!(loaded.set_current_sheet_path(
        "/71000000-0000-0000-0000-000000000181/71000000-0000-0000-0000-000000000182"
    ));
    let child = loaded.current_schematic().expect("current child schematic");
    let shape = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Rectangle => Some(shape),
            _ => None,
        })
        .expect("child rectangle");
    assert!(!shape.hatch_lines.is_empty());
    assert!(!shape.hatch_dirty);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn updates_symbol_references_from_loaded_sheet_paths() {
    let dir = env::temp_dir().join(format!(
        "ki2_update_screen_refs_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000033")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "71000000-0000-0000-0000-000000000021")
    (property "Reference" "R?" (at 1 2 90) (hide yes) (show_name no))
    (property "Value" "seed" (at 3 4 180) (do_not_autoplace yes))
    (property "Footprint" "seed-footprint" (at 5 6 270))
    (at 10 10 0)
    (instances
      (project "demo"
        (path "/71000000-0000-0000-0000-000000000031/71000000-0000-0000-0000-000000000032"
          (reference "R11")
          (unit 3)
          (value "22k")
          (footprint "Resistor_SMD:R_0402")))))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000031")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000032")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/71000000-0000-0000-0000-000000000032" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let symbol = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.unit, Some(3));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R11")
    );
    let reference = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolReference)
        .expect("reference");
    assert_eq!(reference.at, Some([11.0, 12.0]));
    assert_eq!(reference.angle, Some(90.0));
    assert!(!reference.visible);
    assert!(!reference.show_name);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("22k")
    );
    let value = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .expect("value");
    assert_eq!(value.at, Some([13.0, 14.0]));
    assert_eq!(value.angle, Some(180.0));
    assert!(!value.can_autoplace);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0402")
    );
    let footprint = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolFootprint)
        .expect("footprint");
    assert_eq!(footprint.at, Some([15.0, 16.0]));
    assert_eq!(footprint.angle, Some(270.0));
    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn update_all_screen_references_refreshes_instance_value_and_footprint_after_late_properties() {
    let dir = env::temp_dir().join(format!(
        "ki2_update_screen_refs_value_footprint_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let child_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000053")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "71000000-0000-0000-0000-000000000041")
    (property "Reference" "R?" (at 1 2 90))
    (property "Value" "seed" (at 3 4 180))
    (property "Footprint" "seed-footprint" (at 5 6 270))
    (at 10 10 0)
    (instances
      (project "demo"
        (path "/71000000-0000-0000-0000-000000000051/71000000-0000-0000-0000-000000000052"
          (reference "R11")
          (unit 3)
          (value "22k")
          (footprint "Resistor_SMD:R_0402"))))
    (property "Value" "late-stale")
    (property "Footprint" "late-stale-footprint"))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000051")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "71000000-0000-0000-0000-000000000052")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/71000000-0000-0000-0000-000000000052" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let child = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("child.kicad_sch"))
        .expect("child schematic");
    let symbol = child
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("22k")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0402")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn fixes_legacy_global_power_symbol_value_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_legacy_power_fix_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20230220)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000061")
  (paper "A4")
  (lib_symbols
    (symbol "power:VCC"
      (power global)
      (property "Value" "OLDLIB")
      (symbol "VCC_1_1"
        (pin power_in line
          hide
          (at 0 0 0)
          (length 0)
          (name "VCC")
          (number "1")))))
  (symbol
    (lib_id "power:VCC")
    (property "Value" "WRONG")
    (at 10 10 0)
    (uuid "71000000-0000-0000-0000-000000000062"))
)"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let lib_symbol = root.screen.lib_symbols.first().expect("lib symbol");
    assert!(lib_symbol.power);
    assert!(!lib_symbol.local_power);
    let first_pin = lib_symbol
        .units
        .iter()
        .flat_map(|unit| unit.draw_items.iter())
        .find(|item| item.kind == "pin")
        .expect("lib pin");
    assert_eq!(first_pin.electrical_type.as_deref(), Some("power_in"));
    assert!(!first_pin.visible);
    assert_eq!(first_pin.name.as_deref(), Some("VCC"));
    let symbol = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("VCC")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn does_not_fix_local_or_visible_power_symbol_values_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_legacy_power_no_fix_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20230220)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000161")
  (paper "A4")
  (lib_symbols
    (symbol "power:LOCAL"
      (power local)
      (property "Value" "OLDLOCAL")
      (symbol "LOCAL_1_1"
        (pin power_in line
          hide
          (at 0 0 0)
          (length 0)
          (name "LOCAL")
          (number "1"))))
    (symbol "power:VISIBLE"
      (power global)
      (property "Value" "OLDVISIBLE")
      (symbol "VISIBLE_1_1"
        (pin power_in line
          (at 0 0 0)
          (length 0)
          (name "VISIBLE")
          (number "1")))))
  (symbol
    (lib_id "power:LOCAL")
    (property "Value" "WRONG-LOCAL")
    (at 10 10 0)
    (uuid "71000000-0000-0000-0000-000000000162"))
  (symbol
    (lib_id "power:VISIBLE")
    (property "Value" "WRONG-VISIBLE")
    (at 20 10 0)
    (uuid "71000000-0000-0000-0000-000000000163"))
)"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let symbols = root
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        symbols[0]
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("WRONG-LOCAL")
    );
    assert_eq!(
        symbols[1]
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("WRONG-VISIBLE")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn preserves_power_symbol_reference_metadata_during_annotation() {
    let dir = env::temp_dir().join(format!(
        "ki2_power_annotation_metadata_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000071")
  (paper "A4")
  (lib_symbols
    (symbol "power:VCC"
      (power global)
      (symbol "VCC_1_1"
        (pin power_in line
          hide
          (at 0 0 0)
          (length 0)
          (name "VCC")
          (number "1")))))
  (symbol
    (lib_id "power:VCC")
    (property "Reference" "PWR" (at 1 2 90) (hide yes))
    (at 10 10 0)
    (uuid "71000000-0000-0000-0000-000000000072"))
)"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let symbol = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    let reference = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolReference)
        .expect("reference");
    assert_eq!(reference.value, "#PWR");
    assert_eq!(reference.at, Some([11.0, 12.0]));
    assert_eq!(reference.angle, Some(90.0));
    assert!(!reference.visible);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn fixes_legacy_global_label_intersheet_ref_position_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_global_label_iref_fixup_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000081")
  (global_label "VCC" (at 10 20 0))
)"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let label = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let intersheet_refs = label
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs field");

    assert_eq!(intersheet_refs.at, Some([10.0, 20.0]));
    assert!(!intersheet_refs.visible);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn annotates_power_symbol_references_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_power_annotation_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000091")
  (paper "A4")
  (lib_symbols
    (symbol "power:VCC"
      (power global)
      (property "Reference" "VCC")
      (symbol "VCC_1_1"
        (pin power_in line
          hide
          (at 0 0 0)
          (length 0)
          (name "VCC")
          (number "1")))))
  (symbol
    (lib_id "power:VCC")
    (property "Reference" "VCC1")
    (at 10 10 0)
    (uuid "71000000-0000-0000-0000-000000000092")))
"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let symbol = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("#VCC1")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn annotates_duplicate_power_symbol_references_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_power_annotation_duplicates_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000191")
  (paper "A4")
  (lib_symbols
    (symbol "power:VCC"
      (power global)
      (property "Reference" "VCC")
      (symbol "VCC_1_1"
        (pin power_in line
          hide
          (at 0 0 0)
          (length 0)
          (name "VCC")
          (number "1")))))
  (symbol
    (lib_id "power:VCC")
    (property "Reference" "VCC1")
    (at 10 10 0)
    (uuid "71000000-0000-0000-0000-000000000192"))
  (symbol
    (lib_id "power:VCC")
    (property "Reference" "VCC1")
    (at 20 10 0)
    (uuid "71000000-0000-0000-0000-000000000193")))
"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let references = root
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Symbol(symbol) => symbol
                .properties
                .iter()
                .find(|property| property.kind == PropertyKind::SymbolReference)
                .map(|property| property.value.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(references, vec!["#VCC1".to_string(), "#VCC".to_string()]);

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn annotates_local_power_symbol_references_after_load() {
    let dir = env::temp_dir().join(format!(
        "ki2_local_power_annotation_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "71000000-0000-0000-0000-000000000291")
  (paper "A4")
  (lib_symbols
    (symbol "power:LOCAL"
      (power local)
      (property "Reference" "LOCAL")
      (symbol "LOCAL_1_1"
        (pin power_in line
          hide
          (at 0 0 0)
          (length 0)
          (name "LOCAL")
          (number "1")))))
  (symbol
    (lib_id "power:LOCAL")
    (property "Reference" "L_PWR1")
    (at 10 10 0)
    (uuid "71000000-0000-0000-0000-000000000292")))
"#;

    fs::write(&root_path, root_src).expect("write root");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    let root = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path.ends_with("root.kicad_sch"))
        .expect("root schematic");
    let symbol = root
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("#L_PWR1")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn rejects_direct_ancestor_sheet_cycles() {
    let dir = env::temp_dir().join(format!(
        "ki2_cycle_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("mkdir");
    let root_path = dir.join("root.kicad_sch");
    let child_path = dir.join("child.kicad_sch");

    let root_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000001")
  (paper "A4")
  (sheet (at 0 0) (size 10 10) (uuid "72000000-0000-0000-0000-000000000002")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let child_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "72000000-0000-0000-0000-000000000003")
  (paper "A4")
  (sheet (at 0 0) (size 10 10) (uuid "72000000-0000-0000-0000-000000000004")
    (property "Sheetname" "RootAgain")
    (property "Sheetfile" "root.kicad_sch"))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let err = load_schematic_tree(&root_path).expect_err("must reject cycle");
    assert!(
        err.to_string()
            .contains("sheet recursion detected through direct ancestors")
    );

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn rejects_unknown_top_level_sections() {
    let err =
        parse_schematic_file(&fixture("sample.kicad_sch")).expect_err("must reject future token");
    let message = err.to_string();
    assert!(
        message.contains(
            "expecting bitmap, bus, bus_alias, bus_entry, class_label, embedded_files, global_label, hierarchical_label, junction, label, line, no_connect, page, paper, rule_area, sheet, symbol, symbol_instances, text, title_block"
        )
    );
}

#[test]
fn rejects_invalid_generator_and_legacy_host_tokens() {
    let bad_generator = r#"(kicad_sch
  (version 20260306)
  (generator (bogus))
  (uuid "74000000-0000-0000-0000-000000000001")
)"#;
    let bad_generator_path = temp_schematic("bad_generator_token", bad_generator);
    let err = parse_schematic_file(Path::new(&bad_generator_path))
        .expect_err("must reject invalid generator token");
    assert!(err.to_string().contains("expecting symbol"));

    let bad_host = r#"(kicad_sch
  (version 20200826)
  (host "eeschema" (bogus))
  (uuid "74000000-0000-0000-0000-000000000002")
)"#;
    let bad_host_path = temp_schematic("bad_legacy_host_version_token", bad_host);
    let err = parse_schematic_file(Path::new(&bad_host_path))
        .expect_err("must reject invalid legacy host version token");
    assert!(err.to_string().contains("expecting symbol"));

    let _ = fs::remove_file(bad_generator_path);
    let _ = fs::remove_file(bad_host_path);
}

#[test]
fn rejects_invalid_title_block_comment_number() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000003")
  (paper "A4")
  (title_block (comment 10 "bad"))
)"#;
    let path = temp_schematic("bad_title_block_comment", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject invalid comment");
    assert!(
        err.to_string()
            .contains("Invalid title block comment number")
    );
    match &err {
        Error::Validation { diagnostic, .. } => {
            assert!(matches!(diagnostic.kind, DiagnosticKind::Validation));
            let span = diagnostic.span.expect("diagnostic span");
            assert_eq!(&src[span.start..span.end], "10");
            assert_eq!(diagnostic.line, Some(6));
            assert_eq!(diagnostic.column, Some(25));
            assert!(err.to_string().contains(&format!(
                ":{}:{}",
                diagnostic.line.expect("line"),
                diagnostic.column.expect("column")
            )));
        }
        other => panic!("expected validation error, got {other:?}"),
    }
    let _ = fs::remove_file(path);

    let valid_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000004")
  (paper "A4")
  (title_block (comment 9 "ok"))
)"#;
    let valid_path = temp_schematic("title_block_comment_nine", valid_src);
    let schematic =
        parse_schematic_file(Path::new(&valid_path)).expect("must accept comment slot 9");
    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.comment(9), Some("ok"));
    let _ = fs::remove_file(valid_path);

    let numeric_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000005")
  (paper "A4")
  (title_block (title 2026) (rev 7) (comment 1 99))
)"#;
    let numeric_path = temp_schematic("title_block_numeric_values", numeric_src);
    let schematic =
        parse_schematic_file(Path::new(&numeric_path)).expect("must accept numeric title values");
    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.title.as_deref(), Some("2026"));
    assert_eq!(title_block.revision.as_deref(), Some("7"));
    assert_eq!(title_block.comment(1), Some("99"));
    let _ = fs::remove_file(numeric_path);
}

#[test]
fn parser_normalizes_legacy_short_uuids_and_increments_duplicates() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "1")
  (paper "A4")
  (junction (at 0 0) (uuid "1"))
  (no_connect (at 1 1) (uuid "1"))
)"#;
    let path = temp_schematic("legacy_and_duplicate_uuids", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert_eq!(
        schematic.screen.uuid.as_deref(),
        Some("00000000-0000-0000-0000-000000000001")
    );

    let junction = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Junction(junction) => Some(junction),
            _ => None,
        })
        .expect("junction");
    assert_eq!(
        junction.uuid.as_deref(),
        Some("00000000-0000-0000-0000-000000000002")
    );

    let no_connect = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::NoConnect(no_connect) => Some(no_connect),
            _ => None,
        })
        .expect("no_connect");
    assert_eq!(
        no_connect.uuid.as_deref(),
        Some("00000000-0000-0000-0000-000000000003")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parser_replaces_malformed_uuids_with_generated_ids() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-symbolic-id")
  (paper "A4")
  (junction (at 0 0) (uuid "junction-symbolic-id"))
  (no_connect (at 1 1) (uuid "junction-symbolic-id"))
)"#;
    let path = temp_schematic("malformed_symbolic_uuids", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let root_uuid = schematic.screen.uuid.as_deref().expect("root uuid");
    assert!(Uuid::parse_str(root_uuid).is_ok());

    let junction = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Junction(junction) => Some(junction),
            _ => None,
        })
        .expect("junction");
    let junction_uuid = junction.uuid.as_deref().expect("junction uuid");
    assert!(Uuid::parse_str(junction_uuid).is_ok());

    let no_connect = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::NoConnect(no_connect) => Some(no_connect),
            _ => None,
        })
        .expect("no_connect");
    let no_connect_uuid = no_connect.uuid.as_deref().expect("no_connect uuid");
    assert!(Uuid::parse_str(no_connect_uuid).is_ok());

    assert_ne!(root_uuid, "root-symbolic-id");
    assert_ne!(junction_uuid, "junction-symbolic-id");
    assert_ne!(no_connect_uuid, "junction-symbolic-id");
    assert_ne!(junction_uuid, no_connect_uuid);

    let _ = fs::remove_file(path);
}

#[test]
fn parses_extended_top_level_sections() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (generator_version "9.0")
  (uuid "74000000-0000-0000-0000-000000000006")
  (paper "A4")
  (title_block (title "Demo") (date "2026-02-25") (rev "A") (company "Acme") (comment 1 "c1"))
  (bus_alias "ADDR" (members "A0" "A1" "A2"))
  (embedded_fonts no)
  (embedded_files
    (file (name "A.bin") (data |abc|))
    (file (name "B.bin") (data |def|)))
  (lib_symbols
    (symbol "Device:R"
      (power local)
      (body_styles demorgan "N" "P")
      (pin_names (offset 0.5) hide)
      (pin_numbers (hide no))
      (exclude_from_sim yes)
      (in_bom no)
      (on_board yes)
      (in_pos_files no)
      (duplicate_pin_numbers_are_jumpers yes)
      (jumper_pin_groups ("1" "2") ("3" "4"))
      (property "ki_keywords" "analog precision")
      (property "ki_description" "precision resistor")
      (property "ki_fp_filters" "R_* 0603")
      (property "ki_locked" "1")
      (property "Reference" "U" (id 0) (at 1 2 0) (hide yes) (effects (font (size 1 1))))
      (extends "Device:Base")
      (circle)
      (symbol "R_1_1"
        (unit_name "Amplifier")
        (arc (start 0 0) (mid 1 1) (end 2 0) (stroke (width 0.1)))
        (text "AMP" (at 3 4 90) (effects (font (size 1.2 1.3)) (hide)))
        (text_box private "TB" (at 1 2 0) (size 3 4))
        (pin input line
          (at 5 6 180)
          (length 2.5)
          (name "IN" (effects (font (size 0.8 0.9))))
          (number "1" (effects (font (size 1.1 1.2))))
          (alternate "ALT" output clock)))
      (embedded_fonts yes)
      (embedded_files (file (name "sym.bin") (data |xyz|)))))
  (text "hello" (at 10 20 0) (uuid "74000000-0000-0000-0000-000000000007"))
  (text_box "box" (at 0 0 0) (size 5 5) (margins 1 2 3 4) (uuid "74000000-0000-0000-0000-000000000008"))
  (table
    (column_count 2)
    (column_widths 10 20)
    (row_heights 30)
    (cells
      (table_cell "c1" (at 0 0 0) (size 5 5))
      (table_cell "c2" (at 5 0 0) (size 5 5) (span 2 1)))
    (border (external yes) (header no) (stroke (width 0.3) (type solid)))
    (separators (rows yes) (cols no) (stroke (width 0.4) (type dash)))
    (uuid "74000000-0000-0000-0000-000000000009"))
  (image
    (at 1 2)
    (scale 2.5)
    (data
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptV"
      "AAAACklEQVR4nGNgAAAAAgABSK+kcQAAAABJRU5ErkJggg==")
    (uuid "74000000-0000-0000-0000-00000000000a"))
  (polyline (pts (xy 0 0) (xy 1 1)) (uuid "74000000-0000-0000-0000-00000000000b"))
  (arc (start 0 0) (mid 1 1) (end 2 0) (uuid "74000000-0000-0000-0000-00000000000c"))
  (circle (center 0 0) (radius 1) (uuid "74000000-0000-0000-0000-00000000000d"))
  (rectangle (start 0 0) (end 2 2) (uuid "74000000-0000-0000-0000-00000000000e"))
  (bezier (pts (xy 0 0) (xy 1 1) (xy 2 1) (xy 3 0)) (uuid "74000000-0000-0000-0000-00000000000f"))
  (rule_area (polyline (pts (xy 0 0) (xy 2 0) (xy 2 2)) (uuid "74000000-0000-0000-0000-000000000010")) (exclude_from_sim yes) (in_bom no) (on_board yes) (dnp no))
  (directive_label "D" (shape input) (at 10 10 0) (uuid "74000000-0000-0000-0000-000000000011"))
  (netclass_flag "N" (shape input) (at 11 10 0) (uuid "74000000-0000-0000-0000-000000000012"))
  (group "grp" (uuid "74000000-0000-0000-0000-000000000013") (lib_id "lib:block") (members "74000000-0000-0000-0000-00000000000b" "74000000-0000-0000-0000-00000000000c"))
  (sheet_instances (path "/" (page "1")))
)"#;
    let path = temp_schematic("extended_sections", src);

    let schematic = parse_schematic_file(Path::new(&path)).expect("extended schematic must parse");
    assert_eq!(
        schematic
            .screen
            .title_block
            .as_ref()
            .and_then(|tb| tb.title.as_deref()),
        Some("Demo")
    );
    assert_eq!(schematic.screen.embedded_fonts, Some(false));
    assert_eq!(schematic.screen.embedded_files.len(), 2);
    assert_eq!(schematic.screen.bus_aliases.len(), 1);
    assert_eq!(
        schematic.screen.bus_aliases[0].members,
        vec!["A0", "A1", "A2"]
    );
    assert_eq!(schematic.screen.lib_symbols.len(), 1);
    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(lib_symbol.lib_id, "Device:R");
    assert_eq!(lib_symbol.name, "R");
    assert!(lib_symbol.power);
    assert!(lib_symbol.local_power);
    assert_eq!(lib_symbol.body_style_names, vec!["N", "P"]);
    assert_eq!(lib_symbol.pin_name_offset, Some(0.5));
    assert!(!lib_symbol.show_pin_names);
    assert!(lib_symbol.show_pin_numbers);
    assert!(lib_symbol.excluded_from_sim);
    assert!(!lib_symbol.in_bom);
    assert!(lib_symbol.on_board);
    assert!(!lib_symbol.in_pos_files);
    assert!(lib_symbol.duplicate_pin_numbers_are_jumpers);
    assert_eq!(lib_symbol.jumper_pin_groups.len(), 2);
    assert_eq!(
        lib_symbol.jumper_pin_groups[0],
        BTreeSet::from(["1".to_string(), "2".to_string()])
    );
    assert_eq!(
        lib_symbol.jumper_pin_groups[1],
        BTreeSet::from(["3".to_string(), "4".to_string()])
    );
    assert_eq!(lib_symbol.keywords.as_deref(), Some("analog precision"));
    assert_eq!(
        lib_symbol.description.as_deref(),
        Some("precision resistor")
    );
    assert_eq!(lib_symbol.fp_filters, vec!["R_*", "0603"]);
    assert!(lib_symbol.locked_units);
    assert_eq!(lib_symbol.extends.as_deref(), Some("Device:Base"));
    assert_eq!(lib_symbol.properties.len(), 5);
    assert!(
        !lib_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .expect("reference field")
            .visible
    );
    assert_eq!(lib_symbol.units.len(), 1);
    assert_eq!(lib_symbol.units[0].unit_number, 1);
    assert_eq!(lib_symbol.units[0].body_style, 1);
    assert_eq!(lib_symbol.units[0].unit_name.as_deref(), Some("Amplifier"));
    assert_eq!(
        lib_symbol.units[0].draw_item_kinds,
        vec!["arc", "circle", "field", "text_box", "pin"]
    );
    assert_eq!(lib_symbol.units[0].draw_items.len(), 5);
    assert_eq!(lib_symbol.units[0].draw_items[0].kind, "arc");
    assert_eq!(lib_symbol.units[0].draw_items[1].kind, "circle");
    assert_eq!(lib_symbol.units[0].draw_items[0].points.len(), 3);
    assert_eq!(lib_symbol.units[0].draw_items[0].arc_center, None);
    assert_eq!(lib_symbol.units[0].draw_items[0].radius, None);
    assert_eq!(lib_symbol.units[0].draw_items[0].arc_start_angle, None);
    assert_eq!(lib_symbol.units[0].draw_items[0].arc_end_angle, None);
    assert_eq!(
        lib_symbol.units[0].draw_items[0]
            .stroke
            .as_ref()
            .and_then(|stroke| stroke.width),
        Some(0.1)
    );
    assert_eq!(
        lib_symbol.units[0].draw_items[2].text.as_deref(),
        Some("AMP")
    );
    assert_eq!(lib_symbol.units[0].draw_items[2].at, Some([3.0, -4.0]));
    assert_eq!(lib_symbol.units[0].draw_items[2].angle, Some(9.0));
    assert_eq!(lib_symbol.units[0].draw_items[2].kind, "field");
    assert_eq!(lib_symbol.units[0].draw_items[2].field_id, Some(0));
    assert_eq!(lib_symbol.units[0].draw_items[2].field_ordinal, Some(42));
    assert!(lib_symbol.units[0].draw_items[3].is_private);
    assert_eq!(
        lib_symbol.units[0].draw_items[3].text.as_deref(),
        Some("TB")
    );
    assert_eq!(
        lib_symbol.units[0].draw_items[4].name.as_deref(),
        Some("IN")
    );
    assert_eq!(
        lib_symbol.units[0].draw_items[4].number.as_deref(),
        Some("1")
    );
    assert_eq!(lib_symbol.units[0].draw_items[4].length, Some(2.5));
    assert_eq!(
        lib_symbol.units[0].draw_items[4]
            .name_effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([0.8, 0.8])
    );
    assert_eq!(
        lib_symbol.units[0].draw_items[4]
            .number_effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([1.1, 1.1])
    );
    assert_eq!(lib_symbol.units[0].draw_items[4].alternates.len(), 1);
    assert!(
        lib_symbol.units[0].draw_items[4]
            .alternates
            .contains_key("ALT")
    );
    assert_eq!(lib_symbol.embedded_fonts, Some(true));
    assert_eq!(lib_symbol.embedded_files.len(), 1);
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Text(text) if text.kind == TextKind::Text))
    );
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::TextBox(_)))
    );
    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert_eq!(text_box.at, [0.0, 0.0]);
    assert_eq!(text_box.end, [5.0, 5.0]);
    assert_eq!(text_box.margins, Some([1.0, 2.0, 3.0, 4.0]));
    assert!(!text_box.has_effects);
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Table(_)))
    );
    let table = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");
    assert_eq!(table.column_count, Some(2));
    assert_eq!(table.cells.len(), 2);
    assert_eq!(table.cells[0].col_span, 1);
    assert_eq!(table.cells[0].row_span, 1);
    assert_eq!(table.cells[0].row, 0);
    assert_eq!(table.cells[0].column, 0);
    assert_eq!(table.cells[1].col_span, 2);
    assert_eq!(table.cells[1].row_span, 1);
    assert_eq!(table.cells[1].row, 0);
    assert_eq!(table.cells[1].column, 1);
    assert!(table.border_external);
    assert!(!table.border_header);
    assert_eq!(table.border_stroke.width, Some(0.3));
    assert!(table.separators_rows);
    assert!(!table.separators_cols);
    assert_eq!(table.separators_stroke.style, StrokeStyle::Dash);
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Image(_)))
    );
    let image = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Image(image) => Some(image),
            _ => None,
        })
        .expect("image");
    assert_eq!(image.scale, 2.5);
    assert_eq!(
        image.data.as_deref(),
        Some(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAAAAgABSK+kcQAAAABJRU5ErkJggg=="
        )
    );
    let arc = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Arc => Some(shape),
            _ => None,
        })
        .expect("arc");
    assert!(!arc.has_stroke);
    assert!(!arc.has_fill);
    assert_eq!(arc.stroke.as_ref().expect("arc stroke").width, Some(0.0));
    assert_eq!(
        arc.fill.as_ref().expect("arc fill").fill_type,
        FillType::None
    );
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Shape(shape) if shape.kind == ShapeKind::Arc))
    );
    assert!(
        schematic.screen.items.iter().any(
            |item| matches!(item, SchItem::Label(label) if label.kind == LabelKind::Directive)
        )
    );
    assert_eq!(
        schematic
            .screen
            .items
            .iter()
            .filter(
                |item| matches!(item, SchItem::Label(label) if label.kind == LabelKind::Directive)
            )
            .count(),
        2
    );
    assert!(schematic.screen.items.iter().any(
        |item| matches!(item, SchItem::Group(Group { name, uuid, .. }) if name.as_deref() == Some("grp") && uuid.as_deref() == Some("74000000-0000-0000-0000-000000000013"))
    ));

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_placed_symbols_to_local_lib_symbols_after_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R")
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54) (name "P") (number "1")))))
  (symbol
    (lib_id "Device:R")
    (lib_name "Device:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_link", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");

    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");
    assert_eq!(linked.lib_id, "Device:R");
    assert_eq!(linked.units[0].draw_items[0].kind, "pin");

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_symbols_after_lib_cache_fixups() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000002")
  (paper "A4")
  (embedded_files (file (name "shared.bin") (checksum "sha256:123") (type font) (data |abc123|)))
  (lib_symbols
    (symbol "Root:R"
      (embedded_files (file (name "shared.bin")))
      (symbol "R_1_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1))))))
    (symbol "Child:R"
      (extends "Root:R")
      (embedded_files (file (name "shared.bin")))))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_fixups", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert!(linked.has_demorgan);
    assert_eq!(linked.embedded_files.len(), 1);
    assert_eq!(
        linked.embedded_files[0].checksum.as_deref(),
        Some("sha256:123")
    );
    assert_eq!(
        linked.embedded_files[0].file_type,
        Some(EmbeddedFileType::Font)
    );
    assert_eq!(linked.embedded_files[0].data.as_deref(), Some("abc123"));
    assert_eq!(linked.lib_id, "Child:R");
    assert_eq!(linked.name, "R");
    assert_eq!(linked.units.len(), 2);
    assert_eq!(linked.units[0].name, "R_1_1");
    let inherited_unit = linked
        .units
        .iter()
        .find(|unit| unit.unit_number == 1 && unit.body_style == 2)
        .expect("flattened inherited unit");
    assert_eq!(inherited_unit.name, "R_1_2");
    let inherited_text = inherited_unit
        .draw_items
        .iter()
        .find(|item| item.kind == "text")
        .expect("flattened inherited text");
    assert_eq!(inherited_text.text.as_deref(), Some("ALT"));
    assert_eq!(inherited_unit.draw_item_kinds, vec!["text"]);

    let _ = fs::remove_file(path);
}

#[test]
fn parser_flatten_keeps_child_embedded_files_without_overwriting_parent() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000003")
  (paper "A4")
  (embedded_files
    (file (name "parent.bin") (checksum "sha256:parent") (type font) (data |aaa|))
    (file (name "child.bin") (checksum "sha256:child") (type font) (data |bbb|))
    (file (name "shared.bin") (checksum "sha256:root") (type font) (data |ccc|)))
  (lib_symbols
    (symbol "Root:R"
      (embedded_files
        (file (name "parent.bin"))
        (file (name "shared.bin"))))
    (symbol "Child:R"
      (extends "Root:R")
      (embedded_files
        (file (name "child.bin"))
        (file (name "shared.bin")))))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_flatten_child_embedded_files", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert_eq!(linked.embedded_files.len(), 3);

    let parent_file = linked
        .embedded_files
        .iter()
        .find(|file| file.name.as_deref() == Some("parent.bin"))
        .expect("parent embedded file");
    assert_eq!(parent_file.data.as_deref(), Some("aaa"));

    let child_file = linked
        .embedded_files
        .iter()
        .find(|file| file.name.as_deref() == Some("child.bin"))
        .expect("child embedded file");
    assert_eq!(child_file.data.as_deref(), Some("bbb"));

    let shared_file = linked
        .embedded_files
        .iter()
        .find(|file| file.name.as_deref() == Some("shared.bin"))
        .expect("shared embedded file");
    assert_eq!(shared_file.checksum.as_deref(), Some("sha256:root"));
    assert_eq!(shared_file.data.as_deref(), Some("ccc"));

    let _ = fs::remove_file(path);
}

#[test]
fn parser_flatten_skips_intermediate_parent_embedded_files() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000004")
  (paper "A4")
  (embedded_files
    (file (name "root.bin") (checksum "sha256:root") (type font) (data |aaa|))
    (file (name "mid.bin") (checksum "sha256:mid") (type font) (data |bbb|))
    (file (name "child.bin") (checksum "sha256:child") (type font) (data |ccc|)))
  (lib_symbols
    (symbol "Root:R"
      (embedded_files (file (name "root.bin"))))
    (symbol "Mid:R"
      (extends "Root:R")
      (embedded_files (file (name "mid.bin"))))
    (symbol "Child:R"
      (extends "Mid:R")
      (embedded_files (file (name "child.bin")))))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_flatten_intermediate_embedded_files", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert_eq!(linked.embedded_files.len(), 2);
    assert!(
        linked
            .embedded_files
            .iter()
            .any(|file| file.name.as_deref() == Some("root.bin"))
    );
    assert!(
        linked
            .embedded_files
            .iter()
            .any(|file| file.name.as_deref() == Some("child.bin"))
    );
    assert!(
        !linked
            .embedded_files
            .iter()
            .any(|file| file.name.as_deref() == Some("mid.bin"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_child_non_field_draw_items() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000005")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (symbol "R_1_1"
        (text "PARENT" (at 1 1 0) (effects (font (size 1 1))))))
    (symbol "Child:R"
      (extends "Root:R")
      (symbol "R_1_1"
        (text "CHILD" (at 2 2 0) (effects (font (size 1 1)))))))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_items", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");
    let unit = linked
        .units
        .iter()
        .find(|unit| unit.unit_number == 1 && unit.body_style == 1)
        .expect("flattened root unit");
    let texts: Vec<_> = unit
        .draw_items
        .iter()
        .filter(|item| item.kind == "text")
        .filter_map(|item| item.text.as_deref())
        .collect();

    assert_eq!(texts, vec!["PARENT"]);

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_child_unit_field_overrides() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000006")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (symbol "R_2_1"
        (text "PARENT" (at 1 1 0) (effects (font (size 1 1)) (hide)))))
    (symbol "Child:R"
      (extends "Root:R")
      (symbol "R_2_1"
        (text "CHILD" (at 2 2 0) (effects (font (size 1 1)) (hide))))))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_unit_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");
    let unit = linked
        .units
        .iter()
        .find(|unit| unit.unit_number == 2 && unit.body_style == 1)
        .expect("flattened child unit");
    let fields: Vec<_> = unit
        .draw_items
        .iter()
        .filter(|item| item.kind == "field")
        .filter_map(|item| item.text.as_deref())
        .collect();

    assert_eq!(fields, vec!["CHILD"]);

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_child_optional_metadata_overrides() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000007")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (embedded_fonts yes)
      (property "ki_keywords" "root words")
      (property "ki_description" "root desc"))
    (symbol "Child:R"
      (extends "Root:R")
      (embedded_fonts no)
      (property "ki_keywords" "")
      (property "ki_description" "child desc")))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_metadata", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert_eq!(linked.keywords.as_deref(), Some("root words"));
    assert_eq!(linked.description.as_deref(), Some("child desc"));
    assert_eq!(
        linked
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolDescription)
            .map(|property| property.value.as_str()),
        Some("child desc")
    );
    assert_eq!(linked.embedded_fonts, Some(true));

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_child_unit_name_overrides() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000008")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (symbol "R_1_1"
        (unit_name "ParentUnit")))
    (symbol "Child:R"
      (extends "Root:R")
      (symbol "R_1_1"
        (unit_name "ChildUnit"))))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_unit_name", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert_eq!(linked.units[0].unit_name.as_deref(), Some("ParentUnit"));

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_child_body_style_overrides() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000009")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (body_styles demorgan "RootA" "RootB"))
    (symbol "Child:R"
      (extends "Root:R")
      (body_styles "ChildOnly")))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_body_styles", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert_eq!(linked.body_style_names, vec!["RootA", "RootB"]);
    assert!(linked.has_demorgan);

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_child_fp_filter_overrides() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000000a")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (property "ki_fp_filters" "SOIC* TSSOP*"))
    (symbol "Child:R"
      (extends "Root:R")
      (property "ki_fp_filters" "")))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_fp_filters", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    assert_eq!(linked.fp_filters, vec!["SOIC*", "TSSOP*"]);
    assert!(linked.fp_filters_specified);

    let _ = fs::remove_file(path);
}

#[test]
fn parser_does_not_link_derived_local_symbol_with_missing_parent() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000000b")
  (paper "A4")
  (lib_symbols
    (symbol "Child:R"
      (extends "Missing:R")
      (property "Reference" "R")
      (property "Value" "10k")))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_missing_parent", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");

    assert!(symbol.lib_symbol.is_none());

    let _ = fs::remove_file(path);
}

#[test]
fn parser_links_derived_lib_symbols_with_user_field_overrides() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000000c")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (property "UserA" "root-a")
      (property "UserB" "root-b"))
    (symbol "Child:R"
      (extends "Root:R")
      (property "UserA" "child-a")
      (property "UserC" "child-c")))
  (symbol
    (lib_id "Child:R")
    (at 1 2 0)
    (property "Reference" "R1")
    (property "Value" "10k")))
"#;
    let path = temp_schematic("parser_local_lib_symbol_child_user_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("placed symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");

    let root_unit = linked
        .units
        .iter()
        .find(|unit| unit.unit_number == 1 && unit.body_style == 1)
        .expect("flattened root unit");
    let user_a = root_unit
        .draw_items
        .iter()
        .find(|item| item.kind == "field" && item.name.as_deref() == Some("UserA"))
        .expect("overridden child user field");
    let user_b = root_unit
        .draw_items
        .iter()
        .find(|item| item.kind == "field" && item.name.as_deref() == Some("UserB"))
        .expect("inherited parent user field");
    let user_c = root_unit
        .draw_items
        .iter()
        .find(|item| item.kind == "field" && item.name.as_deref() == Some("UserC"))
        .expect("new child user field");

    assert_eq!(user_a.text.as_deref(), Some("child-a"));
    assert_eq!(user_b.text.as_deref(), Some("root-b"));
    assert_eq!(user_c.text.as_deref(), Some("child-c"));

    let _ = fs::remove_file(path);
}

#[test]
fn lib_fp_filters_unescape_kicad_string_markers() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "ki_fp_filters" "SOIC{slash}SO-8 R{space}0603"))))
"#;
    let path = temp_schematic("lib_fp_filters_unescape_markers", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = schematic.screen.lib_symbols.first().expect("lib symbol");

    assert_eq!(lib_symbol.fp_filters, vec!["SOIC/SO-8", "R 0603"]);

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_text_uses_decidegree_angles_like_upstream() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000002")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (text "AMP" (at 3 4 90) (effects (font (size 1 1)))))))
)"#;
    let path = temp_schematic("lib_text_decidegrees", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(lib_symbol.units[0].draw_items[0].kind, "text");
    assert_eq!(lib_symbol.units[0].draw_items[0].angle, Some(9.0));
    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_text_box_supports_legacy_start_end_and_rejects_schematic_only_tokens() {
    let legacy_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000003")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (text_box "TB" (start 1 2) (end 4 6) (effects (font (size 1 1)))))))
)"#;
    let legacy_path = temp_schematic("lib_text_box_legacy", legacy_src);
    let schematic = parse_schematic_file(Path::new(&legacy_path)).expect("must parse");
    let item = &schematic.screen.lib_symbols[0].units[0].draw_items[0];
    assert_eq!(item.kind, "text_box");
    assert_eq!(item.at, Some([1.0, -2.0]));
    assert_eq!(item.end, Some([4.0, -6.0]));
    let margins = item.margins.expect("default lib text_box margins");
    let expected = 0.1524 / 2.0 + 1.0 * 0.75;
    assert!((margins[0] - expected).abs() < 1e-9);
    assert!((margins[1] - expected).abs() < 1e-9);
    assert!((margins[2] - expected).abs() < 1e-9);
    assert!((margins[3] - expected).abs() < 1e-9);
    let _ = fs::remove_file(legacy_path);

    let bad_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000004")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (text_box "TB" (at 1 2 0) (size 3 4) (uuid "tb")))))
)"#;
    let bad_path = temp_schematic("lib_text_box_bad_uuid", bad_src);
    let err =
        parse_schematic_file(Path::new(&bad_path)).expect_err("must reject malformed lib symbol");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, fill or effects")
    );
    let _ = fs::remove_file(bad_path);
}

#[test]
fn rejects_invalid_lib_pin_orientation() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000005")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin input line (at 1 2 45)))))
)"#;
    let path = temp_schematic("bad_lib_pin_orientation", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject malformed lib symbol");
    assert!(err.to_string().contains("expecting 0, 90, 180, or 270"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_unit_name_suffix() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000006")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_bad" (arc))))
)"#;
    let path = temp_schematic("bad_lib_unit_suffix", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject malformed lib symbol");
    assert!(
        err.to_string()
            .contains("Invalid symbol unit name suffix bad")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_lib_symbol_unit_name_without_separator_as_bad_suffix() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000007")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R1_1" (arc))))
)"#;
    let path = temp_schematic("bad_lib_unit_missing_separator", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject malformed lib symbol");
    assert!(
        err.to_string()
            .contains("Invalid symbol unit name suffix _1")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_body_style_number() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "75000000-0000-0000-0000-000000000008")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_bad" (arc))))
)"#;
    let path = temp_schematic("bad_lib_body_style_number", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject malformed lib symbol");
    assert!(
        err.to_string()
            .contains("Invalid symbol body style number 1_bad")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_generator_version_before_supported_version() {
    let src = r#"(kicad_sch
  (version 20230101)
  (generator "eeschema")
  (generator_version "8.0")
)"#;
    let path = temp_schematic("old_generator_version", src);
    let schematic = parse_schematic_file(Path::new(&path))
        .expect("must accept generator_version on older schematic versions");
    assert_eq!(schematic.generator_version.as_deref(), Some("8.0"));
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_embedded_files_before_supported_version() {
    let src = r#"(kicad_sch
  (version 20230101)
  (generator "eeschema")
  (embedded_files
    (file (name "old.bin")))
)"#;
    let path = temp_schematic("old_embedded_files", src);
    let schematic = parse_schematic_file(Path::new(&path))
        .expect("must accept embedded_files on older schematic versions");
    assert_eq!(schematic.screen.embedded_files.len(), 1);
    assert_eq!(
        schematic.screen.embedded_files[0].name.as_deref(),
        Some("old.bin")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_table_before_supported_version() {
    let src = r#"(kicad_sch
  (version 20230101)
  (generator "eeschema")
  (table
    (column_count 1)
    (column_widths 10)
    (row_heights 20)
    (cells
      (table_cell "c1" (at 0 0 0) (size 5 5))))
)"#;
    let path = temp_schematic("old_table_version", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept table on older versions");
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Table(_)))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_rule_area_before_supported_version() {
    let src = r#"(kicad_sch
  (version 20230101)
  (generator "eeschema")
  (rule_area
    (polyline (pts (xy 0 0) (xy 1 0)))
    (exclude_from_sim yes))
)"#;
    let path = temp_schematic("old_rule_area_version", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept rule_area on older versions");
    assert!(
        schematic
            .screen
            .items
            .iter()
            .any(|item| matches!(item, SchItem::Shape(shape) if shape.kind == ShapeKind::RuleArea))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_modern_page_sniff_block() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "76000000-0000-0000-0000-000000000001")
  (page 1 1)
)"#;
    let path = temp_schematic("modern_page", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must accept modern page");
    assert_eq!(
        schematic
            .screen
            .page
            .as_ref()
            .map(|page| (&page.page, &page.sheet)),
        Some((&"1".to_string(), &"1".to_string()))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn remaps_old_page_token_to_paper() {
    let src = r#"(kicad_sch
  (version 20200506)
  (generator "eeschema")
  (page "A4")
)"#;
    let path = temp_schematic("old_page_to_paper", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must accept old page as paper");
    assert_eq!(
        schematic
            .screen
            .paper
            .as_ref()
            .map(|paper| paper.kind.as_str()),
        Some("A4")
    );
    assert!(schematic.screen.page.is_none());
    let _ = fs::remove_file(path);

    let portrait_src = r#"(kicad_sch
  (version 20200506)
  (generator "eeschema")
  (page "User" 40 50 portrait)
)"#;
    let portrait_path = temp_schematic("old_page_to_user_paper", portrait_src);
    let schematic =
        parse_schematic_file(Path::new(&portrait_path)).expect("must accept old page as paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "User");
    assert_eq!(paper.width, Some(40.0));
    assert_eq!(paper.height, Some(50.0));
    assert!(paper.portrait);
    assert!(schematic.screen.page.is_none());
    let _ = fs::remove_file(portrait_path);

    let modern_path = temp_schematic(
        "modern_page_stays_sniff",
        r#"(kicad_sch
  (version 20200507)
  (generator "eeschema")
  (page 7 9)
)"#,
    );
    let schematic = parse_schematic_file(Path::new(&modern_path)).expect("must parse page sniff");
    assert_eq!(
        schematic
            .screen
            .paper
            .as_ref()
            .map(|paper| paper.kind.as_str()),
        Some("A4")
    );
    assert_eq!(
        schematic
            .screen
            .page
            .as_ref()
            .map(|page| (page.page.as_str(), page.sheet.as_str())),
        Some(("7", "9"))
    );
    let _ = fs::remove_file(modern_path);
}

#[test]
fn parses_user_paper_dimensions_and_portrait() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000001")
  (paper "User" 123.4 234.5 portrait)
)"#;
    let path = temp_schematic("user_paper", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "User");
    assert_eq!(paper.width, Some(123.4));
    assert_eq!(paper.height, Some(234.5));
    assert!(paper.portrait);
    let _ = fs::remove_file(path);

    let tall_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000002")
  (paper "User" 123.4 234.5)
)"#;
    let tall_path = temp_schematic("tall_user_paper", tall_src);
    let schematic = parse_schematic_file(Path::new(&tall_path)).expect("must parse tall paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.width, Some(123.4));
    assert_eq!(paper.height, Some(234.5));
    assert!(paper.portrait);
    let _ = fs::remove_file(tall_path);

    let lower_user_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000003")
  (paper user 123.4 234.5)
)"#;
    let lower_user_path = temp_schematic("lower_user_paper", lower_user_src);
    let schematic =
        parse_schematic_file(Path::new(&lower_user_path)).expect("must parse lower-case user");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "User");
    assert_eq!(paper.width, Some(123.4));
    assert_eq!(paper.height, Some(234.5));
    assert!(paper.portrait);
    let _ = fs::remove_file(lower_user_path);

    let clamped_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000004")
  (paper "User" 1 1000000)
)"#;
    let clamped_path = temp_schematic("clamped_user_paper", clamped_src);
    let schematic = parse_schematic_file(Path::new(&clamped_path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.width, Some(25.4));
    assert_eq!(paper.height, Some(3048.0));
    let _ = fs::remove_file(clamped_path);

    let quoted_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000005")
  (paper "User" "123.4" 234.5)
)"#;
    let quoted_path = temp_schematic("quoted_user_paper_dimension", quoted_src);
    let err = parse_schematic_file(Path::new(&quoted_path)).expect_err("must reject quoted width");
    assert!(err.to_string().contains("missing width"));
    let _ = fs::remove_file(quoted_path);

    let quoted_height_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000006")
  (paper "User" 123.4 "234.5")
)"#;
    let quoted_height_path = temp_schematic("quoted_user_paper_height", quoted_height_src);
    let err = parse_schematic_file(Path::new(&quoted_height_path))
        .expect_err("must reject quoted height");
    assert!(err.to_string().contains("missing height"));
    let _ = fs::remove_file(quoted_height_path);

    let quoted_portrait_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000007")
  (paper "User" 123.4 234.5 "portrait")
)"#;
    let quoted_portrait_path = temp_schematic("quoted_user_paper_portrait", quoted_portrait_src);
    let err = parse_schematic_file(Path::new(&quoted_portrait_path))
        .expect_err("must reject quoted portrait");
    assert!(err.to_string().contains("expecting portrait"));
    let _ = fs::remove_file(quoted_portrait_path);

    let upper_portrait_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000008")
  (paper "User" 123.4 234.5 PORTRAIT)
)"#;
    let upper_portrait_path = temp_schematic("upper_user_paper_portrait", upper_portrait_src);
    let err = parse_schematic_file(Path::new(&upper_portrait_path))
        .expect_err("must reject upper-case portrait keyword");
    assert!(err.to_string().contains("expecting portrait"));
    let _ = fs::remove_file(upper_portrait_path);

    let bogus_tail_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000009")
  (paper "User" 123.4 234.5 sideways)
)"#;
    let bogus_tail_path = temp_schematic("bogus_user_paper_tail", bogus_tail_src);
    let err = parse_schematic_file(Path::new(&bogus_tail_path))
        .expect_err("must reject non-portrait tail token");
    assert!(err.to_string().contains("expecting portrait"));
    let _ = fs::remove_file(bogus_tail_path);
}

#[test]
fn standard_paper_resolves_upstream_dimensions_and_portrait_rotation() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000000a")
  (paper "A4")
)"#;
    let path = temp_schematic("a4_paper_dimensions", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "A4");
    assert_eq!(paper.width, Some(297.0));
    assert_eq!(paper.height, Some(210.0));
    assert!(!paper.portrait);
    let _ = fs::remove_file(path);

    let portrait_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000000b")
  (paper "A4" portrait)
)"#;
    let portrait_path = temp_schematic("a4_paper_portrait", portrait_src);
    let schematic = parse_schematic_file(Path::new(&portrait_path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.width, Some(210.0));
    assert_eq!(paper.height, Some(297.0));
    assert!(paper.portrait);
    let _ = fs::remove_file(portrait_path);

    let unquoted_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000000c")
  (paper A4)
)"#;
    let unquoted_path = temp_schematic("unquoted_a4_paper", unquoted_src);
    let schematic = parse_schematic_file(Path::new(&unquoted_path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "A4");
    assert_eq!(paper.width, Some(297.0));
    assert_eq!(paper.height, Some(210.0));
    let _ = fs::remove_file(unquoted_path);

    let gerber_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000000d")
  (paper GERBER)
)"#;
    let gerber_path = temp_schematic("gerber_paper", gerber_src);
    let schematic = parse_schematic_file(Path::new(&gerber_path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "GERBER");
    assert_eq!(paper.width, Some(812.8));
    assert_eq!(paper.height, Some(812.8));
    let _ = fs::remove_file(gerber_path);

    let mixed_case_usletter_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000000e")
  (paper usletter)
)"#;
    let mixed_case_usletter_path =
        temp_schematic("mixed_case_usletter_paper", mixed_case_usletter_src);
    let schematic = parse_schematic_file(Path::new(&mixed_case_usletter_path))
        .expect("must parse mixed-case paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "USLetter");
    assert_eq!(paper.width, Some(279.4));
    assert_eq!(paper.height, Some(215.9));
    assert!(!paper.portrait);
    let _ = fs::remove_file(mixed_case_usletter_path);

    let mixed_case_gerber_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000000f")
  (paper gerber)
)"#;
    let mixed_case_gerber_path = temp_schematic("mixed_case_gerber_paper", mixed_case_gerber_src);
    let schematic = parse_schematic_file(Path::new(&mixed_case_gerber_path))
        .expect("must parse mixed-case gerber");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "GERBER");
    assert_eq!(paper.width, Some(812.8));
    assert_eq!(paper.height, Some(812.8));
    let _ = fs::remove_file(mixed_case_gerber_path);

    let bogus_tail_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000010")
  (paper A4 sideways)
)"#;
    let bogus_tail_path = temp_schematic("standard_paper_bad_tail", bogus_tail_src);
    let err = parse_schematic_file(Path::new(&bogus_tail_path))
        .expect_err("must reject non-portrait tail for standard paper");
    assert!(err.to_string().contains("expecting portrait"));
    let _ = fs::remove_file(bogus_tail_path);

    let extra_after_portrait_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000011")
  (paper A4 portrait sideways)
)"#;
    let extra_after_portrait_path = temp_schematic(
        "standard_paper_extra_after_portrait",
        extra_after_portrait_src,
    );
    let err = parse_schematic_file(Path::new(&extra_after_portrait_path))
        .expect_err("must reject tokens after portrait");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(extra_after_portrait_path);
}

#[test]
fn duplicate_paper_and_page_sections_use_last_value_like_upstream() {
    let paper_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000012")
  (paper "A4")
  (paper "User" 40 50 portrait)
)"#;
    let paper_path = temp_schematic("duplicate_paper", paper_src);
    let schematic = parse_schematic_file(Path::new(&paper_path)).expect("must parse paper");
    let paper = schematic.screen.paper.as_ref().expect("paper");
    assert_eq!(paper.kind, "User");
    assert_eq!(paper.width, Some(40.0));
    assert_eq!(paper.height, Some(50.0));
    assert!(paper.portrait);
    let _ = fs::remove_file(paper_path);

    let page_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000013")
  (page 1 1)
  (page 2 4)
)"#;
    let page_path = temp_schematic("duplicate_page", page_src);
    let schematic = parse_schematic_file(Path::new(&page_path)).expect("must parse page");
    let page = schematic.screen.page.as_ref().expect("page");
    assert_eq!(page.page, "2");
    assert_eq!(page.sheet, "4");
    let _ = fs::remove_file(page_path);

    let quoted_page_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000014")
  (page "" "")
)"#;
    let quoted_page_path = temp_schematic("quoted_page_sniff", quoted_page_src);
    let schematic =
        parse_schematic_file(Path::new(&quoted_page_path)).expect("must parse quoted page");
    let page = schematic.screen.page.as_ref().expect("page");
    assert_eq!(page.page, "");
    assert_eq!(page.sheet, "");
    let _ = fs::remove_file(quoted_page_path);

    let numeric_page_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000015")
  (page 3 4)
)"#;
    let numeric_page_path = temp_schematic("numeric_page_sniff", numeric_page_src);
    let schematic =
        parse_schematic_file(Path::new(&numeric_page_path)).expect("must parse page sniff");
    let page = schematic.screen.page.as_ref().expect("page");
    assert_eq!(page.page, "3");
    assert_eq!(page.sheet, "4");
    let _ = fs::remove_file(numeric_page_path);

    let mixed_quoted_page_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "30000000-0000-0000-0000-000000000001")
  (page 3 "4")
)"#;
    let mixed_quoted_page_path = temp_schematic("mixed_quoted_page_sniff", mixed_quoted_page_src);
    let schematic = parse_schematic_file(Path::new(&mixed_quoted_page_path))
        .expect("must parse quoted sheet token");
    let page = schematic.screen.page.as_ref().expect("page");
    assert_eq!(page.page, "3");
    assert_eq!(page.sheet, "4");
    let _ = fs::remove_file(mixed_quoted_page_path);

    let raw_quoted_page_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000016")
  (page " 7 " " 9 ")
)"#;
    let raw_quoted_page_path = temp_schematic("raw_quoted_page_sniff", raw_quoted_page_src);
    let schematic = parse_schematic_file(Path::new(&raw_quoted_page_path))
        .expect("must preserve raw page sniff tokens");
    let page = schematic.screen.page.as_ref().expect("page");
    assert_eq!(page.page, " 7 ");
    assert_eq!(page.sheet, " 9 ");
    let _ = fs::remove_file(raw_quoted_page_path);

    let keyword_page_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000017")
  (page portrait paper)
)"#;
    let keyword_page_path = temp_schematic("keyword_page_sniff", keyword_page_src);
    let schematic = parse_schematic_file(Path::new(&keyword_page_path))
        .expect("must accept keyword tokens in page sniff");
    let page = schematic.screen.page.as_ref().expect("page");
    assert_eq!(page.page, "portrait");
    assert_eq!(page.sheet, "paper");
    let _ = fs::remove_file(keyword_page_path);
}

#[test]
fn rejects_invalid_page_type() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000018")
  (paper "BogusSize")
)"#;
    let path = temp_schematic("invalid_page_type", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject page type");
    assert!(err.to_string().contains("Invalid page type"));
    match &err {
        Error::Validation { diagnostic, .. } => {
            assert!(matches!(diagnostic.kind, DiagnosticKind::Validation));
            let span = diagnostic.span.expect("diagnostic span");
            assert_eq!(&src[span.start..span.end], "\"BogusSize\"");
            assert_eq!(diagnostic.line, Some(5));
            assert_eq!(diagnostic.column, Some(10));
            assert!(err.to_string().contains(&format!(
                ":{}:{}",
                diagnostic.line.expect("line"),
                diagnostic.column.expect("column")
            )));
        }
        _ => panic!("expected validation error"),
    }
    let _ = fs::remove_file(path);

    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-000000000019")
  (paper "usletter")
)"#;
    let path = temp_schematic("canonical_page_type", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse canonical page");
    assert_eq!(
        schematic
            .screen
            .paper
            .as_ref()
            .map(|paper| paper.kind.as_str()),
        Some("USLetter")
    );
    let _ = fs::remove_file(path);

    let numeric_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000001a")
  (paper 123)
)"#;
    let path = temp_schematic("numeric_page_type", numeric_src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject numeric page type");
    assert!(err.to_string().contains("missing paper kind"));
    let _ = fs::remove_file(path);
}

#[test]
fn structured_diagnostics_tag_expecting_and_unexpected_parser_failures() {
    let expecting_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000007")
  (paper "A4")
  (wire (pts (bogus 0 0) (xy 1 1)))
)"#;
    let expecting_path = temp_schematic("structured_expect_diagnostic", expecting_src);
    let err = parse_schematic_file(Path::new(&expecting_path)).expect_err("must reject bad pts");
    assert!(err.to_string().contains("parse error at"));
    assert!(err.to_string().contains("expecting xy"));
    match &err {
        Error::Validation { diagnostic, .. } => {
            assert!(matches!(
                diagnostic.kind,
                DiagnosticKind::Expecting { ref expected } if expected == "xy"
            ));
        }
        other => panic!("expected validation error, got {other:?}"),
    }
    let _ = fs::remove_file(expecting_path);

    let unexpected_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-000000000008")
  (paper "A4")
  (text "hello" (property "X" "Y"))
)"#;
    let unexpected_path = temp_schematic("structured_unexpected_diagnostic", unexpected_src);
    let err =
        parse_schematic_file(Path::new(&unexpected_path)).expect_err("must reject bad property");
    assert!(err.to_string().contains("parse error at"));
    assert!(err.to_string().contains("unexpected property"));
    match &err {
        Error::Validation { diagnostic, .. } => {
            assert!(matches!(
                diagnostic.kind,
                DiagnosticKind::Unexpected { ref found } if found == "property"
            ));
        }
        other => panic!("expected validation error, got {other:?}"),
    }
    let _ = fs::remove_file(unexpected_path);
}

#[test]
fn validation_diagnostics_render_with_parse_error_prefix() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "74000000-0000-0000-0000-00000000000a")
  (paper "A4")
  (title_block (comment 10 "bad"))
)"#;
    let path = temp_schematic("validation_diagnostic_parse_error_prefix", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject invalid comment");
    let rendered = err.to_string();
    assert!(rendered.contains("parse error at"));
    assert!(!rendered.contains("validation error at"));
    assert!(rendered.contains("Invalid title block comment number"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_missing_tokens_in_paper_and_page_branches() {
    let missing_paper_kind_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000001b")
  (paper)
)"#;
    let missing_paper_kind_path = temp_schematic("missing_paper_kind", missing_paper_kind_src);
    let err = parse_schematic_file(Path::new(&missing_paper_kind_path))
        .expect_err("must reject missing paper kind");
    assert!(err.to_string().contains("missing paper kind"));
    let _ = fs::remove_file(missing_paper_kind_path);

    let missing_user_width_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000001c")
  (paper User)
)"#;
    let missing_user_width_path = temp_schematic("missing_user_width", missing_user_width_src);
    let err = parse_schematic_file(Path::new(&missing_user_width_path))
        .expect_err("must reject missing custom width");
    assert!(err.to_string().contains("missing width"));
    let _ = fs::remove_file(missing_user_width_path);

    let missing_user_height_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000001d")
  (paper User 123.4)
)"#;
    let missing_user_height_path = temp_schematic("missing_user_height", missing_user_height_src);
    let err = parse_schematic_file(Path::new(&missing_user_height_path))
        .expect_err("must reject missing custom height");
    assert!(err.to_string().contains("missing height"));
    let _ = fs::remove_file(missing_user_height_path);

    let missing_page_sheet_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000001e")
  (page 7)
)"#;
    let missing_page_sheet_path = temp_schematic("missing_page_sheet", missing_page_sheet_src);
    let err = parse_schematic_file(Path::new(&missing_page_sheet_path))
        .expect_err("must reject missing page sheet token");
    assert!(err.to_string().contains("expecting page sheet"));
    let _ = fs::remove_file(missing_page_sheet_path);

    let missing_page_right_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "10000000-0000-0000-0000-00000000001f")
  (page 7 9
)"#;
    let missing_page_right_path = temp_schematic("missing_page_right", missing_page_right_src);
    let err = parse_schematic_file(Path::new(&missing_page_right_path))
        .expect_err("must reject missing closing paren in page sniff");
    assert!(matches!(err, Error::Validation { .. }));
    assert!(err.to_string().contains("expecting ("));
    let _ = fs::remove_file(missing_page_right_path);
}

#[test]
fn accepts_future_schematic_version_without_generator_version() {
    let src = r#"(kicad_sch
  (version 20990101)
  (generator "eeschema")
  (uuid "76000000-0000-0000-0000-000000000002")
)"#;
    let path = temp_schematic("future_version", src);
    let schematic = parse_schematic_file(Path::new(&path))
        .expect("must match upstream future-version leniency");
    assert_eq!(schematic.version, 20990101);
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_future_schematic_version_at_generator_version_branch() {
    let src = r#"(kicad_sch
  (version 20990101)
  (generator "eeschema")
  (generator_version "9.0")
  (bogus "later")
)"#;
    let path = temp_schematic("future_version_generator_version_branch", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject future version early");
    assert!(
        err.to_string()
            .contains("future schematic version `20990101` is newer than supported `20260306`")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn future_version_generator_version_branch_still_owns_close_first() {
    let src = r#"(kicad_sch
  (version 20990101)
  (generator "eeschema")
  (generator_version "9.0" "trailing")
)"#;
    let path = temp_schematic("future_version_generator_version_trailing", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("bad generator_version trailer should fail before future-version check");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(path);
}

#[test]
fn list_generator_version_payload_fails_at_branch_close() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (generator_version (bogus))
)"#;
    let path = temp_schematic("list_generator_version_payload", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("list generator_version payload should fail at close like upstream");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_empty_generator_version_payload_like_upstream() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (generator_version)
)"#;
    let path = temp_schematic("empty_generator_version", src);
    let schematic = parse_schematic_file(Path::new(&path))
        .expect("empty generator_version should stay on the upstream close-owned path");
    assert_eq!(schematic.generator_version.as_deref(), Some(""));
    let _ = fs::remove_file(path);
}

#[test]
fn defaults_missing_header_version_and_rejects_late_version_section() {
    let missing_src = r#"(kicad_sch
  (generator "eeschema")
  (uuid "76000000-0000-0000-0000-000000000003")
)"#;
    let missing_path = temp_schematic("missing_header_version", missing_src);
    let schematic = parse_schematic_file(Path::new(&missing_path)).expect("must parse");
    assert_eq!(schematic.version, 20260306);
    let paper = schematic.screen.paper.as_ref().expect("default paper");
    assert_eq!(paper.kind, "A4");
    assert_eq!(paper.width, Some(297.0));
    assert_eq!(paper.height, Some(210.0));
    assert!(!paper.portrait);
    let _ = fs::remove_file(missing_path);

    let explicit_a4_src = r#"(kicad_sch
  (generator "eeschema")
  (uuid "76000000-0000-0000-0000-000000000004")
  (paper "A4")
)"#;
    let explicit_a4_path = temp_schematic("explicit_a4_page", explicit_a4_src);
    let explicit = parse_schematic_file(Path::new(&explicit_a4_path)).expect("must parse");
    assert_eq!(explicit.screen.paper, schematic.screen.paper);
    let _ = fs::remove_file(explicit_a4_path);

    let late_src = r#"(kicad_sch
  (generator "eeschema")
  (version 20250114)
  (uuid "76000000-0000-0000-0000-000000000005")
  (paper "A4")
)"#;
    let late_path = temp_schematic("late_version_section", late_src);
    let err = parse_schematic_file(Path::new(&late_path)).expect_err("must reject late version");
    assert!(
        err.to_string()
            .contains(
                "expecting bitmap, bus, bus_alias, bus_entry, class_label, embedded_files, global_label, hierarchical_label, junction, label, line, no_connect, page, paper, rule_area, sheet, symbol, symbol_instances, text, title_block"
            )
    );
    let _ = fs::remove_file(late_path);
}

#[test]
fn parser_accepts_leading_full_line_comments() {
    let src = "# generated comment\n  # another comment\n(kicad_sch\n  (version 20260306)\n  (generator \"eeschema\")\n  (uuid \"61000000-0000-0000-0000-00000000000f\")\n  (text \"note\" (at 1 2 0)))\n";
    let path = temp_schematic("leading_comment_lines", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse commented schematic");

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(text.text, "note");

    let _ = fs::remove_file(path);
}

#[test]
fn parser_accepts_nul_as_whitespace() {
    let src = "(kicad_sch\0(version\020260306)\0(generator\0\"eeschema\")\0(uuid\0\"61000000-0000-0000-0000-000000000010\")\0(text\0\"note\"\0(at\01\02\00)))";
    let path = temp_schematic("nul_whitespace", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must parse NUL-separated schematic");

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(text.text, "note");
    assert_eq!(text.at, [1.0, 2.0, 0.0]);

    let _ = fs::remove_file(path);
}

#[test]
fn accepts_legacy_host_and_generates_root_uuid_for_old_files() {
    let src = r#"(kicad_sch
  (version 20200826)
  (host "eeschema" "5.99")
  (paper "A4")
  (sheet_instances
    (path "/child" (page "1")))
  (symbol_instances
    (path "/R1" (reference "R1") (unit 1)))
)"#;
    let path = temp_schematic("legacy_host", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.generator, "");
    assert!(schematic.root_sheet.uuid.is_some());
    assert_eq!(schematic.root_sheet.uuid, schematic.screen.uuid);
    let root_uuid = schematic.root_sheet.uuid.as_deref().expect("root uuid");
    assert_eq!(
        schematic.screen.sheet_instances[0].path,
        format!("/{root_uuid}/child")
    );
    assert_eq!(
        schematic.screen.symbol_instances[0].path,
        format!("/{root_uuid}/R1")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn missing_generator_section_is_accepted() {
    let src = r#"(kicad_sch
  (version 20260306)
  (uuid "61000000-0000-0000-0000-000000000011")
)"#;
    let path = temp_schematic("missing_generator_section", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert_eq!(schematic.generator, "");
    assert_eq!(schematic.version, 20260306);
    assert_eq!(
        schematic.root_sheet.uuid.as_deref(),
        Some("61000000-0000-0000-0000-000000000011")
    );
    assert_eq!(
        schematic.screen.uuid.as_deref(),
        Some("61000000-0000-0000-0000-000000000011")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_header_and_title_block_sections_use_last_value_like_upstream() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "first")
  (generator "second")
  (generator_version "9.0")
  (generator_version "9.1")
  (uuid "76000000-0000-0000-0000-000000000006")
  (uuid "76000000-0000-0000-0000-000000000007")
  (paper "A4")
  (title_block (title "Old") (comment 1 "one"))
  (title_block (title "New") (comment 2 "two"))
)"#;
    let path = temp_schematic("duplicate_header_sections", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.generator, "second");
    assert_eq!(schematic.generator_version.as_deref(), Some("9.1"));
    assert_eq!(
        schematic.root_sheet.uuid.as_deref(),
        Some("76000000-0000-0000-0000-000000000007")
    );
    assert_eq!(
        schematic.screen.uuid.as_deref(),
        Some("76000000-0000-0000-0000-000000000007")
    );

    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.title.as_deref(), Some("New"));
    assert_eq!(title_block.comment(2), Some("two"));
    assert_eq!(title_block.comment(1), None);

    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_title_block_comment_numbers_overwrite_existing_slots() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "76000000-0000-0000-0000-000000000008")
  (paper "A4")
  (title_block
    (comment 1 "first")
    (comment 1 "second")
    (comment 2 "two"))
)"#;
    let path = temp_schematic("duplicate_title_block_comments", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.comment_count(), 2);
    assert_eq!(title_block.comment(1), Some("second"));
    assert_eq!(title_block.comment(2), Some("two"));
    let _ = fs::remove_file(path);
}

#[test]
fn normalizes_symbol_and_sheet_instance_paths_and_legacy_empty_text() {
    let src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "30000000-0000-0000-0000-000000000001")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 90) (in_bom no) (on_board no) (dnp yes) (fields_autoplaced) (property "Value" "~"))
  (sheet (at 0 0) (size 10 10) (in_bom no) (on_board no) (dnp yes) (fields_autoplaced)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page " 1  "))
    (path "/child" (page "")))
  (symbol_instances
    (path "/sym1" (reference "R1") (unit 1) (value "~") (footprint "~")))
)"#;
    let path = temp_schematic("normalized_instances", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert!(!symbol.in_bom);
    assert!(!symbol.on_board);
    assert!(symbol.dnp);
    assert_eq!(symbol.fields_autoplaced, FieldAutoplacement::Auto);
    assert_eq!(symbol.properties[0].value, "");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(!sheet.in_bom);
    assert!(!sheet.on_board);
    assert!(sheet.dnp);
    assert_eq!(sheet.fields_autoplaced, FieldAutoplacement::Auto);

    assert_eq!(schematic.screen.root_sheet_page, None);
    assert!(schematic.screen.content_modified);
    assert_eq!(schematic.screen.sheet_instances.len(), 2);
    assert_eq!(
        schematic.screen.sheet_instances[0].path,
        "/30000000-0000-0000-0000-000000000001"
    );
    assert_eq!(
        schematic.screen.sheet_instances[0].page.as_deref(),
        Some("1")
    );
    assert_eq!(
        schematic.screen.sheet_instances[1].path,
        "/30000000-0000-0000-0000-000000000001/child"
    );
    assert_eq!(
        schematic.screen.sheet_instances[1].page.as_deref(),
        Some("#")
    );
    assert_eq!(
        schematic.screen.symbol_instances[0].path,
        "/30000000-0000-0000-0000-000000000001/sym1"
    );
    assert_eq!(
        schematic.screen.symbol_instances[0].value.as_deref(),
        Some("")
    );
    assert_eq!(
        schematic.screen.symbol_instances[0].footprint.as_deref(),
        Some("")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn modern_root_sheet_instance_page_is_stored_on_screen_root_page() {
    let src = r#"(kicad_sch
  (version 20221110)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000001")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "7"))
    (path "/child" (page "2")))
)"#;
    let path = temp_schematic("modern_root_sheet_page", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert_eq!(schematic.screen.file_format_version_at_load, Some(20221110));
    assert_eq!(schematic.screen.root_sheet_page.as_deref(), Some("7"));
    assert!(!schematic.screen.content_modified);
    assert_eq!(schematic.screen.sheet_instances.len(), 1);
    assert_eq!(schematic.screen.sheet_instances[0].path, "/child");
    assert_eq!(
        schematic.screen.sheet_instances[0].page.as_deref(),
        Some("2")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn root_symbol_instance_paths_are_prefixed_with_root_uuid() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000002")
  (symbol_instances
    (path "" (reference "R1") (unit 1))
    (path "/child/R2" (reference "R2") (unit 2)))
)"#;
    let path = temp_schematic("root_symbol_instance_path", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert_eq!(schematic.screen.symbol_instances.len(), 2);
    assert_eq!(
        schematic.screen.symbol_instances[0].path,
        "/40000000-0000-0000-0000-000000000002"
    );
    assert_eq!(
        schematic.screen.symbol_instances[1].path,
        "/40000000-0000-0000-0000-000000000002/child/R2"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_page_normalization_only_hashes_truly_empty_tokens() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000000d")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S" (page "   ")))))
  (sheet_instances
    (path "/S2" (page "")))
)"#;
    let path = temp_schematic("sheet_page_normalization_whitespace", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.instances[0].page.as_deref(), Some(""));
    assert!(schematic.screen.content_modified);
    assert_eq!(
        schematic.screen.sheet_instances[0].page.as_deref(),
        Some("#")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn maps_legacy_sim_enable_fields_to_exclude_from_sim() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000003")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0) (property "Sim.Enable" "0"))
  (symbol (lib_id "Device:R") (at 3 4 0) (property "Sim.Enable" "false"))
  (symbol (lib_id "Device:R") (at 5 6 0) (property "Sim.Enable" "no"))
  (symbol (lib_id "Device:R") (at 7 8 0) (property "Spice_Netlist_Enabled" "N"))
)"#;
    let path = temp_schematic("legacy_sim_enable", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbols = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(symbols.len(), 4);
    assert!(symbols[0].excluded_from_sim);
    assert!(symbols[1].excluded_from_sim);
    assert!(symbols[2].excluded_from_sim);
    assert!(symbols[3].excluded_from_sim);
    assert_eq!(symbols[0].properties.len(), 5);
    assert_eq!(symbols[1].properties.len(), 5);
    assert_eq!(symbols[2].properties.len(), 5);
    assert_eq!(symbols[3].properties.len(), 5);
    assert!(
        symbols[0]
            .properties
            .iter()
            .all(|property| property.value.is_empty())
    );
    assert!(
        symbols[1]
            .properties
            .iter()
            .all(|property| property.value.is_empty())
    );
    assert!(
        symbols[2]
            .properties
            .iter()
            .all(|property| property.value.is_empty())
    );
    assert!(
        symbols[3]
            .properties
            .iter()
            .all(|property| property.value.is_empty())
    );
    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_mid_v7_sim_field_names() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000004")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54) (name "A") (number "1"))
        (pin passive line (at 10 0 0) (length 2.54) (name "B") (number "2")))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim_Device" "R")
    (property "Sim_Type" "R")
    (property "Sim_Params" "r=10k")
    (property "Sim_Pins" "2 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_mid_v7_sim_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Device")
    );
    assert!(
        symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Type")
    );
    assert!(
        symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Params")
    );
    assert!(
        symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Pins")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim_Device")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim_Type")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim_Params")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim_Pins")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=- 2=+")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("R")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("R")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("r=10k")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_mid_v7_pot_pin_names() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000005")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim_Device" "R")
    (property "Sim_Type" "POT")
    (property "Sim_Pins" "1=+ 2=- 3=w")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_mid_v7_pot_pin_names", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=r1 2=r0 3=w")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "r1".to_string()),
            ("2".to_string(), "r0".to_string()),
            ("3".to_string(), "w".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_mid_v7_random_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000006")
  (paper "A4")
  (symbol
    (lib_id "Device:I")
    (property "Reference" "I?")
    (property "Sim_Device" "I")
    (property "Sim_Type" "RANDNORMAL")
    (property "Sim_Params" "MIN=0 MAX=0 DT=2N STDDEV=3 MEAN=4")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_mid_v7_random_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("RANDGAUSSIAN")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("ts=2n stddev=3 mean=4")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("RANDGAUSSIAN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("ts=2n stddev=3 mean=4")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_mid_v7_mutual_type_to_device() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000007")
  (paper "A4")
  (symbol
    (lib_id "Device:L")
    (property "Reference" "L?")
    (property "Sim_Device" "L")
    (property "Sim_Type" "MUTUAL")
    (property "Sim_Params" "k=0.8")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_mid_v7_mutual_type", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("K")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Type")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("K")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        None
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_structured_sim_enable_state() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030f")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Params" "type=\"R\" model=\"10k\" lib=\"\"")
    (property "Sim.Enable" "0")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_structured_sim_enable", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(symbol.excluded_from_sim);
    assert_eq!(
        symbol.sim_model.as_ref().map(|sim_model| sim_model.enabled),
        Some(false)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::RawSpice)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_preserves_serializer_style_sim_field_payloads() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032a")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Params" "flag gain=2 model=\"BC\\\"547\" extra=\"x y\"")
    (property "Sim.Pins" "1=\"PIN A\" 2=B")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_preserves_serializer_style_sim_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![
            ("flag".to_string(), "1".to_string()),
            ("gain".to_string(), "2".to_string()),
            ("model".to_string(), "BC\"547".to_string()),
            ("extra".to_string(), "x y".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "PIN A".to_string()),
            ("2".to_string(), "B".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_normalizes_explicit_sim_param_field_values() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032b")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Params" "gain=1Meg bias=3,300u extra=\"x y\"")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_normalizes_explicit_sim_param_field_values", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some(r#"gain=1Meg bias=3,300u extra="x y""#)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![
            ("gain".to_string(), "1M".to_string()),
            ("bias".to_string(), "3300u".to_string()),
            ("extra".to_string(), "x y".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("bias".to_string(), "3300u".to_string()),
            ("extra".to_string(), "x y".to_string()),
            ("gain".to_string(), "1M".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_current_noise_sim_pins_from_explicit_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032c")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Sim.Device" "V")
    (property "Sim.Type" "WHITENOISE")
    (property "Sim.Params" "rms=1 dt=2n")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_defaults_current_noise_sim_pins_from_explicit_fields",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.parse_warnings.is_empty());
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_accepts_explicit_random_sim_type_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032d")
  (paper "A4")
  (lib_symbols
    (symbol "Device:I"
      (symbol "I_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:I")
    (property "Reference" "I?")
    (property "Sim.Device" "I")
    (property "Sim.Type" "RANDGAUSSIAN")
    (property "Sim.Params" "ts=1n td=2n stddev=3 mean=4")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_accepts_explicit_random_sim_type_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.parse_warnings.is_empty());
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_library_sources_from_embedded_stack_before_filesystem() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000310")
  (paper "A4")
  (embedded_files
    (file (name "top.kicad_sim") (type model) (data |aaa|)))
  (lib_symbols
    (symbol "Device:Q"
      (embedded_files
        (file (name "lib.kicad_sim") (type model) (data |bbb|)))))
  (symbol
    (lib_id "Device:Q")
    (property "Reference" "Q?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "lib.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_sim_library_stack", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    let sources = collect_symbol_sim_library_sources(&schematic.path, &schematic.screen, symbol);
    assert_eq!(
        sources,
        vec![
            SimLibrarySource::SchematicEmbedded {
                name: "top.kicad_sim".to_string()
            },
            SimLibrarySource::SymbolEmbedded {
                name: "lib.kicad_sim".to_string()
            },
            SimLibrarySource::Filesystem(
                schematic
                    .path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("lib.kicad_sim")
            ),
        ]
    );
    assert_eq!(
        resolve_symbol_sim_library_source(&schematic.path, &schematic.screen, symbol),
        Some(SimLibrarySource::SymbolEmbedded {
            name: "lib.kicad_sim".to_string()
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_library_source_to_filesystem_when_not_embedded() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000311")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/model.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_filesystem_sim_library", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_library_source(&schematic.path, &schematic.screen, symbol),
        Some(SimLibrarySource::Filesystem(
            schematic
                .path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("models/model.kicad_sim")
        ))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn loads_symbol_sim_library_content_from_embedded_source() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000312")
  (paper "A4")
  (embedded_files
    (file (name "top.kicad_sim") (type model) (data |model-text|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_sim_library_content", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        load_symbol_sim_library_content(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryContent {
            source: SimLibrarySource::SchematicEmbedded {
                name: "top.kicad_sim".to_string(),
            },
            text: "model-text".to_string(),
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn loads_symbol_sim_library_content_from_filesystem() {
    let dir = env::temp_dir().join(format!(
        "sim_lib_content_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("models")).expect("create dir");
    fs::write(dir.join("models/model.kicad_sim"), "file-model").expect("write sim lib");
    let path = dir.join("content_loader.kicad_sch");
    fs::write(
        &path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000313")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/model.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#,
    )
    .expect("write schematic");

    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        load_symbol_sim_library_content(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryContent {
            source: SimLibrarySource::Filesystem(
                schematic
                    .path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("models/model.kicad_sim"),
            ),
            text: "file-model".to_string(),
        })
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(dir.join("models/model.kicad_sim"));
    let _ = fs::remove_dir(dir.join("models"));
    let _ = fs::remove_dir(dir);
}

#[test]
fn loads_symbol_sim_library_content_from_spice_lib_dir() {
    let _env_lock = env_lock();
    let dir = env::temp_dir().join(format!(
        "sim_lib_dir_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("models")).expect("create dir");
    fs::write(dir.join("models/model.kicad_sim"), "env-model").expect("write sim lib");
    let old_spice_lib_dir = env::var_os("SPICE_LIB_DIR");
    // SAFETY: tests run in-process and we restore the environment before returning.
    unsafe {
        env::set_var("SPICE_LIB_DIR", &dir);
    }
    let path = dir.join("outside/content_loader.kicad_sch");
    fs::create_dir_all(path.parent().expect("parent")).expect("create schematic dir");
    fs::write(
        &path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000314")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/model.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#,
    )
    .expect("write schematic");

    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        load_symbol_sim_library_content(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryContent {
            source: SimLibrarySource::Filesystem(dir.join("models/model.kicad_sim")),
            text: "env-model".to_string(),
        })
    );

    match old_spice_lib_dir {
        Some(value) => {
            // SAFETY: restore previous process environment after the test mutation above.
            unsafe { env::set_var("SPICE_LIB_DIR", value) }
        }
        None => {
            // SAFETY: restore previous process environment after the test mutation above.
            unsafe { env::remove_var("SPICE_LIB_DIR") }
        }
    }
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(dir.join("models/model.kicad_sim"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn loads_symbol_sim_library_content_from_env_expanded_path() {
    let _env_lock = env_lock();
    let dir = env::temp_dir().join(format!(
        "sim_lib_env_path_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("models")).expect("create dir");
    fs::write(dir.join("models/model.kicad_sim"), "env-expanded-model").expect("write sim lib");
    let old_sim_models_dir = env::var_os("KI2_SIM_MODELS");
    // SAFETY: tests run in-process and we restore the environment before returning.
    unsafe {
        env::set_var("KI2_SIM_MODELS", dir.join("models"));
    }
    let path = dir.join("env_path_loader.kicad_sch");
    fs::write(
        &path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000314")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "$KI2_SIM_MODELS/model.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#,
    )
    .expect("write schematic");

    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        load_symbol_sim_library_content(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryContent {
            source: SimLibrarySource::Filesystem(dir.join("models/model.kicad_sim")),
            text: "env-expanded-model".to_string(),
        })
    );

    match old_sim_models_dir {
        Some(value) => {
            // SAFETY: restore previous process environment after the test mutation above.
            unsafe { env::set_var("KI2_SIM_MODELS", value) }
        }
        None => {
            // SAFETY: restore previous process environment after the test mutation above.
            unsafe { env::remove_var("KI2_SIM_MODELS") }
        }
    }
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(dir.join("models/model.kicad_sim"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn loads_symbol_sim_library_content_from_braced_env_expanded_path() {
    let _env_lock = env_lock();
    let dir = env::temp_dir().join(format!(
        "sim_lib_braced_env_path_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("models")).expect("create dir");
    fs::write(
        dir.join("models/model.kicad_sim"),
        "braced-env-expanded-model",
    )
    .expect("write sim lib");
    let old_sim_models_dir = env::var_os("KI2_SIM_MODELS_BRACED");
    // SAFETY: tests run in-process and we restore the environment before returning.
    unsafe {
        env::set_var("KI2_SIM_MODELS_BRACED", dir.join("models"));
    }
    let path = dir.join("braced_env_path_loader.kicad_sch");
    fs::write(
        &path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000315")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "${KI2_SIM_MODELS_BRACED}/model.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#,
    )
    .expect("write schematic");

    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        load_symbol_sim_library_content(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryContent {
            source: SimLibrarySource::Filesystem(dir.join("models/model.kicad_sim")),
            text: "braced-env-expanded-model".to_string(),
        })
    );

    match old_sim_models_dir {
        Some(value) => {
            // SAFETY: restore previous process environment after the test mutation above.
            unsafe { env::set_var("KI2_SIM_MODELS_BRACED", value) }
        }
        None => {
            // SAFETY: restore previous process environment after the test mutation above.
            unsafe { env::remove_var("KI2_SIM_MODELS_BRACED") }
        }
    }
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(dir.join("models/model.kicad_sim"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn loads_symbol_sim_library_content_from_windows_style_relative_path() {
    let dir = env::temp_dir().join(format!(
        "sim_lib_windows_path_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("models")).expect("create dir");
    fs::write(dir.join("models/model.kicad_sim"), "windows-style-model").expect("write sim lib");
    let path = dir.join("windows_path_loader.kicad_sch");
    fs::write(
        &path,
        r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000316")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models\\model.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#,
    )
    .expect("write schematic");

    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let resolved_path = dir
        .join("models/model.kicad_sim")
        .canonicalize()
        .unwrap_or(dir.join("models/model.kicad_sim"));

    assert_eq!(
        load_symbol_sim_library_content(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryContent {
            source: SimLibrarySource::Filesystem(resolved_path),
            text: "windows-style-model".to_string(),
        })
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(dir.join("models/model.kicad_sim"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn classifies_symbol_sim_library_kind_from_embedded_sources() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000315")
  (paper "A4")
  (embedded_files
    (file (name "driver.ibs") (type model) (data |ibis-model|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_sim_library_kind", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        classify_symbol_sim_library_kind(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryKind::Ibis)
    );
    assert_eq!(
        resolve_symbol_sim_library(&schematic.path, &schematic.screen, symbol),
        Some(ResolvedSimLibrary {
            source: SimLibrarySource::SchematicEmbedded {
                name: "driver.ibs".to_string(),
            },
            kind: SimLibraryKind::Ibis,
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn classifies_uppercase_ibs_extension_as_spice_library_kind() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000315")
  (paper "A4")
  (embedded_files
    (file (name "driver.IBS") (type model) (data |ibis-model|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.IBS")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_uppercase_ibs_library_kind", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        classify_symbol_sim_library_kind(&schematic.path, &schematic.screen, symbol),
        Some(SimLibraryKind::Spice)
    );
    assert_eq!(
        resolve_symbol_sim_library(&schematic.path, &schematic.screen, symbol),
        Some(ResolvedSimLibrary {
            source: SimLibrarySource::SchematicEmbedded {
                name: "driver.IBS".to_string(),
            },
            kind: SimLibraryKind::Spice,
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_resolved_sim_library_on_symbol() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000316")
  (paper "A4")
  (embedded_files
    (file (name "top.kicad_sim") (type model) (data |model-text|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_resolved_sim_library", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_library.clone()),
        Some(ResolvedSimLibrary {
            source: SimLibrarySource::SchematicEmbedded {
                name: "top.kicad_sim".to_string(),
            },
            kind: SimLibraryKind::Spice,
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_model_from_embedded_spice_subckt() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000317")
  (paper "A4")
  (embedded_files
    (file
      (name "top.kicad_sim")
      (type model)
      (data |* comment
.subckt MODEL IN OUT VSS PARAMS: RVAL=1k
+ EXTRA=1
.ends MODEL|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_spice_subckt", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "top.kicad_sim".to_string(),
                },
                kind: SimLibraryKind::Spice,
            },
            name: "MODEL".to_string(),
            kind: ResolvedSimModelKind::SpiceSubckt,
            model_type: None,
            ibis_model_type: None,
            diff_pin: None,
            pins: vec!["IN".to_string(), "OUT".to_string(), "VSS".to_string()],
            params: vec![
                ("RVAL".to_string(), Some("1k".to_string())),
                ("EXTRA".to_string(), Some("1".to_string())),
            ],
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_model_from_embedded_spice_include_chain() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000321")
  (paper "A4")
  (embedded_files
    (file
      (name "models/top.kicad_sim")
      (type model)
      (data |.include "child.lib"|))
    (file
      (name "models/child.lib")
      (type model)
      (data |.subckt MODEL IN OUT
.ends MODEL|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_spice_include_chain", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "models/top.kicad_sim".to_string(),
                },
                kind: SimLibraryKind::Spice,
            },
            name: "MODEL".to_string(),
            kind: ResolvedSimModelKind::SpiceSubckt,
            model_type: None,
            ibis_model_type: None,
            diff_pin: None,
            pins: vec!["IN".to_string(), "OUT".to_string()],
            params: Vec::new(),
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_model_from_mixed_case_spice_include_chain() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032d")
  (paper "A4")
  (embedded_files
    (file
      (name "models/top.kicad_sim")
      (type model)
      (data |.InClUdE "child.lib"|))
    (file
      (name "models/child.lib")
      (type model)
      (data |.subckt MODEL IN OUT
.ends MODEL|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_mixed_case_spice_include_chain", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "models/top.kicad_sim".to_string(),
                },
                kind: SimLibraryKind::Spice,
            },
            name: "MODEL".to_string(),
            kind: ResolvedSimModelKind::SpiceSubckt,
            model_type: None,
            ibis_model_type: None,
            diff_pin: None,
            pins: vec!["IN".to_string(), "OUT".to_string()],
            params: Vec::new(),
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_model_from_embedded_ibis_component() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031a")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
[Model] MODEL_A
Model_type Output
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_ibis_component", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "driver.ibs".to_string(),
                },
                kind: SimLibraryKind::Ibis,
            },
            name: "DRIVER".to_string(),
            kind: ResolvedSimModelKind::IbisComponent,
            model_type: Some("MODEL_A".to_string()),
            ibis_model_type: Some("Output".to_string()),
            diff_pin: None,
            pins: vec!["A1".to_string(), "B2".to_string()],
            params: Vec::new(),
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_model_diff_pin_from_embedded_ibis_component() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031f")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
[Diff Pin]
A1 B2
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_ibis_diff_pin", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "driver.ibs".to_string(),
                },
                kind: SimLibraryKind::Ibis,
            },
            name: "DRIVER".to_string(),
            kind: ResolvedSimModelKind::IbisComponent,
            model_type: Some("MODEL_A".to_string()),
            ibis_model_type: None,
            diff_pin: Some("B2".to_string()),
            pins: vec!["A1".to_string(), "B2".to_string()],
            params: Vec::new(),
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_resolved_spice_model_pins_on_symbol() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000318")
  (paper "A4")
  (embedded_files
    (file
      (name "top.kicad_sim")
      (type model)
      (data |.subckt MODEL A B C PARAMS: RVAL=1k EXTRA=1
.ends MODEL|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_resolved_spice_model_pins", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_name.as_deref()),
        Some("MODEL")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::SpiceSubckt)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_pin_names.clone()),
        Some(vec!["A".to_string(), "B".to_string(), "C".to_string()])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![
            ("RVAL".to_string(), Some("1k".to_string())),
            ("EXTRA".to_string(), Some("1".to_string())),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_resolved_spice_model_from_filesystem_include_chain() {
    let dir = temp_dir_path("loader_spice_include_chain");
    fs::create_dir_all(dir.join("models")).expect("create models dir");
    fs::write(dir.join("models/top.kicad_sim"), ".include child.lib\n").expect("write top sim lib");
    fs::write(dir.join("models/child.lib"), ".model MODEL NPN (BF=100)\n")
        .expect("write child sim lib");

    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000322")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = dir.join("include_chain.kicad_sch");
    fs::write(&path, src).expect("write schematic");
    let loaded = load_schematic_tree(&path).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("NPN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![("BF".to_string(), Some("100".to_string()))])
    );

    let _ = fs::remove_file(dir.join("models/top.kicad_sim"));
    let _ = fs::remove_file(dir.join("models/child.lib"));
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_tree_hydrates_filesystem_spice_include_from_project_fallback_path() {
    let dir = temp_dir_path("loader_filesystem_spice_include_project_fallback");
    fs::create_dir_all(dir.join("models")).expect("create models dir");
    fs::write(dir.join("models/top.kicad_sim"), ".include child.lib\n").expect("write top sim lib");
    fs::write(dir.join("child.lib"), ".model MODEL NPN (BF=100)\n").expect("write child sim lib");

    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000322")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = dir.join("include_chain.kicad_sch");
    fs::write(&path, src).expect("write schematic");
    let loaded = load_schematic_tree(&path).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("NPN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![("BF".to_string(), Some("100".to_string()))])
    );

    let _ = fs::remove_file(dir.join("models/top.kicad_sim"));
    let _ = fs::remove_file(dir.join("child.lib"));
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_tree_hydrates_embedded_spice_include_from_project_fallback_path() {
    let dir = temp_dir_path("loader_embedded_spice_include_project_fallback");
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("child.lib"), ".model MODEL NPN (BF=100)\n").expect("write child sim lib");

    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000322")
  (paper "A4")
  (embedded_files
    (file
      (name "models/top.kicad_sim")
      (type model)
      (data |.include child.lib
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = dir.join("include_chain.kicad_sch");
    fs::write(&path, src).expect("write schematic");
    let loaded = load_schematic_tree(&path).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("NPN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![("BF".to_string(), Some("100".to_string()))])
    );

    let _ = fs::remove_file(dir.join("child.lib"));
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_tree_hydrates_resolved_spice_model_from_backslash_include_chain() {
    let dir = temp_dir_path("loader_spice_backslash_include_chain");
    fs::create_dir_all(dir.join("models/child")).expect("create models dir");
    fs::write(
        dir.join("models/top.kicad_sim"),
        ".include \"child\\\\child.lib\"\n",
    )
    .expect("write top sim lib");
    fs::write(
        dir.join("models/child/child.lib"),
        ".model MODEL NPN (BF=100)\n",
    )
    .expect("write child sim lib");

    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000323")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models/top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = dir.join("backslash_include_chain.kicad_sch");
    fs::write(&path, src).expect("write schematic");
    let loaded = load_schematic_tree(&path).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("NPN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![("BF".to_string(), Some("100".to_string()))])
    );

    let _ = fs::remove_file(dir.join("models/top.kicad_sim"));
    let _ = fs::remove_file(dir.join("models/child/child.lib"));
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_tree_hydrates_resolved_ibis_component_pins_on_symbol() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031b")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
[Model] MODEL_A
Model_type Output
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_resolved_ibis_component_pins", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_name.as_deref()),
        Some("DRIVER")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::IbisComponent)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("MODEL_A")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_ibis_model_type.as_deref()),
        Some("Output")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_pin_names.clone()),
        Some(vec!["A1".to_string(), "B2".to_string()])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(Vec::new())
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_overrides_resolved_ibis_kind_from_sim_type() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031f")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
[Model] MODEL_A
Model_type Output
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Type" "DCDRIVER")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_overrides_resolved_ibis_kind", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "driver.ibs".to_string(),
                },
                kind: SimLibraryKind::Ibis,
            },
            name: "DRIVER".to_string(),
            kind: ResolvedSimModelKind::IbisDriverDc,
            model_type: Some("MODEL_A".to_string()),
            ibis_model_type: Some("Output".to_string()),
            diff_pin: None,
            pins: vec!["A1".to_string()],
            params: Vec::new(),
        })
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::IbisDriverDc)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_ibis_model_type_from_selected_pin_without_explicit_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031e")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
[Model] MODEL_A
Model_type Output
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_ibis_model_type_from_pin", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("MODEL_A")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_ibis_model_type.as_deref()),
        Some("Output")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_does_not_override_resolved_ibis_kind_for_lowercase_type() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031f")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
[Model] MODEL_A
Model_type Output
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Type" "dcdriver")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_does_not_override_resolved_ibis_kind_for_lowercase_type",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "driver.ibs".to_string(),
                },
                kind: SimLibraryKind::Ibis,
            },
            name: "DRIVER".to_string(),
            kind: ResolvedSimModelKind::IbisComponent,
            model_type: Some("MODEL_A".to_string()),
            ibis_model_type: Some("Output".to_string()),
            diff_pin: None,
            pins: vec!["A1".to_string()],
            params: Vec::new(),
        })
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::IbisComponent)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_resolved_ibis_diff_pin_on_symbol() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000320")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
[Diff Pin]
A1 B2
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (property "Sim.Ibis.Diff" "1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_resolved_ibis_diff_pin", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.ibis_diff),
        Some(true)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_ibis_diff_pin.as_deref()),
        Some("B2")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_marks_resolved_ibis_libraries_as_ibis_origin() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031d")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_marks_resolved_ibis_origin", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::Ibis)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_symbol_sim_model_from_embedded_spice_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000031c")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100 VAF=50)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_embedded_spice_model", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "models.lib".to_string(),
                },
                kind: SimLibraryKind::Spice,
            },
            name: "MODEL".to_string(),
            kind: ResolvedSimModelKind::SpiceModel,
            model_type: Some("NPN".to_string()),
            ibis_model_type: None,
            diff_pin: None,
            pins: Vec::new(),
            params: vec![
                ("BF".to_string(), Some("100".to_string())),
                ("VAF".to_string(), Some("50".to_string())),
            ],
        })
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("NPN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::SpiceModel)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![
            ("BF".to_string(), Some("100".to_string())),
            ("VAF".to_string(), Some("50".to_string())),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_embedded_spice_model_from_single_statement_only() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032b")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100)
* comment that should be ignored
.model OTHER PNP (IS=1)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_spice_model_statement_only", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "models.lib".to_string(),
                },
                kind: SimLibraryKind::Spice,
            },
            name: "MODEL".to_string(),
            kind: ResolvedSimModelKind::SpiceModel,
            model_type: Some("NPN".to_string()),
            ibis_model_type: None,
            diff_pin: None,
            pins: Vec::new(),
            params: vec![("BF".to_string(), Some("100".to_string()))],
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_embedded_spice_model_with_continuation_lines() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032c")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100
+ VAF=50)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_spice_model_continuation", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_param_pairs.clone()),
        Some(vec![
            ("BF".to_string(), Some("100".to_string())),
            ("VAF".to_string(), Some("50".to_string())),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn resolves_library_model_from_sim_name_before_inline_params() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000032e")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL AREA=2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("resolver_strips_inline_sim_name_params", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        Some(ki2::sim::ResolvedSimModel {
            library: ResolvedSimLibrary {
                source: SimLibrarySource::SchematicEmbedded {
                    name: "models.lib".to_string(),
                },
                kind: SimLibraryKind::Spice,
            },
            name: "MODEL".to_string(),
            kind: ResolvedSimModelKind::SpiceModel,
            model_type: Some("NPN".to_string()),
            ibis_model_type: None,
            diff_pin: None,
            pins: Vec::new(),
            params: vec![("BF".to_string(), Some("100".to_string()))],
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_library_backed_sim_pins_from_resolved_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000319")
  (paper "A4")
  (embedded_files
    (file
      (name "top.kicad_sim")
      (type model)
      (data |.subckt MODEL IN OUT VSS
.ends MODEL|)))
  (lib_symbols
    (symbol "Device:Q"
      (symbol "Q_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P3" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:Q")
    (property "Reference" "Q?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "top.kicad_sim")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_defaults_library_backed_sim_pins", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=IN 2=OUT 3=VSS")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "IN".to_string()),
            ("2".to_string(), "OUT".to_string()),
            ("3".to_string(), "VSS".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_structured_sim_model_from_existing_sim_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000909")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Sim.Device" "V")
    (property "Sim.Type" "PULSE")
    (property "Sim.Library" "models.kicad_sim")
    (property "Sim.Name" "MyPulse")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "DRIVER")
    (property "Sim.Params" "y1=0 y2=2 td=1n")
    (property "Sim.Pins" "1=1 2=2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_structured_sim_model", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(!schematic.screen.content_modified);
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref()),
        Some("models.kicad_sim")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("MyPulse")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.ibis_pin.as_deref()),
        Some("A1")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.ibis_model.as_deref()),
        Some("DRIVER")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("y1=0 y2=2 td=1n")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_resolves_sim_name_before_inline_library_params() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090a")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL AREA=2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_resolves_sim_name_before_inline_params", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_name.as_deref()),
        Some("MODEL")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::SpiceModel)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_model_type.as_deref()),
        Some("NPN")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn keeps_non_space_sim_name_separators_in_library_lookup() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090b")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL\tAREA=2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_keeps_non_space_sim_name_separators", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_name.as_deref()),
        Some("MODEL\tAREA=2")
    );
    assert_eq!(
        resolve_symbol_sim_model(&schematic.path, &schematic.screen, symbol),
        None
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_records_warning_for_missing_library_backed_sim_name() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090c")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model MODEL NPN (BF=100)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_warns_missing_sim_name", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Error loading simulation model: no 'Sim.Name' field"))
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_name.as_deref()),
        Some("unknown")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_records_warning_for_missing_sim_library() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090f")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "missing.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_warns_missing_sim_library", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let canonical = path.canonicalize().unwrap_or(path.clone());
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == canonical)
        .expect("loaded schematic");
    let expected = canonical
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("missing.lib")
        .display()
        .to_string();

    assert!(schematic.screen.parse_warnings.iter().any(|warning| {
        warning.contains("Simulation model library not found at")
            && warning.contains(expected.as_str())
    }));

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_records_warning_for_missing_sim_library_in_project_and_spice_lib_dir() {
    let _env_lock = env_lock();
    let dir = env::temp_dir().join(format!(
        "ki2_missing_sim_library_warning_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let spice_dir = dir.join("spice");
    fs::create_dir_all(&spice_dir).expect("mkdir");

    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000910")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "missing.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = dir.join("root.kicad_sch");
    fs::write(&path, src).expect("write schematic");

    let old_spice_lib_dir = env::var_os("SPICE_LIB_DIR");

    unsafe { env::set_var("SPICE_LIB_DIR", &spice_dir) };

    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let canonical = path.canonicalize().unwrap_or(path.clone());
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == canonical)
        .expect("loaded schematic");
    let project_location = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("missing.lib")
        .display()
        .to_string();
    let spice_location = spice_dir.join("missing.lib").display().to_string();

    assert!(schematic.screen.parse_warnings.iter().any(|warning| {
        warning.contains("Simulation model library not found at")
            && warning.contains(project_location.as_str())
            && warning.contains(spice_location.as_str())
            && warning.contains("' or '")
    }));

    match old_spice_lib_dir {
        Some(value) => unsafe { env::set_var("SPICE_LIB_DIR", value) },
        None => unsafe { env::remove_var("SPICE_LIB_DIR") },
    }

    let _ = fs::remove_file(path);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_tree_records_warning_for_missing_library_base_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000910")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.model OTHER NPN (BF=100)|)))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_warns_missing_base_model", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.parse_warnings.iter().any(|warning| {
        warning.contains("Error loading simulation model: could not find base model 'MODEL'")
            && warning.contains("models.lib")
    }));
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_name.as_deref()),
        Some("MODEL")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_records_warning_for_invalid_current_sim_type_pair() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000911")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "R")
    (property "Sim.Type" "PULSE")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_warns_invalid_current_sim_pair", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic.screen.parse_warnings.iter().any(
            |warning| warning.contains("No simulation model definition found for symbol 'R?'."),
        )
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_records_warning_for_lowercase_current_sim_type_pair() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000911")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Sim.Device" "V")
    (property "Sim.Type" "pulse")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_warns_lowercase_current_sim_type_pair", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic.screen.parse_warnings.iter().any(
            |warning| warning.contains("No simulation model definition found for symbol 'V?'."),
        )
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_records_warning_for_invalid_current_sim_type_without_reference() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000912")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Sim.Device" "I")
    (property "Sim.Type" "DEVICE")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_warns_invalid_current_sim_without_ref", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("No simulation model definition found."),)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_does_not_warn_for_valid_behavioral_current_sim_pairs() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000913")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "R")
    (property "Sim.Type" "=")
    (at 1 2 0))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Sim.Device" "V")
    (property "Sim.Type" "=")
    (at 3 4 0))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "S?")
    (property "Sim.Device" "SW")
    (property "Sim.Type" "V")
    (at 5 6 0))
)"#;
    let path = temp_schematic("loader_accepts_valid_current_sim_pairs", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .all(|warning| !warning.contains("No simulation model definition found"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_does_not_warn_for_valid_transistor_current_sim_pairs() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000914")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN")
    (property "Reference" "Q?")
    (property "Sim.Device" "NPN")
    (property "Sim.Type" "VBIC")
    (at 1 2 0))
  (symbol
    (lib_id "Device:M")
    (property "Reference" "M?")
    (property "Sim.Device" "NMOS")
    (property "Sim.Type" "BSIM3")
    (at 3 4 0))
)"#;
    let path = temp_schematic("loader_accepts_valid_transistor_sim_pairs", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .all(|warning| !warning.contains("No simulation model definition found"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_does_not_warn_for_hicum2_current_sim_pairs() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000930")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN")
    (property "Reference" "Q?")
    (property "Sim.Device" "NPN")
    (property "Sim.Type" "HICUM2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_accepts_hicum2_current_sim_pairs", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .all(|warning| !warning.contains("No simulation model definition found"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_does_not_warn_for_hicuml2_current_sim_pairs() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000930")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN")
    (property "Reference" "Q?")
    (property "Sim.Device" "NPN")
    (property "Sim.Type" "HICUML2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_accepts_hicuml2_current_sim_pairs", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");

    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .all(|warning| !warning.contains("No simulation model definition found"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_marks_control_source_sim_fields_as_built_in() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000931")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "E?")
    (property "Sim.Device" "E")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_marks_control_source_sim_fields_as_built_in", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_control_source_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000931")
  (paper "A4")
  (lib_symbols
    (symbol "Device:E"
      (symbol "E_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C+" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C-" (effects (font (size 1.27 1.27))))
          (number "4" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:E")
    (property "Reference" "E?")
    (property "Value" "2")
    (property "Sim.Device" "E")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_control_source_sim_model_from_value", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("gain=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
            ("3".to_string(), "C+".to_string()),
            ("4".to_string(), "C-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_voltage_controlled_current_source_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000931")
  (paper "A4")
  (lib_symbols
    (symbol "Device:G"
      (symbol "G_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C+" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C-" (effects (font (size 1.27 1.27))))
          (number "4" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:G")
    (property "Reference" "G?")
    (property "Value" "5m")
    (property "Sim.Device" "G")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_voltage_controlled_current_source_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("5m")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("gain=5m")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
            ("3".to_string(), "C+".to_string()),
            ("4".to_string(), "C-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_current_controlled_voltage_source_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000931")
  (paper "A4")
  (lib_symbols
    (symbol "Device:H"
      (symbol "H_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:H")
    (property "Reference" "H?")
    (property "Value" "7")
    (property "Sim.Device" "H")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_current_controlled_voltage_source_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("7")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("gain=7")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_keeps_invalid_sim_pairs_on_field_origin() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000932")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "R")
    (property "Sim.Type" "V")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_keeps_invalid_sim_pairs_on_field_origin", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::Fields)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_infers_passive_structured_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000915")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "10k")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_infers_passive_structured_sim_model", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::InferredValue)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("R")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        None
    );
    assert_eq!(
        symbol.sim_model.as_ref().map(|sim_model| sim_model.enabled),
        Some(true)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("10k")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("r=\"10k\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_inferred_sim_model_preserves_exclude_from_sim_state() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000916")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (exclude_from_sim yes)
    (property "Reference" "R?")
    (property "Value" "10k")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_inferred_sim_preserves_exclude_from_sim", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::InferredValue)
    );
    assert_eq!(
        symbol.sim_model.as_ref().map(|sim_model| sim_model.enabled),
        Some(false)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_infers_source_structured_sim_model_from_ac_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000916")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "AC 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_infers_source_structured_sim_model", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::InferredValue)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("DC")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("ac=\"1\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("AC 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_keeps_lowercase_inferred_source_prefix_as_raw_dc_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000916")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "ac 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_keeps_lowercase_inferred_source_prefix_raw", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::InferredValue)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("DC")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("dc=\"ac 1\"")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_infers_structured_sim_model_after_legacy_pin_only_migration() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000917")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "22k")
    (property "Spice_Primitive" "R")
    (property "Spice_Node_Sequence" "2 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_infers_structured_after_legacy_pin_only", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::InferredValue)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("r=\"22k\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("2".to_string(), "1".to_string()),
            ("1".to_string(), "2".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("22k")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_current_resistor_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000918")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "10k")
    (property "Sim.Device" "R")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_hydrates_current_resistor_sim_model_from_value", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("10k")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("r=10k")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![("r".to_string(), "10k".to_string())])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([("r".to_string(), "10k".to_string())]))
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_current_dc_source_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000919")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "1")
    (property "Sim.Device" "V")
    (property "Sim.Type" "DC")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_current_dc_source_sim_model_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("1")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("dc=1")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![("dc".to_string(), "1".to_string())])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_current_behavioral_resistor_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091a")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "{V(in)-V(out)}")
    (property "Sim.Device" "R")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_current_behavioral_resistor_sim_model_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("=")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("{V(in)-V(out)}")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([(
            "r".to_string(),
            "{V(in)-V(out)}".to_string()
        )]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_normalizes_current_resistor_value_notation() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000933")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "3,300u")
    (property "Sim.Device" "R")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_normalizes_current_resistor_value_notation", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![("r".to_string(), "3300u".to_string())])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_normalizes_current_source_value_notation() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000934")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "AC 1Meg")
    (property "Sim.Device" "V")
    (property "Sim.Type" "DC")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_normalizes_current_source_value_notation", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![("ac".to_string(), "1M".to_string())])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_current_resistor_sim_pins_from_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091b")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim.Device" "R")
    (property "Sim.Params" "r=10k")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_defaults_current_resistor_sim_pins_from_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([("r".to_string(), "10k".to_string())]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_current_pulse_source_sim_pins_from_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091c")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Sim.Device" "V")
    (property "Sim.Type" "PULSE")
    (property "Sim.Params" "y1=0 y2=2 td=1n")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_defaults_current_pulse_source_sim_pins_from_fields",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("td".to_string(), "1n".to_string()),
            ("y1".to_string(), "0".to_string()),
            ("y2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_missing_current_primary_param_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091d")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "1Meg")
    (property "Sim.Device" "V")
    (property "Sim.Type" "PULSE")
    (property "Sim.Params" "y2=2 td=1n")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_missing_current_primary_param_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("1Meg")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![
            ("y1".to_string(), "1M".to_string()),
            ("y2".to_string(), "2".to_string()),
            ("td".to_string(), "1n".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_tracks_current_raw_spice_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091d")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "B=V(in,out)")
    (property "Sim.Device" "SPICE")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_tracks_current_raw_spice_model_from_value", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::RawSpice)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("B=V(in,out)")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_current_library_backed_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091e")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.subckt MODEL IN OUT
.ends MODEL|)))
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "10k")
    (property "Sim.Device" "R")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_current_library_backed_sim_model_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::LibraryReference)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::SpiceSubckt)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("10k")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![("r".to_string(), "10k".to_string())])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_hydrates_library_backed_control_source_sim_model_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000091e")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.subckt MODEL OUT IN CTRL_P CTRL_N
.ends MODEL|)))
  (lib_symbols
    (symbol "Device:G"
      (symbol "G_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "+" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "-" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C+" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C-" (effects (font (size 1.27 1.27))))
          (number "4" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:G")
    (property "Reference" "G?")
    (property "Value" "5m")
    (property "Sim.Device" "G")
    (property "Sim.Library" "models.lib")
    (property "Sim.Name" "MODEL")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_hydrates_library_backed_control_source_sim_model_from_value",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::LibraryReference)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::SpiceSubckt)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Value)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.stored_value.as_deref()),
        Some("5m")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("gain=5m")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "+".to_string()),
            ("2".to_string(), "-".to_string()),
            ("3".to_string(), "C+".to_string()),
            ("4".to_string(), "C-".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_fixes_legacy_global_power_symbol_value_from_hidden_power_pin() {
    let src = r##"(kicad_sch
  (version 20230220)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090d")
  (paper "A4")
  (lib_symbols
    (symbol "power:VCC"
      (power)
      (property "Reference" "#PWR")
      (property "Value" "VCC")
      (symbol "VCC_1_1"
        (pin power_in line
          (at 0 0 180)
          (length 2.54)
          (hide yes)
          (name "PWR_FLAG")
          (number "1")))))
  (symbol
    (lib_id "power:VCC")
    (property "Reference" "#PWR?")
    (property "Value" "WRONG")
    (at 1 2 0))
)"##;
    let path = temp_schematic("loader_fixes_legacy_global_power_symbol_value", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("PWR_FLAG")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_leaves_local_power_and_visible_power_pins_untouched() {
    let src = r##"(kicad_sch
  (version 20230220)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090e")
  (paper "A4")
  (lib_symbols
    (symbol "power:LOCAL"
      (power)
      (property "Reference" "#PWR")
      (property "Value" "LOCAL")
      (symbol "LOCAL_1_1"
        (pin power_in line
          (at 0 0 180)
          (length 2.54)
          (hide yes)
          (name "LOCALPIN")
          (number "1"))))
    (symbol "power:VISIBLE"
      (power)
      (property "Reference" "#PWR")
      (property "Value" "VISIBLE")
      (symbol "VISIBLE_1_1"
        (pin power_in line
          (at 0 0 180)
          (length 2.54)
          (name "VISIBLEPIN")
          (number "1")))))
  (symbol
    (lib_id "power:LOCAL")
    (lib_name "LOCAL")
    (property "Reference" "#PWR?")
    (property "Value" "KEEP_LOCAL")
    (at 1 2 0))
  (symbol
    (lib_id "power:VISIBLE")
    (property "Reference" "#PWR?")
    (property "Value" "KEEP_VISIBLE")
    (at 3 4 0))
)"##;
    let path = temp_schematic("loader_leaves_local_and_visible_power_symbols", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbols = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(symbols.len(), 2);
    assert_eq!(
        symbols[0]
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("KEEP_LOCAL")
    );
    assert_eq!(
        symbols[1]
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("KEEP_VISIBLE")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_fixes_legacy_power_symbol_value_for_active_unit_pin() {
    let src = r##"(kicad_sch
  (version 20230220)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090f")
  (paper "A4")
  (lib_symbols
    (symbol "power:MULTI"
      (power)
      (property "Reference" "#PWR")
      (property "Value" "MULTI")
      (symbol "MULTI_1_1"
        (pin power_in line
          (at 0 0 180)
          (length 2.54)
          (hide yes)
          (name "UNIT1")
          (number "1")))
      (symbol "MULTI_2_1"
        (pin power_in line
          (at 0 0 180)
          (length 2.54)
          (hide yes)
          (name "UNIT2")
          (number "1")))))
  (symbol
    (lib_id "power:MULTI")
    (unit 2)
    (property "Reference" "#PWR?")
    (property "Value" "WRONG")
    (at 1 2 0))
)"##;
    let path = temp_schematic("loader_fixes_legacy_power_symbol_active_unit", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("UNIT2")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_preserves_mid_v7_sim_pin_indexes_without_source_pins() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000304")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Sim_Device" "R")
    (property "Sim_Type" "R")
    (property "Sim_Params" "r=10k")
    (property "Sim_Pins" "2 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_preserves_mid_v7_sim_pin_indexes", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_spice_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000005")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Spice_Primitive" "R")
    (property "Spice_Model" "1k")
    (property "Spice_Node_Sequence" "2 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Model")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Node_Sequence")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"R\" model=\"1k\" lib=\"\"")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2=1 1=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("R")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("type=\"R\" model=\"1k\" lib=\"\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("lib".to_string(), "".to_string()),
            ("model".to_string(), "1k".to_string()),
            ("type".to_string(), "R".to_string()),
        ]))
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("R")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "2".to_string()),
            ("2".to_string(), "1".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_model_only_legacy_spice_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000305")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Spice_Model" "1k")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_model_only_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Model")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("1k")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("1k")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_lib_only_legacy_spice_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000306")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_lib_only_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Lib_File")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"\" model=\"\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("type=\"\" model=\"\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("lib".to_string(), "models.lib".to_string()),
            ("model".to_string(), "".to_string()),
            ("type".to_string(), "".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_sim_pins_from_active_symbol_unit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000406")
  (paper "A4")
  (lib_symbols
    (symbol "Device:U"
      (symbol "U_1_1"
        (pin input line (at 0 0 180) (length 2.54) (name "A") (number "1")))
      (symbol "U_2_1"
        (pin input line (at 0 0 180) (length 2.54) (name "B") (number "2"))
        (pin input line (at 5 0 180) (length 2.54) (name "C") (number "3")))))
  (symbol
    (lib_id "Device:U")
    (unit 2)
    (property "Reference" "U?")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_defaults_sim_pins_from_active_unit", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2=1 3=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("2".to_string(), "1".to_string()),
            ("3".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_sorts_default_sim_pins_numerically() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000410")
  (paper "A4")
  (lib_symbols
    (symbol "Device:Q"
      (symbol "Q_1_1"
        (pin input line (at 0 0 180) (length 2.54) (name "A") (number "10"))
        (pin input line (at 5 0 180) (length 2.54) (name "B") (number "2"))
        (pin input line (at 10 0 180) (length 2.54) (name "C") (number "1")))))
  (symbol
    (lib_id "Device:Q")
    (property "Reference" "Q?")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_sorts_default_sim_pins_numerically", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=1 2=2 10=3")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
            ("10".to_string(), "3".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_primitive_and_lib_legacy_spice_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000307")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN_EBC")
    (property "Reference" "Q?")
    (property "Spice_Primitive" "Q")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_primitive_lib_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"Q\" model=\"\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("Q")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("type=\"Q\" model=\"\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("lib".to_string(), "models.lib".to_string()),
            ("model".to_string(), "".to_string()),
            ("type".to_string(), "Q".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_comma_separated_legacy_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000308")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "pulse(0,2,1n,2n,3n,4n,5n,6)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_comma_separated_legacy_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("y1=0 y2=2 td=1n tr=2n tf=3n tw=4n per=5n np=6")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("y1=0 y2=2 td=1n tr=2n tf=3n tw=4n per=5n np=6")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_pairs.clone()),
        Some(vec![
            ("y1".to_string(), "0".to_string()),
            ("y2".to_string(), "2".to_string()),
            ("td".to_string(), "1n".to_string()),
            ("tr".to_string(), "2n".to_string()),
            ("tf".to_string(), "3n".to_string()),
            ("tw".to_string(), "4n".to_string()),
            ("per".to_string(), "5n".to_string()),
            ("np".to_string(), "6".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("np".to_string(), "6".to_string()),
            ("per".to_string(), "5n".to_string()),
            ("td".to_string(), "1n".to_string()),
            ("tf".to_string(), "3n".to_string()),
            ("tr".to_string(), "2n".to_string()),
            ("tw".to_string(), "4n".to_string()),
            ("y1".to_string(), "0".to_string()),
            ("y2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_primitive_only_legacy_spice_fields_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000309")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "1k")
    (property "Spice_Primitive" "Q")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_primitive_only_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("${SIM.PARAMS}")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"Q\" model=\"1k\" lib=\"\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("Q")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("1k")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_value_backed_legacy_spice_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030a")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN_EBC")
    (property "Reference" "Q?")
    (property "Value" "BC547")
    (property "Spice_Primitive" "Q")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_value_backed_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("${SIM.PARAMS}")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"Q\" model=\"BC547\" lib=\"\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("Q")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("BC547")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Params)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::RawSpice)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_value_backed_legacy_spice_lib_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030b")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN_EBC")
    (property "Reference" "Q?")
    (property "Value" "BC547")
    (property "Spice_Primitive" "Q")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_value_backed_legacy_spice_lib_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive" || property.key == "Spice_Lib_File")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("${SIM.PARAMS}")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"Q\" model=\"BC547\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("BC547")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Params)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::LibraryReference)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_resolved_legacy_spice_lib_fields_to_library_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030b")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.subckt BC547 C B E
.ends BC547|)))
  (lib_symbols
    (symbol "Device:Q"
      (symbol "Q_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "B" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "E" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:Q")
    (property "Reference" "Q?")
    (property "Value" "seed")
    (property "Spice_Primitive" "Q")
    (property "Spice_Model" "BC547 area=2")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_resolved_legacy_spice_lib_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(!symbol.properties.iter().any(|property| matches!(
        property.key.as_str(),
        "Spice_Primitive" | "Spice_Model" | "Spice_Lib_File"
    )));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Library")
            .map(|property| property.value.as_str()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Name")
            .map(|property| property.value.as_str()),
        Some("BC547")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("area=2")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=C 2=B 3=E")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("BC547")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("area=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::LibraryReference)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::SpiceSubckt)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "C".to_string()),
            ("2".to_string(), "B".to_string()),
            ("3".to_string(), "E".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_value_backed_resolved_legacy_spice_lib_fields_to_library_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030c")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.subckt BC547 C B E
.ends BC547|)))
  (lib_symbols
    (symbol "Device:Q"
      (symbol "Q_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "C" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "B" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "E" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:Q")
    (property "Reference" "Q?")
    (property "Value" "BC547")
    (property "Spice_Primitive" "Q")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic(
        "loader_migrates_value_backed_resolved_legacy_spice_lib_fields",
        src,
    );
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("${SIM.NAME}")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Library")
            .map(|property| property.value.as_str()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Name")
            .map(|property| property.value.as_str()),
        Some("BC547")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Name)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::LibraryReference)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "C".to_string()),
            ("2".to_string(), "B".to_string()),
            ("3".to_string(), "E".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_legacy_library_backed_sim_pins_from_resolved_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030c")
  (paper "A4")
  (embedded_files
    (file
      (name "models.lib")
      (type model)
      (data |.subckt MODEL IN OUT VSS
.ends MODEL|)))
  (lib_symbols
    (symbol "Device:Q"
      (symbol "Q_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P3" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:Q")
    (property "Reference" "Q?")
    (property "Value" "MODEL")
    (property "Spice_Primitive" "Q")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_defaults_legacy_library_backed_sim_pins", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=IN 2=OUT 3=VSS")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::LibraryReference)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "IN".to_string()),
            ("2".to_string(), "OUT".to_string()),
            ("3".to_string(), "VSS".to_string()),
        ])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_explicit_ibis_sim_pins_from_resolved_component() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090f")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
C3 VSS POWER
|)))
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P3" (effects (font (size 1.27 1.27))))
          (number "3" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "DRIVER")
    (property "Sim.Device" "SPICE")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (property "Sim.Ibis.Pin" "A1")
    (property "Sim.Ibis.Model" "MODEL_A")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_defaults_explicit_ibis_sim_pins", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=A1 2=B2 3=C3")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::Ibis)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pin_pairs.clone()),
        Some(vec![
            ("1".to_string(), "A1".to_string()),
            ("2".to_string(), "B2".to_string()),
            ("3".to_string(), "C3".to_string()),
        ])
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.generated_pin_names.clone()),
        Some(vec!["A1".to_string(), "B2".to_string(), "C3".to_string()])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_treats_explicit_ibis_device_as_ibis_origin() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000990")
  (paper "A4")
  (embedded_files
    (file
      (name "driver.ibs")
      (type model)
      (data |[Component] DRIVER
[Pin]
pin signal model
A1 SIGA MODEL_A
B2 SIGB MODEL_B
|)))
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P1" (effects (font (size 1.27 1.27))))
          (number "1" (effects (font (size 1.27 1.27)))))
        (pin passive line (at 0 0 0) (length 2.54)
          (name "P2" (effects (font (size 1.27 1.27))))
          (number "2" (effects (font (size 1.27 1.27))))))))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "DRIVER")
    (property "Sim.Device" "IBIS")
    (property "Sim.Library" "driver.ibs")
    (property "Sim.Name" "DRIVER")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_treats_explicit_ibis_device_as_ibis_origin", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::Ibis)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_library.as_ref())
            .map(|library| library.kind),
        Some(SimLibraryKind::Ibis)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.resolved_kind),
        Some(ResolvedSimModelKind::IbisComponent)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_strips_inline_params_from_legacy_spice_lib_model_name() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030e")
  (paper "A4")
  (symbol
    (lib_id "Device:Q_NPN_EBC")
    (property "Reference" "Q?")
    (property "Value" "seed")
    (property "Spice_Primitive" "Q")
    (property "Spice_Model" "BC547 area=2")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_strips_inline_legacy_spice_lib_model_params", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"Q\" model=\"BC547\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("BC547")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_value_backed_legacy_dc_source_fields_when_not_inferred() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030c")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "X?")
    (property "Value" "dc(1)")
    (property "Spice_Primitive" "V")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_value_backed_legacy_dc_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("1")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("DC")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("DC")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_value_backed_legacy_source_fields_when_not_inferred() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000030d")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "X?")
    (property "Value" "pulse(0 2 1n 2n 3n 4n 5n 6)")
    (property "Spice_Primitive" "V")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_value_backed_legacy_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("${SIM.PARAMS}")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("y1=0 y2=2 td=1n tr=2n tf=3n tw=4n per=5n np=6")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.origin),
        Some(SimModelOrigin::BuiltIn)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.value_binding),
        Some(SimValueBinding::Params)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_spice_pin_maps_with_newline_whitespace() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000205")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Spice_Primitive" "R")
    (property "Spice_Model" "1k")
    (property "Spice_Node_Sequence" "2\t1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_spice_pin_map_whitespace", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2=1 1=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "2".to_string()),
            ("2".to_string(), "1".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_spice_lib_fields_to_raw_sim_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000105")
  (paper "A4")
  (lib_symbols
    (symbol "Device:Q_NPN_EBC"
      (symbol "Q_NPN_EBC_1_1"
        (pin input line (at 0 0 180) (length 2.54) (name "B") (number "2"))
        (pin input line (at 5 0 180) (length 2.54) (name "C") (number "3"))
        (pin input line (at 10 0 180) (length 2.54) (name "E") (number "1")))))
  (symbol
    (lib_id "Device:Q_NPN_EBC")
    (property "Reference" "Q?")
    (property "Spice_Primitive" "Q")
    (property "Spice_Model" "BC\"547")
    (property "Spice_Lib_File" "models.lib")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_spice_lib_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Model")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Lib_File")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("type=\"Q\" model=\"BC\\\"547\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=1 2=2 3=3")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("SPICE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("Q")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("type=\"Q\" model=\"BC\\\"547\" lib=\"models.lib\"")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref()),
        Some("models.lib")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.name.as_deref()),
        Some("BC\"547")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("lib".to_string(), "models.lib".to_string()),
            ("model".to_string(), "BC\"547".to_string()),
            ("type".to_string(), "Q".to_string()),
        ]))
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
            ("3".to_string(), "3".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_inferred_legacy_spice_fields_from_value() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000006")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Value" "1k")
    (property "Spice_Primitive" "R")
    (property "Spice_Node_Sequence" "2 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_inferred_legacy_spice_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Node_Sequence")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Device")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Sim.Params")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("1k")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2=1 1=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "2".to_string()),
            ("2".to_string(), "1".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_dc_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000007")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "dc(1)")
    (property "Spice_Node_Sequence" "1 2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_dc_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Model")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("1")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("V")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("DC")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=1 2=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("DC")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        None
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_dc_source_fields_with_whitespace_model() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000207")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "dc(\t1)")
    (property "Spice_Node_Sequence" "1 2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_dc_source_fields_whitespace", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("1")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("DC")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("DC")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_mixed_case_legacy_dc_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000407")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "Dc(1)")
    (property "Spice_Node_Sequence" "1 2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_mixed_case_legacy_dc_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("1")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("DC")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_sin_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000008")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "sin(0 1 1k 2n)")
    (property "Spice_Node_Sequence" "1 2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_sin_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Model")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("seed")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("V")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("SIN")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("dc=0 ampl=1 f=1k td=2n")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=1 2=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("SIN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("dc=0 ampl=1 f=1k td=2n")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_mixed_case_legacy_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000408")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "sIn(0 1 1k 2n)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_mixed_case_legacy_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("SIN")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("dc=0 ampl=1 f=1k td=2n")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("SIN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("dc=0 ampl=1 f=1k td=2n")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_pulse_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000009")
  (paper "A4")
  (symbol
    (lib_id "Device:I")
    (property "Reference" "I?")
    (property "Value" "seed")
    (property "Spice_Primitive" "I")
    (property "Spice_Model" "pulse(0 2 1n 2n 3n 4n 5n 6)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_pulse_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("I")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("y1=0 y2=2 td=1n tr=2n tf=3n tw=4n per=5n np=6")
    );
    assert!(
        !symbol
            .properties
            .iter()
            .any(|property| property.key == "Spice_Primitive" || property.key == "Spice_Model")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("seed")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("I")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("y1=0 y2=2 td=1n tr=2n tf=3n tw=4n per=5n np=6")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("np".to_string(), "6".to_string()),
            ("per".to_string(), "5n".to_string()),
            ("td".to_string(), "1n".to_string()),
            ("tf".to_string(), "3n".to_string()),
            ("tr".to_string(), "2n".to_string()),
            ("tw".to_string(), "4n".to_string()),
            ("y1".to_string(), "0".to_string()),
            ("y2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_pwl_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090a")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "pwl(0 0 1n 5)")
    (property "Spice_Node_Sequence" "1 2")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_pwl_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("V")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("PWL")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some(r#"pwl="0 0 1n 5""#)
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.param_values.get("pwl"))
            .map(|value| value.as_str()),
        Some("0 0 1n 5")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("PWL")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=1 2=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_whitenoise_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090b")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "whitenoise(1 2n)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_whitenoise_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("WHITENOISE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("rms=1 dt=2n")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("WHITENOISE")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("rms=1 dt=2n")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("dt".to_string(), "2n".to_string()),
            ("rms".to_string(), "1".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_random_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000090c")
  (paper "A4")
  (symbol
    (lib_id "Device:I")
    (property "Reference" "I?")
    (property "Value" "seed")
    (property "Spice_Primitive" "I")
    (property "Spice_Model" "randgaussian(1n 2n 3 4)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_random_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("I")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("RANDGAUSSIAN")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("ts=1n td=2n stddev=3 mean=4")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("I")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("RANDGAUSSIAN")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("ts=1n td=2n stddev=3 mean=4")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.param_values.clone()),
        Some(BTreeMap::from([
            ("mean".to_string(), "4".to_string()),
            ("stddev".to_string(), "3".to_string()),
            ("td".to_string(), "2n".to_string()),
            ("ts".to_string(), "1n".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_defaults_source_pin_map_for_legacy_source_models() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000109")
  (paper "A4")
  (lib_symbols
    (symbol "Device:V"
      (symbol "V_1_1"
        (pin passive line (at 0 0 180) (length 2.54) (name "+") (number "2"))
        (pin passive line (at 10 0 0) (length 2.54) (name "-") (number "1")))))
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "pulse(0 2 1n 2n 3n 4n 5n 6)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_defaults_legacy_source_pin_map", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("V")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("PULSE")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("1=1 2=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_punctuated_legacy_spice_pin_maps() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-000000000409")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R?")
    (property "Spice_Primitive" "R")
    (property "Spice_Model" "1k")
    (property "Spice_Node_Sequence" "{2:1}")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_punctuated_legacy_spice_pin_maps", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2=1 1=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "2".to_string()),
            ("2".to_string(), "1".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_exp_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000000a")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "exp(0 5 1n 2n 3n 4n)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_exp_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("EXP")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("y1=0 y2=5 td1=1n tau1=2n td2=3n tau2=4n")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("EXP")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("y1=0 y2=5 td1=1n tau1=2n td2=3n tau2=4n")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_am_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000000b")
  (paper "A4")
  (symbol
    (lib_id "Device:V")
    (property "Reference" "V?")
    (property "Value" "seed")
    (property "Spice_Primitive" "V")
    (property "Spice_Model" "am(1 2 3 4k 5k 6n 7 8)")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_am_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("AM")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("vo=1 vmo=2 vma=3 fm=4k fc=5k td=6n phasem=7 phasec=8")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("V")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("AM")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("vo=1 vmo=2 vma=3 fm=4k fc=5k td=6n phasem=7 phasec=8")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn load_tree_migrates_legacy_sffm_source_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000000c")
  (paper "A4")
  (symbol
    (lib_id "Device:I")
    (property "Reference" "I?")
    (property "Value" "seed")
    (property "Spice_Primitive" "I")
    (property "Spice_Model" "sffm(1 2 3k 4 5k 6 7)")
    (property "Spice_Node_Sequence" "2 1")
    (at 1 2 0))
)"#;
    let path = temp_schematic("loader_migrates_legacy_sffm_source_fields", src);
    let loaded = load_schematic_tree(Path::new(&path)).expect("must load");
    let schematic = loaded
        .schematics
        .iter()
        .find(|schematic| schematic.path == path.canonicalize().unwrap_or(path.clone()))
        .expect("loaded schematic");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert!(schematic.screen.content_modified);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.as_str()),
        Some("I")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.as_str()),
        Some("SFFM")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.as_str()),
        Some("vo=1 va=2 fm=3k mdi=4 fc=5k phasem=6 phasec=7")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| property.value.as_str()),
        Some("2=1 1=2")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.device.as_deref()),
        Some("I")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.model_type.as_deref()),
        Some("SFFM")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.params.as_deref()),
        Some("vo=1 va=2 fm=3k mdi=4 fc=5k phasem=6 phasec=7")
    );
    assert_eq!(
        symbol
            .sim_model
            .as_ref()
            .map(|sim_model| sim_model.pins.clone()),
        Some(BTreeMap::from([
            ("1".to_string(), "2".to_string()),
            ("2".to_string(), "1".to_string()),
        ]))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parses_symbol_mirror_body_style_and_sheet_pins() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "20000000-0000-0000-0000-000000000001")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 10 20 270) (mirror x) (body_style 2) (uuid "20000000-0000-0000-0000-000000000002"))
  (sheet (at 0 0) (size 20 10) (uuid "20000000-0000-0000-0000-000000000003")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input (at 0 5 180) (uuid "20000000-0000-0000-0000-000000000004"))
    (pin "OUT" output (at 20 5 0) (uuid "20000000-0000-0000-0000-000000000005"))
    (pin "TOP" bidirectional (at 10 0 90) (uuid "20000000-0000-0000-0000-000000000006"))
    (pin "BOT" tri_state (at 10 10 270) (uuid "20000000-0000-0000-0000-000000000007"))
    (pin "PASS" passive (at 5 5 0) (effects (font (size 1 2)) (hide)) (uuid "20000000-0000-0000-0000-000000000008")))
)"#;
    let path = temp_schematic("symbol_sheet_pins", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.angle, 270.0);
    assert_eq!(symbol.mirror, Some(MirrorAxis::X));
    assert_eq!(symbol.body_style, Some(2));

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 5);
    assert_eq!(sheet.pins[0].shape, SheetPinShape::Input);
    assert_eq!(sheet.pins[0].side, SheetSide::Left);
    assert_eq!(sheet.pins[1].side, SheetSide::Right);
    assert_eq!(sheet.pins[2].side, SheetSide::Top);
    assert_eq!(sheet.pins[3].side, SheetSide::Bottom);
    assert_eq!(sheet.pins[4].shape, SheetPinShape::Unspecified);
    assert!(sheet.pins[4].has_effects);
    assert!(!sheet.pins[4].visible);
    assert_eq!(
        sheet.pins[4]
            .effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([2.0, 1.0])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_pin_without_at_keeps_default_geometry() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "40000000-0000-0000-0000-00000000000d")
  (paper "A4")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input))
)"#;
    let path = temp_schematic("sheet_pin_without_at", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].at, [0.0, 0.0]);
    assert_eq!(sheet.pins[0].side, SheetSide::Left);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_pin_without_at_uses_current_sheet_orientation() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000000e")
  (paper "A4")
  (sheet
    (size 5 20)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input))
)"#;
    let path = temp_schematic("sheet_pin_without_at_vertical_sheet", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].at, [0.0, 0.0]);
    assert_eq!(sheet.pins[0].side, SheetSide::Top);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_pin_before_size_keeps_pre_size_default_side() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000000f")
  (paper "A4")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input)
    (size 5 20))
)"#;
    let path = temp_schematic("sheet_pin_before_size_default_side", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].side, SheetSide::Left);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_pin_before_at_moves_with_sheet_position() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000010")
  (paper "A4")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input)
    (at 11 22))
)"#;
    let path = temp_schematic("sheet_pin_before_at_moves_with_sheet", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].at, [11.0, 22.0]);
    assert_eq!(sheet.pins[0].side, SheetSide::Left);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_pin_at_uses_explicit_side_on_sheet_edge() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000011")
  (paper "A4")
  (sheet
    (at 10 20)
    (size 30 40)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input (at 999 25 0)))
)"#;
    let path = temp_schematic("sheet_pin_at_uses_explicit_side_edge", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].at, [40.0, 20.0]);
    assert_eq!(sheet.pins[0].side, SheetSide::Right);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_size_reconstrains_existing_pin_position() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000012")
  (paper "A4")
  (sheet
    (size 50 50)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input (at 0 30 180))
    (size 10 20))
)"#;
    let path = temp_schematic("sheet_size_reconstrains_existing_pin", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].at, [0.0, 20.0]);
    assert_eq!(sheet.pins[0].side, SheetSide::Left);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_pin_without_at_uses_sheet_owner_position_defaults() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000013")
  (paper "A4")
  (sheet
    (at 11 22)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input))
)"#;
    let path = temp_schematic("sheet_pin_without_at_owner_position", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].at, [11.0, 0.0]);
    assert_eq!(sheet.pins[0].side, SheetSide::Left);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_sheet_pin_name() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000001")
  (sheet
    (at 0 0)
    (size 10 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin (at 1 2 180) input))
)"#;
    let path = temp_schematic("invalid_sheet_pin_name", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must fail");
    assert!(format!("{err}").contains("Invalid sheet pin name"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_sheet_pin_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000002")
  (sheet
    (at 0 0)
    (size 10 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input (at 1 2 180) (uuid (bogus))))
)"#;
    let path = temp_schematic("invalid_sheet_pin_uuid", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must fail");
    assert!(format!("{err}").contains("expecting uuid"));
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_decimal_sheet_pin_side_angles() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000003")
  (sheet
    (at 0 0)
    (size 10 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input (at 1 2 90.0)))
)"#;
    let path = temp_schematic("decimal_sheet_pin_angle", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.pins.len(), 1);
    assert_eq!(sheet.pins[0].side, SheetSide::Top);
    let _ = fs::remove_file(path);
}

#[test]
fn parses_property_metadata_semantics() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000014")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (property private "UserField" "R1"
      (id 0)
      (at 11 22 90)
      (hide yes)
      (show_name no)
      (do_not_autoplace yes)
      (effects (font (size 1.27 1.27)))))
)"#;
    let path = temp_schematic("property_metadata", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let property = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => symbol
                .properties
                .iter()
                .find(|property| property.key == "UserField"),
            _ => None,
        })
        .expect("property");

    assert_eq!(property.id, Some(0));
    assert_eq!(property.ordinal, 42);
    assert!(property.is_private);
    assert_eq!(property.at, Some([11.0, 22.0]));
    assert_eq!(property.angle, Some(90.0));
    assert!(!property.visible);
    assert!(!property.show_name);
    assert!(!property.can_autoplace);
    assert!(property.has_effects);
    let effects = property.effects.as_ref().expect("property effects");
    assert_eq!(effects.font_size, Some([1.27, 1.27]));
    assert_eq!(effects.h_justify, TextHJustify::Center);
    assert_eq!(effects.v_justify, TextVJustify::Center);

    let default_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000015")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (property "UserField" "R1"))
)"#;
    let default_path = temp_schematic("property_metadata_default_show_name", default_src);
    let schematic =
        parse_schematic_file(Path::new(&default_path)).expect("must parse default property");
    let property = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => symbol
                .properties
                .iter()
                .find(|property| property.key == "UserField"),
            _ => None,
        })
        .expect("property");
    assert!(!property.show_name);
    assert_eq!(property.ordinal, 42);
    assert_eq!(property.at, Some([0.0, 0.0]));
    assert_eq!(property.angle, Some(0.0));
    let _ = fs::remove_file(default_path);

    let ordinal_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000016")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (property "UserA" "A")
    (property "UserB" "B"))
)"#;
    let ordinal_path = temp_schematic("property_metadata_ordinals", ordinal_src);
    let schematic =
        parse_schematic_file(Path::new(&ordinal_path)).expect("must parse ordinal properties");
    let mut ordinals = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(
                symbol
                    .properties
                    .iter()
                    .filter(|property| matches!(property.kind, PropertyKind::User))
                    .map(|property| (property.key.as_str(), property.ordinal))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("symbol");
    ordinals.sort_by_key(|(key, _)| *key);
    assert_eq!(ordinals, vec![("UserA", 42), ("UserB", 43)]);
    let _ = fs::remove_file(ordinal_path);

    let bare_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000017")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (property "UserField" "R1"
      (show_name)
      (do_not_autoplace)))
)"#;
    let bare_path = temp_schematic("property_metadata_bare_bools", bare_src);
    let schematic = parse_schematic_file(Path::new(&bare_path)).expect("must parse bare bools");
    let property = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => symbol
                .properties
                .iter()
                .find(|property| property.key == "UserField"),
            _ => None,
        })
        .expect("property");
    assert!(property.show_name);
    assert!(!property.can_autoplace);
    let _ = fs::remove_file(bare_path);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_property_header_tokens() {
    let invalid_name_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000004")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0) (property))
)"#;
    let invalid_name_path = temp_schematic("invalid_property_name", invalid_name_src);
    let err = parse_schematic_file(Path::new(&invalid_name_path))
        .expect_err("must reject invalid property name");
    assert!(err.to_string().contains("Invalid property name"));
    let _ = fs::remove_file(invalid_name_path);

    let empty_name_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000005")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0) (property "" "v"))
)"#;
    let empty_name_path = temp_schematic("empty_property_name", empty_name_src);
    let err = parse_schematic_file(Path::new(&empty_name_path))
        .expect_err("must reject empty property name");
    assert!(err.to_string().contains("Empty property name"));
    let _ = fs::remove_file(empty_name_path);

    let invalid_value_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000006")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0) (property "X"))
)"#;
    let invalid_value_path = temp_schematic("invalid_property_value", invalid_value_src);
    let err = parse_schematic_file(Path::new(&invalid_value_path))
        .expect_err("must reject invalid property value");
    assert!(err.to_string().contains("Invalid property value"));
    let _ = fs::remove_file(invalid_value_path);

    let quoted_field_head_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000001")
  (symbol
    (lib_id "Device:R")
    (property "User" "v"
      ("at" 1 2 0)))
)"#;
    let quoted_field_head_path = temp_schematic("quoted_sch_field_head", quoted_field_head_src);
    let err = parse_schematic_file(Path::new(&quoted_field_head_path))
        .expect_err("must reject quoted sch field head token");
    assert!(
        err.to_string()
            .contains("expecting id, at, hide, show_name, do_not_autoplace or effects")
    );
    let _ = fs::remove_file(quoted_field_head_path);
}

#[test]
fn private_only_survives_on_true_user_fields() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000007")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0)
    (property private "UserField" "sym"))
  (sheet (at 0 0) (size 10 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (property private "UserField" "sheet"))
  (global_label "G" (shape input) (at 0 0 0)
    (property private "UserField" "glob"))
)"#;
    let path = temp_schematic("private_true_user_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let symbol_user = symbol
        .properties
        .iter()
        .find(|property| property.key == "UserField")
        .expect("symbol user field");
    assert!(symbol_user.is_private);

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    let sheet_user = sheet
        .properties
        .iter()
        .find(|property| property.kind == ki2::model::PropertyKind::SheetUser)
        .expect("sheet user field");
    assert_eq!(sheet_user.id, Some(9));
    assert!(!sheet_user.is_private);

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let global_user = global
        .properties
        .iter()
        .find(|property| property.key == "UserField")
        .expect("global user field");
    assert!(global_user.is_private);

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_user_fields_advance_pending_ordinals() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000002")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (property "UserA" "A")
    (property "UserB" "B"))
)"#;
    let path = temp_schematic("sheet_user_field_ordinals", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let mut ordinals = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(
                sheet
                    .properties
                    .iter()
                    .filter(|property| property.kind == PropertyKind::SheetUser)
                    .map(|property| (property.key.as_str(), property.ordinal))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("sheet");
    ordinals.sort_by_key(|(key, _)| *key);
    assert_eq!(ordinals, vec![("UserA", 42), ("UserB", 42)]);

    let _ = fs::remove_file(path);
}

#[test]
fn respects_hide_inside_property_effects() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000003")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (property "UserField" "R1"
      (effects (hide) (font (size 1.27 1.27)))))
)"#;
    let path = temp_schematic("property_effects_hide", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let property = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => symbol
                .properties
                .iter()
                .find(|property| property.key == "UserField"),
            _ => None,
        })
        .expect("property");

    assert!(!property.visible);
    assert!(property.has_effects);
    assert!(property.effects.as_ref().expect("effects").hidden);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_sheet_missing_required_properties() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000004")
  (paper "A4")
  (sheet (at 0 0) (size 20 10)
    (property "Sheetname" "Child"))
)"#;
    let path = temp_schematic("missing_sheet_file", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject missing sheet file");
    assert!(err.to_string().contains("Missing sheet file property"));
    let _ = fs::remove_file(path);
}

#[test]
fn sheet_does_not_require_at_or_size() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000005")
  (paper "A4")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let path = temp_schematic("sheet_without_at_or_size", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept sheet without at or size");
    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.at, [0.0, 0.0]);
    assert_eq!(sheet.size, [0.0, 0.0]);
    assert_eq!(sheet.border_width, 0.0);
    assert_eq!(sheet.border_color, None);
    assert_eq!(sheet.background_color, None);
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_sheet_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000006")
  (paper "A4")
  (sheet (at 0 0) (size 20 10)
    (bogus 1)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let path = temp_schematic("unexpected_sheet_child", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad sheet child");
    let msg = err.to_string();
    assert!(
        msg.contains("expecting at, size, stroke, background, instances, uuid, property, or pin")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn canonicalizes_and_replaces_mandatory_properties() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000007")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (property "Reference" "R1")
    (property "reference" "R2")
    (property "Value" "10k"))
  (sheet (at 0 0) (size 20 10)
    (property "Sheet name" "Legacy Name")
    (property "Sheetname" "New Name")
    (property "Sheet file" "legacy.kicad_sch")
    (property "Sheetfile" "new.kicad_sch"))
)"#;
    let path = temp_schematic("canonical_props", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let reference = symbol
        .properties
        .iter()
        .find(|property| property.key == "Reference")
        .expect("reference property");
    assert_eq!(reference.value, "R2");
    assert_eq!(
        symbol
            .properties
            .iter()
            .filter(|property| property.key == "Reference")
            .count(),
        1
    );

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.name(), Some("Legacy Name"));
    assert_eq!(sheet.filename().as_deref(), Some("legacy.kicad_sch"));
    assert_eq!(
        sheet
            .properties
            .iter()
            .filter(|property| property.kind == PropertyKind::SheetName)
            .count(),
        2
    );
    assert_eq!(
        sheet
            .properties
            .iter()
            .filter(|property| property.kind == PropertyKind::SheetFile)
            .count(),
        2
    );

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_duplicate_user_properties_overwrite_by_name() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000008")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (property "UserField" "First")
    (property "UserField" "Second"))
)"#;
    let path = temp_schematic("symbol_duplicate_user_properties", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    let matching = symbol
        .properties
        .iter()
        .filter(|property| property.key == "UserField")
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].value, "Second");

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_starts_with_root_unit_even_without_root_draw_items() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "77000000-0000-0000-0000-000000000009")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_2_1"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1)))))))
)"#;
    let path = temp_schematic("lib_symbol_default_root_unit", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(lib_symbol.units.len(), 2);
    assert_eq!(lib_symbol.units[0].name, "R_1_1");
    assert_eq!(lib_symbol.units[0].unit_number, 1);
    assert_eq!(lib_symbol.units[0].body_style, 1);
    assert!(lib_symbol.units[0].draw_items.is_empty());
    assert_eq!(lib_symbol.units[1].name, "R_2_1");

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_materializes_missing_body_style_slots_from_nested_units() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_2_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1)))))))
)"#;
    let path = temp_schematic("lib_symbol_materialize_body_style_slots", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(lib_symbol.units.len(), 4);
    assert_eq!(lib_symbol.units[0].name, "R_1_1");
    assert_eq!(lib_symbol.units[1].name, "R_1_2");
    assert_eq!(lib_symbol.units[2].name, "R_2_1");
    assert_eq!(lib_symbol.units[3].name, "R_2_2");
    assert!(lib_symbol.units[0].draw_items.is_empty());
    assert!(lib_symbol.units[1].draw_items.is_empty());
    assert!(lib_symbol.units[2].draw_items.is_empty());
    assert_eq!(lib_symbol.units[3].draw_items.len(), 1);
    assert_eq!(lib_symbol.units[3].draw_items[0].kind, "text");

    let _ = fs::remove_file(path);
}

#[test]
fn lib_property_skips_user_field_after_nine_suffix_attempts() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000002")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "MPN" "0")
      (property "MPN" "1")
      (property "MPN" "2")
      (property "MPN" "3")
      (property "MPN" "4")
      (property "MPN" "5")
      (property "MPN" "6")
      (property "MPN" "7")
      (property "MPN" "8")
      (property "MPN" "9")
      (property "MPN" "10")))
)"#;
    let path = temp_schematic("lib_property_suffix_limit", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(
        lib_symbol.units[0]
            .draw_items
            .iter()
            .filter(|item| item.kind == "field")
            .filter_map(|item| item.name.as_deref())
            .filter(|name| name.starts_with("MPN"))
            .collect::<Vec<_>>(),
        vec![
            "MPN", "MPN_1", "MPN_2", "MPN_3", "MPN_4", "MPN_5", "MPN_6", "MPN_7", "MPN_8", "MPN_9"
        ]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn schematic_text_box_outline_fill_uses_stroke_color() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000003")
  (paper "A4")
  (text_box "TB"
    (at 1 2 0)
    (size 3 4)
    (stroke (width 0.1) (color 10 20 30 0.5))
    (fill (type outline))))"#;
    let path = temp_schematic("textbox_outline_fill", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");

    assert_eq!(
        text_box.fill.as_ref().map(|fill| fill.fill_type.clone()),
        Some(FillType::Color)
    );
    assert_eq!(
        text_box.fill.as_ref().and_then(|fill| fill.color),
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 0.5])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn recovers_legacy_sheet_field_ids_during_parse() {
    let src = r#"(kicad_sch
  (version 20200310)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000018")
  (paper "A4")
  (sheet
    (property "UserField" "Child" (id 0))
    (property "AnotherField" "child.kicad_sch" (id 0)))
)"#;
    let path = temp_schematic("legacy_sheet_field_ids", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    let name = sheet
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SheetName)
        .expect("sheet name");
    assert_eq!(name.key, "Sheetname");
    assert_eq!(name.id, Some(7));

    let file = sheet
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SheetFile)
        .expect("sheet file");
    assert_eq!(file.key, "Sheetfile");
    assert_eq!(file.id, Some(8));

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_sheet_field_recovery_maps_all_later_fields_to_sheetfile() {
    let src = r#"(kicad_sch
  (version 20200310)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000008")
  (paper "A4")
  (sheet
    (property "WrongOne" "Child" (id 0))
    (property "WrongTwo" "child-a.kicad_sch" (id 0))
    (property "WrongThree" "child-b.kicad_sch" (id 0)))
)"#;
    let path = temp_schematic("legacy_sheet_field_ids_many", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    assert_eq!(sheet.properties.len(), 3);
    assert_eq!(sheet.properties[0].kind, PropertyKind::SheetName);
    assert_eq!(sheet.properties[0].key, "Sheetname");
    assert_eq!(sheet.properties[0].id, Some(7));
    assert_eq!(sheet.properties[1].kind, PropertyKind::SheetFile);
    assert_eq!(sheet.properties[1].key, "Sheetfile");
    assert_eq!(sheet.properties[1].id, Some(8));
    assert_eq!(sheet.properties[2].kind, PropertyKind::SheetFile);
    assert_eq!(sheet.properties[2].key, "Sheetfile");
    assert_eq!(sheet.properties[2].id, Some(8));

    let _ = fs::remove_file(path);
}

#[test]
fn preserves_duplicate_local_sheet_instances_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-000000000009")
  (paper "A4")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/A" (page "1"))
        (path "/A" (page "2")))))
)"#;
    let path = temp_schematic("duplicate_local_sheet_instances", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    assert_eq!(sheet.instances.len(), 2);
    assert_eq!(sheet.instances[0].path, "/A");
    assert_eq!(sheet.instances[0].page.as_deref(), Some("1"));
    assert_eq!(sheet.instances[1].path, "/A");
    assert_eq!(sheet.instances[1].page.as_deref(), Some("2"));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_symbol_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "50000000-0000-0000-0000-00000000000a")
  (symbol
    (lib_id "Device:R")
    (at 10 20 0)
    (bogus 1))
)"#;
    let path = temp_schematic("unexpected_symbol_child", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol child");
    let msg = err.to_string();
    assert!(msg.contains(
        "expecting lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances"
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn raw_shape_and_textbox_uuids_do_not_increment_duplicates() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000004")
  (paper "A4")
  (arc (start 0 0) (mid 1 1) (end 2 0) (uuid "00000000-0000-0000-0000-000000000001"))
  (rectangle (start 0 0) (end 1 1) (uuid "00000000-0000-0000-0000-000000000001"))
  (bezier (pts (xy 0 0) (xy 1 1) (xy 2 1) (xy 3 0)) (uuid "00000000-0000-0000-0000-000000000001"))
  (text_box "A" (at 0 0 0) (size 1 1) (uuid "00000000-0000-0000-0000-000000000002"))
  (text_box "B" (at 2 0 0) (size 1 1) (uuid "00000000-0000-0000-0000-000000000002"))
)"#;
    let path = temp_schematic("raw_shape_textbox_duplicate_uuids", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let raw_shape_uuids = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Shape(shape) => shape.uuid.clone(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let text_box_uuids = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::TextBox(text_box) => text_box.uuid.clone(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        raw_shape_uuids,
        vec![
            "00000000-0000-0000-0000-000000000001".to_string(),
            "00000000-0000-0000-0000-000000000001".to_string(),
            "00000000-0000-0000-0000-000000000001".to_string(),
        ]
    );
    assert_eq!(
        text_box_uuids,
        vec![
            "00000000-0000-0000-0000-000000000002".to_string(),
            "00000000-0000-0000-0000-000000000002".to_string(),
        ]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parsed_kiid_branches_still_increment_duplicate_uuids() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "00000000-0000-0000-0000-000000000010")
  (paper "A4")
  (junction (at 0 0) (uuid "00000000-0000-0000-0000-000000000011"))
  (junction (at 1 1) (uuid "00000000-0000-0000-0000-000000000011"))
  (polyline (pts (xy 0 0) (xy 1 1) (xy 2 0)) (uuid "00000000-0000-0000-0000-000000000012"))
  (polyline (pts (xy 3 0) (xy 4 1) (xy 5 0)) (uuid "00000000-0000-0000-0000-000000000012"))
)"#;
    let path = temp_schematic("parsed_kiid_duplicate_junction_uuids", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let uuids = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Junction(junction) => junction.uuid.clone(),
            SchItem::Shape(shape) if shape.kind == ShapeKind::Polyline => shape.uuid.clone(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        uuids,
        vec![
            "00000000-0000-0000-0000-000000000011".to_string(),
            "00000000-0000-0000-0000-000000000012".to_string(),
            "00000000-0000-0000-0000-000000000013".to_string(),
            "00000000-0000-0000-0000-000000000014".to_string(),
        ]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parses_text_and_label_semantics() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000019")
  (paper "A4")
  (text "note" (exclude_from_sim yes) (at 1 2 90) (fields_autoplaced) (effects (font (size 1.27 1.27))) (uuid "79000000-0000-0000-0000-000000000001"))
  (global_label "VCC" (shape input) (exclude_from_sim yes) (at 5 6 180) (uuid "79000000-0000-0000-0000-000000000002")
    (iref 9 10)
    (property "Intersheet References" "show")
    (effects (font (size 1.27 1.27))))
  (directive_label "D" (shape dot) (length 3.5) (at 2 3 0) (uuid "79000000-0000-0000-0000-000000000003"))
  (label "LOCAL" (at 7 8 0) (uuid "79000000-0000-0000-0000-000000000004"))
)"#;
    let path = temp_schematic("text_label_semantics", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert!(text.excluded_from_sim);
    assert_eq!(text.fields_autoplaced, FieldAutoplacement::Auto);
    assert!(text.has_effects);
    assert!(text.visible);
    assert_eq!(
        text.effects.as_ref().and_then(|effects| effects.font_size),
        Some([1.27, 1.27])
    );

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    assert!(global.excluded_from_sim);
    assert_eq!(global.fields_autoplaced, FieldAutoplacement::None);
    assert_eq!(global.properties.len(), 1);
    assert!(global.has_effects);
    assert!(global.visible);
    assert_eq!(global.angle, 0.0);
    assert_eq!(global.spin, LabelSpin::Right);
    assert_eq!(
        global.effects.as_ref().map(|effects| effects.h_justify),
        Some(TextHJustify::Center)
    );
    let intersheet_refs = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("default intersheet refs property");
    assert_eq!(intersheet_refs.value, "show");
    assert_eq!(intersheet_refs.at, Some([0.0, 0.0]));
    assert!(intersheet_refs.visible);

    let local = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Local => Some(label),
            _ => None,
        })
        .expect("local label");
    assert_eq!(local.fields_autoplaced, FieldAutoplacement::Auto);

    let directive = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Directive => Some(label),
            _ => None,
        })
        .expect("directive label");
    assert_eq!(directive.pin_length, Some(3.5));

    let _ = fs::remove_file(path);
}

#[test]
fn global_label_without_user_fields_keeps_parser_default_autoplace_state() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000005")
  (paper "A4")
  (global_label "GL" (at 1 2 0) (shape input) (uuid "79000000-0000-0000-0000-000000000005"))
)"#;
    let path = temp_schematic("global_label_default_autoplace", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");

    assert_eq!(global.fields_autoplaced, FieldAutoplacement::None);
    assert_eq!(global.properties.len(), 1);
    assert_eq!(
        global.properties[0].kind,
        PropertyKind::GlobalLabelIntersheetRefs
    );

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_shared_pin_lengths_to_kicad_internal_unit_limit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000001a")
  (paper "A4")
  (directive_label "D" (shape dot) (length 9999999) (at 2 3 0))
  (lib_symbols
    (symbol "Device:R"
      (pin passive line (at 0 0 0) (length 9999999) (name "P") (number "1"))))
)"#;
    let path = temp_schematic("clamped_shared_pin_lengths", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let directive = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Directive => Some(label),
            _ => None,
        })
        .expect("directive label");
    let lib_pin = schematic.screen.lib_symbols[0].units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "pin")
        .expect("lib pin");

    let expected_max = (f64::from(i32::MAX) * 0.7071) / 1e4;

    let directive_length = directive.pin_length.expect("directive length");
    let pin_length = lib_pin.length.expect("pin length");

    assert!(directive_length < 9_999_999.0);
    assert!(pin_length < 9_999_999.0);
    assert!((directive_length - expected_max).abs() < 1e-9);
    assert!((pin_length - expected_max).abs() < 1e-9);

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_internal_unit_geometry_lengths_to_kicad_limit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000001b")
  (paper "A4")
  (junction (at 0 0) (diameter 9999999))
  (circle (center 1 2) (radius 9999999))
  (lib_symbols
    (symbol "Device:R"
      (arc (radius (at 0 0) (length 9999999) (angles 0 90)) (stroke (width 0)))
      (circle (center 0 0) (radius 9999999) (stroke (width 0)))))
)"#;
    let path = temp_schematic("clamped_internal_unit_geometry_lengths", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let expected_max = (f64::from(i32::MAX) * 0.7071) / 1e4;

    let junction = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Junction(junction) => Some(junction),
            _ => None,
        })
        .expect("junction");
    let sch_circle = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Circle => Some(shape),
            _ => None,
        })
        .expect("schematic circle");
    let lib_units = &schematic.screen.lib_symbols[0].units[0].draw_items;
    let lib_arc = lib_units
        .iter()
        .find(|item| item.kind == "arc")
        .expect("lib arc");
    let lib_circle = lib_units
        .iter()
        .find(|item| item.kind == "circle")
        .expect("lib circle");

    let measured = [
        junction.diameter.expect("junction diameter"),
        sch_circle.radius.expect("schematic circle radius"),
        lib_arc.radius.expect("lib arc radius"),
        lib_circle.radius.expect("lib circle radius"),
    ];

    for value in measured {
        assert!(value < 9_999_999.0);
        assert!((value - expected_max).abs() < 1e-9);
    }

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_shared_stroke_width_to_kicad_internal_unit_limit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000001c")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (width 9999999)))
  (text_box "body" (at 0 0 0) (size 1 1) (stroke (width 9999999)))
  (lib_symbols
    (symbol "Device:R"
      (circle (center 0 0) (radius 1) (stroke (width 9999999))))))
"#;
    let path = temp_schematic("clamped_shared_stroke_width", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let expected_max = (f64::from(i32::MAX) * 0.7071) / 1e4;

    let wire = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Wire(line) if line.kind == LineKind::Wire => Some(line),
            _ => None,
        })
        .expect("wire");
    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    let lib_circle = schematic.screen.lib_symbols[0].units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "circle")
        .expect("lib circle");

    let widths: [f64; 3] = [
        wire.stroke
            .as_ref()
            .and_then(|stroke| stroke.width)
            .expect("wire width"),
        text_box
            .stroke
            .as_ref()
            .and_then(|stroke| stroke.width)
            .expect("text box width"),
        lib_circle
            .stroke
            .as_ref()
            .and_then(|stroke| stroke.width)
            .expect("lib circle width"),
    ];

    for width in widths {
        assert!(width < 9_999_999.0);
        assert!((width - expected_max).abs() < 1e-9);
    }

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_shared_xy_coordinates_and_sizes_to_kicad_internal_unit_limit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000001d")
  (paper "A4")
  (text "note" (at 9999999 9999999 90))
  (sheet (at 9999999 9999999) (size 9999999 9999999)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (wire (pts (xy 0 0) (xy 9999999 9999999)))
)"#;
    let path = temp_schematic("clamped_shared_xy", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let expected_max = (f64::from(i32::MAX) * 0.7071) / 1e4;

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    let wire = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Wire(line) if line.kind == LineKind::Wire => Some(line),
            _ => None,
        })
        .expect("wire");

    assert!((text.at[0] - expected_max).abs() < 1e-9);
    assert!((text.at[1] - expected_max).abs() < 1e-9);
    assert!((sheet.at[0] - expected_max).abs() < 1e-9);
    assert!((sheet.at[1] - expected_max).abs() < 1e-9);
    assert!((sheet.size[0] - expected_max).abs() < 1e-9);
    assert!((sheet.size[1] - expected_max).abs() < 1e-9);
    assert!((wire.points[1][0] - expected_max).abs() < 1e-9);
    assert!((wire.points[1][1] - expected_max).abs() < 1e-9);

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_table_dimensions_and_rectangle_corner_radii_to_kicad_limit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000001e")
  (paper "A4")
  (rectangle (start 0 0) (end 1 1) (radius 9999999))
  (table
    (column_count 1)
    (column_widths 9999999)
    (row_heights 9999999)
    (cells (table_cell "A" (at 0 0 0) (size 1 1))))
  (lib_symbols
    (symbol "Device:R"
      (rectangle (start 0 0) (end 1 1) (radius 9999999))))
)"#;
    let path = temp_schematic("clamped_table_and_rectangles", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let expected_max = (f64::from(i32::MAX) * 0.7071) / 1e4;

    let rectangle = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Rectangle => Some(shape),
            _ => None,
        })
        .expect("schematic rectangle");
    let table = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");
    let lib_rectangle = schematic.screen.lib_symbols[0].units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "rectangle")
        .expect("lib rectangle");

    assert!(
        (rectangle.corner_radius.expect("schematic corner radius") - expected_max).abs() < 1e-9
    );
    assert!((table.column_widths[0] - expected_max).abs() < 1e-9);
    assert!((table.row_heights[0] - expected_max).abs() < 1e-9);
    assert!((lib_rectangle.radius.expect("lib corner radius") - expected_max).abs() < 1e-9);

    let _ = fs::remove_file(path);
}

#[test]
fn global_label_starts_with_hidden_intersheet_refs_field() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000001f")
  (global_label "GL")
)"#;
    let path = temp_schematic("global_label_default_intersheet_field", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let intersheet_refs = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs field");

    assert_eq!(global.fields_autoplaced, FieldAutoplacement::None);
    assert_eq!(intersheet_refs.id, Some(6));
    assert_eq!(intersheet_refs.key, "Intersheet References");
    assert_eq!(intersheet_refs.value, "${INTERSHEET_REFS}");
    assert_eq!(intersheet_refs.at, Some([0.0, 0.0]));
    assert!(!intersheet_refs.visible);

    let _ = fs::remove_file(path);
}

#[test]
fn global_label_at_moves_default_intersheet_refs_field_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000020")
  (paper "A4")
  (global_label "GL" (at 10 20 0))
)"#;
    let path = temp_schematic("global_label_at_moves_default_iref_field", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let intersheet_refs = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("intersheet refs field");

    assert_eq!(intersheet_refs.at, Some([10.0, 20.0]));
    assert!(!intersheet_refs.visible);

    let _ = fs::remove_file(path);
}

#[test]
fn label_at_offsets_existing_fields_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000079")
  (paper "A4")
  (label "L"
    (property "User" "V" (at 3 4 0))
    (at 10 20 0))
)"#;
    let path = temp_schematic("label_at_offsets_existing_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let label = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Local => Some(label),
            _ => None,
        })
        .expect("label");
    let property = label
        .properties
        .iter()
        .find(|property| property.key == "User")
        .expect("user field");

    assert_eq!(property.at, Some([13.0, 24.0]));

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_at_moves_default_mandatory_fields_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000007a")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"))
  (symbol
    (lib_id "Device:R")
    (at 10 20 90))
)"#;
    let path = temp_schematic("symbol_at_moves_default_mandatory_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    for kind in [
        PropertyKind::SymbolReference,
        PropertyKind::SymbolValue,
        PropertyKind::SymbolFootprint,
        PropertyKind::SymbolDatasheet,
        PropertyKind::SymbolDescription,
    ] {
        let property = symbol
            .properties
            .iter()
            .find(|property| property.kind == kind)
            .expect("mandatory field");
        assert_eq!(property.at, Some([10.0, 20.0]));
    }

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_at_offsets_existing_fields_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000007b")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"))
  (symbol
    (lib_id "Device:R")
    (property "Reference" "R1" (at 3 4 0))
    (at 10 20 0))
)"#;
    let path = temp_schematic("symbol_at_offsets_existing_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let property = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolReference)
        .expect("reference field");

    assert_eq!(property.at, Some([13.0, 24.0]));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_tokens_in_shared_sch_text_parser() {
    let text_property_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000006")
  (paper "A4")
  (text "hello" (property "X" "Y"))
)"#;
    let text_property_path = temp_schematic("text_unexpected_property", text_property_src);
    let err = parse_schematic_file(Path::new(&text_property_path))
        .expect_err("must reject property on schematic text");
    assert!(err.to_string().contains("unexpected property"));
    let _ = fs::remove_file(text_property_path);

    let local_label_shape_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000007")
  (paper "A4")
  (label "L" (at 0 0 0) (shape input))
)"#;
    let local_label_shape_path =
        temp_schematic("local_label_unexpected_shape", local_label_shape_src);
    let err = parse_schematic_file(Path::new(&local_label_shape_path))
        .expect_err("must reject shape on local label");
    assert!(err.to_string().contains("unexpected shape"));
    let _ = fs::remove_file(local_label_shape_path);

    let bad_global_shape_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000008")
  (paper "A4")
  (global_label "G" (at 0 0 0) (shape (bogus)))
)"#;
    let bad_global_shape_path =
        temp_schematic("global_label_invalid_shape_token", bad_global_shape_src);
    let err = parse_schematic_file(Path::new(&bad_global_shape_path))
        .expect_err("must reject invalid shape token on global label");
    assert!(
        err.to_string().contains(
            "expecting input, output, bidirectional, tri_state, passive, dot, round, diamondor rectangle"
        )
    );
    let _ = fs::remove_file(bad_global_shape_path);

    let quoted_global_shape_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-000000000009")
  (paper "A4")
  (global_label "G" (at 0 0 0) (shape "input"))
)"#;
    let quoted_global_shape_path =
        temp_schematic("quoted_global_label_shape_token", quoted_global_shape_src);
    let err = parse_schematic_file(Path::new(&quoted_global_shape_path))
        .expect_err("must reject quoted shape token on nonlocal label");
    assert!(
        err.to_string().contains(
            "expecting input, output, bidirectional, tri_state, passive, dot, round, diamondor rectangle"
        )
    );
    let _ = fs::remove_file(quoted_global_shape_path);

    let hierarchical_length_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-00000000000a")
  (paper "A4")
  (hierarchical_label "H" (at 0 0 0) (shape input) (length 10))
)"#;
    let hierarchical_length_path = temp_schematic(
        "hierarchical_label_unexpected_length",
        hierarchical_length_src,
    );
    let err = parse_schematic_file(Path::new(&hierarchical_length_path))
        .expect_err("must reject length on hierarchical label");
    assert!(err.to_string().contains("unexpected length"));
    let _ = fs::remove_file(hierarchical_length_path);

    let local_iref_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "78000000-0000-0000-0000-00000000000b")
  (paper "A4")
  (label "L" (at 0 0 0) (iref 1 2))
)"#;
    let local_iref_path = temp_schematic("local_label_iref", local_iref_src);
    let err = parse_schematic_file(Path::new(&local_iref_path))
        .expect_err("must reject iref payload on local label");
    assert!(err.to_string().contains("expecting ("));
    let _ = fs::remove_file(local_iref_path);

    let empty_local_iref_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000007c")
  (paper "A4")
  (label "L" (at 0 0 0) (iref))
)"#;
    let empty_local_iref_path = temp_schematic("local_label_empty_iref", empty_local_iref_src);
    let err = parse_schematic_file(Path::new(&empty_local_iref_path))
        .expect_err("empty legacy iref on local label should fall out to shared parser flow");
    assert!(err.to_string().contains("expecting end of file"));
    let _ = fs::remove_file(empty_local_iref_path);

    let text_iref_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000007d")
  (paper "A4")
  (text "T" (at 0 0 0) (iref 1 2))
)"#;
    let text_iref_path = temp_schematic("plain_text_iref", text_iref_src);
    let err = parse_schematic_file(Path::new(&text_iref_path))
        .expect_err("plain text iref should follow shared late-failure flow");
    assert!(err.to_string().contains("expecting ("));
    let _ = fs::remove_file(text_iref_path);

    let quoted_text_head_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000007e")
  (paper "A4")
  (text "hello" ("at" 1 2 0))
)"#;
    let quoted_text_head_path = temp_schematic("quoted_sch_text_head", quoted_text_head_src);
    let err = parse_schematic_file(Path::new(&quoted_text_head_path))
        .expect_err("must reject quoted shared sch text head token");
    assert!(
        err.to_string()
            .contains("expecting at, shape, iref, uuid or effects")
    );
    let _ = fs::remove_file(quoted_text_head_path);

    let quoted_text_fields_autoplaced_head_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000007f")
  (paper "A4")
  (text "hello" ("fields_autoplaced"))
)"#;
    let quoted_text_fields_autoplaced_head_path = temp_schematic(
        "quoted_sch_text_fields_autoplaced_head",
        quoted_text_fields_autoplaced_head_src,
    );
    let err = parse_schematic_file(Path::new(&quoted_text_fields_autoplaced_head_path))
        .expect_err("must reject quoted fields_autoplaced head in shared sch text parser");
    assert!(
        err.to_string()
            .contains("expecting at, shape, iref, uuid or effects")
    );
    let _ = fs::remove_file(quoted_text_fields_autoplaced_head_path);
}

#[test]
fn non_local_labels_do_not_require_shape() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000096")
  (paper "A4")
  (global_label "VCC" (at 5 6 180) (uuid "78000000-0000-0000-0000-00000000000c"))
  (hierarchical_label "NET_A" (at 7 8 90) (uuid "78000000-0000-0000-0000-00000000000d"))
)"#;
    let path = temp_schematic("non_local_labels_without_shape", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept non-local labels without shape");
    let labels: Vec<_> = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Label(label) => Some(label),
            _ => None,
        })
        .collect();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].shape, LabelShape::Bidirectional);
    assert_eq!(labels[1].shape, LabelShape::Input);
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_label_and_sheet_pin_shape_tokens() {
    let quoted_label_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000001")
  (global_label "VCC" (shape "input"))
)"#;
    let quoted_label_shape_path = temp_schematic("quoted_label_shape_token", quoted_label_shape);
    let err = parse_schematic_file(Path::new(&quoted_label_shape_path))
        .expect_err("must reject quoted label shape token");
    assert!(
        err.to_string().contains(
            "expecting input, output, bidirectional, tri_state, passive, dot, round, diamondor rectangle"
        )
    );

    let quoted_sheet_pin_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000002")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "P1" "input"))
)"#;
    let quoted_sheet_pin_shape_path =
        temp_schematic("quoted_sheet_pin_shape_token", quoted_sheet_pin_shape);
    let err = parse_schematic_file(Path::new(&quoted_sheet_pin_shape_path))
        .expect_err("must reject quoted sheet pin shape token");
    assert!(
        err.to_string()
            .contains("expecting input, output, bidirectional, tri_state, or passive")
    );

    let _ = fs::remove_file(quoted_label_shape_path);
    let _ = fs::remove_file(quoted_sheet_pin_shape_path);
}

#[test]
fn passive_label_shape_maps_to_upstream_unspecified_shape() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000003")
  (global_label "VCC" (shape passive))
)"#;
    let path = temp_schematic("passive_label_shape_unspecified", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse passive label");
    let label = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) => Some(label),
            _ => None,
        })
        .expect("global label");
    assert_eq!(label.shape, LabelShape::Unspecified);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_symbol_mirror_and_lib_pin_type_shape_tokens() {
    let quoted_mirror = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000004")
  (symbol
    (lib_id "Device:R")
    (mirror "x"))
)"#;
    let quoted_mirror_path = temp_schematic("quoted_symbol_mirror", quoted_mirror);
    let err = parse_schematic_file(Path::new(&quoted_mirror_path))
        .expect_err("must reject quoted mirror axis");
    assert!(err.to_string().contains("expecting x or y"));

    let quoted_lib_pin_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000005")
  (lib_symbols
    (symbol "MyLib:U"
      (pin "input" line
        (at 0 0 0)
        (length 2.54)
        (name "PIN")
        (number "1"))))
)"#;
    let quoted_lib_pin_type_path = temp_schematic("quoted_lib_pin_type", quoted_lib_pin_type);
    let err = parse_schematic_file(Path::new(&quoted_lib_pin_type_path))
        .expect_err("must reject quoted lib pin type");
    assert!(err.to_string().contains(
        "expecting input, output, bidirectional, tri_state, passive, unspecified, power_in, power_out, open_collector, open_emitter, free or no_connect"
    ));

    let quoted_lib_pin_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000006")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input "line"
        (at 0 0 0)
        (length 2.54)
        (name "PIN")
        (number "1"))))
)"#;
    let quoted_lib_pin_shape_path = temp_schematic("quoted_lib_pin_shape", quoted_lib_pin_shape);
    let err = parse_schematic_file(Path::new(&quoted_lib_pin_shape_path))
        .expect_err("must reject quoted lib pin shape");
    assert!(err.to_string().contains(
        "expecting line, inverted, clock, inverted_clock, input_low, clock_low, output_low, edge_clock_high, non_logic"
    ));

    let _ = fs::remove_file(quoted_mirror_path);
    let _ = fs::remove_file(quoted_lib_pin_type_path);
    let _ = fs::remove_file(quoted_lib_pin_shape_path);
}

#[test]
fn rejects_quoted_lib_pin_property_and_style_keyword_heads() {
    let quoted_lib_pin_effects = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000007")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input line
        (at 0 0 0)
        (length 2.54)
        (name "PIN" ("effects" (font (size 1 1))))
        (number "1"))))
)"#;
    let quoted_lib_pin_effects_path =
        temp_schematic("quoted_lib_pin_effects", quoted_lib_pin_effects);
    let err = parse_schematic_file(Path::new(&quoted_lib_pin_effects_path))
        .expect_err("must reject quoted lib pin effects head");
    assert!(err.to_string().contains("expecting effects"));

    let quoted_lib_property_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000008")
  (lib_symbols
    (symbol "MyLib:U"
      (property "User" "v" ("at" 1 2 0))))
)"#;
    let quoted_lib_property_head_path =
        temp_schematic("quoted_lib_property_head", quoted_lib_property_head);
    let err = parse_schematic_file(Path::new(&quoted_lib_property_head_path))
        .expect_err("must reject quoted lib property head");
    assert!(
        err.to_string()
            .contains("expecting id, at, hide, show_name, do_not_autoplace, or effects")
    );

    let quoted_stroke_width = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000009")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke ("width" 0.2)))
)"#;
    let quoted_stroke_width_path = temp_schematic("quoted_stroke_width", quoted_stroke_width);
    let err = parse_schematic_file(Path::new(&quoted_stroke_width_path))
        .expect_err("must reject quoted stroke width head");
    assert!(err.to_string().contains("expecting width, type or color"));

    let quoted_fill_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000a")
  (rectangle (start 0 0) (end 1 1) (fill ("color" 1 2 3 0.5)))
)"#;
    let quoted_fill_color_path = temp_schematic("quoted_fill_color", quoted_fill_color);
    let err = parse_schematic_file(Path::new(&quoted_fill_color_path))
        .expect_err("must reject quoted fill color head");
    assert!(err.to_string().contains("expecting type or color"));

    let _ = fs::remove_file(quoted_lib_pin_effects_path);
    let _ = fs::remove_file(quoted_lib_property_head_path);
    let _ = fs::remove_file(quoted_stroke_width_path);
    let _ = fs::remove_file(quoted_fill_color_path);
}

#[test]
fn rejects_quoted_lib_symbols_top_level_symbol_head() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000b")
  (lib_symbols
    ("symbol" "MyLib:U"))
)"#;
    let path = temp_schematic("quoted_lib_symbols_symbol_head", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("quoted lib_symbols head should fail before lib-symbol recovery starts");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_pts_and_embedded_file_keyword_heads() {
    let quoted_polyline_xy = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000c")
  (polyline (pts ("xy" 0 0) (xy 1 1)))
)"#;
    let quoted_polyline_xy_path = temp_schematic("quoted_pts_xy", quoted_polyline_xy);
    let err = parse_schematic_file(Path::new(&quoted_polyline_xy_path))
        .expect_err("must reject quoted pts xy head");
    assert!(err.to_string().contains("expecting xy"));

    let quoted_embedded_file_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000d")
  (embedded_files (file ("name" "A.bin") (data "abc")))
)"#;
    let quoted_embedded_file_name_path =
        temp_schematic("quoted_embedded_file_name", quoted_embedded_file_name);
    let schematic = parse_schematic_file(Path::new(&quoted_embedded_file_name_path))
        .expect("quoted embedded file head should record a warning and continue");
    assert!(schematic.screen.embedded_files.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting checksum, data or name"))
    );

    let _ = fs::remove_file(quoted_polyline_xy_path);
    let _ = fs::remove_file(quoted_embedded_file_name_path);
}

#[test]
fn rejects_quoted_symbol_and_sheet_keyword_heads() {
    let quoted_symbol_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000e")
  (symbol
    (lib_id "Device:R")
    ("at" 1 2 0))
)"#;
    let quoted_symbol_head_path = temp_schematic("quoted_symbol_head", quoted_symbol_head);
    let err = parse_schematic_file(Path::new(&quoted_symbol_head_path))
        .expect_err("must reject quoted top-level symbol head token");
    assert!(err.to_string().contains(
        "expecting lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances"
    ));

    let quoted_symbol_project = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000f")
  (symbol
    (lib_id "Device:R")
    (instances
      ("project" "Demo"))))
"#;
    let quoted_symbol_project_path =
        temp_schematic("quoted_symbol_instance_project", quoted_symbol_project);
    let err = parse_schematic_file(Path::new(&quoted_symbol_project_path))
        .expect_err("must reject quoted symbol instances project head");
    assert!(err.to_string().contains("expecting project"));

    let quoted_sheet_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000010")
  (sheet
    ("at" 0 0)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let quoted_sheet_head_path = temp_schematic("quoted_sheet_head", quoted_sheet_head);
    let err = parse_schematic_file(Path::new(&quoted_sheet_head_path))
        .expect_err("must reject quoted top-level sheet head token");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, background, instances, uuid, property, or pin")
    );

    let quoted_sheet_project = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000011")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      ("project" "Demo"))))
"#;
    let quoted_sheet_project_path =
        temp_schematic("quoted_sheet_instance_project", quoted_sheet_project);
    let err = parse_schematic_file(Path::new(&quoted_sheet_project_path))
        .expect_err("must reject quoted sheet instances project head");
    assert!(err.to_string().contains("expecting project"));

    let quoted_symbol_fields_autoplaced = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000012")
  (symbol
    (lib_id "Device:R")
    ("fields_autoplaced"))
)"#;
    let quoted_symbol_fields_autoplaced_path = temp_schematic(
        "quoted_symbol_fields_autoplaced_head",
        quoted_symbol_fields_autoplaced,
    );
    let err = parse_schematic_file(Path::new(&quoted_symbol_fields_autoplaced_path))
        .expect_err("must reject quoted symbol fields_autoplaced head");
    assert!(err.to_string().contains(
        "expecting lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances"
    ));

    let quoted_sheet_fields_autoplaced = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000013")
  (sheet
    ("fields_autoplaced")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let quoted_sheet_fields_autoplaced_path = temp_schematic(
        "quoted_sheet_fields_autoplaced_head",
        quoted_sheet_fields_autoplaced,
    );
    let err = parse_schematic_file(Path::new(&quoted_sheet_fields_autoplaced_path))
        .expect_err("must reject quoted sheet fields_autoplaced head");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, background, instances, uuid, property, or pin")
    );

    let quoted_sheet_pin_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000014")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "P" input
      ("at" 0 0 0)))
)"#;
    let quoted_sheet_pin_head_path = temp_schematic("quoted_sheet_pin_head", quoted_sheet_pin_head);
    let err = parse_schematic_file(Path::new(&quoted_sheet_pin_head_path))
        .expect_err("must reject quoted sheet pin head token");
    assert!(err.to_string().contains("expecting at, uuid or effects"));

    let quoted_top_sheet_instance = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000015")
  (sheet_instances
    ("path" "/"))
)"#;
    let quoted_top_sheet_instance_path =
        temp_schematic("quoted_top_sheet_instance", quoted_top_sheet_instance);
    let err = parse_schematic_file(Path::new(&quoted_top_sheet_instance_path))
        .expect_err("must reject quoted top-level sheet_instances path head");
    assert!(err.to_string().contains("expecting path"));

    let quoted_top_symbol_instance = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000016")
  (symbol_instances
    (path "/sym"
      ("reference" "R1")))
)"#;
    let quoted_top_symbol_instance_path =
        temp_schematic("quoted_top_symbol_instance", quoted_top_symbol_instance);
    let err = parse_schematic_file(Path::new(&quoted_top_symbol_instance_path))
        .expect_err("must reject quoted top-level symbol_instances child head");
    assert!(
        err.to_string()
            .contains("expecting path, unit, value or footprint")
    );

    let _ = fs::remove_file(quoted_symbol_head_path);
    let _ = fs::remove_file(quoted_symbol_project_path);
    let _ = fs::remove_file(quoted_symbol_fields_autoplaced_path);
    let _ = fs::remove_file(quoted_sheet_head_path);
    let _ = fs::remove_file(quoted_sheet_project_path);
    let _ = fs::remove_file(quoted_sheet_fields_autoplaced_path);
    let _ = fs::remove_file(quoted_sheet_pin_head_path);
    let _ = fs::remove_file(quoted_top_sheet_instance_path);
    let _ = fs::remove_file(quoted_top_symbol_instance_path);
}

#[test]
fn rejects_quoted_top_level_section_head_with_dispatch_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000017")
  ("group" "G" (uuid "60000000-0000-0000-0000-000000000097") (members))
)"#;
    let path = temp_schematic("quoted_top_level_section_head", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject quoted top-level section head");
    assert!(err.to_string().contains(
        "expecting bitmap, bus, bus_alias, bus_entry, class_label, embedded_files, global_label, hierarchical_label, junction, label, line, no_connect, page, paper, rule_area, sheet, symbol, symbol_instances, text, title_block"
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_effects_keyword_tokens() {
    let quoted_justify = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000018")
  (text "note" (effects (justify "left")))
)"#;
    let quoted_justify_path = temp_schematic("quoted_effects_justify", quoted_justify);
    let err = parse_schematic_file(Path::new(&quoted_justify_path))
        .expect_err("must reject quoted justify token");
    assert!(
        err.to_string()
            .contains("expecting left, right, top, bottom, or mirror")
    );

    let quoted_font_bold = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000019")
  (text "note" (effects (font "bold")))
)"#;
    let quoted_font_bold_path = temp_schematic("quoted_effects_font_bold", quoted_font_bold);
    let err = parse_schematic_file(Path::new(&quoted_font_bold_path))
        .expect_err("must reject quoted font keyword");
    assert!(
        err.to_string()
            .contains("expecting face, size, thickness, line_spacing, bold, or italic")
    );

    let quoted_font_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001a")
  (text "note" (effects (font ("color" 1 2 3 0.5))))
)"#;
    let quoted_font_color_path = temp_schematic("quoted_effects_font_color", quoted_font_color);
    let err = parse_schematic_file(Path::new(&quoted_font_color_path))
        .expect_err("must reject quoted font color keyword");
    assert!(
        err.to_string()
            .contains("expecting face, size, thickness, line_spacing, bold, or italic")
    );

    let quoted_hide = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001b")
  (text "note" (effects "hide"))
)"#;
    let quoted_hide_path = temp_schematic("quoted_effects_hide", quoted_hide);
    let err = parse_schematic_file(Path::new(&quoted_hide_path))
        .expect_err("must reject quoted hide token");
    assert!(
        err.to_string()
            .contains("expecting font, justify, hide or href")
    );

    let _ = fs::remove_file(quoted_justify_path);
    let _ = fs::remove_file(quoted_font_bold_path);
    let _ = fs::remove_file(quoted_font_color_path);
    let _ = fs::remove_file(quoted_hide_path);
}

#[test]
fn accepts_bare_effects_heads_when_they_are_last_children() {
    let bare_font_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001c")
  (text "note" (effects (font color 10 20 30 0.5)))
)"#;
    let bare_font_color_path = temp_schematic("bare_effects_font_color", bare_font_color);
    let schematic = parse_schematic_file(Path::new(&bare_font_color_path))
        .expect("must accept bare font color child");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(
        text.effects.as_ref().and_then(|effects| effects.color),
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 0.5])
    );

    let bare_hide_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001d")
  (text "note" (effects hide))
)"#;
    let bare_hide_head_path = temp_schematic("bare_effects_hide_head", bare_hide_head);
    let schematic =
        parse_schematic_file(Path::new(&bare_hide_head_path)).expect("must accept bare hide head");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    let effects = text.effects.as_ref().expect("effects");
    assert!(effects.hidden);
    assert!(text.visible);

    let bare_font_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001e")
  (text "note" (effects font (size 1 1)))
)"#;
    let bare_font_head_path = temp_schematic("bare_effects_font_head", bare_font_head);
    let schematic = parse_schematic_file(Path::new(&bare_font_head_path))
        .expect("must accept bare font head like upstream");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(
        text.effects.as_ref().and_then(|effects| effects.font_size),
        Some([1.0, 1.0])
    );

    let bare_justify_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001f")
  (text "note" (effects justify left top))
)"#;
    let bare_justify_head_path = temp_schematic("bare_effects_justify_head", bare_justify_head);
    let schematic = parse_schematic_file(Path::new(&bare_justify_head_path))
        .expect("must accept bare justify head like upstream");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    let effects = text.effects.as_ref().expect("effects");
    assert_eq!(effects.h_justify, TextHJustify::Left);
    assert_eq!(effects.v_justify, TextVJustify::Top);

    let _ = fs::remove_file(bare_font_color_path);
    let _ = fs::remove_file(bare_hide_head_path);
    let _ = fs::remove_file(bare_font_head_path);
    let _ = fs::remove_file(bare_justify_head_path);

    let bare_href_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000020")
  (text "note" (effects href "https://example.com"))
)"#;
    let bare_href_head_path = temp_schematic("bare_effects_href_head", bare_href_head);
    let schematic = parse_schematic_file(Path::new(&bare_href_head_path))
        .expect("must accept bare href head like upstream");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(
        text.effects
            .as_ref()
            .and_then(|effects| effects.hyperlink.as_deref()),
        Some("https://example.com")
    );
    let _ = fs::remove_file(bare_href_head_path);
}

#[test]
fn bare_effects_font_payload_cannot_have_trailing_siblings() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000021")
  (text "note" (effects font size 1 1 bold))
)"#;
    let path = temp_schematic("bare_effects_font_trailing", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("bare font payload should close immediately like upstream");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(path);
}

#[test]
fn bare_effects_href_head_cannot_have_trailing_siblings() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000022")
  (text "note" (effects href "https://example.com" hide))
)"#;
    let path = temp_schematic("bare_effects_href_trailing", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("bare href head should close immediately like upstream");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(path);
}

#[test]
fn bare_effects_hide_head_allows_siblings_but_not_bare_bool_payloads() {
    let trailing_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000023")
  (text "note" (at 1 2 0) (effects hide (font (size 1 1))))
)"#;
    let trailing_path = temp_schematic("bare_effects_hide_trailing", trailing_src);
    let schematic =
        parse_schematic_file(Path::new(&trailing_path)).expect("must accept bare hide siblings");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    let effects = text.effects.as_ref().expect("effects");
    assert!(effects.hidden);
    assert_eq!(effects.font_size, Some([1.0, 1.0]));
    assert!(text.visible);
    let _ = fs::remove_file(trailing_path);

    for (name, src) in [
        (
            "bare_effects_hide_yes",
            r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000024")
  (text "note" (at 1 2 0) (effects hide yes))
)"#,
        ),
        (
            "bare_effects_hide_no",
            r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000025")
  (text "note" (at 1 2 0) (effects hide no))
)"#,
        ),
    ] {
        let path = temp_schematic(name, src);
        let err = parse_schematic_file(Path::new(&path))
            .expect_err("bare hide bool payload should fail like native KiCad");
        assert!(
            err.to_string()
                .contains("expecting font, justify, hide or href")
        );
        let _ = fs::remove_file(path);
    }
}

#[test]
fn rejects_non_symbol_effects_face_and_href_payloads() {
    let numeric_face = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000026")
  (text "note" (effects (font (face 123))))
)"#;
    let numeric_face_path = temp_schematic("numeric_effects_font_face", numeric_face);
    let err = parse_schematic_file(Path::new(&numeric_face_path))
        .expect_err("must reject numeric font face");
    assert!(err.to_string().contains("expecting symbol"));

    let numeric_href = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000027")
  (text "note" (effects (href 123)))
)"#;
    let numeric_href_path = temp_schematic("numeric_effects_href", numeric_href);
    let err = parse_schematic_file(Path::new(&numeric_href_path))
        .expect_err("must reject numeric hyperlink");
    assert!(err.to_string().contains("expecting symbol"));

    let _ = fs::remove_file(numeric_face_path);
    let _ = fs::remove_file(numeric_href_path);
}

#[test]
fn rejects_quoted_text_box_table_and_image_keyword_heads() {
    let quoted_text_box_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000001")
  (text_box "body" ("size" 3 4))
)"#;
    let quoted_text_box_head_path = temp_schematic("quoted_text_box_head", quoted_text_box_head);
    let err = parse_schematic_file(Path::new(&quoted_text_box_head_path))
        .expect_err("must reject quoted schematic text_box head");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, fill, effects or uuid")
    );

    let quoted_text_box_margins_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000002")
  (text_box "body" ("margins" 1 2 3 4) (size 3 4))
)"#;
    let quoted_text_box_margins_head_path =
        temp_schematic("quoted_text_box_margins_head", quoted_text_box_margins_head);
    let err = parse_schematic_file(Path::new(&quoted_text_box_margins_head_path))
        .expect_err("must reject quoted schematic text_box margins head");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, fill, effects or uuid")
    );

    let quoted_table_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000003")
  (table
    ("column_count" 1)
    (cells (table_cell "c" (at 0 0 0) (size 5 5))))
)"#;
    let quoted_table_head_path = temp_schematic("quoted_table_head", quoted_table_head);
    let err = parse_schematic_file(Path::new(&quoted_table_head_path))
        .expect_err("must reject quoted table head");
    assert!(err.to_string().contains(
        "expecting columns, col_widths, row_heights, border, separators, uuid, header or cells"
    ));

    let quoted_table_cell_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000004")
  (table
    (column_count 1)
    (cells ("table_cell" "c" (at 0 0 0) (size 5 5))))
)"#;
    let quoted_table_cell_head_path =
        temp_schematic("quoted_table_cell_head", quoted_table_cell_head);
    let err = parse_schematic_file(Path::new(&quoted_table_cell_head_path))
        .expect_err("must reject quoted table_cell head");
    assert!(err.to_string().contains("expecting table_cell"));

    let quoted_table_cell_span_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000005")
  (table
    (column_count 1)
    (cells (table_cell "c" ("span" 1 1) (at 0 0 0) (size 5 5))))
)"#;
    let quoted_table_cell_span_head_path =
        temp_schematic("quoted_table_cell_span_head", quoted_table_cell_span_head);
    let err = parse_schematic_file(Path::new(&quoted_table_cell_span_head_path))
        .expect_err("must reject quoted table_cell span head");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, fill, effects, span or uuid")
    );

    let quoted_image_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000006")
  (image ("scale" 2.0) (data "AA=="))
)"#;
    let quoted_image_head_path = temp_schematic("quoted_image_head", quoted_image_head);
    let err = parse_schematic_file(Path::new(&quoted_image_head_path))
        .expect_err("must reject quoted image head");
    assert!(
        err.to_string()
            .contains("expecting at, scale, uuid or data")
    );

    let _ = fs::remove_file(quoted_text_box_head_path);
    let _ = fs::remove_file(quoted_text_box_margins_head_path);
    let _ = fs::remove_file(quoted_table_head_path);
    let _ = fs::remove_file(quoted_table_cell_head_path);
    let _ = fs::remove_file(quoted_table_cell_span_head_path);
    let _ = fs::remove_file(quoted_image_head_path);
}

#[test]
fn rejects_quoted_schematic_shape_keyword_heads() {
    let quoted_polyline_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000007")
  (polyline ("pts" (xy 0 0) (xy 1 1)))
)"#;
    let quoted_polyline_head_path = temp_schematic("quoted_polyline_head", quoted_polyline_head);
    let err = parse_schematic_file(Path::new(&quoted_polyline_head_path))
        .expect_err("must reject quoted polyline head");
    assert!(
        err.to_string()
            .contains("expecting pts, uuid, stroke, or fill")
    );

    let quoted_arc_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000008")
  (arc ("start" 0 0) (mid 1 1) (end 2 0))
)"#;
    let quoted_arc_head_path = temp_schematic("quoted_arc_head", quoted_arc_head);
    let err = parse_schematic_file(Path::new(&quoted_arc_head_path))
        .expect_err("must reject quoted arc head");
    assert!(
        err.to_string()
            .contains("expecting start, mid, end, stroke, fill or uuid")
    );

    let quoted_circle_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000009")
  (circle ("center" 0 0) (radius 5))
)"#;
    let quoted_circle_head_path = temp_schematic("quoted_circle_head", quoted_circle_head);
    let err = parse_schematic_file(Path::new(&quoted_circle_head_path))
        .expect_err("must reject quoted circle head");
    assert!(
        err.to_string()
            .contains("expecting center, radius, stroke, fill or uuid")
    );

    let quoted_rectangle_radius_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000a")
  (rectangle (start 0 0) (end 1 1) ("radius" 0.5))
)"#;
    let quoted_rectangle_radius_head_path =
        temp_schematic("quoted_rectangle_radius_head", quoted_rectangle_radius_head);
    let err = parse_schematic_file(Path::new(&quoted_rectangle_radius_head_path))
        .expect_err("must reject quoted rectangle radius head");
    assert!(
        err.to_string()
            .contains("expecting start, end, stroke, fill or uuid")
    );

    let quoted_rule_area_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000b")
  (rule_area ("polyline" (pts (xy 0 0) (xy 1 0) (xy 1 1))))
)"#;
    let quoted_rule_area_head_path = temp_schematic("quoted_rule_area_head", quoted_rule_area_head);
    let err = parse_schematic_file(Path::new(&quoted_rule_area_head_path))
        .expect_err("must reject quoted rule_area head");
    assert!(
        err.to_string()
            .contains("expecting exclude_from_sim, on_board, in_bom, dnp, or polyline")
    );

    let _ = fs::remove_file(quoted_polyline_head_path);
    let _ = fs::remove_file(quoted_arc_head_path);
    let _ = fs::remove_file(quoted_circle_head_path);
    let _ = fs::remove_file(quoted_rectangle_radius_head_path);
    let _ = fs::remove_file(quoted_rule_area_head_path);
}

#[test]
fn rejects_quoted_junction_no_connect_and_bus_entry_heads() {
    let quoted_junction_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000c")
  (junction ("at" 1 2))
)"#;
    let quoted_junction_head_path = temp_schematic("quoted_junction_head", quoted_junction_head);
    let err = parse_schematic_file(Path::new(&quoted_junction_head_path))
        .expect_err("must reject quoted junction head");
    assert!(
        err.to_string()
            .contains("expecting at, diameter, color or uuid")
    );

    let quoted_no_connect_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000d")
  (no_connect ("at" 1 2))
)"#;
    let quoted_no_connect_head_path =
        temp_schematic("quoted_no_connect_head", quoted_no_connect_head);
    let err = parse_schematic_file(Path::new(&quoted_no_connect_head_path))
        .expect_err("must reject quoted no_connect head");
    assert!(err.to_string().contains("expecting at or uuid"));

    let quoted_bus_entry_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000e")
  (bus_entry ("at" 1 2) (size 3 4))
)"#;
    let quoted_bus_entry_head_path = temp_schematic("quoted_bus_entry_head", quoted_bus_entry_head);
    let err = parse_schematic_file(Path::new(&quoted_bus_entry_head_path))
        .expect_err("must reject quoted bus_entry head");
    assert!(
        err.to_string()
            .contains("expecting at, size, uuid or stroke")
    );

    let _ = fs::remove_file(quoted_junction_head_path);
    let _ = fs::remove_file(quoted_no_connect_head_path);
    let _ = fs::remove_file(quoted_bus_entry_head_path);
}

#[test]
fn rejects_quoted_lib_power_and_stroke_fill_type_tokens() {
    let quoted_power_scope = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000000f")
  (lib_symbols
    (symbol "MyLib:U"
      (power "local")))
)"#;
    let quoted_power_scope_path = temp_schematic("quoted_power_scope", quoted_power_scope);
    let err = parse_schematic_file(Path::new(&quoted_power_scope_path))
        .expect_err("must reject quoted lib power scope");
    assert!(err.to_string().contains("expecting global or local"));

    let quoted_stroke_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000010")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (type "dash")))
)"#;
    let quoted_stroke_type_path = temp_schematic("quoted_stroke_type", quoted_stroke_type);
    let err = parse_schematic_file(Path::new(&quoted_stroke_type_path))
        .expect_err("must reject quoted stroke type");
    assert!(
        err.to_string()
            .contains("expecting default, dash, dot, dash_dot, dash_dot_dot, or solid")
    );

    let quoted_fill_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000011")
  (polyline (pts (xy 0 0) (xy 1 0) (xy 1 1)) (fill (type "color")))
)"#;
    let quoted_fill_type_path = temp_schematic("quoted_fill_type", quoted_fill_type);
    let err = parse_schematic_file(Path::new(&quoted_fill_type_path))
        .expect_err("must reject quoted fill type");
    assert!(err.to_string().contains(
        "expecting none, outline, hatch, reverse_hatch, cross_hatch, color or background"
    ));

    let _ = fs::remove_file(quoted_power_scope_path);
    let _ = fs::remove_file(quoted_stroke_type_path);
    let _ = fs::remove_file(quoted_fill_type_path);
}

#[test]
fn rejects_quoted_bare_hide_in_lib_pin_names_and_numbers() {
    let quoted_pin_names_hide = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000012")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_names "hide")))
)"#;
    let quoted_pin_names_hide_path = temp_schematic("quoted_pin_names_hide", quoted_pin_names_hide);
    let err = parse_schematic_file(Path::new(&quoted_pin_names_hide_path))
        .expect_err("must reject quoted pin_names hide");
    assert!(err.to_string().contains("expecting ("));

    let quoted_pin_numbers_hide = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000013")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_numbers "hide")))
)"#;
    let quoted_pin_numbers_hide_path =
        temp_schematic("quoted_pin_numbers_hide", quoted_pin_numbers_hide);
    let err = parse_schematic_file(Path::new(&quoted_pin_numbers_hide_path))
        .expect_err("must reject quoted pin_numbers hide");
    assert!(err.to_string().contains("expecting ("));

    let _ = fs::remove_file(quoted_pin_names_hide_path);
    let _ = fs::remove_file(quoted_pin_numbers_hide_path);
}

#[test]
fn quoted_demorgan_in_body_styles_is_not_the_keyword_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000014")
  (lib_symbols
    (symbol "MyLib:U"
      (body_styles "demorgan" "ALT")))
)"#;
    let path = temp_schematic("quoted_demorgan_body_styles", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert_eq!(schematic.screen.lib_symbols.len(), 1);
    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert!(!lib_symbol.has_demorgan);
    assert_eq!(
        lib_symbol.body_style_names,
        vec!["demorgan".to_string(), "ALT".to_string()]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_pin_names_and_pin_numbers_list_heads() {
    let quoted_pin_names_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000015")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_names ("hide" yes))))
)"#;
    let quoted_pin_names_head_path = temp_schematic("quoted_pin_names_head", quoted_pin_names_head);
    let err = parse_schematic_file(Path::new(&quoted_pin_names_head_path))
        .expect_err("must reject quoted pin_names list head");
    assert!(err.to_string().contains("expecting offset or hide"));

    let quoted_pin_numbers_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000016")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_numbers ("hide" yes))))
)"#;
    let quoted_pin_numbers_head_path =
        temp_schematic("quoted_pin_numbers_head", quoted_pin_numbers_head);
    let err = parse_schematic_file(Path::new(&quoted_pin_numbers_head_path))
        .expect_err("must reject quoted pin_numbers list head");
    assert!(err.to_string().contains("expecting hide"));

    let quoted_pin_names_offset = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000017")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_names ("offset" 0.5))))
)"#;
    let quoted_pin_names_offset_path =
        temp_schematic("quoted_pin_names_offset", quoted_pin_names_offset);
    let err = parse_schematic_file(Path::new(&quoted_pin_names_offset_path))
        .expect_err("must reject quoted pin_names offset head");
    assert!(err.to_string().contains("expecting offset or hide"));

    let _ = fs::remove_file(quoted_pin_names_head_path);
    let _ = fs::remove_file(quoted_pin_numbers_head_path);
    let _ = fs::remove_file(quoted_pin_names_offset_path);
}

#[test]
fn rejects_quoted_private_locked_and_bare_lib_pin_hide_keywords() {
    let quoted_group_locked = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000018")
  (group "locked" (uuid "60000000-0000-0000-0000-000000000098") (members))
)"#;
    let quoted_group_locked_path = temp_schematic("quoted_group_locked", quoted_group_locked);
    let schematic = parse_schematic_file(Path::new(&quoted_group_locked_path)).expect("must parse");
    assert!(schematic
        .screen
        .items
        .iter()
        .any(|item| matches!(item, SchItem::Group(Group { name, uuid, .. }) if name.as_deref() == Some("locked") && uuid.as_deref() == Some("60000000-0000-0000-0000-000000000098"))));

    let quoted_property_private = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000019")
  (symbol
    (lib_id "Device:R")
    (property "private" "x"))
)"#;
    let quoted_property_private_path =
        temp_schematic("quoted_property_private", quoted_property_private);
    let schematic =
        parse_schematic_file(Path::new(&quoted_property_private_path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let property = symbol
        .properties
        .iter()
        .find(|property| property.key == "private")
        .expect("property");
    assert_eq!(property.value, "x");
    assert!(!property.is_private);

    let quoted_lib_text_private = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001a")
  (lib_symbols
    (symbol "MyLib:U"
      (symbol "U_1_1"
        (text "private" (at 0 0 0) (effects (font (size 1 1)))))))
)"#;
    let quoted_lib_text_private_path =
        temp_schematic("quoted_lib_text_private", quoted_lib_text_private);
    let schematic =
        parse_schematic_file(Path::new(&quoted_lib_text_private_path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];
    let text = lib_symbol
        .units
        .iter()
        .flat_map(|unit| unit.draw_items.iter())
        .find(|item| item.kind == "text")
        .expect("text draw item");
    assert_eq!(text.text.as_deref(), Some("private"));
    assert!(!text.is_private);

    let quoted_lib_pin_hide = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001b")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input line "hide" (at 0 0 0) (length 2.54) (name "PIN") (number "1"))))
)"#;
    let quoted_lib_pin_hide_path = temp_schematic("quoted_lib_pin_hide", quoted_lib_pin_hide);
    let err = parse_schematic_file(Path::new(&quoted_lib_pin_hide_path))
        .expect_err("must reject quoted bare lib pin hide");
    assert!(err.to_string().contains("expecting ("));

    let _ = fs::remove_file(quoted_group_locked_path);
    let _ = fs::remove_file(quoted_property_private_path);
    let _ = fs::remove_file(quoted_lib_text_private_path);
    let _ = fs::remove_file(quoted_lib_pin_hide_path);
}

#[test]
fn rejects_bare_nested_lib_pin_hide_like_upstream() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001c")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input line
        (at 0 0 0)
        (hide)
        (length 2.54)
        (name "PIN")
        (number "1"))))
)"#;
    let path = temp_schematic("bare_lib_pin_hide", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bare nested hide");
    assert!(err.to_string().contains("expecting yes or no"));
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_unquoted_jumper_pin_group_names() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001d")
  (lib_symbols
    (symbol "MyLib:U"
      (jumper_pin_groups
        (A B))))
)"#;
    let path = temp_schematic("unquoted_jumper_pin_groups", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept unquoted jumper pin groups");
    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.lib_id == "MyLib:U")
        .expect("lib symbol");
    assert_eq!(lib_symbol.jumper_pin_groups.len(), 1);
    assert_eq!(
        lib_symbol.jumper_pin_groups[0],
        ["A".to_string(), "B".to_string()].into_iter().collect()
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unquoted_stroke_keywords_in_jumper_pin_groups() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001e")
  (lib_symbols
    (symbol "MyLib:U"
      (jumper_pin_groups
        (default B))))
)"#;
    let path = temp_schematic("keyword_jumper_pin_groups", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject reserved stroke keywords in jumper pin groups");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_lib_draw_item_list_heads() {
    let quoted_lib_rectangle_start = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000001f")
  (lib_symbols
    (symbol "MyLib:U"
      (rectangle ("start" 0 0) (end 1 1))))
)"#;
    let quoted_lib_rectangle_start_path =
        temp_schematic("quoted_lib_rectangle_start", quoted_lib_rectangle_start);
    let err = parse_schematic_file(Path::new(&quoted_lib_rectangle_start_path))
        .expect_err("must reject bad lib rectangle");
    assert!(
        err.to_string()
            .contains("expecting start, end, stroke, or fill")
    );

    let quoted_lib_rectangle_radius = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000020")
  (lib_symbols
    (symbol "MyLib:U"
      (rectangle (start 0 0) (end 1 1) ("radius" 0.5))))
)"#;
    let quoted_lib_rectangle_radius_path =
        temp_schematic("quoted_lib_rectangle_radius", quoted_lib_rectangle_radius);
    let err = parse_schematic_file(Path::new(&quoted_lib_rectangle_radius_path))
        .expect_err("must reject bad lib rectangle radius");
    assert!(
        err.to_string()
            .contains("expecting start, end, stroke, or fill")
    );

    let quoted_lib_text_at = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000021")
  (lib_symbols
    (symbol "MyLib:U"
      (text "TXT" ("at" 0 0 90))))
)"#;
    let quoted_lib_text_at_path = temp_schematic("quoted_lib_text_at", quoted_lib_text_at);
    let err = parse_schematic_file(Path::new(&quoted_lib_text_at_path))
        .expect_err("must reject bad lib text");
    assert!(err.to_string().contains("expecting at or effects"));

    let quoted_lib_text_box_effects = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000022")
  (lib_symbols
    (symbol "MyLib:U"
      (text_box "TXT" ("effects" (font (size 1 1))) (size 1 1))))
)"#;
    let quoted_lib_text_box_effects_path =
        temp_schematic("quoted_lib_textbox_effects", quoted_lib_text_box_effects);
    let err = parse_schematic_file(Path::new(&quoted_lib_text_box_effects_path))
        .expect_err("must reject bad lib text_box");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, fill or effects")
    );

    let _ = fs::remove_file(quoted_lib_rectangle_start_path);
    let _ = fs::remove_file(quoted_lib_rectangle_radius_path);
    let _ = fs::remove_file(quoted_lib_text_at_path);
    let _ = fs::remove_file(quoted_lib_text_box_effects_path);
}

#[test]
fn labels_do_not_require_at() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000031")
  (paper "A4")
  (label "LOCAL")
  (global_label "GLOBAL" (shape input))
)"#;
    let path = temp_schematic("labels_without_at", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must accept labels without at");
    let local = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Local => Some(label),
            _ => None,
        })
        .expect("local label");
    assert_eq!(local.at, [0.0, 0.0]);
    assert_eq!(local.angle, 0.0);

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    assert_eq!(global.at, [0.0, 0.0]);
    assert_eq!(global.angle, 0.0);
    let _ = fs::remove_file(path);
}

#[test]
fn parses_shared_effects_payload_and_text_hide_override() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000023")
  (paper "A4")
  (text "note" (at 1 2 180)
    (effects
      (font (face "KiCad Font") (size 1.5 2.5) (thickness 0.2) bold italic (color 10 20 30 0.5) (line_spacing 1.2))
      (justify left top mirror)
      (href "https://example.com")
      (hide)))
)"#;
    let path = temp_schematic("effects_payload", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");

    assert!(text.visible);
    let effects = text.effects.as_ref().expect("effects");
    assert_eq!(effects.font_face.as_deref(), Some("KiCad Font"));
    assert_eq!(effects.font_size, Some([2.5, 1.5]));
    assert_eq!(effects.thickness, Some(0.2));
    assert!(effects.bold);
    assert!(effects.italic);
    assert_eq!(
        effects.color,
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 0.5])
    );
    assert_eq!(effects.line_spacing, Some(1.2));
    assert_eq!(effects.h_justify, TextHJustify::Left);
    assert_eq!(effects.v_justify, TextVJustify::Top);
    assert_eq!(effects.hyperlink.as_deref(), Some("https://example.com"));
    assert!(effects.hidden);

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_minimum_effects_font_size_for_schematic_text_family() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000024")
  (paper "A4")
  (text "note" (effects (font (size 0 999))))
)"#;
    let path = temp_schematic("clamped_effects_text_size", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");

    assert_eq!(
        text.effects.as_ref().and_then(|effects| effects.font_size),
        Some([250.0, 0.001])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn library_text_clamps_but_pin_name_and_number_effects_do_not() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000025")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (text "TXT" (at 0 0 0) (effects (font (size 0 999))))
        (pin input line
          (name "N" (effects (font (size 0 999))))
          (number "1" (effects (font (size 0 999))))))))
)"#;
    let path = temp_schematic("lib_unclamped_effects_text_size", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let lib_unit = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.lib_id == "Device:R")
        .expect("lib symbol")
        .units
        .iter()
        .find(|unit| unit.unit_number == 1 && unit.body_style == 1)
        .expect("lib unit");
    let lib_text = lib_unit
        .draw_items
        .iter()
        .find(|item| item.kind == "text")
        .expect("lib text");
    let lib_pin = lib_unit
        .draw_items
        .iter()
        .find(|item| item.kind == "pin")
        .expect("lib pin");

    assert_eq!(
        lib_text
            .effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([250.0, 0.001])
    );
    assert_eq!(
        lib_pin
            .name_effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([0.0, 0.0])
    );
    assert_eq!(
        lib_pin
            .number_effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([0.0, 0.0])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_internal_unit_effects_and_pin_name_offset() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000026")
  (paper "A4")
  (text "note" (effects (font (size 9999999 9999999) (thickness 9999999))))
  (lib_symbols
    (symbol "Device:R"
      (pin_names (offset 9999999))
      (text "TXT" (at 1 2 0) (effects (font (thickness 9999999)))))))
"#;
    let path = temp_schematic("clamped_effects_internal_units", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let expected_max = (f64::from(i32::MAX) * 0.7071) / 1e4;

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    let schematic_effects = text.effects.as_ref().expect("text effects");
    assert_eq!(schematic_effects.font_size, Some([250.0, 250.0]));
    assert!((schematic_effects.thickness.expect("thickness") - expected_max).abs() < 1e-9);

    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert!((lib_symbol.pin_name_offset.expect("pin name offset") - expected_max).abs() < 1e-9);
    let lib_text = lib_symbol.units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "text")
        .expect("lib text");
    assert!(
        (lib_text
            .effects
            .as_ref()
            .and_then(|effects| effects.thickness)
            .expect("lib text thickness")
            - expected_max)
            .abs()
            < 1e-9
    );

    let _ = fs::remove_file(path);
}

#[test]
fn shared_text_family_forces_visible_after_effects_hide() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000027")
  (paper "A4")
  (global_label "GL" (shape input) (at 1 2 180)
    (effects
      (font (size 1.5 2.5))
      (hide)))
)"#;
    let path = temp_schematic("label_effects_hide", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let label = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");

    assert!(label.visible);
    assert!(label.effects.as_ref().expect("effects").hidden);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_effects_hyperlink() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000028")
  (paper "A4")
  (text "note" (at 1 2 0) (effects (href "not-a-url")))
)"#;
    let path = temp_schematic("bad_effects_href", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject invalid hyperlink");
    assert!(
        err.to_string()
            .contains("Invalid hyperlink url 'not-a-url'")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_digit_started_effects_hyperlink_scheme() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000029")
  (text "note" (at 1 2 0) (effects (href "1ttp://example.com")))
)"#;
    let path = temp_schematic("bad_effects_digit_scheme_href", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject digit-started hyperlink scheme");
    assert!(
        err.to_string()
            .contains("Invalid hyperlink url '1ttp://example.com'")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_spaces_in_effects_hyperlink_like_native_kicad() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000002a")
  (text "note" (at 1 2 0) (effects (href "https://example.com bad")))
)"#;
    let path = temp_schematic("bad_effects_space_href", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("native KiCad accepts spaces in hyperlink");
    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(
        text.effects
            .as_ref()
            .and_then(|effects| effects.hyperlink.as_deref()),
        Some("https://example.com bad")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn converts_legacy_overbar_text_and_labels() {
    let src = r#"(kicad_sch
  (version 20210605)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000002b")
  (paper "A4")
  (text "~RST~" (at 1 2 180) (effects (font (size 1 1))))
  (global_label "~BUS~" (shape input) (at 5 6 180) (effects (font (size 1 1))))
)"#;
    let path = temp_schematic("legacy_overbar_text", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("text");
    assert_eq!(text.text, "~{RST}");

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    assert_eq!(global.text, "~{BUS}");

    let _ = fs::remove_file(path);
}

#[test]
fn converts_legacy_overbar_value_fields_when_effects_are_parsed() {
    let src = r#"(kicad_sch
  (version 20210605)
  (generator "eeschema")
  (uuid "30000000-0000-0000-0000-000000000002")
  (lib_symbols
    (symbol "Device:R"
      (property "Value" "~LIB~" (id 1) (at 0 0 0) (effects (font (size 1 1))))))
  (symbol
    (lib_id "Device:R")
    (property "Value" "~SCH~" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (uuid "30000000-0000-0000-0000-000000000003"))
)"#;
    let path = temp_schematic("legacy_overbar_value_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should parse");

    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.lib_id == "Device:R")
        .expect("lib symbol");
    let lib_value = lib_symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .expect("lib value property");
    assert_eq!(lib_value.value, "~{LIB}");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let sch_value = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .expect("schematic value property");
    assert_eq!(sch_value.value, "~{SCH}");

    let _ = fs::remove_file(path);
}

#[test]
fn converts_legacy_overbar_library_text_when_effects_are_parsed() {
    let src = r#"(kicad_sch
  (version 20210605)
  (generator "eeschema")
  (uuid "30000000-0000-0000-0000-000000000004")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (text "~LIBTXT~" (at 0 0 0) (effects (font (size 1 1)))))))
)"#;
    let path = temp_schematic("legacy_overbar_lib_text_effects", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should parse");

    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.lib_id == "Device:R")
        .expect("lib symbol");
    let lib_text = lib_symbol
        .units
        .iter()
        .flat_map(|unit| unit.draw_items.iter())
        .find(|item| item.kind == "text")
        .expect("lib text");
    assert_eq!(lib_text.text.as_deref(), Some("~{LIBTXT}"));

    let _ = fs::remove_file(path);
}

#[test]
fn keeps_legacy_overbar_value_raw_without_effects_path() {
    let src = r#"(kicad_sch
  (version 20210605)
  (generator "eeschema")
  (uuid "30000000-0000-0000-0000-000000000005")
  (lib_symbols
    (symbol "Device:R"
      (property "Value" "~LIBRAW~")))
  (symbol
    (lib_id "Device:R")
    (property "Value" "~SCHRAW~")
    (uuid "30000000-0000-0000-0000-000000000006"))
)"#;
    let path = temp_schematic("legacy_overbar_no_effects_path", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should parse");

    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.lib_id == "Device:R")
        .expect("lib symbol");
    let lib_value = lib_symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .expect("lib value property");
    assert_eq!(lib_value.value, "~LIBRAW~");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let sch_value = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .expect("schematic value property");
    assert_eq!(sch_value.value, "~SCHRAW~");

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_legacy_class_label_alias_like_upstream_dispatch() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000032")
  (paper "A4")
  (class_label "NETCLASS" (shape rectangle) (at 1 2 0) (uuid "60000000-0000-0000-0000-000000000033"))
)"#;
    let path = temp_schematic("class_label_alias", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject class_label from top-level dispatch");
    assert!(err.to_string().contains(
        "expecting bitmap, bus, bus_alias, bus_entry, class_label, embedded_files, global_label, hierarchical_label, junction, label, line, no_connect, page, paper, rule_area, sheet, symbol, symbol_instances, text, title_block"
    ));
    assert!(!err.to_string().contains("directive_label"));
    let _ = fs::remove_file(path);
}

#[test]
fn allows_properties_on_local_and_hierarchical_labels() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000034")
  (paper "A4")
  (label "LOCAL" (at 1 2 0) (property "User" "L"))
  (hierarchical_label "H" (shape input) (at 3 4 0) (property "User" "H"))
)"#;
    let path = temp_schematic("label_properties", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse label properties");
    let labels = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Label(label) => Some(label),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].properties.len(), 1);
    assert_eq!(labels[1].properties.len(), 1);
    let _ = fs::remove_file(path);
}

#[test]
fn parses_upstream_bus_alias_and_legacy_overbar_notation() {
    let src = r#"(kicad_sch
  (version 20210620)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000002c")
  (paper "A4")
  (bus_alias "~BUS~" (members "~A0~" "~A1~"))
)"#;
    let path = temp_schematic("legacy_bus_alias", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.screen.bus_aliases.len(), 1);
    assert_eq!(schematic.screen.bus_aliases[0].name, "~{BUS}");
    assert_eq!(
        schematic.screen.bus_aliases[0].members,
        vec!["~{A0}", "~{A1}"]
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_unquoted_bus_alias_members_like_upstream() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000002d")
  (bus_alias "ADDR" (members A0 A1))
)"#;
    let path = temp_schematic("unquoted_bus_alias_members", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must accept unquoted members");
    assert_eq!(schematic.screen.bus_aliases.len(), 1);
    assert_eq!(schematic.screen.bus_aliases[0].members, vec!["A0", "A1"]);
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_bus_alias_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000002e")
  (bus_alias (bogus) (members "A0" "A1"))
)"#;
    let path = temp_schematic("bad_bus_alias_name", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad bus alias name");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_bus_alias_member_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000002f")
  (bus_alias "ADDR" (members (bogus)))
)"#;
    let path = temp_schematic("bad_bus_alias_member", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad bus alias member");
    assert!(err.to_string().contains("expecting quoted string"));
    let _ = fs::remove_file(path);
}

#[test]
fn allows_empty_bus_alias_members_like_upstream() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000030")
  (bus_alias "ADDR" (members))
)"#;
    let path = temp_schematic("empty_bus_alias_members", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must accept empty bus alias");
    assert_eq!(schematic.screen.bus_aliases.len(), 1);
    assert_eq!(schematic.screen.bus_aliases[0].name, "ADDR");
    assert!(schematic.screen.bus_aliases[0].members.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_table_without_cells() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000035")
  (paper "A4")
  (table (column_count 1))
)"#;
    let path = temp_schematic("table_no_cells", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject empty table");
    assert!(err.to_string().contains("Invalid table: no cells defined"));
    let _ = fs::remove_file(path);
}

#[test]
fn line_and_polyline_keep_upstream_error_text() {
    let bad_wire_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000036")
  (paper "A4")
  (wire (bogus yes))
)"#;
    let bad_wire_path = temp_schematic("bad_wire_child", bad_wire_src);
    let err =
        parse_schematic_file(Path::new(&bad_wire_path)).expect_err("must reject bad wire child");
    assert!(err.to_string().contains("expecting at, uuid or stroke"));
    let _ = fs::remove_file(bad_wire_path);

    let short_wire_pts_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000037")
  (paper "A4")
  (wire (pts (xy 0 0)))
)"#;
    let short_wire_pts_path = temp_schematic("short_wire_pts", short_wire_pts_src);
    let err = parse_schematic_file(Path::new(&short_wire_pts_path))
        .expect_err("must reject one-point wire pts");
    assert!(err.to_string().contains("expecting ("));
    let _ = fs::remove_file(short_wire_pts_path);

    let long_wire_pts_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000038")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1) (xy 2 2)))
)"#;
    let long_wire_pts_path = temp_schematic("long_wire_pts", long_wire_pts_src);
    let err = parse_schematic_file(Path::new(&long_wire_pts_path))
        .expect_err("must reject three-point wire pts");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(long_wire_pts_path);

    let short_polyline_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000039")
  (paper "A4")
  (polyline (pts (xy 0 0)))
)"#;
    let short_polyline_path = temp_schematic("short_polyline", short_polyline_src);
    let err = parse_schematic_file(Path::new(&short_polyline_path))
        .expect_err("must reject short schematic polyline");
    assert!(
        err.to_string()
            .contains("Schematic polyline has too few points")
    );
    let _ = fs::remove_file(short_polyline_path);

    let bad_polyline_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000003a")
  (paper "A4")
  (polyline (bogus yes))
)"#;
    let bad_polyline_path = temp_schematic("bad_polyline_child", bad_polyline_src);
    let err = parse_schematic_file(Path::new(&bad_polyline_path))
        .expect_err("must reject bad polyline child");
    assert!(
        err.to_string()
            .contains("expecting pts, uuid, stroke, or fill")
    );
    let _ = fs::remove_file(bad_polyline_path);
}

#[test]
fn rejects_invalid_image_data() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000003b")
  (paper "A4")
  (image (at 1 2) (scale 1.0) (data "%%%") (uuid "60000000-0000-0000-0000-00000000003c"))
)"#;
    let path = temp_schematic("bad_image_data", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject invalid image data");
    assert!(err.to_string().contains("Failed to read image data."));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_image_data_token() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000003d")
  (paper "A4")
  (image (at 1 2) (scale 1.0) (data (bogus)) (uuid "60000000-0000-0000-0000-00000000003e"))
)"#;
    let path = temp_schematic("bad_image_data_token", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject invalid image data token");
    assert!(err.to_string().contains("expecting base64 image data"));
    let _ = fs::remove_file(path);
}

#[test]
fn image_does_not_require_at() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000003f")
  (paper "A4")
  (image
    (scale 1.0)
    (data "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAAAAgABSK+kcQAAAABJRU5ErkJggg==")
    (uuid "60000000-0000-0000-0000-000000000040"))
)"#;
    let path = temp_schematic("image_without_at", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must accept image without at");
    let image = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Image(image) => Some(image),
            _ => None,
        })
        .expect("image");
    assert_eq!(image.at, [0.0, 0.0]);
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_decoded_non_image_payload() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000041")
  (paper "A4")
  (image (at 1 2) (scale 1.0) (data "QUJD") (uuid "60000000-0000-0000-0000-000000000042"))
)"#;
    let path = temp_schematic("bad_decoded_image_data", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject decoded non-image data");
    assert!(err.to_string().contains("Failed to read image data."));
    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_for_invalid_top_level_embedded_files() {
    let src = r#"(kicad_sch
  (version 20240706)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000043")
  (paper "A4")
  (embedded_files (file (name "A.bin") (bogus "x")))
)"#;
    let path = temp_schematic("bad_embedded_file_child", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must keep loading");
    assert!(schematic.screen.embedded_files.is_empty());
    assert_eq!(schematic.screen.parse_warnings.len(), 1);
    assert!(schematic.screen.parse_warnings[0].contains("expecting checksum, data or name"));
    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_for_invalid_embedded_files_file_head() {
    let src = r#"(kicad_sch
  (version 20240706)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000044")
  (paper "A4")
  (embedded_files ("file" "A.bin" "aaa"))
)"#;
    let path = temp_schematic("bad_embedded_file_head", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must keep loading");
    assert!(schematic.screen.embedded_files.is_empty());
    assert_eq!(schematic.screen.parse_warnings.len(), 1);
    assert!(schematic.screen.parse_warnings[0].contains("expecting file"));
    let _ = fs::remove_file(path);
}

#[test]
fn repeated_embedded_and_lib_symbol_sections_follow_upstream_accumulation_rules() {
    let src = r#"(kicad_sch
  (version 20240706)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000045")
  (paper "A4")
  (embedded_fonts no)
  (embedded_fonts yes)
  (embedded_files (file (name "A.bin") (data |aaa|)))
  (embedded_files (file (name "B.bin") (data |bbb|)))
  (lib_symbols
    (symbol "First:R"))
  (lib_symbols
    (symbol "Second:R"))
)"#;
    let path = temp_schematic("repeated_embedded_and_lib_symbols", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.screen.embedded_fonts, Some(true));
    assert_eq!(schematic.screen.embedded_files.len(), 2);
    assert_eq!(
        schematic
            .screen
            .embedded_files
            .iter()
            .map(|file| file.name.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("A.bin"), Some("B.bin")]
    );
    assert_eq!(schematic.screen.lib_symbols.len(), 2);
    assert_eq!(schematic.screen.lib_symbols[0].lib_id, "First:R");
    assert_eq!(schematic.screen.lib_symbols[0].name, "R");
    assert_eq!(schematic.screen.lib_symbols[1].lib_id, "Second:R");
    assert_eq!(schematic.screen.lib_symbols[1].name, "R");
    let _ = fs::remove_file(path);
}

#[test]
fn parses_embedded_file_checksum_type_and_bar_data() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000046")
  (paper "A4")
  (embedded_files
    (file
      (name "A.bin")
      (checksum deadbeef)
      (type font)
      (data |abc123|)))
)"#;
    let path = temp_schematic("embedded_file_checksum_type", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.screen.embedded_files.len(), 1);
    let file = &schematic.screen.embedded_files[0];
    assert_eq!(file.name.as_deref(), Some("A.bin"));
    assert_eq!(file.checksum.as_deref(), Some("deadbeef"));
    assert_eq!(file.file_type, Some(EmbeddedFileType::Font));
    assert_eq!(file.data.as_deref(), Some("abc123"));
    let _ = fs::remove_file(path);
}

#[test]
fn parses_bar_delimited_embedded_files_when_version_is_not_early() {
    let spacer = " ".repeat(700);
    let src = format!(
        "(kicad_sch{spacer}\n  (version 20260306)\n  (generator \"eeschema\")\n  (uuid \"60000000-0000-0000-0000-000000000047\")\n  (embedded_files (file (name \"late.bin\") (data |abc123|)))\n)"
    );
    let path = temp_schematic("late_version_embedded_files", &src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse delayed version");

    assert_eq!(schematic.screen.embedded_files.len(), 1);
    let file = &schematic.screen.embedded_files[0];
    assert_eq!(file.name.as_deref(), Some("late.bin"));
    assert_eq!(file.data.as_deref(), Some("abc123"));

    let _ = fs::remove_file(path);
}

#[test]
fn skips_empty_embedded_file_blocks_like_kicad() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000048")
  (paper "A4")
  (embedded_files
    (file)
    (file (name "B.bin") (data |bbb|)))
)"#;
    let path = temp_schematic("empty_embedded_file_block", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.screen.embedded_files.len(), 1);
    let file = &schematic.screen.embedded_files[0];
    assert_eq!(file.name.as_deref(), Some("B.bin"));
    assert_eq!(file.data.as_deref(), Some("bbb"));
    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_embedded_file_name_restarts_current_file_state() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000049")
  (paper "A4")
  (embedded_files
    (file
      (name "A.bin")
      (checksum deadbeef)
      (type font)
      (data |abc123|)
      (name "B.bin"))))
"#;
    let path = temp_schematic("duplicate_embedded_file_name", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.screen.embedded_files.len(), 1);
    let file = &schematic.screen.embedded_files[0];
    assert_eq!(file.name.as_deref(), Some("B.bin"));
    assert_eq!(file.checksum, None);
    assert_eq!(file.file_type, None);
    assert_eq!(file.data, None);
    let _ = fs::remove_file(path);
}

#[test]
fn computes_text_box_end_from_size_and_defers_groups_until_after_items() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000099")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (uuid "11111111-1111-1111-1111-111111111111"))
  (text_box "body" (at 10 20 90) (size 3 4) (exclude_from_sim yes) (uuid "22222222-2222-2222-2222-222222222222"))
  (group "G" (uuid "33333333-3333-3333-3333-333333333333") (members "11111111-1111-1111-1111-111111111111"))
)"#;
    let path = temp_schematic("textbox_group", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert_eq!(text_box.at, [10.0, 20.0]);
    assert_eq!(text_box.angle, 90.0);
    assert_eq!(text_box.end, [13.0, 24.0]);
    assert!(text_box.excluded_from_sim);
    assert!(matches!(
        schematic.screen.items.last(),
        Some(SchItem::Group(Group { uuid, members, .. }))
            if uuid.as_deref() == Some("33333333-3333-3333-3333-333333333333")
                && members == &vec!["11111111-1111-1111-1111-111111111111".to_string()]
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn repairs_group_membership_cycles_after_deferred_resolution() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000021")
  (group "A" (uuid "63000000-0000-0000-0000-000000000026") (members "63000000-0000-0000-0000-000000000027"))
  (group "B" (uuid "63000000-0000-0000-0000-000000000027") (members "63000000-0000-0000-0000-000000000026"))
)"#;
    let path = temp_schematic("group_cycle_repair", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let groups = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Group(group) => Some(group),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].uuid.as_deref(),
        Some("63000000-0000-0000-0000-000000000027")
    );
    assert!(groups[0].members.is_empty());

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_table_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000022")
  (table
    (bogus 1)
    (cells
      (table_cell "c" (at 0 0 0) (size 5 5))))
)"#;
    let path = temp_schematic("unexpected_table_child", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad table child");
    let msg = err.to_string();
    assert!(msg.contains(
        "expecting columns, col_widths, row_heights, border, separators, uuid, header or cells"
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn schematic_text_box_does_not_require_at() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000004a")
  (paper "A4")
  (text_box "body" (size 3 4) (uuid "60000000-0000-0000-0000-00000000004b"))
)"#;
    let path = temp_schematic("text_box_without_at", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept text_box without at");
    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert_eq!(text_box.at, [0.0, 0.0]);
    assert_eq!(text_box.end, [3.0, 4.0]);
    let _ = fs::remove_file(path);
}

#[test]
fn schematic_text_box_rejects_table_cell_only_span_branch() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000004c")
  (paper "A4")
  (text_box "body" (span 2 1) (size 3 4))
)"#;
    let path = temp_schematic("text_box_rejects_span", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject table-cell-only span in schematic text_box");
    assert!(
        err.to_string()
            .contains("expecting at, size, stroke, fill, effects or uuid")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn applies_upstream_default_text_box_margins_when_omitted() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000004d")
  (paper "A4")
  (text_box "body" (at 10 20 90) (size 3 4) (stroke (width 0.2)) (effects (font (size 2 3))) (uuid "60000000-0000-0000-0000-00000000004e"))
)"#;
    let path = temp_schematic("textbox_default_margins", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");

    let margins = text_box.margins.expect("default margins");
    assert_eq!(
        text_box.stroke.as_ref().expect("text box stroke").width,
        Some(0.2)
    );
    let expected = 0.2 / 2.0 + 2.0 * 0.75;
    assert!((margins[0] - expected).abs() < 1e-9);
    assert!((margins[1] - expected).abs() < 1e-9);
    assert!((margins[2] - expected).abs() < 1e-9);
    assert!((margins[3] - expected).abs() < 1e-9);

    let _ = fs::remove_file(path);
}

#[test]
fn clamps_text_box_margins_to_kicad_internal_unit_limit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000023")
  (paper "A4")
  (text_box "body" (at 0 0 0) (size 5 5) (margins 100000000000000000000 100000000000000000000 100000000000000000000 100000000000000000000))
  (lib_symbols
    (symbol "Device:R"
      (text_box "lib" (at 0 0 0) (size 5 5) (margins 100000000000000000000 100000000000000000000 100000000000000000000 100000000000000000000))))
)"#;
    let path = temp_schematic("clamped_textbox_margins", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let limit = f64::from(i32::MAX) * 0.7071 / 1e4;

    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert_eq!(text_box.margins, Some([limit, limit, limit, limit]));

    let lib_text_box = schematic.screen.lib_symbols[0].units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "text_box")
        .expect("lib text box");
    assert_eq!(lib_text_box.margins, Some([limit, limit, limit, limit]));

    let _ = fs::remove_file(path);
}

#[test]
fn text_boxes_and_table_cells_keep_constructor_graphic_defaults() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000024")
  (text_box "box" (at 0 0 0) (size 5 5))
  (table
    (column_count 1)
    (column_widths 5)
    (row_heights 5)
    (cells
      (table_cell "cell" (at 0 0 0) (size 5 5))))
)"#;
    let path = temp_schematic("textbox_and_table_cell_defaults", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert_eq!(
        text_box
            .stroke
            .as_ref()
            .expect("default text box stroke")
            .width,
        Some(0.0)
    );
    assert_eq!(
        text_box
            .fill
            .as_ref()
            .expect("default text box fill")
            .fill_type,
        FillType::None
    );

    let table = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");
    let cell = table.get_cell(0, 0).expect("table cell");
    assert_eq!(
        cell.stroke
            .as_ref()
            .expect("default table cell stroke")
            .width,
        Some(0.0)
    );
    assert_eq!(
        cell.fill
            .as_ref()
            .expect("default table cell fill")
            .fill_type,
        FillType::None
    );
    assert_eq!(cell.col_span, 1);
    assert_eq!(cell.row_span, 1);

    let _ = fs::remove_file(path);
}

#[test]
fn table_cells_materialize_grid_positions_from_row_spans() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000025")
  (table
    (column_count 2)
    (column_widths 5 5)
    (row_heights 5 5 5)
    (cells
      (table_cell "a" (at 0 0 0) (size 5 10) (span 1 2))
      (table_cell "b" (at 5 0 0) (size 5 5))
      (table_cell "c" (at 5 5 0) (size 5 5))
      (table_cell "d" (at 0 10 0) (size 5 5))))
)"#;
    let path = temp_schematic("table_grid_positions_from_row_spans", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let table = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Table(table) => Some(table),
            _ => None,
        })
        .expect("table");

    assert_eq!(table.row_count(), 3);
    assert_eq!(table.cells[0].row, 0);
    assert_eq!(table.cells[0].column, 0);
    assert_eq!(table.cells[0].col_span, 1);
    assert_eq!(table.cells[0].row_span, 2);
    assert_eq!(table.cells[1].row, 0);
    assert_eq!(table.cells[1].column, 1);
    assert_eq!(table.cells[2].row, 1);
    assert_eq!(table.cells[2].column, 1);
    assert_eq!(table.cells[3].row, 2);
    assert_eq!(table.cells[3].column, 0);
    assert_eq!(
        table.get_cell(1, 0).expect("row-spanned table cell").text,
        "a"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parses_nested_sheet_and_symbol_instances_and_polyline_conversion() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000004f")
  (paper "A4")
  (polyline (pts (xy 0 0) (xy 1 1)) (stroke (width 0.1)) (uuid "60000000-0000-0000-0000-000000000050"))
  (symbol
    (lib_id "Device:R")
    (lib_name "Device:R")
    (at 1 2 0)
    (default_instance
      (reference "R?")
      (unit 1)
      (value "10k")
      (footprint "Resistor_SMD:R_0603"))
    (pin "1" (alternate "A") (uuid "60000000-0000-0000-0000-000000000051"))
    (instances
      (project "demo"
        (path "/A"
          (reference "R1")
          (unit 1)
          (value "10k")
          (footprint "Resistor_SMD:R_0603")
          (variant
            (name "ALT")
            (dnp yes)
            (exclude_from_sim no)
            (in_bom yes)
            (on_board no)
            (in_pos_files yes)
            (field (name "MPN") (value "123")))))))
  (sheet
    (at 0 0)
    (size 20 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S" (page " 2 ")
          (variant
            (name "ASSEMBLY")
            (dnp no)
            (exclude_from_sim yes)
            (in_bom no)
            (on_board yes)
            (in_pos_files no)
            (field (name "POP") (value "DNP")))))))
)"#;
    let path = temp_schematic("nested_instances_polyline", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert!(schematic.screen.items.iter().any(
        |item| matches!(item, SchItem::Polyline(line) if line.kind == LineKind::Polyline && line.points.len() == 2 && line.has_stroke)
    ));

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.instances.len(), 1);
    assert_eq!(symbol.lib_name, None);
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("10k")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("Resistor_SMD:R_0603")
    );
    assert_eq!(symbol.pins.len(), 1);
    assert_eq!(symbol.pins[0].alternate.as_deref(), Some("A"));
    assert_eq!(
        symbol.pins[0].uuid.as_deref(),
        Some("60000000-0000-0000-0000-000000000051")
    );
    assert_eq!(symbol.instances[0].project, "demo");
    assert_eq!(symbol.instances[0].path, "/A");
    assert_eq!(symbol.instances[0].value.as_deref(), Some("10k"));
    assert_eq!(
        symbol.instances[0].footprint.as_deref(),
        Some("Resistor_SMD:R_0603")
    );
    assert_eq!(symbol.instances[0].variants.len(), 1);
    let sym_variant = symbol.instances[0]
        .variants
        .get("ALT")
        .expect("ALT variant");
    assert_eq!(sym_variant.name, "ALT");
    assert_eq!(
        sym_variant.fields.get("MPN").map(String::as_str),
        Some("123")
    );

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.instances.len(), 1);
    assert_eq!(sheet.instances[0].project, "demo");
    assert_eq!(sheet.instances[0].page.as_deref(), Some("2"));
    let sheet_variant = sheet.instances[0]
        .variants
        .get("ASSEMBLY")
        .expect("ASSEMBLY variant");
    assert_eq!(sheet_variant.name, "ASSEMBLY");
    assert!(sheet_variant.in_bom);
    assert!(!sheet_variant.in_pos_files);

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_instance_value_and_footprint_stay_local_during_parse() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000026")
  (symbol
    (lib_id "Device:R")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (default_instance
      (value "default-value")
      (footprint "default-footprint"))
    (instances
      (project "demo"
        (path "/A"
          (value "instance-value")
          (footprint "instance-footprint")))))
)"#;
    let path = temp_schematic("symbol_instance_updates_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.instances[0].reference, None);
    assert_eq!(symbol.instances[0].unit, Some(1));
    assert_eq!(symbol.instances[0].value.as_deref(), Some("instance-value"));
    assert_eq!(
        symbol.instances[0].footprint.as_deref(),
        Some("instance-footprint")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .map(|property| property.value.as_str()),
        Some("default-value")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("default-footprint")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_local_symbol_instance_paths_overwrite_like_kicad() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000027")
  (symbol
    (lib_id "Device:R")
    (instances
      (project "demo"
        (path "/A" (reference "R1") (unit 2))
        (path "/A" (reference "R2") (unit 3)))))
)"#;
    let path = temp_schematic("duplicate_local_symbol_instance_paths", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.instances.len(), 1);
    assert_eq!(symbol.instances[0].reference.as_deref(), Some("R2"));
    assert_eq!(symbol.instances[0].unit, Some(3));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R2")
    );
    assert_eq!(symbol.unit, Some(3));

    let _ = fs::remove_file(path);
}

#[test]
fn first_local_symbol_instance_seeds_live_reference_and_unit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000028")
  (symbol
    (lib_id "Device:R")
    (instances
      (project "demo"
        (path "/A" (reference "R7") (unit 2)))))
)"#;
    let path = temp_schematic("first_local_symbol_instance_updates_live_state", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    assert_eq!(symbol.instances.len(), 1);
    assert_eq!(symbol.instances[0].reference.as_deref(), Some("R7"));
    assert_eq!(symbol.instances[0].unit, Some(2));
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("R7")
    );
    assert_eq!(symbol.unit, Some(2));

    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_local_sheet_instance_paths_preserve_parse_order_like_kicad() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000029")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/A" (page "2"))
        (path "/A" (page "3")))))
)"#;
    let path = temp_schematic("duplicate_local_sheet_instance_paths", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    assert_eq!(sheet.instances.len(), 2);
    assert_eq!(sheet.instances[0].page.as_deref(), Some("2"));
    assert_eq!(sheet.instances[1].page.as_deref(), Some("3"));

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_draw_items_sort_like_kicad_before_return() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000002a")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (pin input line
        (at 5 6 180)
        (length 2)
        (name "IN")
        (number "1"))
      (text_box "TB" (at 1 2 0) (size 3 4))
      (text private "FIELD" (at 3 4 0) (effects (font (size 1 1)) (hide)))
      (text "TXT" (at 2 3 0) (effects (font (size 1 1))))
      (circle (center 0 0) (radius 1)))))
"#;
    let path = temp_schematic("lib_symbol_draw_item_sort", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let unit = &schematic.screen.lib_symbols[0].units[0];

    assert_eq!(
        unit.draw_items
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["circle", "field", "text", "text_box", "pin"]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_duplicate_user_properties_overwrite_existing_field() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000009a")
  (symbol
    (lib_id "Device:R")
    (property "MPN" "first" (at 0 0 0))
    (property "MPN" "second" (at 1 2 0))))
"#;
    let path = temp_schematic("symbol_duplicate_user_property", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");

    let mpn_properties = symbol
        .properties
        .iter()
        .filter(|property| property.key == "MPN")
        .collect::<Vec<_>>();
    assert_eq!(mpn_properties.len(), 1);
    assert_eq!(mpn_properties[0].value, "second");
    assert_eq!(mpn_properties[0].at, Some([1.0, 2.0]));

    let _ = fs::remove_file(path);
}

#[test]
fn global_label_duplicate_user_properties_are_appended() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000009b")
  (global_label "GL"
    (property "FOO" "first" (at 0 0 0))
    (property "FOO" "second" (at 1 2 0))))
"#;
    let path = temp_schematic("global_label_duplicate_user_property", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let label = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");

    let foo_properties = label
        .properties
        .iter()
        .filter(|property| property.key == "FOO")
        .collect::<Vec<_>>();
    assert_eq!(foo_properties.len(), 2);
    assert_eq!(foo_properties[0].value, "first");
    assert_eq!(foo_properties[1].value, "second");

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_variant_in_bom_respects_20260306_fix_boundary() {
    let old_src = r#"(kicad_sch
  (version 20260305)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000002b")
  (sheet
    (at 0 0)
    (size 20 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S"
          (variant
            (name "OLD")
            (in_bom yes))))))
)"#;
    let old_path = temp_schematic("sheet_variant_in_bom_old", old_src);
    let old = parse_schematic_file(Path::new(&old_path)).expect("old version must parse");
    let old_sheet = old
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("old sheet");
    assert!(!old_sheet.instances[0].variants["OLD"].in_bom);

    let new_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000002c")
  (sheet
    (at 0 0)
    (size 20 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S"
          (variant
            (name "NEW")
            (in_bom yes))))))
)"#;
    let new_path = temp_schematic("sheet_variant_in_bom_new", new_src);
    let new = parse_schematic_file(Path::new(&new_path)).expect("new version must parse");
    let new_sheet = new
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("new sheet");
    assert!(new_sheet.instances[0].variants["NEW"].in_bom);

    let _ = fs::remove_file(old_path);
    let _ = fs::remove_file(new_path);
}

#[test]
fn symbol_and_sheet_variants_inherit_parent_attributes() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000002d")
  (symbol
    (lib_id "Device:R")
    (exclude_from_sim yes)
    (in_bom no)
    (on_board no)
    (in_pos_files no)
    (dnp yes)
    (instances
      (project "demo"
        (path "/A"
          (variant
            (name "SYM")
            (field (name "MPN") (value "123")))))))
  (sheet
    (exclude_from_sim yes)
    (in_bom no)
    (on_board no)
    (dnp yes)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S"
          (variant
            (name "SHEET")
            (field (name "POP") (value "DNP")))))))
)"#;
    let path = temp_schematic("variant_inherit_parent_attributes", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let sym_variant = &symbol.instances[0].variants["SYM"];
    assert_eq!(sym_variant.name, "SYM");
    assert!(sym_variant.dnp);
    assert!(sym_variant.excluded_from_sim);
    assert!(!sym_variant.in_bom);
    assert!(!sym_variant.on_board);
    assert!(!sym_variant.in_pos_files);

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    let sheet_variant = &sheet.instances[0].variants["SHEET"];
    assert_eq!(sheet_variant.name, "SHEET");
    assert!(sheet_variant.dnp);
    assert!(sheet_variant.excluded_from_sim);
    assert!(!sheet_variant.in_bom);
    assert!(!sheet_variant.on_board);
    assert!(!sheet_variant.in_pos_files);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_variant_field_name_and_value_tokens() {
    let bad_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000002e")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (instances
      (project "demo"
        (path "/A"
          (variant
            (name "ALT")
            (field (name (bogus)) (value "123")))))))
)"#;
    let bad_name_path = temp_schematic("bad_variant_field_name", bad_name);
    let err =
        parse_schematic_file(Path::new(&bad_name_path)).expect_err("must reject bad field name");
    assert!(err.to_string().contains("Invalid variant field name"));

    let bad_variant_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000002f")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (instances
      (project "demo"
        (path "/A"
          (variant
            (name (bogus))
            (field (name "MPN") (value "123")))))))
)"#;
    let bad_variant_name_path = temp_schematic("bad_variant_name", bad_variant_name);
    let err = parse_schematic_file(Path::new(&bad_variant_name_path))
        .expect_err("must reject bad variant name");
    assert!(err.to_string().contains("Invalid variant name"));

    let bad_value = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000030")
  (sheet
    (at 0 0)
    (size 20 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S"
          (variant
            (name "ALT")
            (field (name "MPN") (value (bogus)))))))
)"#;
    let bad_value_path = temp_schematic("bad_variant_field_value", bad_value);
    let err =
        parse_schematic_file(Path::new(&bad_value_path)).expect_err("must reject bad field value");
    assert!(err.to_string().contains("Invalid variant field value"));

    let _ = fs::remove_file(bad_name_path);
    let _ = fs::remove_file(bad_variant_name_path);
    let _ = fs::remove_file(bad_value_path);
}

#[test]
fn duplicate_variant_names_and_fields_overwrite_by_name() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000031")
  (symbol
    (lib_id "Device:R")
    (instances
      (project "demo"
        (path "/A"
          (variant
            (name "ALT")
            (dnp no)
            (field (name "MPN") (value "111"))
            (field (name "MPN") (value "222")))
          (variant
            (name "ALT")
            (dnp yes)
            (field (name "MPN") (value "333")))))))
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S"
          (variant
            (name "ASSEMBLY")
            (on_board yes)
            (field (name "POP") (value "ONE"))
            (field (name "POP") (value "TWO")))
          (variant
            (name "ASSEMBLY")
            (on_board no)
            (field (name "POP") (value "THREE")))))))
)"#;
    let path = temp_schematic("duplicate_variant_names_and_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.instances[0].variants.len(), 1);
    let symbol_variant = symbol.instances[0]
        .variants
        .get("ALT")
        .expect("ALT variant");
    assert!(symbol_variant.dnp);
    assert_eq!(
        symbol_variant.fields.get("MPN").map(String::as_str),
        Some("333")
    );

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.instances[0].variants.len(), 1);
    let sheet_variant = sheet.instances[0]
        .variants
        .get("ASSEMBLY")
        .expect("ASSEMBLY variant");
    assert!(!sheet_variant.on_board);
    assert_eq!(
        sheet_variant.fields.get("POP").map(String::as_str),
        Some("THREE")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn late_variant_name_preserves_provisional_empty_key() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000032")
  (symbol
    (lib_id "Device:R")
    (instances
      (project "demo"
        (path "/A"
          (variant
            (dnp yes)
            (name "ALT"))))))
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S"
          (variant
            (on_board no)
            (name "ASSEMBLY"))))))
)"#;
    let path = temp_schematic("late_variant_name_preserves_empty_key", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert!(symbol.instances[0].variants.contains_key(""));
    assert!(symbol.instances[0].variants.contains_key("ALT"));

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert!(sheet.instances[0].variants.contains_key(""));
    assert!(sheet.instances[0].variants.contains_key("ASSEMBLY"));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_nested_instance_symbol_headers() {
    let bad_symbol_project = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000033")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (instances
      (project (bogus)
        (path "/A" (reference "R1")))))
)"#;
    let bad_symbol_project_path = temp_schematic("bad_symbol_project_name", bad_symbol_project);
    let err = parse_schematic_file(Path::new(&bad_symbol_project_path))
        .expect_err("must reject bad project name");
    assert!(err.to_string().contains("expecting symbol"));

    let bad_sheet_path = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000034")
  (sheet_instances
    (path (bogus) (page "1")))
)"#;
    let bad_sheet_path_path = temp_schematic("bad_sheet_instance_path", bad_sheet_path);
    let err = parse_schematic_file(Path::new(&bad_sheet_path_path))
        .expect_err("must reject bad sheet instance path");
    assert!(err.to_string().contains("expecting symbol"));

    let bad_symbol_reference = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000035")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (instances
      (project "demo"
        (path "/A" (reference (bogus)))))) 
)"#;
    let bad_symbol_reference_path =
        temp_schematic("bad_symbol_instance_reference", bad_symbol_reference);
    let err = parse_schematic_file(Path::new(&bad_symbol_reference_path))
        .expect_err("must reject bad symbol reference");
    assert!(err.to_string().contains("expecting symbol"));

    let bad_placed_symbol_unit = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000036")
  (symbol
    (lib_id "Device:R")
    (unit (bogus)))
)"#;
    let bad_placed_symbol_unit_path =
        temp_schematic("bad_placed_symbol_unit", bad_placed_symbol_unit);
    let err = parse_schematic_file(Path::new(&bad_placed_symbol_unit_path))
        .expect_err("must reject bad placed symbol unit");
    assert!(err.to_string().contains("missing symbol unit"));

    let bad_top_level_symbol_unit = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000037")
  (symbol_instances
    (path "/A" (unit (bogus))))
)"#;
    let bad_top_level_symbol_unit_path = temp_schematic(
        "bad_top_level_symbol_instance_unit",
        bad_top_level_symbol_unit,
    );
    let err = parse_schematic_file(Path::new(&bad_top_level_symbol_unit_path))
        .expect_err("must reject bad top-level symbol unit");
    assert!(err.to_string().contains("missing symbol unit"));

    let _ = fs::remove_file(bad_symbol_project_path);
    let _ = fs::remove_file(bad_sheet_path_path);
    let _ = fs::remove_file(bad_symbol_reference_path);
    let _ = fs::remove_file(bad_placed_symbol_unit_path);
    let _ = fs::remove_file(bad_top_level_symbol_unit_path);
}

#[test]
fn rejects_invalid_sheet_instance_page_tokens() {
    let bad_top_level = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000038")
  (sheet_instances
    (path "/A" (page (bogus))))
)"#;
    let bad_top_level_path = temp_schematic("bad_sheet_instance_page", bad_top_level);
    let err = parse_schematic_file(Path::new(&bad_top_level_path))
        .expect_err("must reject bad top-level sheet page");
    assert!(err.to_string().contains("expecting symbol"));

    let bad_nested = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000039")
  (sheet
    (at 0 0)
    (size 20 10)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S" (page (bogus))))))
)"#;
    let bad_nested_path = temp_schematic("bad_nested_sheet_page", bad_nested);
    let err = parse_schematic_file(Path::new(&bad_nested_path))
        .expect_err("must reject bad nested sheet page");
    assert!(err.to_string().contains("expecting symbol"));

    let bad_nested_child = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000003a")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (instances
      (project "demo"
        (path "/S" (bogus "x")))))
)"#;
    let bad_nested_child_path = temp_schematic("bad_nested_sheet_instance_child", bad_nested_child);
    let err = parse_schematic_file(Path::new(&bad_nested_child_path))
        .expect_err("must reject bad nested sheet instance child");
    assert!(err.to_string().contains("expecting page or variant"));

    let _ = fs::remove_file(bad_top_level_path);
    let _ = fs::remove_file(bad_nested_path);
    let _ = fs::remove_file(bad_nested_child_path);
}

#[test]
fn rejects_unknown_top_level_symbol_instance_child() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000003b")
  (symbol_instances
    (path "/A" (bogus "R1")))
)"#;
    let path = temp_schematic("bad_top_level_symbol_instance_child", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject bad top-level symbol instance child");
    assert!(
        err.to_string()
            .contains("expecting path, unit, value or footprint")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_default_instance_reference() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000003c")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (default_instance
      (reference (bogus))
      (unit 1)
      (value "10k")
      (footprint "Resistor_SMD:R_0603")))
)"#;
    let path = temp_schematic("bad_default_instance_reference", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject bad default instance reference");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_instance_value_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000003d")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (instances
      (project "demo"
        (path "/A"
          (reference "R1")
          (unit 1)
          (value (bogus))
          (footprint "Resistor_SMD:R_0603")))))
)"#;
    let path = temp_schematic("bad_symbol_instance_value", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol instance value");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_pin_number() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000003e")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin (bogus) (alternate "A")))
)"#;
    let path = temp_schematic("bad_symbol_pin_number", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol pin number");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_pin_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000003f")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin "1" (uuid (bogus))))
)"#;
    let path = temp_schematic("bad_symbol_pin_uuid", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol pin uuid");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn legacy_symbol_pin_uuid_tokens_do_not_consume_shared_uuid_uniqueness() {
    let src = r#"(kicad_sch
  (version 20210125)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000040")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin "1" (uuid "1")))
  (junction (uuid "1"))
)"#;
    let path = temp_schematic("legacy_symbol_pin_uuid_ignored", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.pins[0].uuid, None);

    let junction = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Junction(junction) => Some(junction),
            _ => None,
        })
        .expect("junction");
    assert_eq!(
        junction.uuid.as_deref(),
        Some("00000000-0000-0000-0000-000000000001")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_pin_alternate_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000041")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin "1" (alternate (bogus))))
)"#;
    let path = temp_schematic("bad_symbol_pin_alternate", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol pin alternate");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_mirror_axis_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000042")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (mirror (bogus)))
)"#;
    let path = temp_schematic("bad_symbol_mirror_axis", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad mirror axis token");
    assert!(err.to_string().contains("expecting x or y"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000043")
  (lib_symbols
    (symbol (bogus)))
)"#;
    let path = temp_schematic("bad_lib_symbol_name", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad lib symbol name");
    assert!(err.to_string().contains("Invalid symbol name"));
    let _ = fs::remove_file(path);
}

#[test]
fn parses_bom_prefixed_schematic_header() {
    let src = "\u{feff}(kicad_sch\n  (version 20260306)\n  (generator \"eeschema\")\n  (uuid \"bom-root\")\n  (paper \"A4\")\n)";
    let path = temp_schematic("bom_prefixed_schematic", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse UTF-8 BOM");

    let uuid = schematic.screen.uuid.as_deref().expect("root uuid");
    assert!(Uuid::parse_str(uuid).is_ok());
    assert_ne!(uuid, "bom-root");

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_parent_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000044")
  (lib_symbols
    (symbol "Child:R"
      (extends (bogus))))
)"#;
    let path = temp_schematic("bad_lib_symbol_parent_name", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad lib parent symbol name");
    assert!(err.to_string().contains("Invalid parent symbol name"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_unit_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000045")
  (lib_symbols
    (symbol "Device:R"
      (symbol (bogus))))
)"#;
    let path = temp_schematic("bad_lib_symbol_unit_name", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad lib unit name");
    assert!(err.to_string().contains("Invalid symbol unit name"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_numeric_lib_unit_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000046")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (unit_name 123))))
)"#;
    let path = temp_schematic("bad_lib_unit_name_token", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject numeric lib unit_name token");
    assert!(err.to_string().contains("expecting )"));
    let _ = fs::remove_file(path);
}

#[test]
fn lib_unit_name_applies_across_body_styles_of_same_unit() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000047")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (unit_name "Amplifier"))
      (symbol "R_1_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1)))))))
)"#;
    let path = temp_schematic("lib_unit_name_shared_across_body_styles", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];

    assert_eq!(lib_symbol.units.len(), 2);
    assert_eq!(lib_symbol.units[0].unit_name.as_deref(), Some("Amplifier"));
    assert_eq!(lib_symbol.units[1].unit_name.as_deref(), Some("Amplifier"));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_title_block_value_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000048")
  (title_block
    (title (bogus)))
)"#;
    let path = temp_schematic("bad_title_block_value", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad title block value");
    assert!(err.to_string().contains("missing title"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_title_block_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000049")
  (title_block
    (bogus "x"))
)"#;
    let path = temp_schematic("bad_title_block_child", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad title block child");
    assert!(
        err.to_string()
            .contains("expecting title, date, rev, company, or comment")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_schematic_text_string_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000004a")
  (text (bogus) (at 1 2 0))
)"#;
    let path = temp_schematic("bad_schematic_text_string", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad schematic text string");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_schematic_text_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000004b")
  (text "note" (at 1 2 0) (uuid (bogus)))
)"#;
    let path = temp_schematic("bad_schematic_text_uuid", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad schematic text uuid");
    assert!(err.to_string().contains("expecting uuid"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_schematic_text_box_string_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000004c")
  (text_box (bogus) (at 1 2 0) (size 3 4))
)"#;
    let path = temp_schematic("bad_schematic_text_box_string", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject bad schematic text box string");
    assert!(err.to_string().contains("expecting symbol"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_schematic_text_box_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000004d")
  (text_box "body" (at 1 2 0) (size 3 4) (uuid (bogus)))
)"#;
    let path = temp_schematic("bad_schematic_text_box_uuid", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject bad schematic text box uuid");
    assert!(err.to_string().contains("expecting uuid"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_image_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000004e")
  (image (at 1 2) (uuid (bogus)) (data "QUJD"))
)"#;
    let path = temp_schematic("bad_image_uuid", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad image uuid");
    assert!(err.to_string().contains("expecting uuid"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_uuid_tokens_in_remaining_schematic_items() {
    let bad_root = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid (bogus))
)"#;
    let bad_root_path = temp_schematic("bad_root_uuid", bad_root);
    let err =
        parse_schematic_file(Path::new(&bad_root_path)).expect_err("must reject bad root uuid");
    assert!(err.to_string().contains("expecting uuid"));

    let bad_symbol = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000004f")
  (symbol (lib_id "Device:R") (at 1 2 0) (uuid (bogus)))
)"#;
    let bad_symbol_path = temp_schematic("bad_symbol_uuid", bad_symbol);
    let err =
        parse_schematic_file(Path::new(&bad_symbol_path)).expect_err("must reject bad symbol uuid");
    assert!(err.to_string().contains("expecting uuid"));

    let bad_group = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000050")
  (group "g"
    (uuid (bogus))
    (members "a")))
)"#;
    let bad_group_path = temp_schematic("bad_group_uuid", bad_group);
    let err =
        parse_schematic_file(Path::new(&bad_group_path)).expect_err("must reject bad group uuid");
    assert!(err.to_string().contains("expecting uuid"));

    let bad_group_member = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000051")
  (group "g"
    (uuid "63000000-0000-0000-0000-000000000080")
    (members (bogus))))
)"#;
    let bad_group_member_path = temp_schematic("bad_group_member_uuid", bad_group_member);
    let err = parse_schematic_file(Path::new(&bad_group_member_path))
        .expect_err("must reject bad group member uuid");
    assert!(err.to_string().contains("expecting group member uuid"));

    let bad_group_child = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000052")
  (group "g"
    (bogus "x")))
)"#;
    let bad_group_child_path = temp_schematic("bad_group_child", bad_group_child);
    let err = parse_schematic_file(Path::new(&bad_group_child_path))
        .expect_err("must reject bad group child");
    assert!(err.to_string().contains("expecting uuid, lib_id, members"));

    let bad_group_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000053")
  (group g
    (uuid "63000000-0000-0000-0000-000000000081")
    (members "a")))
)"#;
    let bad_group_name_path = temp_schematic("bad_group_name", bad_group_name);
    let err = parse_schematic_file(Path::new(&bad_group_name_path))
        .expect_err("must reject unquoted group name");
    assert!(err.to_string().contains("expecting group name or locked"));
    let _ = fs::remove_file(&bad_group_name_path);

    let bare_locked_group = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000054")
  (group locked
    (uuid "63000000-0000-0000-0000-000000000082")
    (members "a")))
)"#;
    let bare_locked_group_path = temp_schematic("bad_group_locked", bare_locked_group);
    let err = parse_schematic_file(Path::new(&bare_locked_group_path))
        .expect_err("must reject bare locked group name");
    assert!(err.to_string().contains("expecting group name or locked"));
    let _ = fs::remove_file(bare_locked_group_path);

    let bad_group_lib_id = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000055")
  (group "g"
    (uuid "63000000-0000-0000-0000-000000000083")
    (lib_id "lib:block:bad")
    (members "a")))
)"#;
    let bad_group_lib_id_path = temp_schematic("bad_group_lib_id", bad_group_lib_id);
    let err = parse_schematic_file(Path::new(&bad_group_lib_id_path))
        .expect_err("must reject bad group lib_id");
    assert!(
        err.to_string()
            .contains("Group library link lib:block:bad contains invalid character ':'")
    );

    let empty_group_lib_id = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000056")
  (group "g"
    (uuid "63000000-0000-0000-0000-000000000084")
    (lib_id "lib:")
    (members "a")))
)"#;
    let empty_group_lib_id_path = temp_schematic("bad_group_lib_id_empty_item", empty_group_lib_id);
    let err = parse_schematic_file(Path::new(&empty_group_lib_id_path))
        .expect_err("must reject group lib_id with empty item name");
    assert!(err.to_string().contains("Invalid library ID"));

    let bad_rectangle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000057")
  (rectangle (start 0 0) (end 1 1) (uuid (bogus)))
)"#;
    let bad_rectangle_path = temp_schematic("bad_rectangle_uuid", bad_rectangle);
    let err = parse_schematic_file(Path::new(&bad_rectangle_path))
        .expect_err("must reject bad rectangle uuid");
    assert!(err.to_string().contains("expecting uuid"));

    let _ = fs::remove_file(bad_root_path);
    let _ = fs::remove_file(bad_symbol_path);
    let _ = fs::remove_file(bad_group_path);
    let _ = fs::remove_file(bad_group_member_path);
    let _ = fs::remove_file(bad_group_child_path);
    let _ = fs::remove_file(bad_group_name_path);
    let _ = fs::remove_file(bad_group_lib_id_path);
    let _ = fs::remove_file(empty_group_lib_id_path);
    let _ = fs::remove_file(bad_rectangle_path);
}

#[test]
fn parses_and_rejects_lib_property_header_and_metadata_tokens() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000058")
  (lib_symbols
    (symbol "Device:R"
      (property private "UserField" "R1"
        (id 7)
        (show_name)
        (do_not_autoplace)))))
"#;
    let path = temp_schematic("lib_property_metadata", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];
    let property = lib_symbol.units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "field" && item.name.as_deref() == Some("UserField"))
        .expect("user field");
    assert_eq!(property.field_id, Some(0));
    assert!(property.is_private);
    assert!(property.show_name);
    assert!(!property.can_autoplace);
    let _ = fs::remove_file(path);

    let hidden_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000059")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "UserField" "R1"
        (effects (font (size 1 1)) (hide)))))
)
"#;
    let hidden_path = temp_schematic("lib_property_hidden_effects", hidden_src);
    let schematic = parse_schematic_file(Path::new(&hidden_path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];
    let property = lib_symbol.units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "field" && item.name.as_deref() == Some("UserField"))
        .expect("user field");
    assert!(!property.visible);
    assert!(property.effects.is_some());
    assert!(property.effects.as_ref().expect("effects").hidden);
    let _ = fs::remove_file(hidden_path);

    let bad_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000005a")
  (lib_symbols
    (symbol "Device:R"
      (property (bogus) "R1")))
)"#;
    let bad_name_path = temp_schematic("bad_lib_property_name", bad_name);
    let err =
        parse_schematic_file(Path::new(&bad_name_path)).expect_err("must reject bad property name");
    assert!(err.to_string().contains("Invalid property name"));

    let bad_value = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000005b")
  (lib_symbols
    (symbol "Device:R"
      (property "UserField" (bogus))))
)"#;
    let bad_value_path = temp_schematic("bad_lib_property_value", bad_value);
    let err = parse_schematic_file(Path::new(&bad_value_path))
        .expect_err("must reject bad property value");
    assert!(err.to_string().contains("Invalid property value"));

    let _ = fs::remove_file(bad_name_path);
    let _ = fs::remove_file(bad_value_path);
}

#[test]
fn rejects_invalid_lib_pin_name_number_and_alternate_name_tokens() {
    let bad_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000005c")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (name (bogus)))))
)"#;
    let bad_name_path = temp_schematic("bad_lib_pin_name", bad_name);
    let err =
        parse_schematic_file(Path::new(&bad_name_path)).expect_err("must reject bad pin name");
    assert!(err.to_string().contains("Invalid pin name"));

    let bad_number = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000005d")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (number (bogus)))))
)"#;
    let bad_number_path = temp_schematic("bad_lib_pin_number", bad_number);
    let err =
        parse_schematic_file(Path::new(&bad_number_path)).expect_err("must reject bad pin number");
    assert!(err.to_string().contains("Invalid pin number"));

    let bad_name_trailer = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000005e")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (name "PIN" (bogus)))))
)"#;
    let bad_name_trailer_path = temp_schematic("bad_lib_pin_name_trailer", bad_name_trailer);
    let err = parse_schematic_file(Path::new(&bad_name_trailer_path))
        .expect_err("must reject bad pin name trailer");
    assert!(err.to_string().contains("expecting effects"));

    let bare_name_trailer = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000005f")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (name "PIN" hide))))
)"#;
    let bare_name_trailer_path = temp_schematic("bad_lib_pin_name_bare_trailer", bare_name_trailer);
    let err = parse_schematic_file(Path::new(&bare_name_trailer_path))
        .expect_err("must reject bare trailing token after pin name");
    assert!(err.to_string().contains("expecting effects"));

    let bad_number_trailer = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000060")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (number "1" (bogus)))))
)"#;
    let bad_number_trailer_path = temp_schematic("bad_lib_pin_number_trailer", bad_number_trailer);
    let err = parse_schematic_file(Path::new(&bad_number_trailer_path))
        .expect_err("must reject bad pin number trailer");
    assert!(err.to_string().contains("expecting effects"));

    let bare_number_trailer = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000061")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (number "1" hide))))
)"#;
    let bare_number_trailer_path =
        temp_schematic("bad_lib_pin_number_bare_trailer", bare_number_trailer);
    let err = parse_schematic_file(Path::new(&bare_number_trailer_path))
        .expect_err("must reject bare trailing token after pin number");
    assert!(err.to_string().contains("expecting effects"));

    let bad_alternate = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000062")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate (bogus) passive line))))
)"#;
    let bad_alternate_path = temp_schematic("bad_lib_pin_alternate_name", bad_alternate);
    let err = parse_schematic_file(Path::new(&bad_alternate_path))
        .expect_err("must reject bad alternate pin name");
    assert!(err.to_string().contains("Invalid alternate pin name"));

    let bad_alternate_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000063")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate "ALT" (bogus) line))))
)"#;
    let bad_alternate_type_path = temp_schematic("bad_lib_pin_alternate_type", bad_alternate_type);
    let err = parse_schematic_file(Path::new(&bad_alternate_type_path))
        .expect_err("must reject bad alternate pin type");
    assert!(err.to_string().contains(
        "expecting input, output, bidirectional, tri_state, passive, unspecified, power_in, power_out, open_collector, open_emitter, free or no_connect"
    ));

    let bad_alternate_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000064")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate "ALT" passive (bogus)))))
)"#;
    let bad_alternate_shape_path =
        temp_schematic("bad_lib_pin_alternate_shape", bad_alternate_shape);
    let err = parse_schematic_file(Path::new(&bad_alternate_shape_path))
        .expect_err("must reject bad alternate pin shape");
    assert!(err.to_string().contains(
        "expecting line, inverted, clock, inverted_clock, input_low, clock_low, output_low, edge_clock_high, non_logic"
    ));

    let _ = fs::remove_file(bad_name_path);
    let _ = fs::remove_file(bad_number_path);
    let _ = fs::remove_file(bad_name_trailer_path);
    let _ = fs::remove_file(bare_name_trailer_path);
    let _ = fs::remove_file(bad_number_trailer_path);
    let _ = fs::remove_file(bare_number_trailer_path);
    let _ = fs::remove_file(bad_alternate_path);
    let _ = fs::remove_file(bad_alternate_type_path);
    let _ = fs::remove_file(bad_alternate_shape_path);
}

#[test]
fn rejects_invalid_lib_pin_type_and_shape_tokens() {
    let bad_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000065")
  (lib_symbols
    (symbol "Device:R"
      (pin (bogus) line)))
)"#;
    let bad_type_path = temp_schematic("bad_lib_pin_type", bad_type);
    let err =
        parse_schematic_file(Path::new(&bad_type_path)).expect_err("must reject bad pin type");
    assert!(err.to_string().contains(
        "expecting input, output, bidirectional, tri_state, passive, unspecified, power_in, power_out, open_collector, open_emitter, free or no_connect"
    ));

    let bad_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000066")
  (lib_symbols
    (symbol "Device:R"
      (pin passive (bogus))))
)"#;
    let bad_shape_path = temp_schematic("bad_lib_pin_shape", bad_shape);
    let err =
        parse_schematic_file(Path::new(&bad_shape_path)).expect_err("must reject bad pin shape");
    assert!(err.to_string().contains(
        "expecting line, inverted, clock, inverted_clock, input_low, clock_low, output_low, edge_clock_high, non_logic"
    ));

    let _ = fs::remove_file(bad_type_path);
    let _ = fs::remove_file(bad_shape_path);
}

#[test]
fn lib_pin_alternate_names_stay_raw_symbols() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000067")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate "~" passive line)))))
"#;
    let path = temp_schematic("lib_pin_alternate_name_raw_symbol", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let pin = &schematic.screen.lib_symbols[0].units[0].draw_items[0];

    assert!(pin.alternates.contains_key("~"));
    assert_eq!(pin.alternates["~"].name, "~");

    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_lib_pin_alternates_overwrite_by_name() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000068")
  (lib_symbols
    (symbol "Device:R"
      (pin input line
        (alternate "ALT" output clock)
        (alternate "ALT" passive inverted))))
)"#;
    let path = temp_schematic("duplicate_lib_pin_alternate", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let pin = &schematic.screen.lib_symbols[0].units[0].draw_items[0];

    assert_eq!(pin.alternates.len(), 1);
    let alternate = pin.alternates.get("ALT").expect("alternate");
    assert_eq!(alternate.electrical_type, "passive");
    assert_eq!(alternate.graphic_shape, "inverted");

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_text_string_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000069")
  (lib_symbols
    (symbol "Device:R"
      (text (bogus) (at 0 0 0))))
)"#;
    let path = temp_schematic("bad_lib_text_string", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad lib text");
    assert!(err.to_string().contains("Invalid text string"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_text_box_string_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000006a")
  (lib_symbols
    (symbol "Device:R"
      (text_box (bogus) (at 0 0 0) (size 1 1))))
)"#;
    let path = temp_schematic("bad_lib_text_box_string", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad lib text box");
    assert!(err.to_string().contains("Invalid text string"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_jumper_pin_group_member_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000006b")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups ((bogus) "2"))))
)"#;
    let path = temp_schematic("bad_lib_jumper_pin_group_member", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad jumper pin group");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn deduplicates_lib_jumper_pin_group_members_like_upstream_sets() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000006c")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups ("2" "1" "2"))))
)"#;
    let path = temp_schematic("lib_jumper_pin_group_sets", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic must parse");
    let lib_symbol = schematic
        .screen
        .lib_symbols
        .first()
        .expect("lib symbol should parse");
    assert_eq!(lib_symbol.jumper_pin_groups.len(), 1);
    assert_eq!(
        lib_symbol.jumper_pin_groups[0],
        BTreeSet::from(["1".to_string(), "2".to_string()])
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_unquoted_numeric_lib_jumper_pin_group_members() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000006d")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups (1 2 10))))
)"#;
    let path = temp_schematic("unquoted_numeric_jumper_pin_groups", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept bare numeric pin names");
    let lib_symbol = &schematic.screen.lib_symbols[0];

    assert_eq!(lib_symbol.jumper_pin_groups.len(), 1);
    assert_eq!(
        lib_symbol.jumper_pin_groups[0],
        BTreeSet::from(["1".to_string(), "2".to_string(), "10".to_string()])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unquoted_keyword_lib_jumper_pin_group_members() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000006e")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups (hide 2))))
)"#;
    let path = temp_schematic("keyword_jumper_pin_groups", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject unquoted keyword pin names");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_parser_head_keywords_as_unquoted_lib_jumper_pin_group_members() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000006f")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups (margins 2))))
)"#;
    let path = temp_schematic("parser_keyword_jumper_pin_groups", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject unquoted parser-head pin names");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_lib_pin_keywords_as_unquoted_lib_jumper_pin_group_members() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000070")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups (power_in 2))))
)"#;
    let path = temp_schematic("lib_pin_keyword_jumper_pin_groups", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject unquoted lib-pin keyword pin names");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_effects_as_unquoted_lib_jumper_pin_group_member() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000071")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups (effects 2))))
)"#;
    let path = temp_schematic("effects_keyword_jumper_pin_groups", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject unquoted effects keyword pin names");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_mirror_axis_keywords_as_unquoted_lib_jumper_pin_group_members() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000072")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups (x y))))
)"#;
    let path = temp_schematic("mirror_axis_keyword_jumper_pin_groups", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject unquoted mirror-axis keyword pin names");
    assert!(err.to_string().contains("expecting list of pin names"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_lib_symbol_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000073")
  (lib_symbols
    (symbol "Device:R"
      (bogus 1)))
)"#;
    let path = temp_schematic("bad_lib_symbol_child", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject unexpected lib child");
    assert!(err.to_string().contains(
        "expecting pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text"
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_lib_symbol_top_level_child_head_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000074")
  (lib_symbols
    (symbol "Device:R"
      ("power" local)))
)"#;
    let path = temp_schematic("quoted_lib_symbol_child", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject quoted unexpected lib child");
    assert!(err.to_string().contains(
        "expecting pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text"
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_lib_symbol_unit_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000075")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (bogus 1))))
)"#;
    let path = temp_schematic("bad_lib_symbol_unit_child", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject unexpected lib unit child");
    assert!(
        err.to_string()
            .contains("expecting arc, bezier, circle, pin, polyline, rectangle, or text")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn preserves_shape_stroke_and_fill_presence() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000052")
  (paper "A4")
  (arc (start 0 0) (mid 1 1) (end 2 0) (stroke (width 0.1) (color 10 20 30 0.5)) (fill (type outline)) (uuid "60000000-0000-0000-0000-000000000053"))
  (rule_area (polyline (pts (xy 0 0) (xy 2 0) (xy 2 2)) (stroke (width 0.2)) (fill (type background)) (uuid "60000000-0000-0000-0000-000000000054")))
)"#;
    let path = temp_schematic("shape_stroke_fill", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let shapes = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Shape(shape) => Some(shape),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shapes.len(), 2);
    assert!(shapes[0].has_stroke);
    assert!(shapes[0].has_fill);
    assert_eq!(
        shapes[0].stroke.as_ref().and_then(|stroke| stroke.width),
        Some(0.1)
    );
    assert_eq!(
        shapes[0].fill.as_ref().map(|fill| fill.fill_type.clone()),
        Some(FillType::Color)
    );
    assert_eq!(
        shapes[0].fill.as_ref().and_then(|fill| fill.color),
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 0.5])
    );
    assert!(shapes[1].has_stroke);
    assert!(shapes[1].has_fill);
    assert_eq!(
        shapes[1].stroke.as_ref().and_then(|stroke| stroke.width),
        Some(0.2)
    );
    assert_eq!(
        shapes[1].fill.as_ref().map(|fill| fill.fill_type.clone()),
        Some(FillType::Background)
    );
    let _ = fs::remove_file(path);
}

#[test]
fn parses_explicit_sheet_line_and_bus_entry_stroke_tokens() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000055")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (width 0.15)) (uuid "60000000-0000-0000-0000-000000000056"))
  (bus_entry (at 1 1) (size 2 3) (stroke (width 0.2)) (uuid "60000000-0000-0000-0000-000000000057"))
  (sheet
    (at 0 0)
    (size 20 10)
    (stroke (width 0.1))
    (fill (color 0 0 0 0.0))
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let path = temp_schematic("explicit_stroke_tokens", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let wire = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Wire(line) => Some(line),
            _ => None,
        })
        .expect("wire");
    assert!(wire.has_stroke);
    assert_eq!(
        wire.stroke.as_ref().and_then(|stroke| stroke.width),
        Some(0.15)
    );

    let bus_entry = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::BusEntry(entry) => Some(entry),
            _ => None,
        })
        .expect("bus entry");
    assert!(bus_entry.has_stroke);
    assert_eq!(
        bus_entry.stroke.as_ref().and_then(|stroke| stroke.width),
        Some(0.2)
    );

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(sheet.border_width, 0.1);
    assert_eq!(sheet.background_color, Some([0.0, 0.0, 0.0, 0.0]));

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_bus_entry_default_stroke_does_not_rewrite_to_dash() {
    let src = r#"(kicad_sch
  (version 20211123)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000076")
  (bus_entry (stroke (width 0.2)) (uuid "60000000-0000-0000-0000-000000000058"))
)"#;
    let path = temp_schematic("legacy_bus_entry_default_stroke", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let bus_entry = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::BusEntry(bus_entry) => Some(bus_entry),
            _ => None,
        })
        .expect("bus_entry");

    assert_eq!(
        bus_entry.stroke.as_ref().expect("bus_entry stroke").style,
        StrokeStyle::Default
    );

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_wire_default_stroke_does_not_rewrite_to_dash() {
    let src = r#"(kicad_sch
  (version 20211123)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000077")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (width 0.2)) (uuid "60000000-0000-0000-0000-000000000059"))
)"#;
    let path = temp_schematic("legacy_wire_default_stroke", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let wire = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Wire(wire) => Some(wire),
            _ => None,
        })
        .expect("wire");

    assert_eq!(
        wire.stroke.as_ref().expect("wire stroke").style,
        StrokeStyle::Default
    );

    let _ = fs::remove_file(path);
}

#[test]
fn bus_entry_size_uses_distinct_height_and_width_errors() {
    let bad_height = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000078")
  (bus_entry (size (bogus) 2))
)"#;
    let bad_height_path = temp_schematic("bad_bus_entry_height", bad_height);
    let err = parse_schematic_file(Path::new(&bad_height_path))
        .expect_err("must reject bad bus entry height");
    assert!(err.to_string().contains("missing bus entry height"));

    let bad_width = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-000000000079")
  (bus_entry (size 2 (bogus)))
)"#;
    let bad_width_path = temp_schematic("bad_bus_entry_width", bad_width);
    let err = parse_schematic_file(Path::new(&bad_width_path))
        .expect_err("must reject bad bus entry width");
    assert!(err.to_string().contains("missing bus entry width"));

    let _ = fs::remove_file(bad_height_path);
    let _ = fs::remove_file(bad_width_path);
}

#[test]
fn junction_no_connect_and_bus_entry_do_not_require_geometry_tokens() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000005a")
  (paper "A4")
  (junction)
  (no_connect)
  (bus_entry)
)"#;
    let path = temp_schematic("default_point_items", src);
    let schematic = parse_schematic_file(Path::new(&path))
        .expect("must accept missing geometry on point-style items");

    let junction = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Junction(junction) => Some(junction),
            _ => None,
        })
        .expect("junction");
    assert_eq!(junction.at, [0.0, 0.0]);

    let no_connect = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::NoConnect(no_connect) => Some(no_connect),
            _ => None,
        })
        .expect("no_connect");
    assert_eq!(no_connect.at, [0.0, 0.0]);
    assert_eq!(no_connect.size, 1.2192);

    let bus_entry = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::BusEntry(bus_entry) => Some(bus_entry),
            _ => None,
        })
        .expect("bus_entry");
    assert_eq!(bus_entry.at, [0.0, 0.0]);
    assert_eq!(bus_entry.size, [2.54, 2.54]);
    assert_eq!(
        bus_entry
            .stroke
            .as_ref()
            .expect("default bus_entry stroke")
            .width,
        Some(0.0)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn wire_and_bus_do_not_require_pts() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000005b")
  (paper "A4")
  (wire)
  (bus)
)"#;
    let path = temp_schematic("default_wire_bus", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept wire/bus without pts");

    let lines: Vec<_> = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Wire(line) | SchItem::Bus(line) => Some(line),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|line| line.points == vec![[0.0, 0.0], [0.0, 0.0]])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn schematic_arc_and_circle_use_upstream_safe_defaults() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000005c")
  (paper "A4")
  (arc)
  (circle)
)"#;
    let path = temp_schematic("default_arc_circle", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept missing arc/circle geometry");

    let arc = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Arc => Some(shape),
            _ => None,
        })
        .expect("arc");
    assert_eq!(arc.points, vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]]);

    let circle = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Circle => Some(shape),
            _ => None,
        })
        .expect("circle");
    assert_eq!(circle.points, vec![[0.0, 0.0]]);
    assert_eq!(circle.radius, Some(0.0));

    let _ = fs::remove_file(path);
}

#[test]
fn schematic_shape_outline_fill_does_not_retroactively_use_later_stroke_color() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000005d")
  (paper "A4")
  (circle
    (fill (type outline))
    (stroke (width 0.2) (color 10 20 30 0.5))))
"#;
    let path = temp_schematic("schematic_shape_fill_before_stroke", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let circle = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Circle => Some(shape),
            _ => None,
        })
        .expect("circle");

    assert_eq!(
        circle.fill.as_ref().map(|fill| fill.fill_type.clone()),
        Some(FillType::Color)
    );
    assert_eq!(circle.fill.as_ref().and_then(|fill| fill.color), None);
    assert_eq!(
        circle.stroke.as_ref().and_then(|stroke| stroke.color),
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 0.5])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn rule_area_does_not_require_three_points() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000005e")
  (paper "A4")
  (rule_area (polyline (pts (xy 0 0) (xy 1 0))))
)"#;
    let path = temp_schematic("rule_area_two_points", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept short rule_area polyline");
    let rule_area = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::RuleArea => Some(shape),
            _ => None,
        })
        .expect("rule area");
    assert_eq!(rule_area.points, vec![[0.0, 0.0], [1.0, 0.0]]);
    let _ = fs::remove_file(path);
}

#[test]
fn rectangle_uses_upstream_safe_defaults() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000005f")
  (paper "A4")
  (rectangle)
)"#;
    let path = temp_schematic("default_rectangle", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must accept rectangle without start/end");
    let rectangle = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Rectangle => Some(shape),
            _ => None,
        })
        .expect("rectangle");
    assert_eq!(rectangle.points, vec![[0.0, 0.0], [0.0, 0.0]]);
    let _ = fs::remove_file(path);
}

#[test]
fn schematic_bezier_follows_upstream_control_point_rules() {
    let short_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000060")
  (paper "A4")
  (bezier (pts (xy 0 0) (xy 1 1)))
)"#;
    let short_path = temp_schematic("short_schematic_bezier", short_src);
    let schematic = parse_schematic_file(Path::new(&short_path))
        .expect("must accept missing bezier control points");
    let bezier = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Shape(shape) if shape.kind == ShapeKind::Bezier => Some(shape),
            _ => None,
        })
        .expect("bezier");
    assert_eq!(
        bezier.points,
        vec![[0.0, 0.0], [1.0, 1.0], [0.0, 0.0], [0.0, 0.0]]
    );
    let _ = fs::remove_file(short_path);

    let extra_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000061")
  (paper "A4")
  (bezier (pts (xy 0 0) (xy 1 1) (xy 2 2) (xy 3 3) (xy 4 4)))
)"#;
    let extra_path = temp_schematic("extra_schematic_bezier", extra_src);
    let err = parse_schematic_file(Path::new(&extra_path))
        .expect_err("must reject extra bezier control point");
    assert!(err.to_string().contains("unexpected control point"));
    let _ = fs::remove_file(extra_path);
}

#[test]
fn parses_stroke_and_fill_payload_details() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000062")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (width 0.15) (type dash_dot) (color 10 20 30 0.5)) (uuid "60000000-0000-0000-0000-000000000063"))
  (text_box "body" (at 10 20 90) (size 3 4) (stroke (width 0.2) (type dot)) (fill (type color) (color 1 2 3 0.25)) (uuid "60000000-0000-0000-0000-000000000064"))
)"#;
    let path = temp_schematic("stroke_fill_payload", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let wire = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Wire(line) => Some(line),
            _ => None,
        })
        .expect("wire");
    let stroke = wire.stroke.as_ref().expect("wire stroke");
    assert_eq!(stroke.width, Some(0.15));
    assert_eq!(stroke.style, StrokeStyle::DashDot);
    assert_eq!(
        stroke.color,
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 0.5])
    );

    let text_box = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::TextBox(text_box) => Some(text_box),
            _ => None,
        })
        .expect("text box");
    assert_eq!(
        text_box.stroke.as_ref().map(|stroke| stroke.style.clone()),
        Some(StrokeStyle::Dot)
    );
    assert_eq!(
        text_box.fill.as_ref().map(|fill| fill.fill_type.clone()),
        Some(FillType::Color)
    );
    assert_eq!(
        text_box.fill.as_ref().and_then(|fill| fill.color),
        Some([1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0, 0.25])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_schematic_polyline_defaults_to_dash_stroke() {
    let src = r#"(kicad_sch
  (version 20211123)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000065")
  (paper "A4")
  (polyline (pts (xy 0 0) (xy 1 1)) (stroke (width 0.15)))
)"#;
    let path = temp_schematic("legacy_polyline_default_dash", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let line = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Polyline(line) => Some(line),
            _ => None,
        })
        .expect("polyline");
    assert_eq!(
        line.stroke.as_ref().map(|stroke| stroke.style.clone()),
        Some(StrokeStyle::Dash)
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unknown_stroke_children() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000066")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (bogus 1)) (uuid "60000000-0000-0000-0000-000000000067"))
)"#;
    let path = temp_schematic("bad_stroke_child", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad stroke child");
    assert!(err.to_string().contains("expecting width, type or color"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unknown_children_in_strict_item_parsers() {
    let bad_no_connect = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000068")
  (paper "A4")
  (no_connect (at 1 2) (bogus yes))
)"#;
    let no_connect_path = temp_schematic("bad_no_connect_child", bad_no_connect);
    let err = parse_schematic_file(Path::new(&no_connect_path))
        .expect_err("must reject bad no_connect child");
    assert!(err.to_string().contains("expecting at or uuid"));
    let _ = fs::remove_file(no_connect_path);

    let bad_symbol = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000069")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0) (bogus yes))
)"#;
    let symbol_path = temp_schematic("bad_symbol_child", bad_symbol);
    let err =
        parse_schematic_file(Path::new(&symbol_path)).expect_err("must reject bad symbol child");
    assert!(err.to_string().contains(
        "expecting lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances"
    ));
    let _ = fs::remove_file(symbol_path);
}

#[test]
fn parses_symbol_in_pos_files_and_validates_library_ids() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000006a")
  (paper "A4")
  (lib_symbols (symbol "Device:R"))
  (symbol (lib_id "Device:R") (at 1 2 0) (in_pos_files no))
)"#;
    let path = temp_schematic("symbol_in_pos_files", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert!(!symbol.in_pos_files);
    let _ = fs::remove_file(path);

    let bad_lib_id = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000006b")
  (paper "A4")
  (symbol (lib_id "Device:R:bad") (at 1 2 0))
)"#;
    let bad_path = temp_schematic("bad_symbol_lib_id", bad_lib_id);
    let err = parse_schematic_file(Path::new(&bad_path)).expect_err("must reject malformed lib_id");
    assert!(err.to_string().contains("contains invalid character ':'"));
    let _ = fs::remove_file(bad_path);

    let empty_item_lib_id = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000006c")
  (paper "A4")
  (symbol (lib_id "Device:") (at 1 2 0))
)"#;
    let empty_item_path = temp_schematic("bad_symbol_lib_id_empty_item", empty_item_lib_id);
    let err = parse_schematic_file(Path::new(&empty_item_path))
        .expect_err("must reject lib_id with empty item name");
    assert!(err.to_string().contains("Invalid symbol library ID"));
    let _ = fs::remove_file(empty_item_path);

    let empty_nickname_lib_id = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000006d")
  (paper "A4")
  (symbol (lib_id ":R") (at 1 2 0))
)"#;
    let empty_nickname_path = temp_schematic("symbol_lib_id_empty_nickname", empty_nickname_lib_id);
    let schematic = parse_schematic_file(Path::new(&empty_nickname_path))
        .expect("must accept lib_id with empty library nickname like KiCad");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.lib_id, ":R");
    let _ = fs::remove_file(empty_nickname_path);

    let invalid_token_lib_id = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000006e")
  (paper "A4")
  (symbol (lib_id (bogus)) (at 1 2 0))
)"#;
    let quoted_path = temp_schematic("bad_symbol_lib_id_token", invalid_token_lib_id);
    let err = parse_schematic_file(Path::new(&quoted_path))
        .expect_err("must reject invalid lib_id token");
    assert!(err.to_string().contains("expecting symbol|number"));
    let _ = fs::remove_file(quoted_path);

    let bad_lib_name = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000006f")
  (paper "A4")
  (symbol (lib_id "Device:R") (lib_name (bogus)) (at 1 2 0))
)"#;
    let bad_lib_name_path = temp_schematic("bad_symbol_lib_name", bad_lib_name);
    let err =
        parse_schematic_file(Path::new(&bad_lib_name_path)).expect_err("must reject bad lib_name");
    assert!(err.to_string().contains("Invalid symbol library name"));
    let _ = fs::remove_file(bad_lib_name_path);

    let defaults_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000070")
  (paper "A4")
  (symbol)
)"#;
    let defaults_path = temp_schematic("symbol_without_lib_id_or_at", defaults_src);
    let schematic = parse_schematic_file(Path::new(&defaults_path))
        .expect("must accept symbol without lib_id or at");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.lib_id, "");
    assert_eq!(symbol.at, [0.0, 0.0]);
    assert_eq!(symbol.angle, 0.0);
    let _ = fs::remove_file(defaults_path);
}

#[test]
fn omits_redundant_symbol_lib_name_but_keeps_overrides() {
    let redundant_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000071")
  (paper "A4")
  (symbol (lib_id "Device:R") (lib_name "Device:R") (at 1 2 0))
)"#;
    let redundant_path = temp_schematic("redundant_symbol_lib_name", redundant_src);
    let schematic = parse_schematic_file(Path::new(&redundant_path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.lib_name, None);
    let _ = fs::remove_file(redundant_path);

    let override_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000072")
  (paper "A4")
  (symbol (lib_id "Device:R") (lib_name "Local{slash}R") (at 1 2 0))
)"#;
    let override_path = temp_schematic("override_symbol_lib_name", override_src);
    let schematic = parse_schematic_file(Path::new(&override_path)).expect("must parse");
    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(symbol.lib_name.as_deref(), Some("Local/R"));
    let _ = fs::remove_file(override_path);
}

#[test]
fn links_symbols_to_local_lib_symbols_and_hydrates_embedded_files() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000073")
  (paper "A4")
  (embedded_files (file (name "shared.bin") (checksum "sha256:123") (type font) (data |abc123|)))
  (lib_symbols
    (symbol "Local:R"
      (embedded_files (file (name "shared.bin")))))
  (symbol (lib_id "Device:R") (lib_name "Local:R") (at 1 2 0))
)"#;
    let path = temp_schematic("linked_local_lib_symbol", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(
        lib_symbol.embedded_files[0].checksum.as_deref(),
        Some("sha256:123")
    );
    assert_eq!(
        lib_symbol.embedded_files[0].file_type,
        Some(EmbeddedFileType::Font)
    );
    assert_eq!(lib_symbol.embedded_files[0].data.as_deref(), Some("abc123"));

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    let linked = symbol.lib_symbol.as_ref().expect("linked local lib symbol");
    assert_eq!(
        linked.embedded_files[0].checksum.as_deref(),
        Some("sha256:123")
    );
    assert_eq!(
        linked.embedded_files[0].file_type,
        Some(EmbeddedFileType::Font)
    );
    assert_eq!(linked.embedded_files[0].data.as_deref(), Some("abc123"));

    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_for_invalid_lib_symbol_embedded_files() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000074")
  (paper "A4")
  (lib_symbols
    (symbol "Local:R"
      (embedded_files (file (name "shared.bin") (bogus "x")))))
)"#;
    let path = temp_schematic("invalid_lib_embedded_files", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must keep loading");
    assert_eq!(schematic.screen.parse_warnings.len(), 1);
    assert!(schematic.screen.parse_warnings[0].contains("expecting checksum, data or name"));
    assert!(schematic.screen.lib_symbols[0].embedded_files.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn accumulates_repeated_lib_symbol_embedded_files_sections() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000075")
  (paper "A4")
  (lib_symbols
    (symbol "Local:R"
      (embedded_files (file (name "A.bin") (data |aaa|)))
      (embedded_files (file (name "B.bin") (data |bbb|))))))
"#;
    let path = temp_schematic("repeated_lib_symbol_embedded_files", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    assert_eq!(schematic.screen.lib_symbols[0].embedded_files.len(), 2);
    assert_eq!(
        schematic.screen.lib_symbols[0].embedded_files[0]
            .name
            .as_deref(),
        Some("A.bin")
    );
    assert_eq!(
        schematic.screen.lib_symbols[0].embedded_files[1]
            .name
            .as_deref(),
        Some("B.bin")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_and_skips_invalid_lib_symbol_block() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000076")
  (paper "A4")
  (lib_symbols
    (symbol "Bad:R"
      (symbol "R_1_1"
        (pin input line
          (at 0 0 45))))
    (symbol "Good:R"))
)"#;
    let path = temp_schematic("invalid_lib_symbol_block", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject malformed lib symbol");
    assert!(err.to_string().contains("expecting 0, 90, 180, or 270"));
    let _ = fs::remove_file(path);
}

#[test]
fn resolves_groups_after_items_and_drops_unknown_members() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000085")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (uuid "11111111-1111-1111-1111-111111111111"))
  (group "G1" (uuid "22222222-2222-2222-2222-222222222222") (members "11111111-1111-1111-1111-111111111111" "missing"))
  (group "G2" (uuid "33333333-3333-3333-3333-333333333333") (members "22222222-2222-2222-2222-222222222222" "missing-2"))
)"#;
    let path = temp_schematic("resolved_groups", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let groups: Vec<&Group> = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Group(group) => Some(group),
            _ => None,
        })
        .collect();

    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0].members,
        vec!["11111111-1111-1111-1111-111111111111"]
    );
    assert_eq!(
        groups[1].members,
        vec!["22222222-2222-2222-2222-222222222222"]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn group_members_follow_shared_uuid_normalization() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000007a")
  (wire
    (pts (xy 0 0) (xy 10 0))
    (uuid "1"))
  (group "G1" (uuid "63000000-0000-0000-0000-00000000007c") (members "1"))
)"#;
    let path = temp_schematic("group_member_uuid_normalization", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let group = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Group(group) => Some(group),
            _ => None,
        })
        .expect("group");

    assert_eq!(
        group.members,
        vec!["00000000-0000-0000-0000-000000000001".to_string()]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn group_members_accept_number_tokens_and_drop_unknown_entries_later() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000007b")
  (wire (pts (xy 0 0) (xy 1 0)) (uuid "11111111-1111-1111-1111-111111111111"))
  (group "G1" (uuid "22222222-2222-2222-2222-222222222222") (members 123 "11111111-1111-1111-1111-111111111111"))
)"#;
    let path = temp_schematic("group_number_member", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let group = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Group(group) => Some(group),
            _ => None,
        })
        .expect("group");

    assert_eq!(group.members, vec!["11111111-1111-1111-1111-111111111111"]);

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_duplicate_user_properties_follow_upstream_renaming() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000077")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Field" "A")
      (property "Field" "B")
      (property "Field" "C")
      (property "Reference" "U")
      (property "Reference" "J"))))"#;
    let path = temp_schematic("lib_symbol_duplicate_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];

    assert_eq!(lib_symbol.properties.len(), 5);
    assert_eq!(
        lib_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.as_str()),
        Some("J")
    );
    let extra_fields = lib_symbol.units[0]
        .draw_items
        .iter()
        .filter(|item| {
            item.kind == "field"
                && matches!(item.name.as_deref(), Some("Field" | "Field_1" | "Field_2"))
        })
        .collect::<Vec<_>>();
    assert_eq!(extra_fields.len(), 3);
    assert_eq!(extra_fields[0].name.as_deref(), Some("Field"));
    assert_eq!(extra_fields[0].text.as_deref(), Some("A"));
    assert_eq!(extra_fields[1].name.as_deref(), Some("Field_1"));
    assert_eq!(extra_fields[1].text.as_deref(), Some("B"));
    assert_eq!(extra_fields[2].name.as_deref(), Some("Field_2"));
    assert_eq!(extra_fields[2].text.as_deref(), Some("C"));

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_private_is_preserved_on_mandatory_and_user_fields() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000078")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property private "Reference" "U")
      (property private "UserField" "X"))))"#;
    let path = temp_schematic("lib_symbol_private_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];

    assert_eq!(lib_symbol.properties.len(), 5);
    assert!(
        lib_symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .expect("reference field")
            .is_private
    );
    assert!(
        lib_symbol.units[0]
            .draw_items
            .iter()
            .find(|item| item.kind == "field" && item.name.as_deref() == Some("UserField"))
            .expect("user field")
            .is_private
    );

    let _ = fs::remove_file(path);
}

#[test]
fn hidden_lib_text_converts_to_named_user_field() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000007c")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (text "A" (at 1 2 90) (effects (font (size 1 1)) (hide)))
      (text "B" (at 3 4 90) (effects (font (size 1 1)) (hide)))))
)"#;
    let path = temp_schematic("hidden_lib_text_named_field", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let unit = &schematic.screen.lib_symbols[0].units[0];
    let extra_fields = unit
        .draw_items
        .iter()
        .filter(|item| item.kind == "field")
        .collect::<Vec<_>>();

    assert_eq!(extra_fields.len(), 2);
    assert_eq!(extra_fields[0].name.as_deref(), Some("Field"));
    assert_eq!(extra_fields[0].text.as_deref(), Some("A"));
    assert_eq!(extra_fields[0].field_id, Some(0));
    assert_eq!(extra_fields[0].field_ordinal, Some(42));
    assert_eq!(extra_fields[1].name.as_deref(), Some("Field"));
    assert_eq!(extra_fields[1].text.as_deref(), Some("B"));
    assert_eq!(extra_fields[1].field_id, Some(0));
    assert_eq!(extra_fields[1].field_ordinal, Some(43));

    let _ = fs::remove_file(path);
}

#[test]
fn lib_property_name_collision_checks_all_symbol_units() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "63000000-0000-0000-0000-00000000007d")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_2_1"
        (text "HIDDEN" (at 1 2 90) (effects (font (size 1 1)) (hide))))
      (property "Field" "PROP"))))
"#;
    let path = temp_schematic("lib_property_nested_unit_field_collision", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];
    let property_field = lib_symbol.units[0]
        .draw_items
        .iter()
        .find(|item| item.kind == "field" && item.text.as_deref() == Some("PROP"))
        .expect("property field");

    assert_eq!(property_field.name.as_deref(), Some("Field_1"));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_pin_alternate_type_and_shape() {
    let bad_alt_type = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000086")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "A")
          (number "1")
          (alternate "ALT" bogus line)))))
)"#;
    let bad_type_path = temp_schematic("bad_lib_pin_alt_type", bad_alt_type);
    let err = parse_schematic_file(Path::new(&bad_type_path))
        .expect_err("must reject malformed lib symbol");
    assert!(
        err.to_string()
            .contains("expecting input, output, bidirectional")
    );
    let _ = fs::remove_file(bad_type_path);

    let bad_alt_shape = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000087")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "A")
          (number "1")
          (alternate "ALT" input bogus)))))
)"#;
    let bad_shape_path = temp_schematic("bad_lib_pin_alt_shape", bad_alt_shape);
    let err = parse_schematic_file(Path::new(&bad_shape_path))
        .expect_err("must reject malformed lib symbol");
    assert!(err.to_string().contains("expecting line, inverted, clock"));
    let _ = fs::remove_file(bad_shape_path);
}

#[test]
fn lib_symbol_arc_and_bezier_follow_upstream_token_sets() {
    let legacy_arc_src = r#"(kicad_sch
  (version 20230121)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000088")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (arc
          (start 0 0)
          (end 2 0)
          (radius
            (at 1 1)
            (length 1.5)
            (angles 0 90))
          (stroke (width 0.1))))))
)"#;
    let legacy_arc_path = temp_schematic("lib_arc_legacy_radius", legacy_arc_src);
    let schematic = parse_schematic_file(Path::new(&legacy_arc_path)).expect("must parse");
    let item = &schematic.screen.lib_symbols[0].units[0].draw_items[0];
    assert_eq!(item.kind, "arc");
    assert_eq!(item.points, vec![[2.0, 0.0], [0.0, 0.0]]);
    assert_eq!(item.arc_center, Some([1.0, -1.0]));
    assert_eq!(item.radius, Some(1.5));
    assert_eq!(item.arc_start_angle, Some(0.0));
    assert_eq!(item.arc_end_angle, Some(90.0));
    let _ = fs::remove_file(legacy_arc_path);

    let default_arc_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000089")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (arc)))) 
)"#;
    let default_arc_path = temp_schematic("lib_arc_default_geometry", default_arc_src);
    let schematic = parse_schematic_file(Path::new(&default_arc_path)).expect("must parse");
    let item = &schematic.screen.lib_symbols[0].units[0].draw_items[0];
    assert_eq!(item.kind, "arc");
    assert_eq!(item.points, vec![[1.0, 0.0], [0.0, 1.0]]);
    assert_eq!(item.arc_center, Some([0.0, 0.0]));
    assert_eq!(item.arc_start_angle, Some(0.0));
    assert_eq!(item.arc_end_angle, Some(90.0));
    let _ = fs::remove_file(default_arc_path);

    let bad_arc_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000008a")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (arc (center 0 0)))))
)"#;
    let bad_arc_path = temp_schematic("lib_arc_bad_token", bad_arc_src);
    let err = parse_schematic_file(Path::new(&bad_arc_path))
        .expect_err("must reject malformed lib symbol");
    assert!(
        err.to_string()
            .contains("expecting start, mid, end, radius, stroke, or fill")
    );
    let _ = fs::remove_file(bad_arc_path);

    let bad_bezier_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000008b")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (bezier (pts (xy 0 0) (xy 1 1) (xy 2 2) (xy 3 3) (xy 4 4))))))
)"#;
    let bad_bezier_path = temp_schematic("lib_bezier_too_many_points", bad_bezier_src);
    let err = parse_schematic_file(Path::new(&bad_bezier_path))
        .expect_err("must reject malformed lib symbol");
    assert!(err.to_string().contains("unexpected control point"));
    let _ = fs::remove_file(bad_bezier_path);

    let quoted_bezier_xy_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000008c")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (bezier (pts ("xy" 0 0) (xy 1 1) (xy 2 2) (xy 3 3))))))
)"#;
    let quoted_bezier_xy_path = temp_schematic("lib_bezier_quoted_xy", quoted_bezier_xy_src);
    let err = parse_schematic_file(Path::new(&quoted_bezier_xy_path))
        .expect_err("must reject malformed lib symbol");
    assert!(err.to_string().contains("expecting xy"));
    let _ = fs::remove_file(quoted_bezier_xy_path);
}

#[test]
fn lib_symbol_arc_and_circle_inherit_upstream_safe_defaults() {
    let src = r#"(kicad_sch
  (version 20230121)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000008d")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_1"
        (arc (radius (angles 0 90)))
        (circle))))
)"#;
    let path = temp_schematic("lib_shape_defaults", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let unit = &schematic.screen.lib_symbols[0].units[0];

    let arc = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "arc")
        .expect("arc");
    assert_eq!(arc.kind, "arc");
    assert_eq!(arc.points, vec![[0.0, 1.0], [1.0, 0.0]]);
    assert_eq!(arc.arc_center, Some([0.0, 0.0]));
    assert_eq!(arc.arc_start_angle, Some(0.0));
    assert_eq!(arc.arc_end_angle, Some(90.0));
    assert_eq!(
        arc.stroke.as_ref().expect("lib arc stroke").width,
        Some(0.0)
    );
    assert_eq!(
        arc.fill.as_ref().expect("lib arc fill").fill_type,
        FillType::None
    );

    let circle = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "circle")
        .expect("circle");
    assert_eq!(circle.kind, "circle");
    assert_eq!(circle.points, vec![[0.0, 0.0]]);
    assert_eq!(circle.radius, Some(1.0));
    assert_eq!(
        circle.stroke.as_ref().expect("lib circle stroke").width,
        Some(0.0)
    );
    assert_eq!(
        circle.fill.as_ref().expect("lib circle fill").fill_type,
        FillType::None
    );

    let _ = fs::remove_file(path);
}

#[test]
fn library_coordinates_use_kicad_inverted_y_axis() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000008e")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (at 1 2 0))
      (symbol "R_1_1"
        (polyline (pts (xy 1 2) (xy 3 4)))
        (rectangle (start 5 6) (end 7 8))
        (text "TXT" (at 9 10 90) (effects (font (size 1 1))))
        (text_box "TB" (at 11 12 0) (size 3 4))
        (pin input line (at 13 14 180) (name "P") (number "1")))))
)"#;
    let path = temp_schematic("lib_inverted_y_coordinates", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];

    let reference = lib_symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolReference)
        .expect("reference property");
    assert_eq!(reference.at, Some([1.0, -2.0]));

    let unit = &lib_symbol.units[0];
    let polyline = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "polyline")
        .expect("polyline");
    assert_eq!(polyline.points, vec![[1.0, -2.0], [3.0, -4.0]]);

    let rectangle = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "rectangle")
        .expect("rectangle");
    assert_eq!(rectangle.points, vec![[5.0, -6.0]]);
    assert_eq!(rectangle.end, Some([7.0, -8.0]));

    let text = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "text")
        .expect("text");
    assert_eq!(text.at, Some([9.0, -10.0]));
    assert_eq!(text.angle, Some(9.0));

    let text_box = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "text_box")
        .expect("text_box");
    assert_eq!(text_box.at, Some([11.0, -12.0]));
    assert_eq!(text_box.end, Some([14.0, -16.0]));

    let pin = unit
        .draw_items
        .iter()
        .find(|item| item.kind == "pin")
        .expect("pin");
    assert_eq!(pin.at, Some([13.0, -14.0]));
    assert_eq!(pin.angle, Some(180.0));

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_lib_symbols_infer_demorgan_body_style_from_units() {
    let legacy_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-00000000008f")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1)))))))
)"#;
    let legacy_path = temp_schematic("legacy_demorgan_inferred", legacy_src);
    let schematic = parse_schematic_file(Path::new(&legacy_path)).expect("must parse");
    assert!(schematic.screen.lib_symbols[0].has_demorgan);
    let _ = fs::remove_file(legacy_path);

    let modern_src = r#"(kicad_sch
  (version 20250827)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000090")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "R_1_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1)))))))
)"#;
    let modern_path = temp_schematic("modern_demorgan_not_inferred", modern_src);
    let schematic = parse_schematic_file(Path::new(&modern_path)).expect("must parse");
    assert!(!schematic.screen.lib_symbols[0].has_demorgan);
    let _ = fs::remove_file(modern_path);
}

#[test]
fn legacy_derived_lib_symbols_inherit_demorgan_state_from_root() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000091")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (symbol "R_1_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1))))))
    (symbol "Child:R"
      (extends "Root:R"))))
"#;
    let path = temp_schematic("legacy_derived_demorgan", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert!(schematic.screen.lib_symbols[0].has_demorgan);
    assert!(schematic.screen.lib_symbols[1].has_demorgan);
    let _ = fs::remove_file(path);
}

#[test]
fn mandatory_properties_keep_default_kicad_field_ids() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000092")
  (paper "A4")
  (global_label "GL" (at 1 2 0) (shape input) (iref 3 4))
  (sheet
    (at 10 20)
    (size 30 40)
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (symbol
    (lib_id "Device:R")
    (at 5 6 0)
    (property "Reference" "R1" (id 0))
    (property "Value" "10k")
    (property "Footprint" "Resistor_SMD:R_0603_1608Metric")
    (property "Datasheet" "ds")))
"#;
    let path = temp_schematic("mandatory_property_ids", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let irefs = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("iref property");
    assert_eq!(irefs.id, Some(6));

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetName)
            .and_then(|property| property.id),
        Some(7)
    );
    assert_eq!(
        sheet
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetFile)
            .and_then(|property| property.id),
        Some(8)
    );

    let symbol = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .expect("symbol");
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .and_then(|property| property.id),
        Some(1)
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .and_then(|property| property.id),
        Some(2)
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .and_then(|property| property.id),
        Some(3)
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolDatasheet)
            .and_then(|property| property.id),
        Some(4)
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolDescription)
            .and_then(|property| property.id),
        Some(5)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn sheetfile_properties_normalize_to_forward_slashes() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000093")
  (paper "A4")
  (sheet
    (at 10 20)
    (size 30 40)
    (property "Sheetname" "Child")
    (property "Sheetfile" "dir\\child.kicad_sch")))"#;
    let path = temp_schematic("sheetfile_forward_slashes", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    assert_eq!(sheet.filename().as_deref(), Some("dir/child.kicad_sch"));

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_preserves_duplicate_mandatory_properties_in_parse_order() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000094")
  (paper "A4")
  (sheet
    (at 10 20)
    (size 30 40)
    (property "Sheetname" "First")
    (property "Sheetfile" "first.kicad_sch")
    (property "Sheetname" "Second")
    (property "Sheetfile" "second.kicad_sch")))"#;
    let path = temp_schematic("sheet_duplicate_mandatory_properties", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let sheet = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Sheet(sheet) => Some(sheet),
            _ => None,
        })
        .expect("sheet");

    assert_eq!(sheet.name(), Some("First"));
    assert_eq!(sheet.filename().as_deref(), Some("first.kicad_sch"));
    assert_eq!(
        sheet
            .properties
            .iter()
            .filter(|property| property.kind == PropertyKind::SheetName)
            .count(),
        2
    );
    assert_eq!(
        sheet
            .properties
            .iter()
            .filter(|property| property.kind == PropertyKind::SheetFile)
            .count(),
        2
    );

    let _ = fs::remove_file(path);
}

#[test]
fn global_label_iref_preserves_existing_intersheet_property_text() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "60000000-0000-0000-0000-000000000095")
  (paper "A4")
  (global_label "GL"
    (at 1 2 0)
    (shape input)
    (property "Intersheet References" "keep-me" (at 8 9 0) (hide yes))
    (iref 3 4)))
"#;
    let path = temp_schematic("global_label_iref_preserves_property", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");

    let global = schematic
        .screen
        .items
        .iter()
        .find_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
            _ => None,
        })
        .expect("global label");
    let property = global
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
        .expect("iref property");

    assert_eq!(property.value, "keep-me");
    assert_eq!(property.at, Some([3.0, 4.0]));
    assert!(property.visible);

    let _ = fs::remove_file(path);
}
