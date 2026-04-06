use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{Property, PropertyKind, SchItem, Schematic, SheetReference};
use crate::parser::parse_schematic_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyLink {
    pub parent_path: PathBuf,
    pub child_path: PathBuf,
    pub sheet_uuid: Option<String>,
    pub sheet_name: Option<String>,
    pub filename: String,
    pub reused_existing_child: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSheetPath {
    pub schematic_path: PathBuf,
    pub instance_path: String,
    pub symbol_path: String,
    pub sheet_uuid: Option<String>,
    pub sheet_name: Option<String>,
    pub page: Option<String>,
    pub sheet_number: usize,
    pub sheet_count: usize,
}

#[derive(Debug)]
pub struct LoadResult {
    pub root_path: PathBuf,
    pub schematics: Vec<Schematic>,
    pub links: Vec<HierarchyLink>,
    pub sheet_paths: Vec<LoadedSheetPath>,
    pub current_sheet_instance_path: String,
}

impl LoadResult {
    pub fn root_sheet_path(&self) -> Option<&LoadedSheetPath> {
        self.sheet_path("")
    }

    pub fn current_sheet_path(&self) -> Option<&LoadedSheetPath> {
        self.sheet_path(&self.current_sheet_instance_path)
    }

    pub fn current_page_number(&self) -> Option<&str> {
        self.current_sheet_path()
            .and_then(|sheet_path| sheet_path.page.as_deref())
    }

    pub fn current_page_count(&self) -> Option<usize> {
        self.current_sheet_path()
            .map(|sheet_path| sheet_path.sheet_count)
    }

    pub fn current_virtual_page_number(&self) -> Option<usize> {
        self.current_sheet_path()
            .map(|sheet_path| sheet_path.sheet_number)
    }

    pub fn current_schematic(&self) -> Option<&Schematic> {
        let current_sheet_path = self.current_sheet_path()?;
        self.schematics
            .iter()
            .find(|schematic| schematic.path == current_sheet_path.schematic_path)
    }

    pub fn set_current_sheet_path(&mut self, instance_path: &str) -> bool {
        if self.sheet_path(instance_path).is_some() {
            let previous = self.current_sheet_path().cloned();
            let next = self.sheet_path(instance_path).cloned();
            reset_reused_screen_symbol_state(
                &mut self.schematics,
                &self.sheet_paths,
                previous.as_ref(),
                next.as_ref(),
            );
            refresh_current_screen_page_state(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                instance_path,
            );
            self.current_sheet_instance_path = instance_path.to_string();
            if let Some(schematic) = self
                .current_sheet_path()
                .map(|sheet_path| sheet_path.schematic_path.clone())
                .and_then(|schematic_path| {
                    self.schematics
                        .iter_mut()
                        .find(|schematic| schematic.path == schematic_path)
                })
            {
                apply_symbol_instance_state(schematic, instance_path);
            }
            true
        } else {
            false
        }
    }

    pub fn children_of<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a HierarchyLink> + 'a {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.links
            .iter()
            .filter(move |link| link.parent_path == canonical)
    }

    pub fn sheet_paths_of<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a LoadedSheetPath> + 'a {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.sheet_paths
            .iter()
            .filter(move |sheet_path| sheet_path.schematic_path == canonical)
    }

    pub fn parents_of<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a HierarchyLink> + 'a {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.links
            .iter()
            .filter(move |link| link.child_path == canonical)
    }

    pub fn sheet_path(&self, instance_path: &str) -> Option<&LoadedSheetPath> {
        self.sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == instance_path)
    }

    pub fn sheet_path_for_symbol_path(&self, symbol_path: &str) -> Option<&LoadedSheetPath> {
        self.sheet_paths
            .iter()
            .filter(|sheet_path| {
                symbol_path == sheet_path.symbol_path
                    || symbol_path.starts_with(&(sheet_path.symbol_path.clone() + "/"))
            })
            .max_by_key(|sheet_path| sheet_path.symbol_path.len())
    }

    pub fn parent_sheet_path(&self, instance_path: &str) -> Option<&LoadedSheetPath> {
        if instance_path.is_empty() {
            return None;
        }

        let current = self.sheet_path(instance_path)?;
        let parent_symbol_path = current
            .symbol_path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or_default();

        self.sheet_path_for_symbol_path(parent_symbol_path)
    }

    pub fn ancestor_sheet_paths<'a>(&'a self, instance_path: &str) -> Vec<&'a LoadedSheetPath> {
        let mut ancestors = Vec::new();
        let mut current = self.parent_sheet_path(instance_path);

        while let Some(sheet_path) = current {
            ancestors.push(sheet_path);
            current = self.parent_sheet_path(&sheet_path.instance_path);
        }

        ancestors
    }

    pub fn child_sheet_paths<'a>(&'a self, instance_path: &str) -> Vec<&'a LoadedSheetPath> {
        self.sheet_paths
            .iter()
            .filter(|sheet_path| {
                self.parent_sheet_path(&sheet_path.instance_path)
                    .is_some_and(|parent| parent.instance_path == instance_path)
            })
            .collect()
    }
}

pub fn load_schematic_tree(root: &Path) -> Result<LoadResult, Error> {
    let mut loader = SchematicLoader::new();
    let root_path = loader.load_schematic_file(root)?;
    let mut sheet_paths = loader.build_sheet_list_sorted_by_page_numbers(&root_path);
    loader.update_symbol_instance_data(&root_path, &sheet_paths);
    loader.update_sheet_instance_data(&root_path, &mut sheet_paths);
    loader.fix_legacy_power_symbol_mismatches(&root_path);
    loader.annotate_power_symbols();
    loader.migrate_sim_models();
    loader.set_sheet_number_and_count(&mut sheet_paths);
    loader.recompute_intersheet_refs(&sheet_paths);
    loader.update_all_screen_references(&sheet_paths);
    Ok(LoadResult {
        root_path,
        schematics: loader.schematics,
        links: loader.links,
        sheet_paths,
        current_sheet_instance_path: String::new(),
    })
}

struct SchematicLoader {
    schematics: Vec<Schematic>,
    links: Vec<HierarchyLink>,
    loaded_by_canonical: HashMap<PathBuf, usize>,
    current_sheet_path: Vec<PathBuf>,
    current_path: Vec<PathBuf>,
}

