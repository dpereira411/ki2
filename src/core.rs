use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::loader::{
    HierarchyLink, LoadResult, LoadedProjectSettings, LoadedSheetPath,
    refresh_current_screen_page_state, refresh_current_sheet_intersheet_refs,
    refresh_live_sheet_variant_state, refresh_live_symbol_occurrence_state,
    reset_reused_screen_symbol_state,
};
use crate::model::Schematic;

#[derive(Debug)]
pub struct SchematicProject {
    pub root_path: PathBuf,
    pub project: Option<LoadedProjectSettings>,
    pub project_local_settings: Option<LoadedProjectSettings>,
    pub schematics: Vec<Schematic>,
    pub links: Vec<HierarchyLink>,
    pub sheet_paths: Vec<LoadedSheetPath>,
    pub current_sheet_instance_path: String,
    pub current_variant: Option<String>,
    intersheet_ref_values: HashMap<String, String>,
    intersheet_ref_pages_by_label: HashMap<String, BTreeSet<usize>>,
    sheet_pages_by_virtual_page: HashMap<usize, String>,
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
            project: load.project,
            project_local_settings: load.project_local_settings,
            schematics: load.schematics,
            links: load.links,
            sheet_paths: load.sheet_paths,
            current_sheet_instance_path: load.current_sheet_instance_path,
            current_variant: load.current_variant,
            intersheet_ref_values: load.intersheet_ref_values,
            intersheet_ref_pages_by_label: load.intersheet_ref_pages_by_label,
            sheet_pages_by_virtual_page: load.sheet_pages_by_virtual_page,
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

    pub fn current_variant(&self) -> Option<&str> {
        self.current_variant.as_deref()
    }

    // Upstream parity: local current-sheet selection helper for project callers. This is not a
    // 1:1 KiCad routine because the reduced Rust project view still exposes selection directly on
    // `SchematicProject`, but it now reuses the same loader-owned occurrence refresh helpers as
    // `LoadResult` while keeping current-sheet intersheet-ref refresh scoped to the selected sheet.
    // Remaining divergence is limited to the same settings/display/model gaps as the loader path.
    pub fn set_current_sheet_path(&mut self, instance_path: &str) -> bool {
        if self.sheet_paths_by_instance.contains_key(instance_path) {
            let previous = self.current_sheet_path().cloned();
            let next = self.sheet_path(instance_path).cloned();
            reset_reused_screen_symbol_state(
                &mut self.schematics,
                &self.sheet_paths,
                previous.as_ref(),
                next.as_ref(),
                self.current_variant.as_deref(),
            );
            refresh_current_screen_page_state(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                instance_path,
            );
            self.current_sheet_instance_path = instance_path.to_string();
            let intersheet_refs_show = self
                .project
                .as_ref()
                .and_then(LoadedProjectSettings::intersheet_refs_show)
                .unwrap_or(false);
            let intersheet_refs_own_page = self
                .project
                .as_ref()
                .and_then(LoadedProjectSettings::intersheet_refs_own_page);
            let intersheet_refs_short = self
                .project
                .as_ref()
                .and_then(LoadedProjectSettings::intersheet_refs_short);
            let intersheet_refs_prefix = self
                .project
                .as_ref()
                .and_then(LoadedProjectSettings::intersheet_refs_prefix);
            let intersheet_refs_suffix = self
                .project
                .as_ref()
                .and_then(LoadedProjectSettings::intersheet_refs_suffix);
            refresh_current_sheet_intersheet_refs(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                &self.intersheet_ref_values,
                &self.intersheet_ref_pages_by_label,
                &self.sheet_pages_by_virtual_page,
                intersheet_refs_show,
                intersheet_refs_own_page,
                intersheet_refs_short,
                intersheet_refs_prefix.as_deref(),
                intersheet_refs_suffix.as_deref(),
            );
            refresh_live_symbol_occurrence_state(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                self.current_variant.as_deref(),
            );
            refresh_live_sheet_variant_state(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                self.current_variant.as_deref(),
            );
            true
        } else {
            false
        }
    }

    // Upstream parity: local project-view analogue for `SCHEMATIC::SetCurrentVariant()`. This is
    // not a 1:1 KiCad boundary because the reduced Rust project view still exposes selection
    // directly on `SchematicProject`, but it now shares the same occurrence refresh path as
    // `LoadResult`. Remaining divergence is limited to broader sheet-occurrence semantics.
    pub fn set_current_variant(&mut self, variant: Option<&str>) {
        self.current_variant = variant
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        refresh_live_sheet_variant_state(
            &mut self.schematics,
            &self.sheet_paths,
            &self.current_sheet_instance_path,
            self.current_variant.as_deref(),
        );
        refresh_live_symbol_occurrence_state(
            &mut self.schematics,
            &self.sheet_paths,
            &self.current_sheet_instance_path,
            self.current_variant.as_deref(),
        );
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
