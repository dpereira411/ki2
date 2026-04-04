use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use ki2::core::SchematicProject;
use ki2::error::Error;
use ki2::loader::load_schematic_tree;
use ki2::model::{
    FieldAutoplacement, FillType, Group, LabelKind, LabelSpin, LineKind, MirrorAxis, PropertyKind,
    SchItem, ShapeKind, SheetPinShape, SheetSide, StrokeStyle, TextHJustify, TextKind,
    TextVJustify,
};
use ki2::parser::parse_schematic_file;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(
        "/Users/Daniel/Desktop/modular/tools/kiutils-rs/crates/kiutils_kicad/tests/fixtures",
    )
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
  (uuid "root-bool-embedded-fonts")
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
  (uuid "root-bool-symbol")
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
  (uuid "root-bool-bold")
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
fn rejects_quoted_number_tokens_in_numeric_fields() {
    let quoted_comment_number = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-comment")
  (title_block (comment "1" "note"))
)"#;
    let quoted_comment_number_path = temp_schematic("quoted_comment_number", quoted_comment_number);
    let err = parse_schematic_file(Path::new(&quoted_comment_number_path))
        .expect_err("must reject quoted comment number");
    assert!(err.to_string().contains("missing comment"));

    let quoted_symbol_angle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-angle")
  (symbol (lib_id "Device:R") (at 1 2 "90"))
)"#;
    let quoted_symbol_angle_path = temp_schematic("quoted_symbol_angle", quoted_symbol_angle);
    let err = parse_schematic_file(Path::new(&quoted_symbol_angle_path))
        .expect_err("must reject quoted numeric angle");
    assert!(err.to_string().contains("missing symbol at angle"));

    let quoted_text_box_size = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-textbox-size")
  (text_box "body" (size "3" 4))
)"#;
    let quoted_text_box_size_path =
        temp_schematic("quoted_text_box_size_number", quoted_text_box_size);
    let err = parse_schematic_file(Path::new(&quoted_text_box_size_path))
        .expect_err("must reject quoted textbox size number");
    assert!(err.to_string().contains("missing text_box size x"));

    let _ = fs::remove_file(quoted_comment_number_path);
    let _ = fs::remove_file(quoted_symbol_angle_path);
    let _ = fs::remove_file(quoted_text_box_size_path);
}

#[test]
fn rejects_non_integer_rgb_color_channels() {
    let decimal_junction_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-decimal-junction-color")
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
  (uuid "root-decimal-effects-color")
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
    assert_eq!(loaded.links[0].sheet_uuid.as_deref(), Some("sheet-uuid-1"));
    assert_eq!(loaded.links[0].sheet_name.as_deref(), Some("PowerSection"));

    let root = &loaded.schematics[0];
    assert_eq!(root.root_sheet.uuid.as_deref(), Some("root-sch-uuid-1234"));
    assert_eq!(root.screen.uuid.as_deref(), Some("root-sch-uuid-1234"));
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
    assert_eq!(root.root_sheet.uuid.as_deref(), Some("root-sch-uuid-1234"));
    assert_eq!(project.children_of(&project.root_path).count(), 1);
    let child_link = project
        .children_of(&project.root_path)
        .next()
        .expect("child link");
    assert_eq!(project.parents_of(&child_link.child_path).count(), 1);
}

#[test]
fn reuses_previously_loaded_child_schematic() {
    let child_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "child-u")
  (paper "A4")
)"#;
    let root_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-u")
  (paper "A4")
  (sheet (at 0 0) (size 10 10) (uuid "sheet-a")
    (property "Sheetname" "A")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet (at 20 0) (size 10 10) (uuid "sheet-b")
    (property "Sheetname" "B")
    (property "Sheetfile" "child.kicad_sch"))
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
  (uuid "child-root")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "sym-u")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "root-u")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "sheet-a")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/sheet-a" (page "1")))
  (symbol_instances
    (path "/sheet-a/sym-u"
      (reference "R7")
      (unit 2)
      (value "47k")
      (footprint "Resistor_SMD:R_0603")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");
    assert_eq!(loaded.sheet_paths.len(), 2);
    assert_eq!(loaded.sheet_paths[0].instance_path, "/root-u/sheet-a");
    assert_eq!(loaded.sheet_paths[0].page.as_deref(), Some("1"));
    assert_eq!(loaded.sheet_paths[0].sheet_number, 1);
    assert_eq!(loaded.sheet_paths[0].sheet_count, 2);
    assert_eq!(loaded.sheet_paths[1].instance_path, "");
    assert_eq!(loaded.sheet_paths[1].page.as_deref(), Some("2"));
    assert_eq!(loaded.sheet_paths[1].sheet_number, 2);
    assert_eq!(loaded.sheet_paths[1].sheet_count, 2);

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

    let _ = fs::remove_file(root_path);
    let _ = fs::remove_file(child_path);
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
  (uuid "child-root")
  (paper "A4")
  (global_label "VCC" (shape input) (at 10 10 0))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-u")
  (paper "A4")
  (global_label "VCC" (shape input) (at 1 2 0))
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "sheet-a")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/sheet-a" (page "1")))
)"#;

    fs::write(&root_path, root_src).expect("write root");
    fs::write(&child_path, child_src).expect("write child");

    let loaded = load_schematic_tree(&root_path).expect("load tree");

    for schematic in &loaded.schematics {
        let global = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Label(label) if label.kind == LabelKind::Global => Some(label),
                _ => None,
            })
            .expect("global label");

        assert_eq!(
            global
                .properties
                .iter()
                .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
                .map(|property| property.value.as_str()),
            Some("[1,2]")
        );
    }

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
  (uuid "child-root")
  (paper "A4")
  (symbol
    (lib_id "Device:R")
    (uuid "sym-u")
    (property "Reference" "R?")
    (property "Value" "seed")
    (property "Footprint" "seed-footprint")
    (at 10 10 0)
    (instances
      (project "demo"
        (path "/root-u/sheet-a"
          (reference "R11")
          (unit 3)
          (value "22k")
          (footprint "Resistor_SMD:R_0402")))))
)"#;
    let root_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-u")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 10 10)
    (uuid "sheet-a")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
  (sheet_instances
    (path "" (page "2"))
    (path "/sheet-a" (page "1")))
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
  (uuid "root-u")
  (paper "A4")
  (lib_symbols
    (symbol "power:VCC"
      (power global)
      (property "Value" "OLDLIB")
      (symbol "power:VCC_1_1"
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
    (uuid "sym-u"))
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
  (uuid "root-u")
  (paper "A4")
  (sheet (at 0 0) (size 10 10) (uuid "sheet-a")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch"))
)"#;
    let child_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "child-u")
  (paper "A4")
  (sheet (at 0 0) (size 10 10) (uuid "sheet-b")
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
    assert!(message.contains("unsupported schematic section `future_sch`"));
}

#[test]
fn rejects_invalid_generator_and_legacy_host_tokens() {
    let bad_generator = r#"(kicad_sch
  (version 20260306)
  (generator (bogus))
  (uuid "u-1")
)"#;
    let bad_generator_path = temp_schematic("bad_generator_token", bad_generator);
    let err = parse_schematic_file(Path::new(&bad_generator_path))
        .expect_err("must reject invalid generator token");
    assert!(err.to_string().contains("expecting generator"));

    let bad_host = r#"(kicad_sch
  (version 20200826)
  (host "eeschema" (bogus))
  (uuid "u-1")
)"#;
    let bad_host_path = temp_schematic("bad_legacy_host_version_token", bad_host);
    let err = parse_schematic_file(Path::new(&bad_host_path))
        .expect_err("must reject invalid legacy host version token");
    assert!(err.to_string().contains("expecting host version"));

    let _ = fs::remove_file(bad_generator_path);
    let _ = fs::remove_file(bad_host_path);
}

#[test]
fn rejects_invalid_title_block_comment_number() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (title_block (comment 10 "bad"))
)"#;
    let path = temp_schematic("bad_title_block_comment", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject invalid comment");
    assert!(
        err.to_string()
            .contains("Invalid title block comment number")
    );
    let _ = fs::remove_file(path);

    let valid_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (title_block (comment 9 "ok"))
)"#;
    let valid_path = temp_schematic("title_block_comment_nine", valid_src);
    let schematic =
        parse_schematic_file(Path::new(&valid_path)).expect("must accept comment slot 9");
    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(
        title_block
            .comments
            .iter()
            .find(|(idx, _)| *idx == 9)
            .map(|(_, value)| value.as_str()),
        Some("ok")
    );
    let _ = fs::remove_file(valid_path);

    let numeric_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-2")
  (paper "A4")
  (title_block (title 2026) (rev 7) (comment 1 99))
)"#;
    let numeric_path = temp_schematic("title_block_numeric_values", numeric_src);
    let schematic =
        parse_schematic_file(Path::new(&numeric_path)).expect("must accept numeric title values");
    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.title.as_deref(), Some("2026"));
    assert_eq!(title_block.revision.as_deref(), Some("7"));
    assert_eq!(
        title_block
            .comments
            .iter()
            .find(|(idx, _)| *idx == 1)
            .map(|(_, value)| value.as_str()),
        Some("99")
    );
    let _ = fs::remove_file(numeric_path);
}

