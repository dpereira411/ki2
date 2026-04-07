use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::loader::{
    ActiveSchematicSettings, DrawingSheetSource, HierarchyLink, LoadResult, LoadedProjectSettings,
    LoadedSheetPath, build_intersheet_ref_maps, refresh_current_screen_page_state,
    refresh_current_sheet_intersheet_refs, refresh_live_sheet_variant_state,
    refresh_live_symbol_occurrence_state, reset_reused_screen_symbol_state,
    resolve_drawing_sheet_source_from_embedded_files, resolve_drawing_sheet_text_var,
    resolve_text_variables,
};
use crate::model::Schematic;
use crate::worksheet::{
    WorksheetTextItem, default_reduced_worksheet_text_items, parse_reduced_worksheet_text_items,
};

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

    // Upstream parity: reduced local analogue for KiCad's current drawing-sheet source selection
    // on the project-facing API. This is not 1:1 because the tree still lacks a schematic-global
    // worksheet owner and full page-layout model, but it keeps the current-sheet ERC path on the
    // same source-selection behavior as `LoadResult`.
    pub fn current_drawing_sheet_source(&self) -> DrawingSheetSource {
        self.current_schematic()
            .map(|schematic| {
                resolve_drawing_sheet_source_from_embedded_files(
                    &schematic.path,
                    self.project.as_ref(),
                    &schematic.screen.embedded_files,
                )
            })
            .unwrap_or(DrawingSheetSource::Default)
    }

    // Upstream parity: reduced local analogue for the drawing-sheet `DS_DRAW_ITEM_TEXT` list on
    // the project-facing API. This is not 1:1 because the local tree still only parses `tbtext`
    // items and lacks the full worksheet draw-item model, but it keeps ERC on the same reduced
    // default/custom worksheet carrier as loader callers.
    pub fn current_drawing_sheet_text_items(&self) -> Result<Vec<WorksheetTextItem>, Error> {
        let Some(current) = self.current_schematic() else {
            return Ok(Vec::new());
        };

        match self.current_drawing_sheet_source() {
            DrawingSheetSource::Default => default_reduced_worksheet_text_items(),
            DrawingSheetSource::Filesystem(path) => {
                let raw = fs::read_to_string(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                parse_reduced_worksheet_text_items(&path, &raw)
            }
            DrawingSheetSource::SchematicEmbedded { name, text } => {
                parse_reduced_worksheet_text_items(
                    &current
                        .path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(name),
                    &text,
                )
            }
        }
    }

    // Upstream parity: reduced local analogue for `DS_DRAW_ITEM_TEXT::GetShownText()` on the
    // project-facing ERC path. This is not 1:1 because the local tree still lacks the full
    // worksheet painter/title-block resolver stack, but it reuses the same reduced worksheet
    // shown-text slice as the loader API.
    pub fn current_drawing_sheet_shown_text_items(&self) -> Result<Vec<WorksheetTextItem>, Error> {
        let Some(loaded_path) = self.current_sheet_path() else {
            return Ok(Vec::new());
        };

        let raw_items = self.current_drawing_sheet_text_items()?;

        Ok(raw_items
            .into_iter()
            .map(|mut item| {
                item.text = resolve_text_variables(
                    &item.text,
                    &|token| {
                        resolve_drawing_sheet_text_var(
                            &self.schematics,
                            &self.sheet_paths,
                            loaded_path,
                            self.project.as_ref(),
                            self.current_variant(),
                            token,
                        )
                    },
                    0,
                );
                item
            })
            .collect())
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
            let schematic_settings =
                ActiveSchematicSettings::from_project_settings(self.project.as_ref());
            refresh_current_sheet_intersheet_refs(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                &self.intersheet_ref_values,
                &self.intersheet_ref_pages_by_label,
                &self.sheet_pages_by_virtual_page,
                &schematic_settings,
                self.project.as_ref(),
                self.current_variant.as_deref(),
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
    // `LoadResult`, including variant-sensitive intersheet-ref recompute. Remaining divergence is
    // limited to broader sheet-occurrence semantics and the broader unported text-variable
    // resolver surface.
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
        let (values, pages_by_label, pages_by_virtual_page) = build_intersheet_ref_maps(
            &self.schematics,
            &self.sheet_paths,
            self.project.as_ref(),
            self.current_variant.as_deref(),
        );
        self.intersheet_ref_values = values;
        self.intersheet_ref_pages_by_label = pages_by_label;
        self.sheet_pages_by_virtual_page = pages_by_virtual_page;
        let schematic_settings =
            ActiveSchematicSettings::from_project_settings(self.project.as_ref());
        refresh_current_sheet_intersheet_refs(
            &mut self.schematics,
            &self.sheet_paths,
            &self.current_sheet_instance_path,
            &self.intersheet_ref_values,
            &self.intersheet_ref_pages_by_label,
            &self.sheet_pages_by_virtual_page,
            &schematic_settings,
            self.project.as_ref(),
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
