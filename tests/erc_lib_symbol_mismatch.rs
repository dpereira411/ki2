use std::path::PathBuf;
use std::process::Command;

fn ki2_binary() -> &'static str {
    env!("CARGO_BIN_EXE_ki2")
}

#[test]
fn cli_erc_reports_top_level_hier_gnd_library_mismatches() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../ki/tests/fixtures/erc_upstream_qa/projects/netlists/top_level_hier_pins/top_level_hier_pins.kicad_sch",
    );
    let report_path = path.with_file_name("top_level_hier_pins-erc.json");

    if !path.exists() {
        return;
    }

    let output = Command::new(ki2_binary())
        .args(["erc", path.to_str().expect("utf-8 fixture path"), "--format", "json"])
        .output()
        .expect("run ki2 erc");

    assert!(output.status.success(), "{output:#?}");

    let report = std::fs::read_to_string(&report_path).expect("erc json report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("json report");
    let mismatches = json["violations"]
        .as_array()
        .expect("violations array")
        .iter()
        .filter(|violation| violation["code"] == "erc-lib-symbol-mismatch")
        .collect::<Vec<_>>();

    assert_eq!(mismatches.len(), 2, "{mismatches:#?}");
    assert!(mismatches.iter().all(|violation| {
        violation["severity"] == "Error"
            && violation["message"] == "Symbol 'GND' doesn't match copy in library 'power'"
    }));

    let _ = std::fs::remove_file(report_path);
}