#[test]
fn parses_extended_top_level_sections() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (generator_version "9.0")
  (uuid "u-1")
  (paper "A4")
  (title_block (title "Demo") (date "2026-02-25") (rev "A") (company "Acme") (comment 1 "c1"))
  (bus_alias "ADDR" (members "A0" "A1" "A2"))
  (embedded_fonts no)
  (embedded_files (file "A.bin" "abc") (file (name "B.bin") (data "def")))
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
      (symbol "Device:R_1_1"
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
      (embedded_files (file "sym.bin" "xyz"))))
  (text "hello" (at 10 20 0) (uuid "t-1"))
  (text_box "box" (at 0 0 0) (size 5 5) (margins 1 2 3 4) (uuid "tb-1"))
  (table
    (column_count 2)
    (column_widths 10 20)
    (row_heights 30)
    (cells
      (table_cell "c1" (at 0 0 0) (size 5 5))
      (table_cell "c2" (at 5 0 0) (size 5 5) (span 2 1)))
    (border (external yes) (header no) (stroke (width 0.3) (type solid)))
    (separators (rows yes) (cols no) (stroke (width 0.4) (type dash)))
    (uuid "tbl-1"))
  (image (at 1 2) (scale 2.5) (data "ab" "cd") (uuid "img-1"))
  (polyline (pts (xy 0 0) (xy 1 1)) (uuid "pl-1"))
  (arc (start 0 0) (mid 1 1) (end 2 0) (uuid "a-1"))
  (circle (center 0 0) (radius 1) (uuid "c-1"))
  (rectangle (start 0 0) (end 2 2) (uuid "r-1"))
  (bezier (pts (xy 0 0) (xy 1 1) (xy 2 1) (xy 3 0)) (uuid "b-1"))
  (rule_area (polyline (pts (xy 0 0) (xy 2 0) (xy 2 2)) (uuid "ra-1")) (exclude_from_sim yes) (in_bom no) (on_board yes) (dnp no))
  (directive_label "D" (shape input) (at 10 10 0) (uuid "d-1"))
  (netclass_flag "N" (shape input) (at 11 10 0) (uuid "n-1"))
  (group "grp" (uuid "g-1") (lib_id "lib:block") (members "pl-1" "a-1"))
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
    assert_eq!(lib_symbol.name, "Device:R");
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
    assert_eq!(lib_symbol.keywords.as_deref(), Some("analog precision"));
    assert_eq!(
        lib_symbol.description.as_deref(),
        Some("precision resistor")
    );
    assert_eq!(lib_symbol.fp_filters, vec!["R_*", "0603"]);
    assert!(lib_symbol.locked_units);
    assert_eq!(lib_symbol.extends.as_deref(), Some("Device:Base"));
    assert_eq!(lib_symbol.properties.len(), 1);
    assert!(!lib_symbol.properties[0].visible);
    assert_eq!(lib_symbol.units.len(), 2);
    assert_eq!(lib_symbol.units[0].unit_number, 1);
    assert_eq!(lib_symbol.units[0].body_style, 1);
    assert_eq!(lib_symbol.units[0].unit_name, None);
    assert_eq!(lib_symbol.units[0].draw_item_kinds, vec!["circle"]);
    assert_eq!(lib_symbol.units[0].draw_items.len(), 1);
    assert_eq!(lib_symbol.units[0].draw_items[0].kind, "circle");
    assert_eq!(lib_symbol.units[1].unit_number, 1);
    assert_eq!(lib_symbol.units[1].body_style, 1);
    assert_eq!(lib_symbol.units[1].unit_name.as_deref(), Some("Amplifier"));
    assert_eq!(
        lib_symbol.units[1].draw_item_kinds,
        vec!["arc", "text", "text_box", "pin"]
    );
    assert_eq!(lib_symbol.units[1].draw_items.len(), 4);
    assert_eq!(lib_symbol.units[1].draw_items[0].points.len(), 3);
    assert_eq!(
        lib_symbol.units[1].draw_items[0]
            .stroke
            .as_ref()
            .and_then(|stroke| stroke.width),
        Some(0.1)
    );
    assert_eq!(
        lib_symbol.units[1].draw_items[1].text.as_deref(),
        Some("AMP")
    );
    assert_eq!(lib_symbol.units[1].draw_items[1].at, Some([3.0, 4.0]));
    assert_eq!(lib_symbol.units[1].draw_items[1].angle, Some(9.0));
    assert!(lib_symbol.units[1].draw_items[1].converted_to_field);
    assert!(lib_symbol.units[1].draw_items[2].is_private);
    assert_eq!(
        lib_symbol.units[1].draw_items[2].text.as_deref(),
        Some("TB")
    );
    assert_eq!(
        lib_symbol.units[1].draw_items[3].name.as_deref(),
        Some("IN")
    );
    assert_eq!(
        lib_symbol.units[1].draw_items[3].number.as_deref(),
        Some("1")
    );
    assert_eq!(lib_symbol.units[1].draw_items[3].length, Some(2.5));
    assert_eq!(
        lib_symbol.units[1].draw_items[3]
            .name_effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([0.8, 0.9])
    );
    assert_eq!(
        lib_symbol.units[1].draw_items[3]
            .number_effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([1.1, 1.2])
    );
    assert_eq!(lib_symbol.units[1].draw_items[3].alternates.len(), 1);
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
    assert_eq!(table.cells[1].span, Some([2, 1]));
    assert_eq!(
        table.border_stroke.as_ref().and_then(|stroke| stroke.width),
        Some(0.3)
    );
    assert_eq!(
        table
            .separators_stroke
            .as_ref()
            .map(|stroke| stroke.style.clone()),
        Some(StrokeStyle::Dash)
    );
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
    assert_eq!(image.data.as_deref(), Some("abcd"));
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
    assert!(schematic.screen.items.iter().any(
        |item| matches!(item, SchItem::Label(label) if label.kind == LabelKind::NetclassFlag)
    ));
    assert!(schematic.screen.items.iter().any(
        |item| matches!(item, SchItem::Group(Group { name, uuid, .. }) if name.as_deref() == Some("grp") && uuid.as_deref() == Some("g-1"))
    ));

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_text_uses_decidegree_angles_like_upstream() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
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
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (text_box "TB" (start 1 2) (end 4 6) (effects (font (size 1 1)))))))
)"#;
    let legacy_path = temp_schematic("lib_text_box_legacy", legacy_src);
    let schematic = parse_schematic_file(Path::new(&legacy_path)).expect("must parse");
    let item = &schematic.screen.lib_symbols[0].units[0].draw_items[0];
    assert_eq!(item.kind, "text_box");
    assert_eq!(item.at, Some([1.0, 2.0]));
    assert_eq!(item.end, Some([4.0, 6.0]));
    let _ = fs::remove_file(legacy_path);

    let bad_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (text_box "TB" (at 1 2 0) (size 3 4) (uuid "tb")))))
)"#;
    let bad_path = temp_schematic("lib_text_box_bad_uuid", bad_src);
    let schematic = parse_schematic_file(Path::new(&bad_path))
        .expect("must warn and skip malformed lib symbol");
    assert!(
        schematic.screen.parse_warnings[0].contains("expecting at, size, stroke, fill or effects")
    );
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(bad_path);
}