#[derive(Clone)]
struct PowerLibSymbolInfo {
    power: bool,
    local_power: bool,
    first_pin_name: Option<String>,
    first_pin_electrical_type: Option<String>,
    first_pin_visible: bool,
}

impl SchematicLoader {
    fn new() -> Self {
        Self {
            schematics: Vec::new(),
            links: Vec::new(),
            loaded_by_canonical: HashMap::new(),
            current_sheet_path: Vec::new(),
            current_path: Vec::new(),
        }
    }

    fn load_schematic_file(&mut self, file_name: &Path) -> Result<PathBuf, Error> {
        let canonical_root = file_name
            .canonicalize()
            .unwrap_or_else(|_| file_name.to_path_buf());
        let root_dir = canonical_root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        self.current_path.push(root_dir);
        self.load_hierarchy(&canonical_root)?;
        self.current_path.pop();

        Ok(canonical_root)
    }

    fn load_hierarchy(&mut self, sheet_path: &Path) -> Result<usize, Error> {
        let canonical = sheet_path
            .canonicalize()
            .unwrap_or_else(|_| sheet_path.to_path_buf());

        self.reject_direct_ancestor_cycle(&canonical)?;

        if let Some(existing_index) = self.loaded_by_canonical.get(&canonical) {
            return Ok(*existing_index);
        }

        self.current_sheet_path.push(canonical.clone());

        let current_dir = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        self.current_path.push(current_dir);

        let schematic = self.load_file(&canonical)?;
        let references = schematic.sheet_references();
        let index = self.schematics.len();
        self.loaded_by_canonical.insert(canonical.clone(), index);
        self.schematics.push(schematic);

        for reference in references {
            self.load_child_sheet(&canonical, reference)?;
        }

        self.current_path.pop();
        self.current_sheet_path.pop();
        Ok(index)
    }

    fn load_file(&self, file_name: &Path) -> Result<Schematic, Error> {
        parse_schematic_file(file_name)
    }

    fn load_child_sheet(
        &mut self,
        parent_path: &Path,
        reference: SheetReference,
    ) -> Result<(), Error> {
        let resolved = if reference.resolved_path.is_absolute() {
            reference.resolved_path.clone()
        } else {
            self.current_path
                .last()
                .cloned()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(&reference.filename)
        };
        let child_canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

        self.reject_direct_ancestor_cycle(&child_canonical)?;

        let reused_existing_child = self.loaded_by_canonical.contains_key(&child_canonical);
        self.links.push(HierarchyLink {
            parent_path: parent_path.to_path_buf(),
            child_path: child_canonical.clone(),
            sheet_uuid: reference.sheet_uuid,
            sheet_name: reference.sheet_name,
            filename: reference.filename,
            reused_existing_child,
        });

        if !reused_existing_child {
            self.load_hierarchy(&resolved)?;
        }

        Ok(())
    }

    fn reject_direct_ancestor_cycle(&self, canonical: &Path) -> Result<(), Error> {
        if let Some(cycle_start) = self
            .current_sheet_path
            .iter()
            .position(|ancestor| ancestor == canonical)
        {
            let mut chain = self.current_sheet_path[cycle_start..]
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>();
            chain.push(canonical.display().to_string());
            return Err(Error::Validation {
                path: canonical.to_path_buf(),
                diagnostic: Diagnostic::error(
                    "sheet-cycle",
                    format!(
                        "sheet recursion detected through direct ancestors: {}",
                        chain.join(" -> ")
                    ),
                )
                .with_path(canonical.to_path_buf()),
            });
        }
        Ok(())
    }

    fn build_sheet_list_sorted_by_page_numbers(&self, root_path: &Path) -> Vec<LoadedSheetPath> {
        let Some(root_index) = self.loaded_by_canonical.get(root_path) else {
            return Vec::new();
        };
        let root = &self.schematics[*root_index];
        let Some(root_uuid) = root
            .root_sheet
            .uuid
            .as_ref()
            .or(root.screen.uuid.as_ref())
            .cloned()
        else {
            return Vec::new();
        };

        let mut sheet_paths = vec![LoadedSheetPath {
            schematic_path: root_path.to_path_buf(),
            instance_path: String::new(),
            symbol_path: format!("/{root_uuid}"),
            sheet_uuid: Some(root_uuid.clone()),
            sheet_name: None,
            page: root.screen.root_sheet_page.clone().or_else(|| {
                root.screen
                    .sheet_instances
                    .iter()
                    .find(|instance| {
                        instance.path.is_empty() || instance.path == format!("/{root_uuid}")
                    })
                    .and_then(|instance| instance.page.clone())
            }),
            sheet_number: 0,
            sheet_count: 0,
        }];

        self.build_child_sheet_paths(root_path, &format!("/{root_uuid}"), &mut sheet_paths);
        sort_loaded_sheet_paths(&mut sheet_paths);
        sheet_paths
    }

    fn build_child_sheet_paths(
        &self,
        parent_path: &Path,
        parent_symbol_path: &str,
        out: &mut Vec<LoadedSheetPath>,
    ) {
        for link in self
            .links
            .iter()
            .filter(|link| link.parent_path == parent_path)
        {
            let Some(sheet_uuid) = link.sheet_uuid.as_ref() else {
                continue;
            };

            let instance_path = format!("{parent_symbol_path}/{sheet_uuid}");
            out.push(LoadedSheetPath {
                schematic_path: link.child_path.clone(),
                instance_path: instance_path.clone(),
                symbol_path: instance_path.clone(),
                sheet_uuid: link.sheet_uuid.clone(),
                sheet_name: link.sheet_name.clone(),
                page: None,
                sheet_number: 0,
                sheet_count: 0,
            });
            self.build_child_sheet_paths(&link.child_path, &instance_path, out);
        }
    }

