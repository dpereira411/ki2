use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use ki2::core::SchematicProject;
use ki2::error::Error;
use ki2::loader::load_schematic_tree;
use ki2::model::{
    FillType, Group, LabelKind, LabelSpin, LineKind, MirrorAxis, SchItem, ShapeKind, SheetPinShape,
    SheetSide, StrokeStyle, TextHJustify, TextKind, TextVJustify,
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
  (bus_alias "ADDR" (members A0 A1 A2))
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
    assert!(symbol.fields_autoplaced);
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
    assert!(sheet.fields_autoplaced);

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
    assert!(err.to_string().contains("missing sheet file property"));
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
    assert!(text.fields_autoplaced);
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
    assert!(!global.fields_autoplaced);
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
    assert!(local.fields_autoplaced);

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
  (text "~RST~" (at 1 2 180))
  (global_label "~BUS~" (shape input) (at 5 6 180))
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
    assert!(err.to_string().contains("invalid table: no cells defined"));
    let _ = fs::remove_file(path);
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
    assert!(err.to_string().contains("failed to read image data"));
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
    assert!(!sheet.instances[0].variants[0].in_bom);
    assert!(!sheet.instances[0].variants[0].in_pos_files);

    let _ = fs::remove_file(path);
}

#[test]
fn preserves_shape_stroke_and_fill_presence() {
    let src = r#"(kicad_sch
  (version 20250114)
  (generator "eeschema")
  (uuid "u-1")
  (paper "A4")
  (arc (start 0 0) (mid 1 1) (end 2 0) (stroke (width 0.1)) (fill (type outline)) (uuid "a-1"))
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
        Some(FillType::Outline)
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
    assert!(err.to_string().contains("expecting lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances"));
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