#[test]
fn rejects_invalid_lib_pin_orientation() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (pin input line (at 1 2 45)))))
)"#;
    let path = temp_schematic("bad_lib_pin_orientation", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must warn and skip malformed lib symbol");
    assert!(schematic.screen.parse_warnings[0].contains("expecting 0, 90, 180, or 270"));
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_unit_name_suffix() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_bad" (arc))))
)"#;
    let path = temp_schematic("bad_lib_unit_suffix", src);
    let schematic =
        parse_schematic_file(Path::new(&path)).expect("must warn and skip malformed lib symbol");
    assert!(schematic.screen.parse_warnings[0].contains("invalid symbol unit number"));
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_generator_version_before_supported_version() {
    let src = r#"(kicad_sch
  (version 20230101)
  (generator "eeschema")
  (generator_version "8.0")
)"#;
    let path = temp_schematic("old_generator_version", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject generator_version");
    assert!(
        err.to_string()
            .contains("generator_version requires schematic version 20231120 or newer")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn accepts_modern_page_sniff_block() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
  (paper "User" "123.4" 234.5)
)"#;
    let quoted_path = temp_schematic("quoted_user_paper_dimension", quoted_src);
    let err = parse_schematic_file(Path::new(&quoted_path)).expect_err("must reject quoted width");
    assert!(err.to_string().contains("missing width"));
    let _ = fs::remove_file(quoted_path);

    let quoted_height_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
  (paper "BogusSize")
)"#;
    let path = temp_schematic("invalid_page_type", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject page type");
    assert!(err.to_string().contains("Invalid page type"));
    match &err {
        Error::Validation { diagnostic, .. } => {
            let span = diagnostic.span.expect("diagnostic span");
            assert_eq!(&src[span.start..span.end], "\"BogusSize\"");
        }
        _ => panic!("expected validation error"),
    }
    let _ = fs::remove_file(path);

    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
  (uuid "root-uuid")
  (paper 123)
)"#;
    let path = temp_schematic("numeric_page_type", numeric_src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject numeric page type");
    assert!(err.to_string().contains("missing paper kind"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_missing_tokens_in_paper_and_page_branches() {
    let missing_paper_kind_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
  (page 7)
)"#;
    let missing_page_sheet_path = temp_schematic("missing_page_sheet", missing_page_sheet_src);
    let err = parse_schematic_file(Path::new(&missing_page_sheet_path))
        .expect_err("must reject missing page sheet token");
    assert!(err.to_string().contains("missing page sheet"));
    let _ = fs::remove_file(missing_page_sheet_path);

    let missing_page_right_src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
fn rejects_future_schematic_version() {
    let src = r#"(kicad_sch
  (version 20990101)
  (generator "eeschema")
  (uuid "u-1")
)"#;
    let path = temp_schematic("future_version", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject future version");
    assert!(
        err.to_string()
            .contains("future schematic version `20990101` is newer than supported `20260306`")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn defaults_missing_header_version_and_rejects_late_version_section() {
    let missing_src = r#"(kicad_sch
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
)"#;
    let explicit_a4_path = temp_schematic("explicit_a4_page", explicit_a4_src);
    let explicit = parse_schematic_file(Path::new(&explicit_a4_path)).expect("must parse");
    assert_eq!(explicit.screen.paper, schematic.screen.paper);
    let _ = fs::remove_file(explicit_a4_path);

    let late_src = r#"(kicad_sch
  (generator "eeschema")
  (version 20250114)
  (uuid "u-1")
  (paper "A4")
)"#;
    let late_path = temp_schematic("late_version_section", late_src);
    let err = parse_schematic_file(Path::new(&late_path)).expect_err("must reject late version");
    assert!(
        err.to_string()
            .contains("unsupported schematic section `version`")
    );
    let _ = fs::remove_file(late_path);
}

#[test]
fn accepts_legacy_host_and_generates_root_uuid_for_old_files() {
    let src = r#"(kicad_sch
  (version 20200826)
  (host "eeschema" "5.99")
  (paper "A4")
)"#;
    let path = temp_schematic("legacy_host", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.generator, "eeschema");
    assert!(schematic.root_sheet.uuid.is_some());
    assert_eq!(schematic.root_sheet.uuid, schematic.screen.uuid);
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
  (uuid "u-1")
  (uuid "u-2")
  (paper "A4")
  (title_block (title "Old") (comment 1 "one"))
  (title_block (title "New") (comment 2 "two"))
)"#;
    let path = temp_schematic("duplicate_header_sections", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    assert_eq!(schematic.generator, "second");
    assert_eq!(schematic.generator_version.as_deref(), Some("9.1"));
    assert_eq!(schematic.root_sheet.uuid.as_deref(), Some("u-2"));
    assert_eq!(schematic.screen.uuid.as_deref(), Some("u-2"));

    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.title.as_deref(), Some("New"));
    assert!(
        title_block
            .comments
            .iter()
            .any(|(idx, value)| *idx == 2 && value == "two")
    );
    assert!(!title_block.comments.iter().any(|(idx, _)| *idx == 1));

    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_title_block_comment_numbers_overwrite_existing_slots() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (title_block
    (comment 1 "first")
    (comment 1 "second")
    (comment 2 "two"))
)"#;
    let path = temp_schematic("duplicate_title_block_comments", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let title_block = schematic.screen.title_block.as_ref().expect("title block");
    assert_eq!(title_block.comments.len(), 2);
    assert!(
        title_block
            .comments
            .iter()
            .any(|(idx, value)| *idx == 1 && value == "second")
    );
    assert!(
        title_block
            .comments
            .iter()
            .any(|(idx, value)| *idx == 2 && value == "two")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn normalizes_symbol_and_sheet_instance_paths_and_legacy_empty_text() {
    let src = r#"(kicad_sch
  (version 20221001)
  (generator "eeschema")
  (uuid "root-uuid")
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

    assert_eq!(schematic.screen.sheet_instances.len(), 2);
    assert_eq!(schematic.screen.sheet_instances[0].path, "");
    assert_eq!(
        schematic.screen.sheet_instances[0].page.as_deref(),
        Some("1")
    );
    assert_eq!(schematic.screen.sheet_instances[1].path, "/root-uuid/child");
    assert_eq!(
        schematic.screen.sheet_instances[1].page.as_deref(),
        Some("#")
    );
    assert_eq!(schematic.screen.symbol_instances[0].path, "/root-uuid/sym1");
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
fn maps_legacy_sim_enable_fields_to_exclude_from_sim() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 1 2 0) (property "Sim.Enable" "0"))
  (symbol (lib_id "Device:R") (at 3 4 0) (property "Spice_Netlist_Enabled" "N"))
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
    assert_eq!(symbols.len(), 2);
    assert!(symbols[0].excluded_from_sim);
    assert!(symbols[1].excluded_from_sim);
    assert!(symbols[0].properties.is_empty());
    assert!(symbols[1].properties.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn parses_symbol_mirror_body_style_and_sheet_pins() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
  (paper "A4")
  (symbol (lib_id "Device:R") (at 10 20 270) (mirror x) (body_style 2) (uuid "sym-1"))
  (sheet (at 0 0) (size 20 10) (uuid "sheet-1")
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "IN" input (at 0 5 180) (uuid "pin-1"))
    (pin "OUT" output (at 20 5 0) (uuid "pin-2"))
    (pin "TOP" bidirectional (at 10 0 90) (uuid "pin-3"))
    (pin "BOT" tri_state (at 10 10 270) (uuid "pin-4"))
    (pin "PASS" passive (at 5 5 0) (effects (font (size 1 2)) (hide)) (uuid "pin-5")))
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
    assert_eq!(sheet.pins[0].side, Some(SheetSide::Left));
    assert_eq!(sheet.pins[1].side, Some(SheetSide::Right));
    assert_eq!(sheet.pins[2].side, Some(SheetSide::Top));
    assert_eq!(sheet.pins[3].side, Some(SheetSide::Bottom));
    assert_eq!(sheet.pins[4].shape, SheetPinShape::Passive);
    assert!(sheet.pins[4].has_effects);
    assert!(!sheet.pins[4].visible);
    assert_eq!(
        sheet.pins[4]
            .effects
            .as_ref()
            .and_then(|effects| effects.font_size),
        Some([1.0, 2.0])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_sheet_pin_name() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
fn parses_property_metadata_semantics() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
            SchItem::Symbol(symbol) => symbol.properties.first(),
            _ => None,
        })
        .expect("property");

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

    let bare_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "root-uuid")
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
            SchItem::Symbol(symbol) => symbol.properties.first(),
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "root-quoted-field-head")
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
  (uuid "u-1")
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
    assert!(symbol.properties[0].is_private);

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
    assert!(global.properties[0].is_private);

    let _ = fs::remove_file(path);
}

#[test]
fn respects_hide_inside_property_effects() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
            SchItem::Symbol(symbol) => symbol.properties.first(),
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
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_sheet_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
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
  (uuid "root-uuid")
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
    assert_eq!(sheet.name.as_deref(), Some("New Name"));
    assert_eq!(sheet.filename.as_deref(), Some("new.kicad_sch"));

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_symbol_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-symbol-unexpected")
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
fn parses_text_and_label_semantics() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
  (paper "A4")
  (text "note" (exclude_from_sim yes) (at 1 2 90) (fields_autoplaced) (effects (font (size 1.27 1.27))) (uuid "t-1"))
  (global_label "VCC" (shape input) (exclude_from_sim yes) (at 5 6 180) (uuid "g-1")
    (iref 9 10)
    (property "Intersheet References" "show")
    (effects (font (size 1.27 1.27))))
  (directive_label "D" (shape dot) (length 3.5) (at 2 3 0) (uuid "d-1"))
  (label "LOCAL" (at 7 8 0) (uuid "l-1"))
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
    assert_eq!(global.iref_at, Some([9.0, 10.0]));
    assert!(global.has_effects);
    assert!(global.visible);
    assert_eq!(global.angle, 0.0);
    assert_eq!(global.spin, Some(LabelSpin::Right));
    assert_eq!(
        global.effects.as_ref().map(|effects| effects.h_justify),
        Some(TextHJustify::Center)
    );

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
fn rejects_unexpected_tokens_in_shared_sch_text_parser() {
    let text_property_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (global_label "G" (at 0 0 0) (shape (bogus)))
)"#;
    let bad_global_shape_path =
        temp_schematic("global_label_invalid_shape_token", bad_global_shape_src);
    let err = parse_schematic_file(Path::new(&bad_global_shape_path))
        .expect_err("must reject invalid shape token on global label");
    assert!(err.to_string().contains("expecting shape"));
    let _ = fs::remove_file(bad_global_shape_path);

    let hierarchical_length_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (label "L" (at 0 0 0) (iref))
)"#;
    let empty_local_iref_path = temp_schematic("local_label_empty_iref", empty_local_iref_src);
    let err = parse_schematic_file(Path::new(&empty_local_iref_path))
        .expect_err("empty legacy iref on local label should fall out to shared parser flow");
    assert!(err.to_string().contains("expecting end of file"));
    let _ = fs::remove_file(empty_local_iref_path);

    let quoted_text_head_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (global_label "VCC" (at 5 6 180) (uuid "g-1"))
  (hierarchical_label "NET_A" (at 7 8 90) (uuid "h-1"))
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
    assert!(labels.iter().all(|label| label.shape.is_none()));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_label_and_sheet_pin_shape_tokens() {
    let quoted_label_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-label-shape")
  (global_label "VCC" (shape "input"))
)"#;
    let quoted_label_shape_path = temp_schematic("quoted_label_shape_token", quoted_label_shape);
    let err = parse_schematic_file(Path::new(&quoted_label_shape_path))
        .expect_err("must reject quoted label shape token");
    assert!(err.to_string().contains("expecting shape"));

    let quoted_sheet_pin_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-sheet-pin-shape")
  (sheet
    (property "Sheetname" "Child")
    (property "Sheetfile" "child.kicad_sch")
    (pin "P1" "input"))
)"#;
    let quoted_sheet_pin_shape_path =
        temp_schematic("quoted_sheet_pin_shape_token", quoted_sheet_pin_shape);
    let err = parse_schematic_file(Path::new(&quoted_sheet_pin_shape_path))
        .expect_err("must reject quoted sheet pin shape token");
    assert!(err.to_string().contains("expecting sheet pin shape"));

    let _ = fs::remove_file(quoted_label_shape_path);
    let _ = fs::remove_file(quoted_sheet_pin_shape_path);
}

