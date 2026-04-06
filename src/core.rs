use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::loader::{HierarchyLink, LoadResult, LoadedSheetPath};
use crate::model::{PropertyKind, SchItem, Schematic};

#[derive(Debug)]
pub struct SchematicProject {
    pub root_path: PathBuf,
    pub schematics: Vec<Schematic>,
    pub links: Vec<HierarchyLink>,
    pub sheet_paths: Vec<LoadedSheetPath>,
    pub current_sheet_instance_path: String,
    by_path: HashMap<PathBuf, usize>,
    sheet_paths_by_instance: HashMap<String, usize>,
}

impl SchematicProject {
    pub fn from_load_result(load: LoadResult) -> Self {
        let by_path = load
            .schematics
            .iter()
            .enumerate()
            .map(|(index, schematic)| (schematic.path.clone(), index))
            .collect();
        let sheet_paths_by_instance = load
            .sheet_paths
            .iter()
            .enumerate()
            .map(|(index, sheet_path)| (sheet_path.instance_path.clone(), index))
            .collect();

        Self {
            root_path: load.root_path,
            schematics: load.schematics,
            links: load.links,
            sheet_paths: load.sheet_paths,
            current_sheet_instance_path: load.current_sheet_instance_path,
            by_path,
            sheet_paths_by_instance,
        }
    }

    pub fn root(&self) -> Option<&Schematic> {
        self.schematic(&self.root_path)
    }

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
        self.schematic(&current_sheet_path.schematic_path)
    }

    pub fn set_current_sheet_path(&mut self, instance_path: &str) -> bool {
        if self.sheet_paths_by_instance.contains_key(instance_path) {
            refresh_current_screen_page_state(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                instance_path,
            );
            self.current_sheet_instance_path = instance_path.to_string();
            if let Some(schematic) = self
                .current_sheet_path()
                .and_then(|sheet_path| self.by_path.get(&sheet_path.schematic_path).copied())
                .and_then(|index| self.schematics.get_mut(index))
            {
                apply_symbol_instance_state(schematic, instance_path);
            }
            true
        } else {
            false
        }
    }

    pub fn schematic(&self, path: &Path) -> Option<&Schematic> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.by_path
            .get(&canonical)
            .and_then(|index| self.schematics.get(*index))
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

    pub fn parents_of<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a HierarchyLink> + 'a {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.links
            .iter()
            .filter(move |link| link.child_path == canonical)
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

    pub fn sheet_path(&self, instance_path: &str) -> Option<&LoadedSheetPath> {
        self.sheet_paths_by_instance
            .get(instance_path)
            .and_then(|index| self.sheet_paths.get(*index))
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
