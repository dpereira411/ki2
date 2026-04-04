use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{Property, PropertyKind, SchItem, Schematic, SheetReference, Symbol};
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
}

impl LoadResult {
    pub fn children_of<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a HierarchyLink> + 'a {
        self.links
            .iter()
            .filter(move |link| link.parent_path == path)
    }
}

pub fn load_schematic_tree(root: &Path) -> Result<LoadResult, Error> {
    let mut loader = SchematicLoader::new();
    let root_path = loader.load_schematic_file(root)?;
    let mut sheet_paths = loader.build_sheet_list_sorted_by_page_numbers(&root_path);
    loader.update_symbol_instance_data(&root_path, &sheet_paths);
    loader.update_sheet_instance_data(&root_path, &mut sheet_paths);
    loader.fix_legacy_power_symbol_mismatches(&root_path);
    loader.set_sheet_number_and_count(&mut sheet_paths);
    loader.recompute_intersheet_refs(&sheet_paths);
    loader.update_all_screen_references(&sheet_paths);
    Ok(LoadResult {
        root_path,
        schematics: loader.schematics,
        links: loader.links,
        sheet_paths,
    })
}

struct SchematicLoader {
    schematics: Vec<Schematic>,
    links: Vec<HierarchyLink>,
    loaded_by_canonical: HashMap<PathBuf, usize>,
    current_sheet_path: Vec<PathBuf>,
    current_path: Vec<PathBuf>,
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
        self.load_hierarchy(None, &canonical_root)?;
        self.current_path.pop();

        Ok(canonical_root)
    }

    fn load_hierarchy(
        &mut self,
        parent_path: Option<&Path>,
        sheet_path: &Path,
    ) -> Result<usize, Error> {
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
            self.load_child_sheet(parent_path.unwrap_or(&canonical), reference)?;
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
            self.load_hierarchy(Some(parent_path), &resolved)?;
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
            page: root
                .screen
                .sheet_instances
                .iter()
                .find(|instance| instance.path.is_empty())
                .and_then(|instance| instance.page.clone()),
            sheet_number: 0,
            sheet_count: 0,
        }];

        self.build_child_sheet_paths(root_path, &format!("/{root_uuid}"), &mut sheet_paths);
        sheet_paths.sort_by(|a, b| {
            let page_cmp = match (&a.page, &b.page) {
                (Some(a_page), Some(b_page)) => a_page.cmp(b_page),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };

            page_cmp
                .then_with(|| a.instance_path.cmp(&b.instance_path))
                .then_with(|| a.schematic_path.cmp(&b.schematic_path))
        });
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
                    upsert_symbol_property(
                        symbol,
                        "Reference",
                        reference.clone(),
                        PropertyKind::SymbolReference,
                    );
                }

                if let Some(unit) = instance.unit {
                    symbol.unit = Some(unit);
                }

                if let Some(value) = instance.value.as_ref() {
                    if !value.is_empty() {
                        upsert_symbol_property(
                            symbol,
                            "Value",
                            value.clone(),
                            PropertyKind::SymbolValue,
                        );
                    }
                }

                if let Some(footprint) = instance.footprint.as_ref() {
                    if !footprint.is_empty() {
                        upsert_symbol_property(
                            symbol,
                            "Footprint",
                            footprint.clone(),
                            PropertyKind::SymbolFootprint,
                        );
                    }
                }
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

        sheet_paths.sort_by(|a, b| {
            let page_cmp = match (&a.page, &b.page) {
                (Some(a_page), Some(b_page)) => a_page.cmp(b_page),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };

            page_cmp
                .then_with(|| a.instance_path.cmp(&b.instance_path))
                .then_with(|| a.schematic_path.cmp(&b.schematic_path))
        });
    }

    fn fix_legacy_power_symbol_mismatches(&mut self, root_path: &Path) {
        let Some(root_index) = self.loaded_by_canonical.get(root_path).copied() else {
            return;
        };
        if self.schematics[root_index].version >= 20230221 {
            return;
        }

        for schematic in &mut self.schematics {
            for item in &mut schematic.screen.items {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };

                let lib_symbol_name = symbol
                    .linked_lib_symbol_name
                    .as_deref()
                    .or(symbol.lib_name.as_deref())
                    .unwrap_or(symbol.lib_id.as_str());

                let Some(lib_symbol) = schematic
                    .screen
                    .lib_symbols
                    .iter()
                    .find(|candidate| candidate.name == lib_symbol_name)
                else {
                    continue;
                };

                if !lib_symbol.power || lib_symbol.local_power {
                    continue;
                }

                let Some(first_pin) = lib_symbol
                    .units
                    .iter()
                    .flat_map(|unit| unit.draw_items.iter())
                    .find(|draw_item| draw_item.kind == "pin")
                else {
                    continue;
                };

                if first_pin.electrical_type.as_deref() != Some("power_in") || first_pin.visible {
                    continue;
                }

                let Some(pin_name) = first_pin.name.clone() else {
                    continue;
                };

                upsert_symbol_property(symbol, "Value", pin_name, PropertyKind::SymbolValue);
            }
        }
    }

    fn set_sheet_number_and_count(&self, sheet_paths: &mut [LoadedSheetPath]) {
        let sheet_count = sheet_paths.len();
        for (index, sheet_path) in sheet_paths.iter_mut().enumerate() {
            sheet_path.sheet_number = index + 1;
            sheet_path.sheet_count = sheet_count;
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

                if let Some(property) = label
                    .properties
                    .iter_mut()
                    .find(|property| property.kind == PropertyKind::GlobalLabelIntersheetRefs)
                {
                    property.value = value;
                } else {
                    label.properties.push(Property {
                        id: None,
                        key: "Intersheet References".to_string(),
                        value,
                        kind: PropertyKind::GlobalLabelIntersheetRefs,
                        is_private: false,
                        at: label.iref_at,
                        angle: None,
                        visible: false,
                        show_name: true,
                        can_autoplace: true,
                        has_effects: false,
                        effects: None,
                    });
                }
            }
        }
    }

    fn update_all_screen_references(&mut self, sheet_paths: &[LoadedSheetPath]) {
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

                let Some(instance) = symbol
                    .instances
                    .iter()
                    .find(|instance| instance.path == sheet_path.instance_path)
                    .cloned()
                else {
                    continue;
                };

                if let Some(reference) = instance.reference {
                    upsert_symbol_property(
                        symbol,
                        "Reference",
                        reference,
                        PropertyKind::SymbolReference,
                    );
                }

                if let Some(unit) = instance.unit {
                    symbol.unit = Some(unit);
                }

                if let Some(value) = instance.value {
                    upsert_symbol_property(symbol, "Value", value, PropertyKind::SymbolValue);
                }

                if let Some(footprint) = instance.footprint {
                    upsert_symbol_property(
                        symbol,
                        "Footprint",
                        footprint,
                        PropertyKind::SymbolFootprint,
                    );
                }
            }
        }
    }
}

fn upsert_symbol_property(symbol: &mut Symbol, key: &str, value: String, kind: PropertyKind) {
    let property = Property {
        id: None,
        key: key.to_string(),
        value,
        kind,
        is_private: false,
        at: None,
        angle: None,
        visible: true,
        show_name: true,
        can_autoplace: true,
        has_effects: false,
        effects: None,
    };

    if let Some(existing) = symbol
        .properties
        .iter_mut()
        .find(|property| property.kind == kind)
    {
        *existing = property;
    } else {
        symbol.properties.push(property);
    }
}