#[test]
fn rejects_quoted_symbol_mirror_and_lib_pin_type_shape_tokens() {
    let quoted_mirror = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-mirror")
  (symbol
    (lib_id "Device:R")
    (mirror "x"))
)"#;
    let quoted_mirror_path = temp_schematic("quoted_symbol_mirror", quoted_mirror);
    let err = parse_schematic_file(Path::new(&quoted_mirror_path))
        .expect_err("must reject quoted mirror axis");
    assert!(err.to_string().contains("expecting mirror axis"));

    let quoted_lib_pin_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-pin-type")
  (lib_symbols
    (symbol "MyLib:U"
      (pin "input" line
        (at 0 0 0)
        (length 2.54)
        (name "PIN")
        (number "1"))))
)"#;
    let quoted_lib_pin_type_path = temp_schematic("quoted_lib_pin_type", quoted_lib_pin_type);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_pin_type_path))
        .expect("quoted lib pin type should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting pin type"))
    );

    let quoted_lib_pin_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-pin-shape")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input "line"
        (at 0 0 0)
        (length 2.54)
        (name "PIN")
        (number "1"))))
)"#;
    let quoted_lib_pin_shape_path = temp_schematic("quoted_lib_pin_shape", quoted_lib_pin_shape);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_pin_shape_path))
        .expect("quoted lib pin shape should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting pin shape"))
    );

    let _ = fs::remove_file(quoted_mirror_path);
    let _ = fs::remove_file(quoted_lib_pin_type_path);
    let _ = fs::remove_file(quoted_lib_pin_shape_path);
}

#[test]
fn rejects_quoted_lib_pin_property_and_style_keyword_heads() {
    let quoted_lib_pin_effects = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-pin-effects")
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
    let schematic = parse_schematic_file(Path::new(&quoted_lib_pin_effects_path))
        .expect("quoted lib pin effects head should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting effects"))
    );

    let quoted_lib_property_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-property-head")
  (lib_symbols
    (symbol "MyLib:U"
      (property "User" "v" ("at" 1 2 0))))
)"#;
    let quoted_lib_property_head_path =
        temp_schematic("quoted_lib_property_head", quoted_lib_property_head);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_property_head_path))
        .expect("quoted lib property head should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(schematic.screen.parse_warnings.iter().any(|warning| {
        warning.contains("expecting id, at, hide, show_name, do_not_autoplace, or effects")
    }));

    let quoted_stroke_width = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-stroke-width")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke ("width" 0.2)))
)"#;
    let quoted_stroke_width_path = temp_schematic("quoted_stroke_width", quoted_stroke_width);
    let err = parse_schematic_file(Path::new(&quoted_stroke_width_path))
        .expect_err("must reject quoted stroke width head");
    assert!(err.to_string().contains("expecting width, type or color"));

    let quoted_fill_color = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-fill-color")
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
  (uuid "root-quoted-lib-symbol-head")
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
  (uuid "root-quoted-pts-xy")
  (polyline (pts ("xy" 0 0) (xy 1 1)))
)"#;
    let quoted_polyline_xy_path = temp_schematic("quoted_pts_xy", quoted_polyline_xy);
    let err = parse_schematic_file(Path::new(&quoted_polyline_xy_path))
        .expect_err("must reject quoted pts xy head");
    assert!(err.to_string().contains("expecting xy"));

    let quoted_embedded_file_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-embedded-file-name")
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
            .any(|warning| warning.contains("expecting name or data"))
    );

    let _ = fs::remove_file(quoted_polyline_xy_path);
    let _ = fs::remove_file(quoted_embedded_file_name_path);
}

#[test]
fn rejects_quoted_symbol_and_sheet_keyword_heads() {
    let quoted_symbol_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-symbol-head")
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
  (uuid "root-quoted-symbol-project")
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
  (uuid "root-quoted-sheet-head")
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
  (uuid "root-quoted-sheet-project")
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
  (uuid "root-quoted-symbol-fields-autoplaced")
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
  (uuid "root-quoted-sheet-fields-autoplaced")
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
  (uuid "root-quoted-sheet-pin-head")
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
  (uuid "root-quoted-top-sheet-instance")
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
  (uuid "root-quoted-top-symbol-instance")
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
            .contains("expecting reference, unit, value or footprint")
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
  (uuid "root-quoted-top-level-head")
  ("group" "G" (uuid "g-1") (members))
)"#;
    let path = temp_schematic("quoted_top_level_section_head", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject quoted top-level section head");
    assert!(err.to_string().contains(
        "expecting generator, host, generator_version, uuid, paper, page, title_block, embedded_fonts, embedded_files, lib_symbols, bus_alias, symbol, sheet, junction, no_connect, bus_entry, wire, bus, polyline, label, global_label, hierarchical_label, directive_label, class_label, netclass_flag, text, text_box, table, image, arc, circle, rectangle, bezier, rule_area, sheet_instances, symbol_instances, or group"
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_effects_keyword_tokens() {
    let quoted_justify = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-justify")
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
  (uuid "root-quoted-font-bold")
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
  (uuid "root-quoted-font-color")
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
  (uuid "root-quoted-hide")
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
fn rejects_quoted_text_box_table_and_image_keyword_heads() {
    let quoted_text_box_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-text-box-head")
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
  (uuid "root-quoted-text-box-margins-head")
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
  (uuid "root-quoted-table-head")
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
  (uuid "root-quoted-table-cell-head")
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
  (uuid "root-quoted-table-cell-span-head")
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
  (uuid "root-quoted-image-head")
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
  (uuid "root-quoted-polyline-head")
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
  (uuid "root-quoted-arc-head")
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
  (uuid "root-quoted-circle-head")
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
  (uuid "root-quoted-rectangle-radius-head")
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
  (uuid "root-quoted-rule-area-head")
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
  (uuid "root-quoted-junction-head")
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
  (uuid "root-quoted-no-connect-head")
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
  (uuid "root-quoted-bus-entry-head")
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
  (uuid "root-quoted-power-scope")
  (lib_symbols
    (symbol "MyLib:U"
      (power "local")))
)"#;
    let quoted_power_scope_path = temp_schematic("quoted_power_scope", quoted_power_scope);
    let schematic = parse_schematic_file(Path::new(&quoted_power_scope_path))
        .expect("quoted lib power scope should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting global or local"))
    );

    let quoted_stroke_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-stroke-type")
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
  (uuid "root-quoted-fill-type")
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
  (uuid "root-quoted-pin-names-hide")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_names "hide")))
)"#;
    let quoted_pin_names_hide_path = temp_schematic("quoted_pin_names_hide", quoted_pin_names_hide);
    let schematic = parse_schematic_file(Path::new(&quoted_pin_names_hide_path))
        .expect("quoted pin_names hide should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting ("))
    );

    let quoted_pin_numbers_hide = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-pin-numbers-hide")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_numbers "hide")))
)"#;
    let quoted_pin_numbers_hide_path =
        temp_schematic("quoted_pin_numbers_hide", quoted_pin_numbers_hide);
    let schematic = parse_schematic_file(Path::new(&quoted_pin_numbers_hide_path))
        .expect("quoted pin_numbers hide should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting ("))
    );

    let _ = fs::remove_file(quoted_pin_names_hide_path);
    let _ = fs::remove_file(quoted_pin_numbers_hide_path);
}