    fn update_symbol_instance_data(&mut self, root_path: &Path, sheet_paths: &[LoadedSheetPath]) {
        let Some(root_index) = self.loaded_by_canonical.get(root_path).copied() else {
            return;
        };
        let root_version = self.schematics[root_index].version;
        if root_version >= 20221002 {
            return;
        }

        let symbol_instances = self.schematics[root_index].screen.symbol_instances.clone();
        let occurrence_counts: HashMap<PathBuf, usize> =
            sheet_paths
                .iter()
                .fold(HashMap::new(), |mut counts, sheet_path| {
                    *counts.entry(sheet_path.schematic_path.clone()).or_insert(0) += 1;
                    counts
                });
        let mut seeded_reused_schematics = BTreeSet::new();

        for sheet_path in sheet_paths {
            let Some(schematic_index) = self
                .loaded_by_canonical
                .get(&sheet_path.schematic_path)
                .copied()
            else {
                continue;
            };

            for item in &mut self.schematics[schematic_index].screen.items {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };
                let Some(symbol_uuid) = symbol.uuid.as_ref() else {
                    continue;
                };

                let full_path = format!("{}/{}", sheet_path.symbol_path, symbol_uuid);
                let Some(instance) = symbol_instances
                    .iter()
                    .find(|instance| instance.path == full_path)
                else {
                    continue;
                };

                if let Some(reference) = instance.reference.as_ref() {
                    symbol.set_field_text(PropertyKind::SymbolReference, reference.clone());
                }

                if let Some(unit) = instance.unit {
                    symbol.unit = Some(unit);
                }

                if let Some(value) = instance.value.as_ref() {
                    if !value.is_empty() {
                        symbol.set_field_text(PropertyKind::SymbolValue, value.clone());
                    }
                }

                if let Some(footprint) = instance.footprint.as_ref() {
                    if !footprint.is_empty() {
                        symbol.set_field_text(PropertyKind::SymbolFootprint, footprint.clone());
                    }
                }

                let mut local_instance = crate::model::SymbolLocalInstance {
                    project: String::new(),
                    path: sheet_path.instance_path.clone(),
                    reference: None,
                    unit: Some(1),
                    value: instance.value.clone(),
                    footprint: instance.footprint.clone(),
                    variants: std::collections::BTreeMap::new(),
                };
                local_instance.reference = instance.reference.clone();
                local_instance.unit = instance.unit;
                if local_instance.unit.is_none() {
                    local_instance.unit = Some(1);
                }
                symbol
                    .instances
                    .retain(|existing| existing.path != local_instance.path);
                symbol.instances.push(local_instance);
            }

            if occurrence_counts
                .get(&sheet_path.schematic_path)
                .copied()
                .unwrap_or(0)
                > 1
                && seeded_reused_schematics.insert(sheet_path.schematic_path.clone())
            {
                seed_first_symbol_instance_state(&mut self.schematics[schematic_index]);
            }
        }
    }

    fn update_sheet_instance_data(&self, root_path: &Path, sheet_paths: &mut [LoadedSheetPath]) {
        let Some(root_index) = self.loaded_by_canonical.get(root_path).copied() else {
            return;
        };
        let sheet_instances = &self.schematics[root_index].screen.sheet_instances;

        for sheet_path in sheet_paths.iter_mut() {
            if let Some(instance) = sheet_instances
                .iter()
                .find(|instance| instance.path == sheet_path.instance_path)
            {
                sheet_path.page = instance.page.clone();
            }
        }

        if all_sheet_page_numbers_empty(sheet_paths) {
            set_initial_page_numbers(sheet_paths);
        }

        sort_loaded_sheet_paths(sheet_paths);
    }

    fn fix_legacy_power_symbol_mismatches(&mut self, root_path: &Path) {
        let Some(root_index) = self.loaded_by_canonical.get(root_path).copied() else {
            return;
        };
        if self.schematics[root_index].version >= 20230221 {
            return;
        }

        for schematic in &mut self.schematics {
            let power_infos = Self::collect_power_lib_symbol_info(schematic);

            for item in &mut schematic.screen.items {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };

                let lib_symbol_name = symbol.lib_name.as_deref().unwrap_or(symbol.lib_id.as_str());

                let Some(lib_symbol) = power_infos.get(lib_symbol_name) else {
                    continue;
                };

                if !lib_symbol.power || lib_symbol.local_power {
                    continue;
                }

                if lib_symbol.first_pin_electrical_type.as_deref() != Some("power_in")
                    || lib_symbol.first_pin_visible
                {
                    continue;
                }

                let Some(pin_name) = lib_symbol.first_pin_name.clone() else {
                    continue;
                };

                symbol.set_field_text(PropertyKind::SymbolValue, pin_name);
            }
        }
    }

    fn annotate_power_symbols(&mut self) {
        let mut seen_refs: HashMap<String, usize> = HashMap::new();
        let mut normalized_refs: Vec<(usize, usize, String)> = Vec::new();

        for (schematic_index, schematic) in self.schematics.iter().enumerate() {
            let power_infos = Self::collect_power_lib_symbol_info(schematic);

            for (item_index, item) in schematic.screen.items.iter().enumerate() {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };

                let lib_symbol_name = symbol.lib_name.as_deref().unwrap_or(symbol.lib_id.as_str());

                let Some(lib_symbol) = power_infos.get(lib_symbol_name) else {
                    continue;
                };

                if !lib_symbol.power {
                    continue;
                }

                let Some(reference) = symbol
                    .properties
                    .iter()
                    .find(|property| property.kind == PropertyKind::SymbolReference)
                    .map(|property| property.value.clone())
                else {
                    continue;
                };

                if reference.is_empty() {
                    continue;
                }

                let duplicate_index = seen_refs.entry(reference.clone()).or_insert(0);
                let mut normalized = reference;

                if *duplicate_index > 0 {
                    while normalized
                        .chars()
                        .last()
                        .is_some_and(|ch| ch.is_ascii_digit())
                    {
                        normalized.pop();
                    }
                }

                *duplicate_index += 1;

                if !normalized.starts_with('#') {
                    normalized.insert(0, '#');
                }

                normalized_refs.push((schematic_index, item_index, normalized));
            }
        }

        for (schematic_index, item_index, normalized) in normalized_refs {
            let Some(SchItem::Symbol(symbol)) = self.schematics[schematic_index]
                .screen
                .items
                .get_mut(item_index)
            else {
                continue;
            };

            symbol.set_field_text(PropertyKind::SymbolReference, normalized);
        }
    }

    fn migrate_sim_models(&mut self) {
        for schematic in &mut self.schematics {
            let mut migrated = false;

            for item in &mut schematic.screen.items {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };

                let has_legacy_spice_fields = symbol.properties.iter().any(|property| {
                    matches!(
                        property.key.as_str(),
                        "Spice_Primitive"
                            | "Spice_Node_Sequence"
                            | "Spice_Model"
                            | "Spice_Netlist_Enabled"
                            | "Spice_Lib_File"
                    )
                });

                if has_legacy_spice_fields {
                    let inferred_device = symbol
                        .properties
                        .iter()
                        .find(|property| {
                            property.kind.is_user_field() && property.key == "Spice_Primitive"
                        })
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let can_infer_from_value = !inferred_device.is_empty()
                        && !symbol.properties.iter().any(|property| {
                            matches!(property.key.as_str(), "Spice_Model" | "Spice_Lib_File")
                        })
                        && matches!(inferred_device.as_str(), "R" | "L" | "C" | "V" | "I")
                        && symbol.prefix.starts_with(&inferred_device);

                    if can_infer_from_value {
                        let _primitive_field = take_symbol_user_field(symbol, "Spice_Primitive");
                        let node_sequence_field =
                            take_symbol_user_field(symbol, "Spice_Node_Sequence");
                        let _legacy_enable =
                            take_symbol_user_field(symbol, "Spice_Netlist_Enabled");
                        let _lib_field = take_symbol_user_field(symbol, "Spice_Lib_File");

                        if let Some(mut pin_map_field) =
                            node_sequence_field.map(legacy_spice_pin_map_field)
                        {
                            pin_map_field.key = "Sim.Pins".to_string();
                            symbol.properties.push(pin_map_field);
                        }

                        symbol.sync_sim_model_from_properties();
                        migrated = true;
                        continue;
                    }

                    let legacy_device = symbol
                        .properties
                        .iter()
                        .find(|property| {
                            property.kind.is_user_field() && property.key == "Spice_Primitive"
                        })
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let legacy_model = symbol
                        .properties
                        .iter()
                        .find(|property| {
                            property.kind.is_user_field() && property.key == "Spice_Model"
                        })
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let has_legacy_lib = symbol.properties.iter().any(|property| {
                        property.kind.is_user_field() && property.key == "Spice_Lib_File"
                    });

                    if !has_legacy_lib
                        && matches!(legacy_device.as_str(), "V" | "I")
                        && parse_legacy_dc_model_value(&legacy_model).is_some()
                    {
                        let _primitive_field = take_symbol_user_field(symbol, "Spice_Primitive");
                        let node_sequence_field =
                            take_symbol_user_field(symbol, "Spice_Node_Sequence");
                        let model_field = take_symbol_user_field(symbol, "Spice_Model");
                        let _legacy_enable =
                            take_symbol_user_field(symbol, "Spice_Netlist_Enabled");
                        let _lib_field = take_symbol_user_field(symbol, "Spice_Lib_File");
                        let source_pins = symbol_source_pin_numbers(symbol);
                        let dc_value = parse_legacy_dc_model_value(
                            model_field
                                .as_ref()
                                .map(|property| property.value.as_str())
                                .unwrap_or(""),
                        )
                        .expect("checked above");

                        symbol.set_field_text(PropertyKind::SymbolValue, dc_value);

                        let device_template = model_field.clone().unwrap_or_else(|| {
                            Property::new_named(PropertyKind::User, "", String::new(), false)
                        });
                        let mut sim_device_field = device_template.clone();
                        sim_device_field.key = "Sim.Device".to_string();
                        sim_device_field.value = legacy_device;
                        let mut sim_type_field = device_template;
                        sim_type_field.key = "Sim.Type".to_string();
                        sim_type_field.value = "DC".to_string();
                        symbol.properties.push(sim_device_field);
                        symbol.properties.push(sim_type_field);

                        if let Some(mut pin_map_field) =
                            node_sequence_field.map(legacy_spice_pin_map_field)
                        {
                            pin_map_field.key = "Sim.Pins".to_string();
                            symbol.properties.push(pin_map_field);
                        } else if !source_pins.is_empty() {
                            let template = model_field.unwrap_or_else(|| {
                                Property::new_named(PropertyKind::User, "", String::new(), false)
                            });
                            let mut pin_map_field = default_sim_pins_field(template, &source_pins);
                            pin_map_field.key = "Sim.Pins".to_string();
                            symbol.properties.push(pin_map_field);
                        }

                        symbol.sync_sim_model_from_properties();
                        migrated = true;
                        continue;
                    }

                    if !has_legacy_lib
                        && matches!(legacy_device.as_str(), "V" | "I")
                        && parse_legacy_source_model(&legacy_model).is_some()
                    {
                        let primitive_field = take_symbol_user_field(symbol, "Spice_Primitive");
                        let node_sequence_field =
                            take_symbol_user_field(symbol, "Spice_Node_Sequence");
                        let model_field = take_symbol_user_field(symbol, "Spice_Model");
                        let _legacy_enable =
                            take_symbol_user_field(symbol, "Spice_Netlist_Enabled");
                        let _lib_field = take_symbol_user_field(symbol, "Spice_Lib_File");
                        let source_pins = symbol_source_pin_numbers(symbol);
                        let source_model = parse_legacy_source_model(
                            model_field
                                .as_ref()
                                .map(|property| property.value.as_str())
                                .unwrap_or(""),
                        )
                        .expect("checked above");

                        let device_template = primitive_field
                            .clone()
                            .or_else(|| model_field.clone())
                            .unwrap_or_else(|| {
                                Property::new_named(PropertyKind::User, "", String::new(), false)
                            });
                        let mut sim_device_field = device_template.clone();
                        sim_device_field.key = "Sim.Device".to_string();
                        sim_device_field.value = legacy_device;
                        let mut sim_type_field = device_template;
                        sim_type_field.key = "Sim.Type".to_string();
                        sim_type_field.value = source_model.kind.to_string();

                        let params_template = model_field
                            .clone()
                            .or_else(|| primitive_field.clone())
                            .unwrap_or_else(|| {
                                Property::new_named(PropertyKind::User, "", String::new(), false)
                            });
                        let mut sim_params_field = params_template;
                        sim_params_field.key = "Sim.Params".to_string();
                        sim_params_field.value = source_model
                            .params
                            .iter()
                            .map(|(name, value)| format!("{name}={value}"))
                            .collect::<Vec<_>>()
                            .join(" ");

                        symbol.properties.push(sim_device_field);
                        symbol.properties.push(sim_type_field);
                        symbol.properties.push(sim_params_field);

                        if let Some(mut pin_map_field) =
                            node_sequence_field.map(legacy_spice_pin_map_field)
                        {
                            pin_map_field.key = "Sim.Pins".to_string();
                            symbol.properties.push(pin_map_field);
                        } else if !source_pins.is_empty() {
                            let template = model_field.unwrap_or_else(|| {
                                Property::new_named(PropertyKind::User, "", String::new(), false)
                            });
                            let mut pin_map_field = default_sim_pins_field(template, &source_pins);
                            pin_map_field.key = "Sim.Pins".to_string();
                            symbol.properties.push(pin_map_field);
                        }

                        symbol.sync_sim_model_from_properties();
                        migrated = true;
                        continue;
                    }

                    let can_raw_migrate = symbol.properties.iter().any(|property| {
                        matches!(property.key.as_str(), "Spice_Model" | "Spice_Lib_File")
                    });

                    if !can_raw_migrate {
                        symbol.sync_sim_model_from_properties();
                        continue;
                    }

                    let primitive_field = take_symbol_user_field(symbol, "Spice_Primitive");
                    let node_sequence_field = take_symbol_user_field(symbol, "Spice_Node_Sequence");
                    let model_field = take_symbol_user_field(symbol, "Spice_Model");
                    let _legacy_enable = take_symbol_user_field(symbol, "Spice_Netlist_Enabled");
                    let lib_field = take_symbol_user_field(symbol, "Spice_Lib_File");

                    let mut pin_map_field = node_sequence_field.map(legacy_spice_pin_map_field);
                    let source_pins = symbol_source_pin_numbers(symbol);
                    let device = primitive_field
                        .as_ref()
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let model = model_field
                        .as_ref()
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let lib = lib_field
                        .as_ref()
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();

                    if pin_map_field.is_none() && !source_pins.is_empty() {
                        let template = primitive_field
                            .clone()
                            .or_else(|| model_field.clone())
                            .or_else(|| lib_field.clone())
                            .unwrap_or_else(|| {
                                Property::new_named(PropertyKind::User, "", String::new(), false)
                            });
                        pin_map_field = Some(default_sim_pins_field(template, &source_pins));
                    }

                    let device_template = primitive_field
                        .clone()
                        .or_else(|| model_field.clone())
                        .or_else(|| lib_field.clone())
                        .unwrap_or_else(|| {
                            Property::new_named(PropertyKind::User, "", String::new(), false)
                        });
                    let mut sim_device_field = device_template;
                    sim_device_field.key = "Sim.Device".to_string();
                    sim_device_field.value = "SPICE".to_string();

                    let params_template = model_field
                        .clone()
                        .or_else(|| primitive_field.clone())
                        .or_else(|| lib_field.clone())
                        .unwrap_or_else(|| {
                            Property::new_named(PropertyKind::User, "", String::new(), false)
                        });
                    let mut sim_params_field = params_template;
                    sim_params_field.key = "Sim.Params".to_string();
                    sim_params_field.value = if device.is_empty() && lib.is_empty() {
                        model
                    } else {
                        format!(
                            "type=\"{device}\" model=\"{}\" lib=\"{lib}\"",
                            sim_params_field_value_escape(&model)
                        )
                    };

                    symbol.properties.push(sim_device_field);
                    symbol.properties.push(sim_params_field);

                    if let Some(mut pin_map_field) = pin_map_field {
                        pin_map_field.key = "Sim.Pins".to_string();
                        symbol.properties.push(pin_map_field);
                    }

                    symbol.sync_sim_model_from_properties();
                    migrated = true;
                    continue;
                }

                let source_pins = symbol_source_pin_numbers(symbol);

                for property in &mut symbol.properties {
                    match property.key.as_str() {
                        "Sim_Type" => {
                            property.key = "Sim.Type".to_string();
                            migrated = true;
                        }
                        "Sim_Device" => {
                            property.key = "Sim.Device".to_string();
                            migrated = true;
                        }
                        "Sim_Pins" => {
                            property.key = "Sim.Pins".to_string();

                            if !source_pins.is_empty() {
                                let pin_indexes =
                                    property.value.split_whitespace().collect::<Vec<_>>();
                                property.value = migrated_sim_pins_value(
                                    &symbol.prefix,
                                    &source_pins,
                                    &pin_indexes,
                                );
                            }

                            migrated = true;
                        }
                        "Sim_Params" => {
                            property.key = "Sim.Params".to_string();
                            migrated = true;
                        }
                        _ => {}
                    }
                }

                symbol.sync_sim_model_from_properties();
            }

            if migrated {
                schematic.screen.content_modified = true;
            }
        }
    }

    fn collect_power_lib_symbol_info(schematic: &Schematic) -> HashMap<String, PowerLibSymbolInfo> {
        schematic
            .screen
            .lib_symbols
            .iter()
            .map(|lib_symbol| {
                let first_pin = lib_symbol
                    .units
                    .iter()
                    .flat_map(|unit| unit.draw_items.iter())
                    .find(|draw_item| draw_item.kind == "pin");

                (
                    lib_symbol.lib_id.clone(),
                    PowerLibSymbolInfo {
                        power: lib_symbol.power,
                        local_power: lib_symbol.local_power,
                        first_pin_name: first_pin.and_then(|pin| pin.name.clone()),
                        first_pin_electrical_type: first_pin
                            .and_then(|pin| pin.electrical_type.clone()),
                        first_pin_visible: first_pin.is_some_and(|pin| pin.visible),
                    },
                )
            })
            .collect()
    }

    fn set_sheet_number_and_count(&mut self, sheet_paths: &mut [LoadedSheetPath]) {
        let sheet_count = sheet_paths.len();
        let occurrence_counts: HashMap<PathBuf, usize> =
            sheet_paths
                .iter()
                .fold(HashMap::new(), |mut counts, sheet_path| {
                    *counts.entry(sheet_path.schematic_path.clone()).or_insert(0) += 1;
                    counts
                });

        for (index, sheet_path) in sheet_paths.iter_mut().enumerate() {
            sheet_path.sheet_number = index + 1;
            sheet_path.sheet_count = sheet_count;

            let Some(schematic_index) = self
                .loaded_by_canonical
                .get(&sheet_path.schematic_path)
                .copied()
            else {
                continue;
            };

            let screen = &mut self.schematics[schematic_index].screen;

            if occurrence_counts
                .get(&sheet_path.schematic_path)
                .copied()
                .unwrap_or(0)
                > 1
            {
                screen.page_number = None;
                screen.page_count = None;
                screen.virtual_page_number = None;
                continue;
            }

            if screen.virtual_page_number.is_some() {
                continue;
            }

            screen.page_number = sheet_path
                .page
                .clone()
                .or_else(|| Some(sheet_path.sheet_number.to_string()));
            screen.page_count = Some(sheet_count);
            screen.virtual_page_number = Some(sheet_path.sheet_number);
        }
    }

    fn recompute_intersheet_refs(&mut self, sheet_paths: &[LoadedSheetPath]) {
        let mut page_refs_map: HashMap<String, BTreeSet<usize>> = HashMap::new();
        let mut virtual_page_to_sheet_page = HashMap::new();

        for sheet_path in sheet_paths {
            virtual_page_to_sheet_page.insert(
                sheet_path.sheet_number,
                sheet_path
                    .page
                    .clone()
                    .unwrap_or_else(|| sheet_path.sheet_number.to_string()),
            );

            let Some(schematic_index) = self
                .loaded_by_canonical
                .get(&sheet_path.schematic_path)
                .copied()
            else {
                continue;
            };

            for item in &self.schematics[schematic_index].screen.items {
                if let SchItem::Label(label) = item {
                    if label.kind == crate::model::LabelKind::Global {
                        page_refs_map
                            .entry(label.text.clone())
                            .or_default()
                            .insert(sheet_path.sheet_number);
                    }
                }
            }
        }

        for sheet_path in sheet_paths {
            let Some(schematic_index) = self
                .loaded_by_canonical
                .get(&sheet_path.schematic_path)
                .copied()
            else {
                continue;
            };

            for item in &mut self.schematics[schematic_index].screen.items {
                let SchItem::Label(label) = item else {
                    continue;
                };
                if label.kind != crate::model::LabelKind::Global {
                    continue;
                }

                let value = if let Some(page_numbers) = page_refs_map.get(&label.text) {
                    let refs = page_numbers
                        .iter()
                        .filter_map(|page_number| virtual_page_to_sheet_page.get(page_number))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("[{refs}]")
                } else {
                    "[?]".to_string()
                };

                let intersheet_refs = label
                    .properties
                    .iter_mut()
                    .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
                    .expect("global labels start with intersheet refs property");
                intersheet_refs.value = value;
                intersheet_refs.id = PropertyKind::GlobalLabelIntersheetRefs.default_field_id();
                intersheet_refs.key = PropertyKind::GlobalLabelIntersheetRefs
                    .canonical_key()
                    .to_string();
            }
        }
    }

    fn update_all_screen_references(&mut self, sheet_paths: &[LoadedSheetPath]) {
        let occurrence_counts: HashMap<PathBuf, usize> =
            sheet_paths
                .iter()
                .fold(HashMap::new(), |mut counts, sheet_path| {
                    *counts.entry(sheet_path.schematic_path.clone()).or_insert(0) += 1;
                    counts
                });
        let mut seeded_reused_schematics = BTreeSet::new();

        for sheet_path in sheet_paths {
            let Some(schematic_index) = self
                .loaded_by_canonical
                .get(&sheet_path.schematic_path)
                .copied()
            else {
                continue;
            };

            let occurrence_count = occurrence_counts
                .get(&sheet_path.schematic_path)
                .copied()
                .unwrap_or(0);

            if occurrence_count == 1 {
                for item in &mut self.schematics[schematic_index].screen.items {
                    let SchItem::Symbol(symbol) = item else {
                        continue;
                    };

                    let Some(instance) = symbol
                        .instances
                        .iter()
                        .find(|instance| instance.path == sheet_path.instance_path)
                        .cloned()
                    else {
                        continue;
                    };

                    if let Some(reference) = instance.reference {
                        symbol.set_field_text(PropertyKind::SymbolReference, reference);
                    }

                    if let Some(unit) = instance.unit {
                        symbol.unit = Some(unit);
                    }

                    if let Some(value) = instance.value {
                        symbol.set_field_text(PropertyKind::SymbolValue, value);
                    }

                    if let Some(footprint) = instance.footprint {
                        symbol.set_field_text(PropertyKind::SymbolFootprint, footprint);
                    }
                }
            } else if seeded_reused_schematics.insert(sheet_path.schematic_path.clone()) {
                for item in &mut self.schematics[schematic_index].screen.items {
                    let SchItem::Symbol(symbol) = item else {
                        continue;
                    };

                    let Some(instance) = symbol.instances.first().cloned() else {
                        continue;
                    };

                    if let Some(reference) = instance.reference {
                        symbol.set_field_text(PropertyKind::SymbolReference, reference);
                    }

                    if let Some(unit) = instance.unit {
                        symbol.unit = Some(unit);
                    }

                    if let Some(value) = instance.value {
                        symbol.set_field_text(PropertyKind::SymbolValue, value);
                    }

                    if let Some(footprint) = instance.footprint {
                        symbol.set_field_text(PropertyKind::SymbolFootprint, footprint);
                    }
                }
            }

            for item in &mut self.schematics[schematic_index].screen.items {
                let SchItem::Label(label) = item else {
                    continue;
                };

                if label.kind != crate::model::LabelKind::Global {
                    continue;
                }

                let Some(intersheet_refs) = label
                    .properties
                    .iter_mut()
                    .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
                else {
                    continue;
                };

                if (intersheet_refs.at.is_none() || intersheet_refs.at == Some([0.0, 0.0]))
                    && !intersheet_refs.visible
                {
                    intersheet_refs.at = Some(label.at);
                }
            }
        }
    }
}

