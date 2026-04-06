use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use ki2::core::SchematicProject;
use ki2::error::Error;
use ki2::loader::load_schematic_tree;
use ki2::model::{
    EmbeddedFileType, FieldAutoplacement, FillType, Group, LabelKind, LabelShape, LabelSpin,
    LineKind, MirrorAxis, PropertyKind, SchItem, ShapeKind, SheetPinShape, SheetSide, StrokeStyle,
    TextHJustify, TextKind, TextVJustify,
};
use ki2::parser::parse_schematic_file;
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
        let property = global
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
            .expect("intersheet refs");
        assert_eq!(property.id, Some(6));
        assert_eq!(property.key, "Intersheet References");
    }

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

    assert_eq!(property.value, "[1,2]");
    assert!(property.visible);
    assert_eq!(property.at, Some([8.0, 9.0]));

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
            let span = diagnostic.span.expect("diagnostic span");
            assert_eq!(&src[span.start..span.end], "10");
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
            let span = diagnostic.span.expect("diagnostic span");
            assert_eq!(&src[span.start..span.end], "\"BogusSize\"");
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
    assert_eq!(symbols[0].properties.len(), 5);
    assert_eq!(symbols[1].properties.len(), 5);
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
fn symbol_instance_value_and_footprint_update_symbol_fields() {
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