#[test]
fn quoted_demorgan_in_body_styles_is_not_the_keyword_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-demorgan")
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
  (uuid "root-quoted-pin-names-head")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_names ("hide" yes))))
)"#;
    let quoted_pin_names_head_path = temp_schematic("quoted_pin_names_head", quoted_pin_names_head);
    let schematic = parse_schematic_file(Path::new(&quoted_pin_names_head_path))
        .expect("quoted pin_names list head should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting offset or hide"))
    );

    let quoted_pin_numbers_head = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-pin-numbers-head")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_numbers ("hide" yes))))
)"#;
    let quoted_pin_numbers_head_path =
        temp_schematic("quoted_pin_numbers_head", quoted_pin_numbers_head);
    let schematic = parse_schematic_file(Path::new(&quoted_pin_numbers_head_path))
        .expect("quoted pin_numbers list head should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting hide"))
    );

    let quoted_pin_names_offset = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-pin-names-offset")
  (lib_symbols
    (symbol "MyLib:U"
      (pin_names ("offset" 0.5))))
)"#;
    let quoted_pin_names_offset_path =
        temp_schematic("quoted_pin_names_offset", quoted_pin_names_offset);
    let schematic = parse_schematic_file(Path::new(&quoted_pin_names_offset_path))
        .expect("quoted pin_names offset head should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting offset or hide"))
    );

    let _ = fs::remove_file(quoted_pin_names_head_path);
    let _ = fs::remove_file(quoted_pin_numbers_head_path);
    let _ = fs::remove_file(quoted_pin_names_offset_path);
}

#[test]
fn rejects_quoted_private_locked_and_bare_lib_pin_hide_keywords() {
    let quoted_group_locked = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-locked")
  (group "locked" (uuid "g-1") (members))
)"#;
    let quoted_group_locked_path = temp_schematic("quoted_group_locked", quoted_group_locked);
    let schematic = parse_schematic_file(Path::new(&quoted_group_locked_path)).expect("must parse");
    assert!(schematic
        .screen
        .items
        .iter()
        .any(|item| matches!(item, SchItem::Group(Group { name, uuid, .. }) if name.as_deref() == Some("locked") && uuid.as_deref() == Some("g-1"))));

    let quoted_property_private = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-private")
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
  (uuid "root-quoted-lib-text-private")
  (lib_symbols
    (symbol "MyLib:U"
      (symbol "MyLib:U_1_1"
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
  (uuid "root-quoted-lib-pin-hide")
  (lib_symbols
    (symbol "MyLib:U"
      (pin input line "hide" (at 0 0 0) (length 2.54) (name "PIN") (number "1"))))
)"#;
    let quoted_lib_pin_hide_path = temp_schematic("quoted_lib_pin_hide", quoted_lib_pin_hide);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_pin_hide_path))
        .expect("quoted bare lib pin hide should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting ("))
    );

    let _ = fs::remove_file(quoted_group_locked_path);
    let _ = fs::remove_file(quoted_property_private_path);
    let _ = fs::remove_file(quoted_lib_text_private_path);
    let _ = fs::remove_file(quoted_lib_pin_hide_path);
}

#[test]
fn rejects_unquoted_jumper_pin_group_names() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-unquoted-jumper-pin-group")
  (lib_symbols
    (symbol "MyLib:U"
      (jumper_pin_groups
        (A B))))
)"#;
    let path = temp_schematic("unquoted_jumper_pin_groups", src);
    let schematic = parse_schematic_file(Path::new(&path))
        .expect("bad lib symbol should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting list of pin names"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_lib_draw_item_list_heads() {
    let quoted_lib_rectangle_start = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-rectangle-start")
  (lib_symbols
    (symbol "MyLib:U"
      (rectangle ("start" 0 0) (end 1 1))))
)"#;
    let quoted_lib_rectangle_start_path =
        temp_schematic("quoted_lib_rectangle_start", quoted_lib_rectangle_start);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_rectangle_start_path))
        .expect("bad lib rectangle should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting start, end, stroke, or fill"))
    );

    let quoted_lib_rectangle_radius = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-rectangle-radius")
  (lib_symbols
    (symbol "MyLib:U"
      (rectangle (start 0 0) (end 1 1) ("radius" 0.5))))
)"#;
    let quoted_lib_rectangle_radius_path =
        temp_schematic("quoted_lib_rectangle_radius", quoted_lib_rectangle_radius);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_rectangle_radius_path))
        .expect("bad lib rectangle radius should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| { warning.contains("expecting start, end, stroke, or fill") })
    );

    let quoted_lib_text_at = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-text-at")
  (lib_symbols
    (symbol "MyLib:U"
      (text "TXT" ("at" 0 0 90))))
)"#;
    let quoted_lib_text_at_path = temp_schematic("quoted_lib_text_at", quoted_lib_text_at);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_text_at_path))
        .expect("bad lib text should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting at or effects"))
    );

    let quoted_lib_text_box_effects = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-quoted-lib-textbox-effects")
  (lib_symbols
    (symbol "MyLib:U"
      (text_box "TXT" ("effects" (font (size 1 1))) (size 1 1))))
)"#;
    let quoted_lib_text_box_effects_path =
        temp_schematic("quoted_lib_textbox_effects", quoted_lib_text_box_effects);
    let schematic = parse_schematic_file(Path::new(&quoted_lib_text_box_effects_path))
        .expect("bad lib text_box should be skipped with a warning");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| { warning.contains("expecting at, size, stroke, fill or effects") })
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
  (uuid "u-1")
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
  (uuid "root-uuid")
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
    assert_eq!(effects.font_size, Some([1.5, 2.5]));
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
fn rejects_invalid_effects_hyperlink() {
    let src = r#"(kicad_sch
  (version 20231120)
  (generator "eeschema")
  (uuid "root-uuid")
  (paper "A4")
  (text "note" (at 1 2 0) (effects (href "not-a-url")))
)"#;
    let path = temp_schematic("bad_effects_href", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject invalid hyperlink");
    assert!(err.to_string().contains("invalid hyperlink url"));
    let _ = fs::remove_file(path);
}