fn apply_symbol_instance_state(schematic: &mut Schematic, instance_path: &str) {
    for item in &mut schematic.screen.items {
        let SchItem::Symbol(symbol) = item else {
            continue;
        };

        let Some(instance) = symbol
            .instances
            .iter()
            .find(|instance| instance.path == instance_path)
            .cloned()
        else {
            continue;
        };

        if let Some(reference) = instance.reference {
            symbol.set_field_text(PropertyKind::SymbolReference, reference);
        }

        if let Some(unit) = instance.unit {
            symbol.unit = Some(unit);
        }

        if let Some(value) = instance.value {
            symbol.set_field_text(PropertyKind::SymbolValue, value);
        }

        if let Some(footprint) = instance.footprint {
            symbol.set_field_text(PropertyKind::SymbolFootprint, footprint);
        }
    }
}

fn seed_first_symbol_instance_state(schematic: &mut Schematic) {
    for item in &mut schematic.screen.items {
        let SchItem::Symbol(symbol) = item else {
            continue;
        };

        let Some(instance) = symbol.instances.first().cloned() else {
            continue;
        };

        if let Some(reference) = instance.reference {
            symbol.set_field_text(PropertyKind::SymbolReference, reference);
        }

        if let Some(unit) = instance.unit {
            symbol.unit = Some(unit);
        }

        if let Some(value) = instance.value {
            symbol.set_field_text(PropertyKind::SymbolValue, value);
        }

        if let Some(footprint) = instance.footprint {
            symbol.set_field_text(PropertyKind::SymbolFootprint, footprint);
        }
    }
}