#[test]
fn converts_legacy_overbar_text_and_labels() {
    let src = r#"(kicad_sch
  (version 20210605)
  (generator "eeschema")
  (uuid "root-uuid")
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
  (uuid "root-overbar-value")
  (lib_symbols
    (symbol "Device:R"
      (property "Value" "~LIB~" (id 1) (at 0 0 0) (effects (font (size 1 1))))))
  (symbol
    (lib_id "Device:R")
    (property "Value" "~SCH~" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (uuid "sym-1"))
)"#;
    let path = temp_schematic("legacy_overbar_value_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should parse");

    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.name == "Device:R")
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
  (uuid "root-overbar-lib-text")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (text "~LIBTXT~" (at 0 0 0) (effects (font (size 1 1)))))))
)"#;
    let path = temp_schematic("legacy_overbar_lib_text_effects", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should parse");

    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.name == "Device:R")
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
  (uuid "root-overbar-no-effects")
  (lib_symbols
    (symbol "Device:R"
      (property "Value" "~LIBRAW~")))
  (symbol
    (lib_id "Device:R")
    (property "Value" "~SCHRAW~")
    (uuid "sym-1"))
)"#;
    let path = temp_schematic("legacy_overbar_no_effects_path", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should parse");

    let lib_symbol = schematic
        .screen
        .lib_symbols
        .iter()
        .find(|symbol| symbol.name == "Device:R")
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
fn accepts_legacy_class_label_alias() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (class_label "NETCLASS" (shape rectangle) (at 1 2 0) (uuid "c-1"))
)"#;
    let path = temp_schematic("class_label_alias", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse class_label");
    assert!(schematic.screen.items.iter().any(
        |item| matches!(item, SchItem::Label(label) if label.kind == LabelKind::NetclassFlag)
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn allows_properties_on_local_and_hierarchical_labels() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "root-uuid")
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
fn rejects_invalid_bus_alias_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-bus-alias")
  (bus_alias (bogus) (members "A0" "A1"))
)"#;
    let path = temp_schematic("bad_bus_alias_name", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad bus alias name");
    assert!(err.to_string().contains("expecting bus alias name"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_bus_alias_member_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-bus-alias-member")
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
  (uuid "root-bus-alias-empty")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (image (at 1 2) (scale 1.0) (data "%%%") (uuid "img-1"))
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
  (uuid "u-1")
  (paper "A4")
  (image (at 1 2) (scale 1.0) (data (bogus)) (uuid "img-1"))
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
  (uuid "u-1")
  (paper "A4")
  (image (scale 1.0) (data "QUJD") (uuid "img-1"))
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
fn records_warning_for_invalid_top_level_embedded_files() {
    let src = r#"(kicad_sch
  (version 20240620)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (embedded_files (file (name "A.bin") (bogus "x")))
)"#;
    let path = temp_schematic("bad_embedded_file_child", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must keep loading");
    assert!(schematic.screen.embedded_files.is_empty());
    assert_eq!(schematic.screen.parse_warnings.len(), 1);
    assert!(schematic.screen.parse_warnings[0].contains("expecting name or data"));
    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_for_invalid_embedded_files_file_head() {
    let src = r#"(kicad_sch
  (version 20240620)
  (generator "eeschema")
  (uuid "u-1")
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
  (version 20240620)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (embedded_fonts no)
  (embedded_fonts yes)
  (embedded_files (file "A.bin" "aaa"))
  (embedded_files (file "B.bin" "bbb"))
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
    assert_eq!(schematic.screen.lib_symbols[0].name, "First:R");
    assert_eq!(schematic.screen.lib_symbols[1].name, "Second:R");
    let _ = fs::remove_file(path);
}

#[test]
fn computes_text_box_end_from_size_and_defers_groups_until_after_items() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (uuid "wire-u"))
  (text_box "body" (at 10 20 90) (size 3 4) (exclude_from_sim yes) (uuid "tb"))
  (group "G" (uuid "group-u") (members "wire-u"))
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
            if uuid.as_deref() == Some("group-u") && members == &vec!["wire-u".to_string()]
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_table_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-table-unexpected")
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
  (uuid "u-1")
  (paper "A4")
  (text_box "body" (size 3 4) (uuid "tb"))
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
fn applies_upstream_default_text_box_margins_when_omitted() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (text_box "body" (at 10 20 90) (size 3 4) (stroke (width 0.2)) (effects (font (size 2 3))) (uuid "tb"))
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
    let expected = 0.2 / 2.0 + 3.0 * 0.75;
    assert!((margins[0] - expected).abs() < 1e-9);
    assert!((margins[1] - expected).abs() < 1e-9);
    assert!((margins[2] - expected).abs() < 1e-9);
    assert!((margins[3] - expected).abs() < 1e-9);

    let _ = fs::remove_file(path);
}

#[test]
fn parses_nested_sheet_and_symbol_instances_and_polyline_conversion() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (polyline (pts (xy 0 0) (xy 1 1)) (stroke (width 0.1)) (uuid "pl-1"))
  (symbol
    (lib_id "Device:R")
    (lib_name "Device:R")
    (at 1 2 0)
    (default_instance
      (reference "R?")
      (unit 1)
      (value "10k")
      (footprint "Resistor_SMD:R_0603"))
    (pin "1" (alternate "A") (uuid "pin-u"))
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
    assert_eq!(symbol.default_reference.as_deref(), Some("R?"));
    assert_eq!(symbol.default_value.as_deref(), Some("10k"));
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
    assert_eq!(symbol.pins[0].uuid.as_deref(), Some("pin-u"));
    assert_eq!(symbol.instances[0].project, "demo");
    assert_eq!(symbol.instances[0].path, "/A");
    assert_eq!(symbol.instances[0].variants.len(), 1);
    assert_eq!(symbol.instances[0].variants[0].name, "ALT");
    assert_eq!(symbol.instances[0].variants[0].fields[0].name, "MPN");

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
    assert_eq!(sheet.instances[0].variants[0].name, "ASSEMBLY");
    assert!(sheet.instances[0].variants[0].in_bom);
    assert!(!sheet.instances[0].variants[0].in_pos_files);

    let _ = fs::remove_file(path);
}

#[test]
fn symbol_instance_value_and_footprint_update_symbol_fields() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-symbol-instance-fields")
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

    assert_eq!(symbol.default_value.as_deref(), Some("default-value"));
    assert_eq!(
        symbol.default_footprint.as_deref(),
        Some("default-footprint")
    );
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
        Some("instance-value")
    );
    assert_eq!(
        symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .map(|property| property.value.as_str()),
        Some("instance-footprint")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn sheet_variant_in_bom_respects_20260306_fix_boundary() {
    let old_src = r#"(kicad_sch
  (version 20260305)
  (generator "eeschema")
  (uuid "root-old")
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
    assert!(!old_sheet.instances[0].variants[0].in_bom);

    let new_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-new")
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
    assert!(new_sheet.instances[0].variants[0].in_bom);

    let _ = fs::remove_file(old_path);
    let _ = fs::remove_file(new_path);
}

#[test]
fn symbol_and_sheet_variants_inherit_parent_attributes() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-variant-inherit")
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
    let sym_variant = &symbol.instances[0].variants[0];
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
    let sheet_variant = &sheet.instances[0].variants[0];
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
  (uuid "root-name")
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
  (uuid "root-variant")
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
  (uuid "root-value")
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
fn rejects_invalid_nested_instance_symbol_headers() {
    let bad_symbol_project = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-project")
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
    assert!(err.to_string().contains("expecting project name"));

    let bad_sheet_path = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-sheet-path")
  (sheet_instances
    (path (bogus) (page "1")))
)"#;
    let bad_sheet_path_path = temp_schematic("bad_sheet_instance_path", bad_sheet_path);
    let err = parse_schematic_file(Path::new(&bad_sheet_path_path))
        .expect_err("must reject bad sheet instance path");
    assert!(err.to_string().contains("expecting sheet instance path"));

    let bad_symbol_reference = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-symbol-ref")
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
    assert!(err.to_string().contains("expecting reference"));

    let _ = fs::remove_file(bad_symbol_project_path);
    let _ = fs::remove_file(bad_sheet_path_path);
    let _ = fs::remove_file(bad_symbol_reference_path);
}

#[test]
fn rejects_invalid_sheet_instance_page_tokens() {
    let bad_top_level = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-sheet-page")
  (sheet_instances
    (path "/A" (page (bogus))))
)"#;
    let bad_top_level_path = temp_schematic("bad_sheet_instance_page", bad_top_level);
    let err = parse_schematic_file(Path::new(&bad_top_level_path))
        .expect_err("must reject bad top-level sheet page");
    assert!(err.to_string().contains("expecting page"));

    let bad_nested = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-sheet-nested-page")
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
    assert!(err.to_string().contains("expecting page"));

    let bad_nested_child = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-sheet-nested-child")
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
  (uuid "root-symbol-instance-child")
  (symbol_instances
    (path "/A" (bogus "R1")))
)"#;
    let path = temp_schematic("bad_top_level_symbol_instance_child", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject bad top-level symbol instance child");
    assert!(
        err.to_string()
            .contains("expecting reference, unit, value or footprint")
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_default_instance_reference() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-default-ref")
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
    assert!(err.to_string().contains("expecting reference"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_instance_value_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-default-value")
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
    assert!(err.to_string().contains("expecting value"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_pin_number() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-pin")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin (bogus) (alternate "A")))
)"#;
    let path = temp_schematic("bad_symbol_pin_number", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol pin number");
    assert!(err.to_string().contains("expecting pin number"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_pin_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-pin-uuid")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin "1" (uuid (bogus))))
)"#;
    let path = temp_schematic("bad_symbol_pin_uuid", src);
    let err = parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol pin uuid");
    assert!(err.to_string().contains("expecting uuid"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_pin_alternate_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-pin-alt")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (pin "1" (alternate (bogus))))
)"#;
    let path = temp_schematic("bad_symbol_pin_alternate", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad symbol pin alternate");
    assert!(err.to_string().contains("expecting alternate"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_symbol_mirror_axis_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-mirror")
  (symbol
    (lib_id "Device:R")
    (at 1 2 0)
    (mirror (bogus)))
)"#;
    let path = temp_schematic("bad_symbol_mirror_axis", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad mirror axis token");
    assert!(err.to_string().contains("expecting mirror axis"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-name")
  (lib_symbols
    (symbol (bogus)))
)"#;
    let path = temp_schematic("bad_lib_symbol_name", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid symbol name"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_parent_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-parent")
  (lib_symbols
    (symbol "Child:R"
      (extends (bogus))))
)"#;
    let path = temp_schematic("bad_lib_symbol_parent_name", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid parent symbol name"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_symbol_unit_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-unit")
  (lib_symbols
    (symbol "Device:R"
      (symbol (bogus))))
)"#;
    let path = temp_schematic("bad_lib_symbol_unit_name", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid symbol unit name"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_numeric_lib_unit_name_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-unit-name")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (unit_name 123))))
)"#;
    let path = temp_schematic("bad_lib_unit_name_token", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting )"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_title_block_value_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-title")
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
  (uuid "root-title")
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
  (uuid "root-text")
  (text (bogus) (at 1 2 0))
)"#;
    let path = temp_schematic("bad_schematic_text_string", src);
    let err =
        parse_schematic_file(Path::new(&path)).expect_err("must reject bad schematic text string");
    assert!(err.to_string().contains("Invalid text string"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_schematic_text_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-text-uuid")
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
  (uuid "root-text-box")
  (text_box (bogus) (at 1 2 0) (size 3 4))
)"#;
    let path = temp_schematic("bad_schematic_text_box_string", src);
    let err = parse_schematic_file(Path::new(&path))
        .expect_err("must reject bad schematic text box string");
    assert!(err.to_string().contains("Invalid text string"));
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_schematic_text_box_uuid_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-text-box-uuid")
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
  (uuid "root-image")
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
  (uuid "root-symbol-uuid")
  (symbol (lib_id "Device:R") (at 1 2 0) (uuid (bogus)))
)"#;
    let bad_symbol_path = temp_schematic("bad_symbol_uuid", bad_symbol);
    let err =
        parse_schematic_file(Path::new(&bad_symbol_path)).expect_err("must reject bad symbol uuid");
    assert!(err.to_string().contains("expecting uuid"));

    let bad_group = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-group-uuid")
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
  (uuid "root-group-member-uuid")
  (group "g"
    (uuid "group-uuid")
    (members (bogus))))
)"#;
    let bad_group_member_path = temp_schematic("bad_group_member_uuid", bad_group_member);
    let err = parse_schematic_file(Path::new(&bad_group_member_path))
        .expect_err("must reject bad group member uuid");
    assert!(err.to_string().contains("expecting group member uuid"));

    let bad_group_child = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-group-child")
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
  (uuid "root-group-name")
  (group g
    (uuid "group-uuid")
    (members "a")))
)"#;
    let bad_group_name_path = temp_schematic("bad_group_name", bad_group_name);
    let err = parse_schematic_file(Path::new(&bad_group_name_path))
        .expect_err("must reject unquoted group name");
    assert!(err.to_string().contains("expecting group name or locked"));

    let bad_group_lib_id = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-group-lib-id")
  (group "g"
    (uuid "group-uuid")
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

    let bad_rectangle = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-rect-uuid")
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
    let _ = fs::remove_file(bad_rectangle_path);
}

#[test]
fn parses_and_rejects_lib_property_header_and_metadata_tokens() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-prop")
  (lib_symbols
    (symbol "Device:R"
      (property private "UserField" "R1"
        (show_name)
        (do_not_autoplace)))))
"#;
    let path = temp_schematic("lib_property_metadata", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];
    let property = lib_symbol
        .properties
        .iter()
        .find(|property| property.key == "UserField")
        .expect("user field");
    assert!(property.is_private);
    assert!(property.show_name);
    assert!(!property.can_autoplace);
    let _ = fs::remove_file(path);

    let hidden_src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-property-hidden")
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
    let property = lib_symbol
        .properties
        .iter()
        .find(|property| property.key == "UserField")
        .expect("user field");
    assert!(!property.visible);
    assert!(property.has_effects);
    assert!(property.effects.as_ref().expect("effects").hidden);
    let _ = fs::remove_file(hidden_path);

    let bad_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-prop-name")
  (lib_symbols
    (symbol "Device:R"
      (property (bogus) "R1")))
)"#;
    let bad_name_path = temp_schematic("bad_lib_property_name", bad_name);
    let schematic =
        parse_schematic_file(Path::new(&bad_name_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid property name"))
    );

    let bad_value = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-prop-value")
  (lib_symbols
    (symbol "Device:R"
      (property "UserField" (bogus))))
)"#;
    let bad_value_path = temp_schematic("bad_lib_property_value", bad_value);
    let schematic =
        parse_schematic_file(Path::new(&bad_value_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid property value"))
    );

    let _ = fs::remove_file(bad_name_path);
    let _ = fs::remove_file(bad_value_path);
}

#[test]
fn rejects_invalid_lib_pin_name_number_and_alternate_name_tokens() {
    let bad_name = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-name")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (name (bogus)))))
)"#;
    let bad_name_path = temp_schematic("bad_lib_pin_name", bad_name);
    let schematic =
        parse_schematic_file(Path::new(&bad_name_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid pin name"))
    );

    let bad_number = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-number")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (number (bogus)))))
)"#;
    let bad_number_path = temp_schematic("bad_lib_pin_number", bad_number);
    let schematic =
        parse_schematic_file(Path::new(&bad_number_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid pin number"))
    );

    let bad_alternate = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-alt")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate (bogus) passive line))))
)"#;
    let bad_alternate_path = temp_schematic("bad_lib_pin_alternate_name", bad_alternate);
    let schematic =
        parse_schematic_file(Path::new(&bad_alternate_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid alternate pin name"))
    );

    let bad_alternate_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-alt-type")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate "ALT" (bogus) line))))
)"#;
    let bad_alternate_type_path = temp_schematic("bad_lib_pin_alternate_type", bad_alternate_type);
    let schematic = parse_schematic_file(Path::new(&bad_alternate_type_path))
        .expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting alternate pin type"))
    );

    let bad_alternate_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-alt-shape")
  (lib_symbols
    (symbol "Device:R"
      (pin passive line
        (alternate "ALT" passive (bogus)))))
)"#;
    let bad_alternate_shape_path =
        temp_schematic("bad_lib_pin_alternate_shape", bad_alternate_shape);
    let schematic = parse_schematic_file(Path::new(&bad_alternate_shape_path))
        .expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting alternate pin shape"))
    );

    let _ = fs::remove_file(bad_name_path);
    let _ = fs::remove_file(bad_number_path);
    let _ = fs::remove_file(bad_alternate_path);
    let _ = fs::remove_file(bad_alternate_type_path);
    let _ = fs::remove_file(bad_alternate_shape_path);
}

#[test]
fn rejects_invalid_lib_pin_type_and_shape_tokens() {
    let bad_type = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-type")
  (lib_symbols
    (symbol "Device:R"
      (pin (bogus) line)))
)"#;
    let bad_type_path = temp_schematic("bad_lib_pin_type", bad_type);
    let schematic =
        parse_schematic_file(Path::new(&bad_type_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting pin type"))
    );

    let bad_shape = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-pin-shape")
  (lib_symbols
    (symbol "Device:R"
      (pin passive (bogus))))
)"#;
    let bad_shape_path = temp_schematic("bad_lib_pin_shape", bad_shape);
    let schematic =
        parse_schematic_file(Path::new(&bad_shape_path)).expect("schematic should recover");
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting pin shape"))
    );

    let _ = fs::remove_file(bad_type_path);
    let _ = fs::remove_file(bad_shape_path);
}

#[test]
fn rejects_invalid_lib_text_string_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-text")
  (lib_symbols
    (symbol "Device:R"
      (text (bogus) (at 0 0 0))))
)"#;
    let path = temp_schematic("bad_lib_text_string", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid text string"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_text_box_string_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-text-box")
  (lib_symbols
    (symbol "Device:R"
      (text_box (bogus) (at 0 0 0) (size 1 1))))
)"#;
    let path = temp_schematic("bad_lib_text_box_string", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Invalid text string"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_jumper_pin_group_member_token() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-jumper-group")
  (lib_symbols
    (symbol "Device:R"
      (jumper_pin_groups ((bogus) "2"))))
)"#;
    let path = temp_schematic("bad_lib_jumper_pin_group_member", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("expecting list of pin names"))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_lib_symbol_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-unexpected")
  (lib_symbols
    (symbol "Device:R"
      (bogus 1)))
)"#;
    let path = temp_schematic("bad_lib_symbol_child", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains(
                "expecting pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text"
            ))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_quoted_lib_symbol_top_level_child_head_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-quoted-child")
  (lib_symbols
    (symbol "Device:R"
      ("power" local)))
)"#;
    let path = temp_schematic("quoted_lib_symbol_child", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(
        schematic
            .screen
            .parse_warnings
            .iter()
            .any(|warning| warning.contains(
                "expecting pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text"
            ))
    );
    let _ = fs::remove_file(path);
}

#[test]
fn rejects_unexpected_lib_symbol_unit_child_with_upstream_expect_list() {
    let src = r#"(kicad_sch
  (version 20260306)
  (generator "eeschema")
  (uuid "root-lib-unit-unexpected")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (bogus 1))))
)"#;
    let path = temp_schematic("bad_lib_symbol_unit_child", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("schematic should recover");
    assert!(schematic.screen.lib_symbols.is_empty());
    assert!(schematic.screen.parse_warnings.iter().any(|warning| {
        warning.contains("expecting arc, bezier, circle, pin, polyline, rectangle, or text")
    }));
    let _ = fs::remove_file(path);
}

#[test]
fn preserves_shape_stroke_and_fill_presence() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (arc (start 0 0) (mid 1 1) (end 2 0) (stroke (width 0.1) (color 10 20 30 0.5)) (fill (type outline)) (uuid "a-1"))
  (rule_area (polyline (pts (xy 0 0) (xy 2 0) (xy 2 2)) (stroke (width 0.2)) (fill (type background)) (uuid "ra-1")))
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
  (uuid "u-1")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (width 0.15)) (uuid "w-1"))
  (bus_entry (at 1 1) (size 2 3) (stroke (width 0.2)) (uuid "be-1"))
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
    assert!(sheet.has_stroke);
    assert!(sheet.has_fill);
    assert_eq!(
        sheet.stroke.as_ref().and_then(|stroke| stroke.width),
        Some(0.1)
    );
    assert_eq!(
        sheet.fill.as_ref().and_then(|fill| fill.color),
        Some([0.0, 0.0, 0.0, 0.0])
    );

    let _ = fs::remove_file(path);
}

#[test]
fn junction_no_connect_and_bus_entry_do_not_require_geometry_tokens() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
    assert_eq!(bus_entry.size, [0.0, 0.0]);

    let _ = fs::remove_file(path);
}

#[test]
fn wire_and_bus_do_not_require_pts() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
fn rule_area_does_not_require_three_points() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (width 0.15) (type dash_dot) (color 10 20 30 0.5)) (uuid "w-1"))
  (text_box "body" (at 10 20 90) (size 3 4) (stroke (width 0.2) (type dot)) (fill (type color) (color 1 2 3 0.25)) (uuid "tb"))
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (stroke (bogus 1)) (uuid "w-1"))
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
  (paper "A4")
  (symbol (lib_id "Device:R:bad") (at 1 2 0))
)"#;
    let bad_path = temp_schematic("bad_symbol_lib_id", bad_lib_id);
    let err = parse_schematic_file(Path::new(&bad_path)).expect_err("must reject malformed lib_id");
    assert!(err.to_string().contains("contains invalid character ':'"));
    let _ = fs::remove_file(bad_path);

    let invalid_token_lib_id = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
  (uuid "u-1")
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
    assert_eq!(symbol.linked_lib_symbol_name.as_deref(), None);
    let _ = fs::remove_file(redundant_path);

    let override_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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
    assert_eq!(symbol.linked_lib_symbol_name.as_deref(), None);
    let _ = fs::remove_file(override_path);
}