fn reset_reused_screen_symbol_state(
    schematics: &mut [Schematic],
    sheet_paths: &[LoadedSheetPath],
    previous: Option<&LoadedSheetPath>,
    next: Option<&LoadedSheetPath>,
) {
    let Some(previous) = previous else {
        return;
    };

    let previous_occurrence_count = sheet_paths
        .iter()
        .filter(|sheet_path| sheet_path.schematic_path == previous.schematic_path)
        .count();
    let switching_schematic = next
        .map(|next| next.schematic_path != previous.schematic_path)
        .unwrap_or(true);

    if previous_occurrence_count <= 1 || !switching_schematic {
        return;
    }

    if let Some(schematic) = schematics
        .iter_mut()
        .find(|schematic| schematic.path == previous.schematic_path)
    {
        seed_first_symbol_instance_state(schematic);
    }
}

fn symbol_source_pin_numbers(symbol: &crate::model::Symbol) -> Vec<String> {
    let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
        return Vec::new();
    };

    let unit_number = symbol.unit.unwrap_or(1);
    let body_style = symbol.body_style.unwrap_or(1);
    let mut pin_numbers = lib_symbol
        .units
        .iter()
        .filter(|unit| unit.unit_number == unit_number && unit.body_style == body_style)
        .flat_map(|unit| unit.draw_items.iter())
        .filter(|item| item.kind == "pin")
        .filter_map(|item| item.number.clone())
        .collect::<Vec<_>>();

    pin_numbers.sort_by(compare_pin_numbers);
    pin_numbers
}