#[test]
fn links_symbols_to_local_lib_symbols_and_hydrates_embedded_files() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (embedded_files (file "shared.bin" "abc123"))
  (lib_symbols
    (symbol "Local:R"
      (embedded_files (file (name "shared.bin")))))
  (symbol (lib_id "Device:R") (lib_name "Local:R") (at 1 2 0))
)"#;
    let path = temp_schematic("linked_local_lib_symbol", src);
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
    assert_eq!(symbol.linked_lib_symbol_name.as_deref(), Some("Local:R"));

    let lib_symbol = &schematic.screen.lib_symbols[0];
    assert_eq!(lib_symbol.embedded_files[0].data.as_deref(), Some("abc123"));

    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_for_invalid_lib_symbol_embedded_files() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Local:R"
      (embedded_files (file (name "shared.bin") (bogus "x")))))
)"#;
    let path = temp_schematic("invalid_lib_embedded_files", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must keep loading");
    assert_eq!(schematic.screen.parse_warnings.len(), 1);
    assert!(schematic.screen.parse_warnings[0].contains("expecting name or data"));
    assert!(schematic.screen.lib_symbols[0].embedded_files.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn records_warning_and_skips_invalid_lib_symbol_block() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Bad:R"
      (symbol "Bad:R_1_1"
        (pin input line
          (at 0 0 45))))
    (symbol "Good:R"))
)"#;
    let path = temp_schematic("invalid_lib_symbol_block", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must keep loading");
    assert_eq!(schematic.screen.parse_warnings.len(), 1);
    assert!(schematic.screen.parse_warnings[0].contains("Skipping symbol and continuing"));
    assert_eq!(schematic.screen.lib_symbols.len(), 1);
    assert_eq!(schematic.screen.lib_symbols[0].name, "Good:R");
    let _ = fs::remove_file(path);
}

#[test]
fn resolves_groups_after_items_and_drops_unknown_members() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 1 1)) (uuid "wire-u"))
  (group "G1" (uuid "g1") (members "wire-u" "missing"))
  (group "G2" (uuid "g2") (members "g1" "missing-2"))
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
    assert_eq!(groups[0].members, vec!["wire-u"]);
    assert_eq!(groups[1].members, vec!["g1"]);

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_duplicate_user_properties_follow_upstream_renaming() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
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

    assert_eq!(lib_symbol.properties.len(), 4);
    assert_eq!(lib_symbol.properties[0].key, "Field");
    assert_eq!(lib_symbol.properties[0].value, "A");
    assert_eq!(lib_symbol.properties[1].key, "Field_1");
    assert_eq!(lib_symbol.properties[1].value, "B");
    assert_eq!(lib_symbol.properties[2].key, "Field_2");
    assert_eq!(lib_symbol.properties[2].value, "C");
    assert_eq!(lib_symbol.properties[3].key, "Reference");
    assert_eq!(lib_symbol.properties[3].value, "J");

    let _ = fs::remove_file(path);
}

#[test]
fn lib_symbol_private_only_survives_on_user_fields() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property private "Reference" "U")
      (property private "UserField" "X"))))"#;
    let path = temp_schematic("lib_symbol_private_fields", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let lib_symbol = &schematic.screen.lib_symbols[0];

    assert_eq!(lib_symbol.properties.len(), 2);
    assert_eq!(lib_symbol.properties[0].key, "Reference");
    assert!(!lib_symbol.properties[0].is_private);
    assert_eq!(lib_symbol.properties[1].key, "UserField");
    assert!(lib_symbol.properties[1].is_private);

    let _ = fs::remove_file(path);
}

#[test]
fn rejects_invalid_lib_pin_alternate_type_and_shape() {
    let bad_alt_type = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "A")
          (number "1")
          (alternate "ALT" bogus line)))))
)"#;
    let bad_type_path = temp_schematic("bad_lib_pin_alt_type", bad_alt_type);
    let schematic = parse_schematic_file(Path::new(&bad_type_path))
        .expect("must warn and skip malformed lib symbol");
    assert!(schematic.screen.parse_warnings[0].contains("expecting input, output, bidirectional"));
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(bad_type_path);

    let bad_alt_shape = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (pin input line
          (at 0 0 0)
          (length 2.54)
          (name "A")
          (number "1")
          (alternate "ALT" input bogus)))))
)"#;
    let bad_shape_path = temp_schematic("bad_lib_pin_alt_shape", bad_alt_shape);
    let schematic = parse_schematic_file(Path::new(&bad_shape_path))
        .expect("must warn and skip malformed lib symbol");
    assert!(schematic.screen.parse_warnings[0].contains("expecting line, inverted, clock"));
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(bad_shape_path);
}

#[test]
fn lib_symbol_arc_and_bezier_follow_upstream_token_sets() {
    let legacy_arc_src = r#"(kicad_sch
  (version 20230121)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
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
    assert_eq!(item.points, vec![[0.0, 0.0], [2.0, 0.0]]);
    assert_eq!(item.arc_center, Some([1.0, 1.0]));
    assert_eq!(item.radius, Some(1.5));
    assert_eq!(item.arc_start_angle, Some(0.0));
    assert_eq!(item.arc_end_angle, Some(90.0));
    let _ = fs::remove_file(legacy_arc_path);

    let default_arc_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
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
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (arc (center 0 0)))))
)"#;
    let bad_arc_path = temp_schematic("lib_arc_bad_token", bad_arc_src);
    let schematic = parse_schematic_file(Path::new(&bad_arc_path))
        .expect("must warn and skip malformed lib symbol");
    assert!(
        schematic.screen.parse_warnings[0]
            .contains("expecting start, mid, end, radius, stroke, or fill")
    );
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(bad_arc_path);

    let bad_bezier_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (bezier (pts (xy 0 0) (xy 1 1) (xy 2 2) (xy 3 3) (xy 4 4))))))
)"#;
    let bad_bezier_path = temp_schematic("lib_bezier_too_many_points", bad_bezier_src);
    let schematic = parse_schematic_file(Path::new(&bad_bezier_path))
        .expect("must warn and skip malformed lib symbol");
    assert!(schematic.screen.parse_warnings[0].contains("unexpected control point"));
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(bad_bezier_path);

    let quoted_bezier_xy_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (bezier (pts ("xy" 0 0) (xy 1 1) (xy 2 2) (xy 3 3))))))
)"#;
    let quoted_bezier_xy_path = temp_schematic("lib_bezier_quoted_xy", quoted_bezier_xy_src);
    let schematic = parse_schematic_file(Path::new(&quoted_bezier_xy_path))
        .expect("must warn and skip malformed lib symbol");
    assert!(schematic.screen.parse_warnings[0].contains("expecting xy"));
    assert!(schematic.screen.lib_symbols.is_empty());
    let _ = fs::remove_file(quoted_bezier_xy_path);
}

#[test]
fn lib_symbol_arc_and_circle_inherit_upstream_safe_defaults() {
    let src = r#"(kicad_sch
  (version 20230121)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_1"
        (arc (radius (angles 0 90)))
        (circle))))
)"#;
    let path = temp_schematic("lib_shape_defaults", src);
    let schematic = parse_schematic_file(Path::new(&path)).expect("must parse");
    let unit = &schematic.screen.lib_symbols[0].units[0];

    let arc = &unit.draw_items[0];
    assert_eq!(arc.kind, "arc");
    assert_eq!(arc.points, vec![[1.0, 0.0], [0.0, 1.0]]);
    assert_eq!(arc.arc_center, Some([0.0, 0.0]));
    assert_eq!(arc.arc_start_angle, Some(0.0));
    assert_eq!(arc.arc_end_angle, Some(90.0));

    let circle = &unit.draw_items[1];
    assert_eq!(circle.kind, "circle");
    assert_eq!(circle.points, vec![[0.0, 0.0]]);
    assert_eq!(circle.radius, Some(1.0));

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_lib_symbols_infer_demorgan_body_style_from_units() {
    let legacy_src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_2"
        (text "ALT" (at 1 2 0) (effects (font (size 1 1)))))))
)"#;
    let legacy_path = temp_schematic("legacy_demorgan_inferred", legacy_src);
    let schematic = parse_schematic_file(Path::new(&legacy_path)).expect("must parse");
    assert!(schematic.screen.lib_symbols[0].has_demorgan);
    let _ = fs::remove_file(legacy_path);

    let modern_src = r#"(kicad_sch
  (version 20250827)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (symbol "Device:R_1_2"
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
  (uuid "u-1")
  (paper "A4")
  (lib_symbols
    (symbol "Root:R"
      (symbol "Root:R_1_2"
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