fn compare_pin_numbers(a: &String, b: &String) -> std::cmp::Ordering {
    match (a.parse::<i32>(), b.parse::<i32>()) {
        (Ok(a_number), Ok(b_number)) => a_number.cmp(&b_number),
        _ => a.cmp(b),
    }
}

fn take_symbol_user_field(symbol: &mut crate::model::Symbol, key: &str) -> Option<Property> {
    let index = symbol
        .properties
        .iter()
        .position(|property| property.kind.is_user_field() && property.key == key)?;
    Some(symbol.properties.remove(index))
}

fn legacy_spice_pin_map_field(mut property: Property) -> Property {
    let pin_map = property
        .value
        .split(|ch: char| matches!(ch, '{' | '}' | ':' | ',' | ';') || ch.is_whitespace())
        .filter(|token| !token.is_empty())
        .enumerate()
        .map(|(index, token)| {
            let symbol_pin = token.parse::<i32>().unwrap_or(1);
            format!("{symbol_pin}={}", index + 1)
        })
        .collect::<Vec<_>>()
        .join(" ");
    property.value = pin_map;
    property
}

fn default_sim_pins_field(mut property: Property, source_pins: &[String]) -> Property {
    property.value = source_pins
        .iter()
        .enumerate()
        .map(|(index, symbol_pin)| format!("{symbol_pin}={}", index + 1))
        .collect::<Vec<_>>()
        .join(" ");
    property
}

fn sim_params_field_value_escape(model: &str) -> String {
    model.replace('"', "\\\"")
}

struct LegacySourceModel {
    kind: &'static str,
    params: Vec<(&'static str, String)>,
}

fn parse_legacy_dc_model_value(model: &str) -> Option<String> {
    let tokens = model
        .split(|ch: char| matches!(ch, '(' | ')') || ch.is_whitespace())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    if tokens.len() == 2 && tokens[0].eq_ignore_ascii_case("dc") {
        return Some(tokens[1].to_string());
    }

    None
}

fn parse_legacy_source_model(model: &str) -> Option<LegacySourceModel> {
    let open = model.find('(')?;
    let close = model.rfind(')')?;

    if close <= open {
        return None;
    }

    let kind = model[..open].trim();
    let args = model[open + 1..close]
        .split(|ch: char| matches!(ch, ' ' | '\t' | '\n' | '\r' | ','))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect::<Vec<_>>();

    let param_names = match kind.to_ascii_uppercase().as_str() {
        "SIN" => Some(("SIN", &["dc", "ampl", "f", "td", "theta", "phase"][..])),
        "PULSE" => Some((
            "PULSE",
            &["y1", "y2", "td", "tr", "tf", "tw", "per", "np"][..],
        )),
        "EXP" => Some(("EXP", &["y1", "y2", "td1", "tau1", "td2", "tau2"][..])),
        "AM" => Some((
            "AM",
            &["vo", "vmo", "vma", "fm", "fc", "td", "phasem", "phasec"][..],
        )),
        "SFFM" => Some((
            "SFFM",
            &["vo", "va", "fm", "mdi", "fc", "phasem", "phasec"][..],
        )),
        _ => None,
    }?;

    let (kind, names) = param_names;
    let params = names
        .iter()
        .zip(args)
        .map(|(name, value)| (*name, value))
        .collect::<Vec<_>>();

    Some(LegacySourceModel { kind, params })
}

fn migrated_sim_pins_value(prefix: &str, source_pins: &[String], pin_indexes: &[&str]) -> String {
    let is_passive = prefix.starts_with('R') || prefix.starts_with('L') || prefix.starts_with('C');

    if is_passive && pin_indexes.len() == 2 && source_pins.len() == 2 {
        if pin_indexes[0] == "2" {
            return format!("{}=- {}=+", source_pins[0], source_pins[1]);
        }

        return format!("{}=+ {}=-", source_pins[0], source_pins[1]);
    }

    source_pins
        .iter()
        .zip(pin_indexes.iter())
        .map(|(symbol_pin, model_pin)| format!("{symbol_pin}={model_pin}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn refresh_current_screen_page_state(
    schematics: &mut [Schematic],
    sheet_paths: &[LoadedSheetPath],
    previous_instance_path: &str,
    next_instance_path: &str,
) {
    let previous = sheet_paths
        .iter()
        .find(|sheet_path| sheet_path.instance_path == previous_instance_path);
    let next = sheet_paths
        .iter()
        .find(|sheet_path| sheet_path.instance_path == next_instance_path);

    if let Some(previous) = previous {
        let previous_occurrence_count = sheet_paths
            .iter()
            .filter(|sheet_path| sheet_path.schematic_path == previous.schematic_path)
            .count();
        let switching_schematic = next
            .map(|next| next.schematic_path != previous.schematic_path)
            .unwrap_or(true);

        if previous_occurrence_count > 1 && switching_schematic {
            if let Some(schematic) = schematics
                .iter_mut()
                .find(|schematic| schematic.path == previous.schematic_path)
            {
                schematic.screen.page_number = None;
                schematic.screen.page_count = None;
                schematic.screen.virtual_page_number = None;
            }
        }
    }

    if let Some(next) = next {
        let next_occurrence_count = sheet_paths
            .iter()
            .filter(|sheet_path| sheet_path.schematic_path == next.schematic_path)
            .count();

        if next_occurrence_count > 1 {
            if let Some(schematic) = schematics
                .iter_mut()
                .find(|schematic| schematic.path == next.schematic_path)
            {
                schematic.screen.page_number = next
                    .page
                    .clone()
                    .or_else(|| Some(next.sheet_number.to_string()));
                schematic.screen.page_count = Some(next.sheet_count);
                schematic.screen.virtual_page_number = Some(next.sheet_number);
            }
        }
    }
}

fn sort_loaded_sheet_paths(sheet_paths: &mut [LoadedSheetPath]) {
    let original_order: HashMap<String, usize> = sheet_paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.instance_path.clone(), index))
        .collect();

    sheet_paths.sort_by(|a, b| compare_loaded_sheet_paths(a, b, &original_order));
}

fn compare_loaded_sheet_paths(
    a: &LoadedSheetPath,
    b: &LoadedSheetPath,
    original_order: &HashMap<String, usize>,
) -> std::cmp::Ordering {
    let page_cmp = match (&a.page, &b.page) {
        (Some(a_page), Some(b_page)) => compare_page_numbers(a_page, b_page),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };

    if page_cmp != std::cmp::Ordering::Equal {
        return page_cmp;
    }

    original_order
        .get(&a.instance_path)
        .cmp(&original_order.get(&b.instance_path))
        .then_with(|| a.instance_path.cmp(&b.instance_path))
        .then_with(|| a.schematic_path.cmp(&b.schematic_path))
}

fn compare_page_numbers(a: &str, b: &str) -> std::cmp::Ordering {
    if a == b {
        return std::cmp::Ordering::Equal;
    }

    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(a_num), Ok(b_num)) => return a_num.cmp(&b_num),
        (Ok(_), Err(_)) => return std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => return std::cmp::Ordering::Greater,
        (Err(_), Err(_)) => {}
    }

    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek().copied(), b_chars.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(a_ch), Some(b_ch)) if a_ch.is_ascii_digit() && b_ch.is_ascii_digit() => {
                let mut a_digits = String::new();
                let mut b_digits = String::new();

                while let Some(ch) = a_chars.peek().copied() {
                    if !ch.is_ascii_digit() {
                        break;
                    }
                    a_digits.push(ch);
                    a_chars.next();
                }

                while let Some(ch) = b_chars.peek().copied() {
                    if !ch.is_ascii_digit() {
                        break;
                    }
                    b_digits.push(ch);
                    b_chars.next();
                }

                let a_trimmed = a_digits.trim_start_matches('0');
                let b_trimmed = b_digits.trim_start_matches('0');
                let a_cmp_digits = if a_trimmed.is_empty() { "0" } else { a_trimmed };
                let b_cmp_digits = if b_trimmed.is_empty() { "0" } else { b_trimmed };

                match a_cmp_digits.len().cmp(&b_cmp_digits.len()) {
                    std::cmp::Ordering::Equal => match a_cmp_digits.cmp(b_cmp_digits) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    },
                    ordering => return ordering,
                }
            }
            (Some(a_ch), Some(b_ch)) => {
                a_chars.next();
                b_chars.next();

                match a_ch.cmp(&b_ch) {
                    std::cmp::Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
        }
    }
}

fn all_sheet_page_numbers_empty(sheet_paths: &[LoadedSheetPath]) -> bool {
    sheet_paths
        .iter()
        .filter(|path| !path.instance_path.is_empty())
        .all(|path| {
            path.page
                .as_deref()
                .is_none_or(|page| page.trim().is_empty())
        })
}

fn set_initial_page_numbers(sheet_paths: &mut [LoadedSheetPath]) {
    let mut page_number = 1;

    for sheet_path in sheet_paths.iter_mut() {
        if sheet_path.instance_path.is_empty() {
            continue;
        }

        sheet_path.page = Some(page_number.to_string());
        page_number += 1;
    }
}
