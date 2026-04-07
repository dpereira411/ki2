use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{
    EmbeddedFile, ItemVariant, Property, PropertyKind, SchItem, Schematic, SheetReference,
    SimLibrarySource, Symbol,
};
use crate::parser::parse_schematic_file;
use crate::sim::{
    expected_missing_sim_library_locations, load_symbol_sim_library_content_from_embedded_files,
    resolve_symbol_sim_library_from_embedded_files, resolve_symbol_sim_model_from_embedded_files,
};
use serde_json::Value;

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

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedProjectSettings {
    pub path: PathBuf,
    pub json: Value,
}

impl LoadedProjectSettings {
    pub fn meta_version(&self) -> Option<i32> {
        self.json
            .get("meta")
            .and_then(Value::as_object)
            .and_then(|meta| meta.get("version"))
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
    }

    // Upstream parity: local settings lookup for KiCad's `SCHEMATIC_SETTINGS::m_IntersheetRefsShow`.
    // This is not a 1:1 upstream routine because the current tree still preserves raw project JSON
    // instead of a typed schematic-settings object; it exists so loader-side current-sheet refresh
    // can honor real project formatting settings when they are available.
    pub fn intersheet_refs_show(&self) -> Option<bool> {
        self.json
            .get("drawing")
            .and_then(Value::as_object)
            .and_then(|drawing| drawing.get("intersheets_ref_show"))
            .and_then(Value::as_bool)
    }

    // Upstream parity: local settings lookup for KiCad's
    // `SCHEMATIC_SETTINGS::m_IntersheetRefsListOwnPage`. This exists for the same reason as the
    // show/hide lookup above: the current tree still carries raw project JSON rather than a typed
    // schematic-settings object.
    pub fn intersheet_refs_own_page(&self) -> Option<bool> {
        self.json
            .get("drawing")
            .and_then(Value::as_object)
            .and_then(|drawing| drawing.get("intersheets_ref_own_page"))
            .and_then(Value::as_bool)
    }

    // Upstream parity: local settings lookup for KiCad's
    // `SCHEMATIC_SETTINGS::m_IntersheetRefsFormatShort`.
    pub fn intersheet_refs_short(&self) -> Option<bool> {
        self.json
            .get("drawing")
            .and_then(Value::as_object)
            .and_then(|drawing| drawing.get("intersheets_ref_short"))
            .and_then(Value::as_bool)
    }

    // Upstream parity: local settings lookup for KiCad's
    // `SCHEMATIC_SETTINGS::m_IntersheetRefsPrefix`.
    pub fn intersheet_refs_prefix(&self) -> Option<String> {
        self.json
            .get("drawing")
            .and_then(Value::as_object)
            .and_then(|drawing| drawing.get("intersheets_ref_prefix"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    // Upstream parity: local settings lookup for KiCad's
    // `SCHEMATIC_SETTINGS::m_IntersheetRefsSuffix`.
    pub fn intersheet_refs_suffix(&self) -> Option<String> {
        self.json
            .get("drawing")
            .and_then(Value::as_object)
            .and_then(|drawing| drawing.get("intersheets_ref_suffix"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}

#[derive(Debug)]
pub struct LoadResult {
    pub root_path: PathBuf,
    pub project: Option<LoadedProjectSettings>,
    pub project_local_settings: Option<LoadedProjectSettings>,
    pub schematics: Vec<Schematic>,
    pub links: Vec<HierarchyLink>,
    pub sheet_paths: Vec<LoadedSheetPath>,
    pub current_sheet_instance_path: String,
    pub current_variant: Option<String>,
    pub intersheet_ref_values: HashMap<String, String>,
    pub intersheet_ref_pages_by_label: HashMap<String, BTreeSet<usize>>,
    pub sheet_pages_by_virtual_page: HashMap<usize, String>,
}

impl LoadResult {
    fn intersheet_refs_show(&self) -> bool {
        self.project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_show)
            .unwrap_or(false)
    }

    fn intersheet_refs_own_page(&self) -> Option<bool> {
        self.project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_own_page)
    }

    fn intersheet_refs_short(&self) -> Option<bool> {
        self.project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_short)
    }

    fn intersheet_refs_prefix(&self) -> Option<String> {
        self.project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_prefix)
    }

    fn intersheet_refs_suffix(&self) -> Option<String> {
        self.project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_suffix)
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
        self.schematics
            .iter()
            .find(|schematic| schematic.path == current_sheet_path.schematic_path)
    }

    pub fn current_variant(&self) -> Option<&str> {
        self.current_variant.as_deref()
    }

    pub fn project(&self) -> Option<&LoadedProjectSettings> {
        self.project.as_ref()
    }

    pub fn project_local_settings(&self) -> Option<&LoadedProjectSettings> {
        self.project_local_settings.as_ref()
    }

    // Upstream parity: current-sheet selection is the local entrypoint that exercises KiCad's
    // reused-screen occurrence switching side effects after load. This helper is not a 1:1
    // upstream routine because the Rust loader exposes selection directly on `LoadResult`, but it
    // owns the live screen-state transition timing. Current symbol and sheet occurrence refresh now
    // follow current-sheet switching through the selected path; remaining divergence is blocked on
    // richer occurrence state beyond the current model.
    pub fn set_current_sheet_path(&mut self, instance_path: &str) -> bool {
        if self.sheet_path(instance_path).is_some() {
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
            refresh_live_sheet_variant_state(
                &mut self.schematics,
                &self.sheet_paths,
                &self.current_sheet_instance_path,
                self.current_variant.as_deref(),
            );
            let intersheet_refs_show = self.intersheet_refs_show();
            let intersheet_refs_own_page = self.intersheet_refs_own_page();
            let intersheet_refs_short = self.intersheet_refs_short();
            let intersheet_refs_prefix = self.intersheet_refs_prefix();
            let intersheet_refs_suffix = self.intersheet_refs_suffix();
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
            if let Some(schematic) = self
                .current_sheet_path()
                .map(|sheet_path| sheet_path.schematic_path.clone())
                .and_then(|schematic_path| {
                    self.schematics
                        .iter_mut()
                        .find(|schematic| schematic.path == schematic_path)
                })
            {
                apply_symbol_instance_state(
                    schematic,
                    instance_path,
                    self.current_variant.as_deref(),
                );
            }
            true
        } else {
            false
        }
    }

    // Upstream parity: local selection analogue for KiCad's `SCHEMATIC::SetCurrentVariant()`.
    // This is not a 1:1 upstream routine because the current tree does not yet have a real
    // schematic-owned controller object; the selected variant lives on `LoadResult` instead. The
    // current implementation now refreshes both symbol and sheet occurrence state; remaining
    // divergence is limited to richer occurrence semantics beyond the current model.
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
    // Upstream parity: local loader entrypoint for KiCad's cross-file schematic/project load
    // boundary. This is not a 1:1 upstream routine because the Rust tree still lacks KiCad's full
    // schematic/project controller classes; for now it also preserves companion `.kicad_pro` and
    // `.kicad_prl` JSON documents so later ERC/project work can consult real settings sources.
    // Remaining divergence is limited to controller-level behavior and project settings not yet
    // modeled.
    let project = load_companion_project_settings(root)?;
    let project_local_settings = load_companion_project_local_settings(root)?;
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
    loader.update_all_screen_references(
        &sheet_paths,
        project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_show)
            .unwrap_or(false),
        project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_own_page),
        project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_short),
        project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_prefix),
        project
            .as_ref()
            .and_then(LoadedProjectSettings::intersheet_refs_suffix),
    );
    snapshot_sheet_occurrence_bases(&mut loader.schematics);
    snapshot_symbol_occurrence_bases(&mut loader.schematics);
    Ok(LoadResult {
        root_path,
        project,
        project_local_settings,
        schematics: loader.schematics,
        links: loader.links,
        sheet_paths,
        current_sheet_instance_path: String::new(),
        current_variant: None,
        intersheet_ref_values: loader.intersheet_ref_values,
        intersheet_ref_pages_by_label: loader.intersheet_ref_pages_by_label,
        sheet_pages_by_virtual_page: loader.sheet_pages_by_virtual_page,
    })
}

// Upstream parity: local helper for the shared companion-project JSON load path. This is not a 1:1
// upstream routine because the current tree still does not model KiCad's project/local-settings
// classes; it exists to keep `.kicad_pro` and `.kicad_prl` discovery on one JSON/diagnostic path
// instead of duplicating parser behavior. Remaining divergence is limited to the still-unconfirmed
// setting keys we need to read from the loaded documents.
fn load_companion_project_json(
    root: &Path,
    extension: &str,
) -> Result<Option<LoadedProjectSettings>, Error> {
    let dir = root.parent().unwrap_or(Path::new("."));
    let Some(stem) = root.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(None);
    };
    let project_path = dir.join(format!("{stem}.{extension}"));
    if !project_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&project_path).map_err(|source| Error::Io {
        path: project_path.clone(),
        source,
    })?;
    let json = serde_json::from_str::<Value>(&raw).map_err(|source| Error::Validation {
        path: project_path.clone(),
        diagnostic: Diagnostic::validation(
            "project_json_invalid",
            format!("invalid .kicad_pro json: {source}"),
        ),
    })?;

    Ok(Some(LoadedProjectSettings {
        path: project_path,
        json,
    }))
}

// Upstream parity: local helper for the project-file side of KiCad's load boundary. This is not a
// 1:1 upstream routine because the current tree still stores only raw JSON, not a real PROJECT_FILE
// object; it exists so loader work can consult companion `.kicad_pro` state without inventing a
// second parser.
fn load_companion_project_settings(root: &Path) -> Result<Option<LoadedProjectSettings>, Error> {
    load_companion_project_json(root, "kicad_pro")
}

// Upstream parity: local helper for project-local settings discovery. This is not a 1:1 upstream
// routine because the current tree still lacks KiCad's PROJECT_LOCAL_SETTINGS object; it exists so
// current-project UI state such as a possible active-variant source can be audited from the real
// companion `.kicad_prl` file instead of guessed from parser state.
fn load_companion_project_local_settings(
    root: &Path,
) -> Result<Option<LoadedProjectSettings>, Error> {
    load_companion_project_json(root, "kicad_prl")
}

struct SchematicLoader {
    schematics: Vec<Schematic>,
    links: Vec<HierarchyLink>,
    loaded_by_canonical: HashMap<PathBuf, usize>,
    current_sheet_path: Vec<PathBuf>,
    current_path: Vec<PathBuf>,
    intersheet_ref_values: HashMap<String, String>,
    intersheet_ref_pages_by_label: HashMap<String, BTreeSet<usize>>,
    sheet_pages_by_virtual_page: HashMap<usize, String>,
}

#[derive(Clone)]
struct PowerLibSymbolInfo {
    power: bool,
}

impl SchematicLoader {
    fn new() -> Self {
        Self {
            schematics: Vec::new(),
            links: Vec::new(),
            loaded_by_canonical: HashMap::new(),
            current_sheet_path: Vec::new(),
            current_path: Vec::new(),
            intersheet_ref_values: HashMap::new(),
            intersheet_ref_pages_by_label: HashMap::new(),
            sheet_pages_by_virtual_page: HashMap::new(),
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

    // Upstream parity: loader-side `BuildSheetListSortedByPageNumbers` analogue. This is not a
    // 1:1 KiCad function boundary because the Rust loader keeps child expansion and sort helpers
    // split out locally, but this routine owns root sheet-path seeding, root-page capture, and the
    // final sorted loaded-sheet list. Direct re-audit plus current tests leave no active mismatch
    // here in the present `LoadedSheetPath` model.
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

    // Local helper for the upstream sheet-list builder. This helper exists because the Rust loader
    // represents hierarchy expansion recursively before sorting the final list; upstream keeps the
    // traversal inside a different set of owning C++ routines. Remaining behavior is intentionally
    // narrow: expand child sheet-path metadata only, leaving page ordering to the owning builder.
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

    // Upstream parity: loader-side `UpdateSymbolInstanceData` analogue for legacy
    // root `symbol_instances` (< 20221002). This is not a 1:1 upstream function boundary because
    // the local loader keeps the surrounding sheet-path traversal in Rust-owned helpers, but this
    // routine owns the legacy root-instance application timing. Remaining divergence is limited to
    // richer hierarchical-reference state beyond the current symbol/local-instance model.
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
                    symbol.set_field_text(PropertyKind::SymbolValue, value.clone());
                }

                if let Some(footprint) = instance.footprint.as_ref() {
                    symbol.set_field_text(PropertyKind::SymbolFootprint, footprint.clone());
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
                seed_first_symbol_instance_state(&mut self.schematics[schematic_index], None);
            }
        }
    }

    // Upstream parity: loader-side `UpdateSheetInstanceData` analogue. This local boundary keeps
    // page propagation on the shared loaded-sheet-path list before later screen page-state refresh,
    // rather than matching KiCad's owning C++ object graph 1:1. Direct re-audit did not find
    // another model-visible mismatch in the current page-field representation; remaining drift is
    // limited to richer reused-screen/current-sheet state beyond `page_number`/`page_count`.
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
            let mut migrated = false;

            for item in &mut schematic.screen.items {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };

                let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
                    continue;
                };

                if !lib_symbol.power || lib_symbol.local_power {
                    continue;
                }

                let unit_number = symbol.unit.unwrap_or(1);
                let body_style = symbol.body_style.unwrap_or(1);
                let first_pin = lib_symbol
                    .units
                    .iter()
                    .filter(|unit| unit.unit_number == unit_number && unit.body_style == body_style)
                    .flat_map(|unit| unit.draw_items.iter())
                    .find(|draw_item| draw_item.kind == "pin");

                if first_pin.and_then(|pin| pin.electrical_type.as_deref()) != Some("power_in")
                    || first_pin.is_some_and(|pin| pin.visible)
                {
                    continue;
                }

                let Some(pin_name) = first_pin.and_then(|pin| pin.name.clone()) else {
                    continue;
                };

                let value_changed = symbol
                    .properties
                    .iter()
                    .find(|property| property.kind == PropertyKind::SymbolValue)
                    .is_none_or(|property| property.value != pin_name);
                symbol.set_field_text(PropertyKind::SymbolValue, pin_name);
                migrated |= value_changed;
            }

            if migrated {
                schematic.screen.content_modified = true;
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
            let schematic_path = schematic.path.clone();
            let embedded_files = schematic.screen.embedded_files.clone();

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
                    let current_value = symbol
                        .properties
                        .iter()
                        .find(|property| property.kind == PropertyKind::SymbolValue)
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let value_field_template = symbol
                        .properties
                        .iter()
                        .find(|property| property.kind == PropertyKind::SymbolValue)
                        .cloned();
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
                        hydrate_resolved_sim_library(
                            &schematic_path,
                            &embedded_files,
                            symbol,
                            &mut schematic.screen.parse_warnings,
                        );
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
                        .filter(|value| !value.is_empty())
                        .or_else(|| (!current_value.is_empty()).then(|| current_value.clone()))
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
                        let model_from_value_field =
                            model_field.is_none() && !current_value.is_empty();
                        let source_pins = symbol_source_pin_numbers(symbol);
                        let dc_value = parse_legacy_dc_model_value(
                            model_field
                                .as_ref()
                                .map(|property| property.value.as_str())
                                .unwrap_or(current_value.as_str()),
                        )
                        .expect("checked above");

                        symbol.set_field_text(PropertyKind::SymbolValue, dc_value);

                        let device_template = model_field
                            .clone()
                            .or_else(|| {
                                model_from_value_field.then(|| {
                                    Property::new_named(
                                        PropertyKind::User,
                                        "",
                                        String::new(),
                                        false,
                                    )
                                })
                            })
                            .unwrap_or_else(|| {
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
                        hydrate_resolved_sim_library(
                            &schematic_path,
                            &embedded_files,
                            symbol,
                            &mut schematic.screen.parse_warnings,
                        );
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
                        let model_from_value_field =
                            model_field.is_none() && !current_value.is_empty();
                        let source_pins = symbol_source_pin_numbers(symbol);
                        let source_model = parse_legacy_source_model(
                            model_field
                                .as_ref()
                                .map(|property| property.value.as_str())
                                .unwrap_or(current_value.as_str()),
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
                            .map(|(name, value)| format_sim_param_pair(name, value))
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

                        if model_from_value_field {
                            symbol.set_field_text(
                                PropertyKind::SymbolValue,
                                "${SIM.PARAMS}".to_string(),
                            );
                        }

                        symbol.sync_sim_model_from_properties();
                        hydrate_resolved_sim_library(
                            &schematic_path,
                            &embedded_files,
                            symbol,
                            &mut schematic.screen.parse_warnings,
                        );
                        migrated = true;
                        continue;
                    }

                    let can_raw_migrate = symbol.properties.iter().any(|property| {
                        matches!(property.key.as_str(), "Spice_Model" | "Spice_Lib_File")
                    }) || !legacy_model.is_empty();

                    if !can_raw_migrate {
                        symbol.sync_sim_model_from_properties();
                        migrated |= hydrate_resolved_sim_library(
                            &schematic_path,
                            &embedded_files,
                            symbol,
                            &mut schematic.screen.parse_warnings,
                        );
                        continue;
                    }

                    let primitive_field = take_symbol_user_field(symbol, "Spice_Primitive");
                    let node_sequence_field = take_symbol_user_field(symbol, "Spice_Node_Sequence");
                    let model_field = take_symbol_user_field(symbol, "Spice_Model");
                    let _legacy_enable = take_symbol_user_field(symbol, "Spice_Netlist_Enabled");
                    let lib_field = take_symbol_user_field(symbol, "Spice_Lib_File");
                    let model_from_value_field = model_field.is_none() && !current_value.is_empty();

                    let mut pin_map_field = node_sequence_field.map(legacy_spice_pin_map_field);
                    let source_pins = symbol_source_pin_numbers(symbol);
                    let device = primitive_field
                        .as_ref()
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let raw_model = model_field
                        .as_ref()
                        .map(|property| property.value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| current_value.clone());
                    let model = if lib_field.is_some() {
                        raw_model
                            .split_once(' ')
                            .map(|(model_name, _)| model_name.to_string())
                            .unwrap_or_else(|| raw_model.clone())
                    } else {
                        raw_model.clone()
                    };
                    let lib = lib_field
                        .as_ref()
                        .map(|property| property.value.trim().to_string())
                        .unwrap_or_default();
                    let model_line_params = if lib_field.is_some() {
                        raw_model
                            .split_once(' ')
                            .map(|(_, params)| params.trim().to_string())
                            .filter(|params| !params.is_empty())
                    } else {
                        None
                    };

                    if !lib.is_empty() && !model.is_empty() {
                        let base_template = primitive_field
                            .clone()
                            .or_else(|| model_field.clone())
                            .or_else(|| lib_field.clone())
                            .or_else(|| value_field_template.clone())
                            .unwrap_or_else(|| {
                                Property::new_named(PropertyKind::User, "", String::new(), false)
                            });
                        let name_template = model_field
                            .clone()
                            .or_else(|| value_field_template.clone())
                            .or_else(|| lib_field.clone())
                            .unwrap_or_else(|| base_template.clone());
                        let lib_template =
                            lib_field.clone().unwrap_or_else(|| base_template.clone());
                        let params_template = model_field
                            .clone()
                            .or_else(|| value_field_template.clone())
                            .or_else(|| primitive_field.clone())
                            .unwrap_or_else(|| base_template.clone());

                        let mut sim_library_field = lib_template;
                        sim_library_field.key = "Sim.Library".to_string();
                        sim_library_field.value = lib.clone();

                        let mut sim_name_field = name_template;
                        sim_name_field.key = "Sim.Name".to_string();
                        sim_name_field.value = model.clone();

                        let sim_params_field = model_line_params.clone().map(|params| {
                            let mut field = params_template;
                            field.key = "Sim.Params".to_string();
                            field.value = params;
                            field
                        });

                        let mut candidate = symbol.clone();
                        candidate.properties.push(sim_library_field.clone());
                        candidate.properties.push(sim_name_field.clone());

                        if let Some(field) = sim_params_field.clone() {
                            candidate.properties.push(field);
                        }

                        candidate.sync_sim_model_from_properties();

                        if resolve_symbol_sim_model_from_embedded_files(
                            &schematic_path,
                            &embedded_files,
                            &candidate,
                        )
                        .is_some()
                        {
                            symbol.properties.push(sim_library_field);
                            symbol.properties.push(sim_name_field);

                            if let Some(field) = sim_params_field {
                                symbol.properties.push(field);
                            }

                            if let Some(mut pin_map_field) = pin_map_field {
                                pin_map_field.key = "Sim.Pins".to_string();
                                symbol.properties.push(pin_map_field);
                            }

                            if model_from_value_field {
                                symbol.set_field_text(
                                    PropertyKind::SymbolValue,
                                    "${SIM.NAME}".to_string(),
                                );
                            }

                            symbol.sync_sim_model_from_properties();
                            hydrate_resolved_sim_library(
                                &schematic_path,
                                &embedded_files,
                                symbol,
                                &mut schematic.screen.parse_warnings,
                            );
                            migrated = true;
                            continue;
                        }
                    }

                    if pin_map_field.is_none() && !source_pins.is_empty() && lib_field.is_none() {
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

                    if model_from_value_field {
                        symbol
                            .set_field_text(PropertyKind::SymbolValue, "${SIM.PARAMS}".to_string());
                    }

                    symbol.sync_sim_model_from_properties();
                    hydrate_resolved_sim_library(
                        &schematic_path,
                        &embedded_files,
                        symbol,
                        &mut schematic.screen.parse_warnings,
                    );
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

                let sim_type_index = symbol
                    .properties
                    .iter()
                    .position(|property| property.key == "Sim.Type");
                let existing_device_subtype = sim_type_index
                    .and_then(|index| symbol.properties.get(index))
                    .map(|property| property.value.to_uppercase());

                if existing_device_subtype.as_deref() == Some("POT") {
                    if let Some(pin_field) = symbol
                        .properties
                        .iter_mut()
                        .find(|property| property.key == "Sim.Pins")
                    {
                        let updated = pin_field.value.replace("=+", "=r1").replace("=-", "=r0");

                        if updated != pin_field.value {
                            pin_field.value = updated;
                            migrated = true;
                        }
                    }
                }

                if existing_device_subtype
                    .as_deref()
                    .is_some_and(|subtype| subtype.starts_with("RAND"))
                {
                    if let Some(index) = sim_type_index {
                        let current = symbol.properties[index].value.to_uppercase();
                        let updated = current.replace("NORMAL", "GAUSSIAN");

                        if updated != symbol.properties[index].value {
                            symbol.properties[index].value = updated;
                            migrated = true;
                        }
                    }

                    if let Some(params_field) = symbol
                        .properties
                        .iter_mut()
                        .find(|property| property.key == "Sim.Params")
                    {
                        let params = params_field.value.to_lowercase();
                        let count = params.replace("min=0 ", "").replace("max=0 ", "");
                        let updated = count.replace("dt=", "ts=");

                        if updated != params_field.value {
                            params_field.value = updated;
                            migrated = true;
                        }
                    }
                }

                if existing_device_subtype.as_deref() == Some("MUTUAL") {
                    if let Some(index) = sim_type_index {
                        symbol.properties.remove(index);
                        migrated = true;
                    }

                    if let Some(device_field) = symbol
                        .properties
                        .iter_mut()
                        .find(|property| property.key == "Sim.Device")
                    {
                        if device_field.value != "K" {
                            device_field.value = "K".to_string();
                            migrated = true;
                        }
                    } else {
                        let template = Property::new_named(
                            PropertyKind::User,
                            "Sim.Device",
                            "K".to_string(),
                            false,
                        );
                        symbol.properties.push(template);
                        migrated = true;
                    }
                }

                symbol.sync_sim_model_from_properties();
                migrated |= hydrate_resolved_sim_library(
                    &schematic_path,
                    &embedded_files,
                    symbol,
                    &mut schematic.screen.parse_warnings,
                );
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
                (
                    lib_symbol.lib_id.clone(),
                    PowerLibSymbolInfo {
                        power: lib_symbol.power,
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

    // Upstream parity: local analogue of `SCHEMATIC::RecomputeIntersheetRefs()`. It now matches
    // KiCad's whole-hierarchy page-ref collection without eagerly rewriting every loaded global
    // label. Current-sheet field refresh stays in `update_all_screen_references()`. Remaining
    // divergence is limited to the still-missing settings gate and richer current-sheet-only field
    // display semantics beyond the current model.
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

        self.intersheet_ref_values = page_refs_map
            .iter()
            .map(|(label, page_numbers)| {
                let refs = page_numbers
                    .iter()
                    .filter_map(|page_number| virtual_page_to_sheet_page.get(page_number))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",");
                (label.clone(), format!("[{refs}]"))
            })
            .collect();
        self.intersheet_ref_pages_by_label = page_refs_map;
        self.sheet_pages_by_virtual_page = virtual_page_to_sheet_page;
    }

    // Upstream parity: loader-side `SCH_SHEET_PATH::UpdateAllScreenReferences()` analogue. This
    // local routine keeps reused-screen refresh on the loaded-sheet-path list plus symbol-instance
    // helpers instead of KiCad's exact screen/instance object boundaries. It now also keeps
    // current-sheet-only intersheet-ref text refresh separate from the hierarchy-wide page-ref map,
    // which is materially closer to native flow. Remaining divergence is limited to the still-
    // missing `m_IntersheetRefsShow` setting gate and richer schematic-shape hatch state.
    fn update_all_screen_references(
        &mut self,
        sheet_paths: &[LoadedSheetPath],
        intersheet_refs_show: bool,
        intersheet_refs_own_page: Option<bool>,
        intersheet_refs_short: Option<bool>,
        intersheet_refs_prefix: Option<String>,
        intersheet_refs_suffix: Option<String>,
    ) {
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

                    apply_selected_symbol_instance(symbol, instance, None);
                }
            } else if seeded_reused_schematics.insert(sheet_path.schematic_path.clone()) {
                for item in &mut self.schematics[schematic_index].screen.items {
                    let SchItem::Symbol(symbol) = item else {
                        continue;
                    };

                    let Some(instance) = symbol.instances.first().cloned() else {
                        continue;
                    };

                    apply_selected_symbol_instance(symbol, instance, None);
                }
            }
        }

        refresh_current_sheet_intersheet_refs(
            &mut self.schematics,
            sheet_paths,
            "",
            &self.intersheet_ref_values,
            &self.intersheet_ref_pages_by_label,
            &self.sheet_pages_by_virtual_page,
            intersheet_refs_show,
            intersheet_refs_own_page,
            intersheet_refs_short,
            intersheet_refs_prefix.as_deref(),
            intersheet_refs_suffix.as_deref(),
        );
    }
}

// Upstream parity: local helper for the global-label portion of
// `SCHEMATIC::RecomputeIntersheetRefs()` plus `SCH_SHEET_PATH::UpdateAllScreenReferences()`. This
// is not a 1:1 upstream routine because the Rust tree still lacks KiCad's current-sheet object and
// settings-owned `m_IntersheetRefsShow` gate, so the page-ref map and current-sheet field refresh
// are split across loader state plus this helper. It exists to keep non-current screens on their
// parsed intersheet-ref field text while applying resolved text/legacy position fixup only on the
// selected sheet. Remaining divergence is limited to the missing show/hide setting gate and shape
// hatching side effects.
pub(crate) fn refresh_current_sheet_intersheet_refs(
    schematics: &mut [Schematic],
    sheet_paths: &[LoadedSheetPath],
    current_sheet_instance_path: &str,
    intersheet_ref_values: &HashMap<String, String>,
    intersheet_ref_pages_by_label: &HashMap<String, BTreeSet<usize>>,
    sheet_pages_by_virtual_page: &HashMap<usize, String>,
    intersheet_refs_show: bool,
    intersheet_refs_own_page: Option<bool>,
    intersheet_refs_short: Option<bool>,
    intersheet_refs_prefix: Option<&str>,
    intersheet_refs_suffix: Option<&str>,
) {
    for schematic in schematics.iter_mut() {
        for item in &mut schematic.screen.items {
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

            if intersheet_refs.base_value.is_none() {
                intersheet_refs.base_value = Some(intersheet_refs.value.clone());
            }

            if let Some(base_value) = intersheet_refs.base_value.clone() {
                intersheet_refs.value = base_value;
            }
        }
    }

    let Some(current_sheet_path) = sheet_paths
        .iter()
        .find(|sheet_path| sheet_path.instance_path == current_sheet_instance_path)
    else {
        return;
    };

    let Some(schematic) = schematics
        .iter_mut()
        .find(|schematic| schematic.path == current_sheet_path.schematic_path)
    else {
        return;
    };

    for item in &mut schematic.screen.items {
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

        intersheet_refs.visible = intersheet_refs_show;

        if !intersheet_refs_show {
            continue;
        }

        let prefix = intersheet_refs_prefix.unwrap_or_default();
        let suffix = intersheet_refs_suffix.unwrap_or_default();
        intersheet_refs.value = match intersheet_ref_pages_by_label.get(&label.text) {
            Some(raw_pages) => {
                let mut pages = raw_pages.iter().copied().collect::<Vec<_>>();
                if intersheet_refs_own_page == Some(false) {
                    pages.retain(|page_number| *page_number != current_sheet_path.sheet_number);
                }

                let refs = if intersheet_refs_short == Some(true) && pages.len() > 2 {
                    let first = pages
                        .first()
                        .and_then(|page_number| sheet_pages_by_virtual_page.get(page_number))
                        .cloned()
                        .unwrap_or_default();
                    let last = pages
                        .last()
                        .and_then(|page_number| sheet_pages_by_virtual_page.get(page_number))
                        .cloned()
                        .unwrap_or_default();
                    format!("{first}..{last}")
                } else {
                    pages
                        .into_iter()
                        .filter_map(|page_number| sheet_pages_by_virtual_page.get(&page_number))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(",")
                };

                if refs.is_empty() {
                    format!("{prefix}{suffix}")
                } else if prefix.is_empty() && suffix.is_empty() {
                    format!("[{refs}]")
                } else {
                    format!("{prefix}{refs}{suffix}")
                }
            }
            None => {
                let refs = intersheet_ref_values
                    .get(&label.text)
                    .cloned()
                    .unwrap_or_else(|| "?".to_string());
                if prefix.is_empty() && suffix.is_empty() {
                    refs
                } else {
                    format!("{prefix}{refs}{suffix}")
                }
            }
        };
        intersheet_refs.id = PropertyKind::GlobalLabelIntersheetRefs.default_field_id();
        intersheet_refs.key = PropertyKind::GlobalLabelIntersheetRefs
            .canonical_key()
            .to_string();
    }
}

// Upstream parity: local helper for the symbol-refresh portion of `UpdateAllScreenReferences`.
// This helper exists because the Rust loader reuses the same symbol-instance application logic for
// both initial load-time refresh and later current-sheet selection. Remaining divergence is blocked
// on richer occurrence state, especially active-variant selection and variant field application.
fn apply_symbol_instance_state(
    schematic: &mut Schematic,
    instance_path: &str,
    current_variant: Option<&str>,
) {
    for item in &mut schematic.screen.items {
        let SchItem::Symbol(symbol) = item else {
            continue;
        };

        symbol.restore_occurrence_base();

        let Some(instance) = symbol
            .instances
            .iter()
            .find(|instance| instance.path == instance_path)
            .cloned()
        else {
            continue;
        };

        apply_selected_symbol_instance(symbol, instance, current_variant);
    }
}

// Upstream parity: local helper for reused-screen first-occurrence baseline refresh. This helper
// exists because the Rust loader keeps reused-screen reset/apply transitions outside the owning C++
// screen classes. Remaining divergence is blocked on richer occurrence state, especially variants.
fn seed_first_symbol_instance_state(schematic: &mut Schematic, current_variant: Option<&str>) {
    for item in &mut schematic.screen.items {
        let SchItem::Symbol(symbol) = item else {
            continue;
        };

        symbol.restore_occurrence_base();

        let Some(instance) = symbol.instances.first().cloned() else {
            continue;
        };

        apply_selected_symbol_instance(symbol, instance, current_variant);
    }
}

// Upstream parity: local helper for the reused-screen reset half of `UpdateAllScreenReferences`.
// This helper exists because the Rust loader performs current-sheet switching directly on
// `LoadResult`. Remaining divergence is blocked on richer per-occurrence state beyond the current
// reference/unit/value/footprint baseline, especially active variant selection.
pub(crate) fn reset_reused_screen_symbol_state(
    schematics: &mut [Schematic],
    sheet_paths: &[LoadedSheetPath],
    previous: Option<&LoadedSheetPath>,
    next: Option<&LoadedSheetPath>,
    current_variant: Option<&str>,
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
        seed_first_symbol_instance_state(schematic, current_variant);
    }
}

// Upstream parity: local helper that applies the currently selected symbol occurrence and variant
// after the loader has restored the symbol's non-occurrence baseline. This helper exists because
// the Rust loader reuses the same occurrence refresh on initial load, reused-screen switching, and
// current-variant switching. Remaining divergence is limited to sheet occurrence variants and any
// richer per-occurrence state beyond the current symbol model.
fn apply_selected_symbol_instance(
    symbol: &mut Symbol,
    instance: crate::model::SymbolLocalInstance,
    current_variant: Option<&str>,
) {
    if let Some(reference) = instance.reference {
        symbol.set_field_text(PropertyKind::SymbolReference, reference);
    }

    if let Some(unit) = instance.unit {
        symbol.unit = Some(unit);
    }

    if let Some(value) = instance
        .value
        .filter(|value| !value.is_empty() && value != "~")
    {
        symbol.set_field_text(PropertyKind::SymbolValue, value);
    }

    if let Some(footprint) = instance
        .footprint
        .filter(|footprint| !footprint.is_empty() && footprint != "~")
    {
        symbol.set_field_text(PropertyKind::SymbolFootprint, footprint);
    }

    if let Some(variant_name) = current_variant {
        if let Some(variant) = instance.variants.get(variant_name) {
            apply_symbol_variant_state(symbol, variant);
        }
    }
}

// Upstream parity: local helper for the symbol subset of occurrence-variant application. This is
// not a 1:1 upstream routine because the current tree still lacks KiCad's fuller current-variant
// and sheet-occurrence model, so the variant is resolved directly against parsed symbol instances.
// Remaining divergence is limited to sheet variants and any broader occurrence metadata not present
// in `ItemVariant`.
fn apply_symbol_variant_state(symbol: &mut Symbol, variant: &ItemVariant) {
    symbol.dnp = variant.dnp;
    symbol.excluded_from_sim = variant.excluded_from_sim;
    symbol.in_bom = variant.in_bom;
    symbol.on_board = variant.on_board;
    symbol.in_pos_files = variant.in_pos_files;

    for (name, value) in &variant.fields {
        if let Some(property) = symbol
            .properties
            .iter_mut()
            .find(|property| property.key == *name)
        {
            property.value = value.clone();
        } else {
            let mut property = Property::new_named(PropertyKind::User, name, value.clone(), false);
            property.ordinal = symbol.next_field_ordinal();
            symbol.properties.push(property);
        }
    }

    if let Some(sim_model) = symbol.sim_model.as_mut() {
        sim_model.enabled = !symbol.excluded_from_sim;
    }
}

// Upstream parity: local helper for the currently representable sheet subset of occurrence-variant
// application. This is not a 1:1 upstream routine because the current tree still lacks KiCad's
// fuller active-occurrence sheet model; for now it applies the selected variant only through the
// live sheet object's first parsed local instance. Remaining divergence is limited to multi-
// occurrence sheet selection and any broader sheet-occurrence semantics beyond this model.
fn apply_sheet_variant_state(sheet: &mut crate::model::Sheet, variant: &ItemVariant) {
    sheet.dnp = variant.dnp;
    sheet.excluded_from_sim = variant.excluded_from_sim;
    sheet.in_bom = variant.in_bom;
    sheet.on_board = variant.on_board;

    for (name, value) in &variant.fields {
        if let Some(property) = sheet
            .properties
            .iter_mut()
            .find(|property| property.key == *name)
        {
            property.value = value.clone();
        } else {
            let mut property =
                Property::new_named(PropertyKind::SheetUser, name, value.clone(), false);
            property.ordinal = sheet.next_field_ordinal();
            sheet.properties.push(property);
        }
    }
}

// Upstream parity: local helper for capturing the non-variant baseline that current-variant
// selection restores before reapplying sheet occurrence state. This helper exists because the Rust
// loader mutates live sheet objects directly after parse. Remaining divergence is limited to live
// selection across multiple local sheet occurrences.
fn snapshot_sheet_occurrence_bases(schematics: &mut [Schematic]) {
    for schematic in schematics {
        for item in &mut schematic.screen.items {
            let SchItem::Sheet(sheet) = item else {
                continue;
            };
            sheet.capture_occurrence_base();
        }
    }
}

// Upstream parity: local helper for capturing the non-variant baseline that current-sheet and
// current-variant selection restore before reapplying occurrence data. This helper exists because
// the Rust loader mutates live symbol objects directly during load and later selection changes.
// Remaining divergence is blocked on expanding the same baseline/variant model to sheets.
fn snapshot_symbol_occurrence_bases(schematics: &mut [Schematic]) {
    for schematic in schematics {
        for item in &mut schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };
            symbol.capture_occurrence_base();
        }
    }
}

// Upstream parity: local helper for project-wide live symbol refresh under current-sheet and
// current-variant selection. This helper exists because the Rust loader exposes both selections on
// `LoadResult` rather than the upstream project/schematic classes. Remaining divergence is blocked
// on sheet occurrence variants and richer per-occurrence state.
pub(crate) fn refresh_live_symbol_occurrence_state(
    schematics: &mut [Schematic],
    sheet_paths: &[LoadedSheetPath],
    current_sheet_instance_path: &str,
    current_variant: Option<&str>,
) {
    let occurrence_counts: HashMap<PathBuf, usize> =
        sheet_paths
            .iter()
            .fold(HashMap::new(), |mut counts, sheet_path| {
                *counts.entry(sheet_path.schematic_path.clone()).or_insert(0) += 1;
                counts
            });
    let selected_sheet_path = sheet_paths
        .iter()
        .find(|sheet_path| sheet_path.instance_path == current_sheet_instance_path);
    let mut refreshed = BTreeSet::new();

    for sheet_path in sheet_paths {
        if !refreshed.insert(sheet_path.schematic_path.clone()) {
            continue;
        }

        let Some(schematic) = schematics
            .iter_mut()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        let occurrence_count = occurrence_counts
            .get(&sheet_path.schematic_path)
            .copied()
            .unwrap_or(0);

        let active_instance_path = if occurrence_count > 1 {
            selected_sheet_path
                .filter(|selected| selected.schematic_path == sheet_path.schematic_path)
                .map(|selected| selected.instance_path.as_str())
                .unwrap_or(sheet_path.instance_path.as_str())
        } else {
            sheet_path.instance_path.as_str()
        };

        apply_symbol_instance_state(schematic, active_instance_path, current_variant);
    }
}

// Upstream parity: local helper for project-wide live sheet refresh under current-variant
// selection. This is not a 1:1 upstream routine because the current tree still lacks KiCad's
// fuller project/settings layer and screen-owned sheet occurrence objects; it resolves the active
// local sheet instance from the selected sheet path and falls back to the first parsed instance
// when no active occurrence matches. Remaining divergence is limited to broader sheet-occurrence
// semantics beyond the current model.
pub(crate) fn refresh_live_sheet_variant_state(
    schematics: &mut [Schematic],
    sheet_paths: &[LoadedSheetPath],
    current_sheet_instance_path: &str,
    current_variant: Option<&str>,
) {
    let selected_sheet_path = sheet_paths
        .iter()
        .find(|sheet_path| sheet_path.instance_path == current_sheet_instance_path);

    for schematic in schematics {
        for item in &mut schematic.screen.items {
            let SchItem::Sheet(sheet) = item else {
                continue;
            };

            sheet.restore_occurrence_base();

            let Some(variant_name) = current_variant else {
                continue;
            };
            let active_instance = if let Some(selected) =
                selected_sheet_path.filter(|selected| !selected.instance_path.is_empty())
            {
                sheet
                    .instances
                    .iter()
                    .filter(|instance| {
                        selected.instance_path == instance.path
                            || instance
                                .path
                                .starts_with(&(selected.instance_path.clone() + "/"))
                    })
                    .max_by_key(|instance| instance.path.len())
                    .or_else(|| sheet.instances.first())
            } else {
                sheet.instances.first()
            };
            let Some(instance) = active_instance else {
                continue;
            };
            let Some(variant) = instance.variants.get(variant_name).cloned() else {
                continue;
            };
            apply_sheet_variant_state(sheet, &variant);
        }
    }
}

fn infer_symbol_sim_model(symbol: &mut Symbol) -> bool {
    let source_pins = symbol_source_pin_numbers(symbol);

    if source_pins.len() != 2 {
        return false;
    }

    let value = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .map(|property| property.value.trim().to_string())
        .unwrap_or_default();

    if value.is_empty() {
        return false;
    }

    let explicit_device = symbol
        .properties
        .iter()
        .find(|property| property.key == "Sim.Device")
        .map(|property| property.value.trim().to_string())
        .unwrap_or_default();
    let explicit_type = symbol
        .properties
        .iter()
        .find(|property| property.key == "Sim.Type")
        .map(|property| property.value.trim().to_string())
        .unwrap_or_default();
    let explicit_library = symbol
        .properties
        .iter()
        .find(|property| property.key == "Sim.Library")
        .map(|property| property.value.trim().to_string())
        .unwrap_or_default();
    let explicit_name = symbol
        .properties
        .iter()
        .find(|property| property.key == "Sim.Name")
        .map(|property| property.value.trim().to_string())
        .unwrap_or_default();
    let explicit_params = symbol
        .properties
        .iter()
        .find(|property| property.key == "Sim.Params")
        .map(|property| property.value.trim().to_string())
        .unwrap_or_default();

    if !explicit_library.is_empty() || !explicit_name.is_empty() {
        return false;
    }

    let prefix = symbol.prefix.trim();
    let mut device = explicit_device.clone();
    let mut model_type = explicit_type.clone();
    let mut params = explicit_params.clone();

    if matches!(device.as_str(), "R" | "L" | "C") && model_type.is_empty() {
        if params.is_empty() {
            let key = device.to_ascii_lowercase();
            let normalized = normalize_inferred_si_value(&value);

            if looks_behavioral_value(&value) {
                model_type = "=".to_string();
            }

            params = format!("{key}=\"{}\"", normalized.as_deref().unwrap_or(&value));
        }
    } else if device.is_empty()
        && model_type.is_empty()
        && matches!(prefix.chars().next(), Some('R' | 'L' | 'C'))
    {
        device = prefix.chars().next().unwrap().to_string();
        let key = device.to_ascii_lowercase();
        let normalized = normalize_inferred_si_value(&value);

        if looks_behavioral_value(&value) {
            model_type = "=".to_string();
        }

        params = format!("{key}=\"{}\"", normalized.as_deref().unwrap_or(&value));
    } else if matches!(device.as_str(), "V" | "I") && (model_type.is_empty() || model_type == "DC")
    {
        if params.is_empty() {
            let (param_name, param_value) = split_inferred_source_value(&value);
            model_type = "DC".to_string();
            params = format!("{param_name}=\"{param_value}\"");
        }
    } else if device.is_empty()
        && model_type.is_empty()
        && matches!(prefix.chars().next(), Some('V' | 'I'))
    {
        device = prefix.chars().next().unwrap().to_string();
        let (param_name, param_value) = split_inferred_source_value(&value);
        model_type = "DC".to_string();
        params = format!("{param_name}=\"{param_value}\"");
    } else {
        return false;
    }

    let pin_pairs = if let Some(pin_field) = symbol
        .properties
        .iter()
        .find(|property| property.key == "Sim.Pins")
    {
        parse_loader_sim_pin_pairs(&pin_field.value)
    } else {
        vec![
            (source_pins[0].clone(), "+".to_string()),
            (source_pins[1].clone(), "-".to_string()),
        ]
    };

    let pins = pin_pairs.iter().cloned().collect::<BTreeMap<_, _>>();
    let param_pairs = crate::model::parse_sim_param_pairs(&params);
    let param_values = param_pairs.iter().cloned().collect::<BTreeMap<_, _>>();

    symbol.sim_model = Some(crate::model::SimModel {
        device: (!device.is_empty()).then_some(device),
        model_type: (!model_type.is_empty()).then_some(model_type),
        library: None,
        name: None,
        ibis_pin: None,
        ibis_model: None,
        ibis_diff: false,
        params: (!params.is_empty()).then_some(params),
        param_pairs,
        param_values,
        pin_pairs,
        pins,
        value_binding: Some(crate::model::SimValueBinding::Value),
        stored_value: Some(value),
        enabled: !symbol.excluded_from_sim,
        origin: Some(crate::model::SimModelOrigin::InferredValue),
        resolved_library: None,
        resolved_name: None,
        resolved_kind: None,
        resolved_model_type: None,
        resolved_ibis_model_type: None,
        resolved_ibis_diff_pin: None,
        generated_pin_names: Vec::new(),
        generated_param_pairs: Vec::new(),
    });

    true
}

fn hydrate_current_value_backed_sim_model(symbol: &mut Symbol) -> bool {
    let source_pins = symbol_source_pin_numbers(symbol);

    let Some(sim_model) = symbol.sim_model.as_mut() else {
        return false;
    };

    let defaulted_pins = maybe_default_current_sim_pins(sim_model, &source_pins);

    let value = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .map(|property| property.value.trim())
        .filter(|value| !value.is_empty() && !matches!(*value, "${SIM.PARAMS}" | "${SIM.NAME}"));

    let Some(value) = value else {
        return defaulted_pins;
    };

    let prefix_param = symbol
        .prefix
        .chars()
        .next()
        .map(|ch| ch.to_ascii_lowercase().to_string())
        .unwrap_or_default();

    let upsert_primary_param =
        |sim_model: &mut crate::model::SimModel, name: String, param_value: String| {
            if let Some(existing) = sim_model
                .param_pairs
                .iter_mut()
                .find(|(param_name, _)| param_name.eq_ignore_ascii_case(&name))
            {
                existing.1 = param_value.clone();
            } else {
                sim_model
                    .param_pairs
                    .insert(0, (name.clone(), param_value.clone()));
            }

            sim_model.param_values = sim_model.param_pairs.iter().cloned().collect();
            sim_model.params = (!sim_model.param_pairs.is_empty()).then(|| {
                sim_model
                    .param_pairs
                    .iter()
                    .map(|(param_name, param_value)| format_sim_param_pair(param_name, param_value))
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            sim_model.value_binding = Some(crate::model::SimValueBinding::Value);
            sim_model.stored_value = Some(value.to_string());
        };

    match (
        sim_model.device.as_deref().map(str::trim),
        sim_model.model_type.as_deref().map(str::trim),
    ) {
        (Some("R") | Some("C") | Some("L"), None | Some("")) => {
            if source_pins.len() != 2 {
                return defaulted_pins;
            }

            if sim_model.value_binding.is_some() {
                return defaulted_pins;
            }

            if looks_behavioral_value(value) {
                sim_model.model_type = Some("=".to_string());
            }

            upsert_primary_param(
                sim_model,
                prefix_param,
                normalize_inferred_si_value(value).unwrap_or_else(|| value.to_string()),
            );
        }
        (Some("V") | Some("I"), None | Some("") | Some("DC")) => {
            if source_pins.len() != 2 {
                return defaulted_pins;
            }

            if sim_model.value_binding.is_some() {
                return defaulted_pins;
            }

            let (param_name, param_value) = split_inferred_source_value(value);
            sim_model.model_type.get_or_insert_with(|| "DC".to_string());
            upsert_primary_param(sim_model, param_name.to_string(), param_value);
        }
        (Some("E") | Some("F") | Some("G") | Some("H"), None | Some("")) => {
            if sim_model.value_binding.is_some() {
                return defaulted_pins;
            }

            let Some(primary_value) = normalize_inferred_si_value(value) else {
                return defaulted_pins;
            };

            upsert_primary_param(sim_model, "gain".to_string(), primary_value);
        }
        (
            Some("V") | Some("I"),
            Some(
                "SIN" | "PULSE" | "EXP" | "AM" | "SFFM" | "WHITENOISE" | "PINKNOISE" | "BURSTNOISE"
                | "RANDUNIFORM" | "RANDGAUSSIAN" | "RANDEXP" | "RANDPOISSON" | "TRNOISE"
                | "TRRANDOM",
            ),
        ) => {
            if source_pins.len() != 2 {
                return defaulted_pins;
            }

            if sim_model.value_binding.is_some() {
                return defaulted_pins;
            }

            let Some(primary_name) = current_internal_source_primary_param_name(
                sim_model.model_type.as_deref().unwrap_or_default(),
            ) else {
                return defaulted_pins;
            };

            let Some(primary_value) = normalize_inferred_si_value(value) else {
                return defaulted_pins;
            };

            upsert_primary_param(sim_model, primary_name.to_string(), primary_value);
        }
        _ => return defaulted_pins,
    }

    maybe_default_current_sim_pins(sim_model, &source_pins);
    true
}

fn current_internal_source_primary_param_name(model_type: &str) -> Option<&'static str> {
    match model_type.trim() {
        "SIN" => Some("dc"),
        "PULSE" => Some("y1"),
        "EXP" => Some("y1"),
        "AM" => Some("vo"),
        "SFFM" => Some("vo"),
        "WHITENOISE" | "PINKNOISE" | "TRNOISE" => Some("rms"),
        "BURSTNOISE" => Some("ampl"),
        "RANDUNIFORM" | "RANDGAUSSIAN" | "RANDEXP" | "RANDPOISSON" | "TRRANDOM" => Some("ts"),
        _ => None,
    }
}

fn hydrate_current_raw_spice_value(symbol: &mut Symbol) -> bool {
    let Some(sim_model) = symbol.sim_model.as_mut() else {
        return false;
    };

    if sim_model.origin != Some(crate::model::SimModelOrigin::RawSpice)
        || sim_model.params.is_some()
        || sim_model.value_binding.is_some()
        || sim_model.stored_value.is_some()
    {
        return false;
    }

    let value = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .map(|property| property.value.trim())
        .filter(|value| !value.is_empty() && !matches!(*value, "${SIM.PARAMS}" | "${SIM.NAME}"));

    let Some(value) = value else {
        return false;
    };

    sim_model.value_binding = Some(crate::model::SimValueBinding::Value);
    sim_model.stored_value = Some(value.to_string());
    true
}

fn maybe_default_current_sim_pins(
    sim_model: &mut crate::model::SimModel,
    source_pins: &[String],
) -> bool {
    if !sim_model.pin_pairs.is_empty() {
        return false;
    }

    let should_default_inferred = matches!(
        (
            sim_model.device.as_deref().map(str::trim),
            sim_model.model_type.as_deref().map(str::trim),
        ),
        (
            Some("R") | Some("C") | Some("L"),
            None | Some("") | Some("=")
        ) | (Some("V") | Some("I"), None | Some("") | Some("DC"))
    );

    if should_default_inferred && source_pins.len() == 2 {
        sim_model.pin_pairs = vec![
            (source_pins[0].clone(), "+".to_string()),
            (source_pins[1].clone(), "-".to_string()),
        ];
        sim_model.pins = sim_model.pin_pairs.iter().cloned().collect();
        return true;
    }

    let should_default_control_source = matches!(
        (
            sim_model.device.as_deref().map(str::trim),
            sim_model.model_type.as_deref().map(str::trim),
        ),
        (Some("E") | Some("G"), None | Some(""))
    );

    if should_default_control_source && source_pins.len() == 4 {
        sim_model.pin_pairs = vec![
            (source_pins[0].clone(), "+".to_string()),
            (source_pins[1].clone(), "-".to_string()),
            (source_pins[2].clone(), "C+".to_string()),
            (source_pins[3].clone(), "C-".to_string()),
        ];
        sim_model.pins = sim_model.pin_pairs.iter().cloned().collect();
        return true;
    }

    let should_default_current_control_source = matches!(
        (
            sim_model.device.as_deref().map(str::trim),
            sim_model.model_type.as_deref().map(str::trim),
        ),
        (Some("F") | Some("H"), None | Some(""))
    );

    if should_default_current_control_source && source_pins.len() == 2 {
        sim_model.pin_pairs = vec![
            (source_pins[0].clone(), "+".to_string()),
            (source_pins[1].clone(), "-".to_string()),
        ];
        sim_model.pins = sim_model.pin_pairs.iter().cloned().collect();
        return true;
    }

    let should_default_internal_source = matches!(
        (
            sim_model.device.as_deref().map(str::trim),
            sim_model.model_type.as_deref().map(str::trim),
        ),
        (
            Some("V") | Some("I"),
            Some("SIN")
                | Some("PULSE")
                | Some("EXP")
                | Some("AM")
                | Some("SFFM")
                | Some("PWL")
                | Some("WHITENOISE")
                | Some("PINKNOISE")
                | Some("BURSTNOISE")
                | Some("RANDUNIFORM")
                | Some("RANDGAUSSIAN")
                | Some("RANDEXP")
                | Some("RANDPOISSON")
                | Some("TRNOISE")
                | Some("TRRANDOM")
        )
    );

    if !should_default_internal_source || source_pins.len() != 2 {
        return false;
    }

    sim_model.pin_pairs = vec![
        (source_pins[0].clone(), "1".to_string()),
        (source_pins[1].clone(), "2".to_string()),
    ];
    sim_model.pins = sim_model.pin_pairs.iter().cloned().collect();
    true
}

fn looks_behavioral_value(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '(' | ')' | '{' | '}' | '[' | ']' | '+' | '-' | '*' | '/' | '='
        ) || (ch.is_ascii_alphabetic()
            && !matches!(
                ch,
                'f' | 'F'
                    | 'p'
                    | 'P'
                    | 'n'
                    | 'N'
                    | 'u'
                    | 'U'
                    | 'm'
                    | 'M'
                    | 'k'
                    | 'K'
                    | 'g'
                    | 'G'
                    | 't'
                    | 'T'
                    | 'r'
                    | 'R'
                    | 'h'
                    | 'H'
                    | 'o'
                    | 'O'
            ))
    })
}

fn split_inferred_source_value(value: &str) -> (&'static str, String) {
    let trimmed = value.trim();

    if let Some(rest) = trimmed.strip_prefix("AC ") {
        return (
            "ac",
            normalize_inferred_si_value(rest.trim()).unwrap_or_else(|| rest.trim().to_string()),
        );
    }

    if let Some(rest) = trimmed.strip_prefix("DC ") {
        return (
            "dc",
            normalize_inferred_si_value(rest.trim()).unwrap_or_else(|| rest.trim().to_string()),
        );
    }

    (
        "dc",
        normalize_inferred_si_value(trimmed).unwrap_or_else(|| trimmed.to_string()),
    )
}

fn normalize_inferred_si_value(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return None;
    }

    let split_at = trimmed
        .find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '.' | ',' | ' ')))
        .unwrap_or(trimmed.len());
    let (mantissa, suffix) = trimmed.split_at(split_at);

    if mantissa.is_empty() {
        return None;
    }

    let normalized_mantissa = normalize_inferred_si_mantissa(mantissa)?;
    let normalized_suffix = normalize_inferred_si_suffix(suffix.trim())?;

    Some(format!("{normalized_mantissa}{normalized_suffix}"))
}

fn normalize_inferred_si_suffix(suffix: &str) -> Option<String> {
    if suffix.is_empty() {
        return Some(String::new());
    }

    if suffix == "µ" || suffix == "μ" {
        return Some("u".to_string());
    }

    if suffix.eq_ignore_ascii_case("Meg") {
        return Some("M".to_string());
    }

    if !suffix.chars().all(|ch| {
        matches!(
            ch,
            'f' | 'F'
                | 'p'
                | 'P'
                | 'n'
                | 'N'
                | 'u'
                | 'U'
                | 'm'
                | 'M'
                | 'k'
                | 'K'
                | 'g'
                | 'G'
                | 't'
                | 'T'
                | 'r'
                | 'R'
                | 'h'
                | 'H'
                | 'o'
                | 'O'
                | 'v'
                | 'V'
                | 'a'
                | 'A'
                | 'Ω'
                | 'Ω'
                | 'µ'
                | 'μ'
        )
    }) {
        return None;
    }

    Some(
        suffix
            .chars()
            .map(|ch| match ch {
                'µ' | 'μ' => 'u',
                _ => ch,
            })
            .collect(),
    )
}

fn normalize_inferred_si_mantissa(mantissa: &str) -> Option<String> {
    let mut compact = mantissa.replace(' ', "");

    if compact.is_empty() {
        return None;
    }

    let mut ambiguous_separator: Option<char> = None;
    let mut thousands_separator: Option<char> = None;
    let mut thousands_found = false;
    let mut decimal_separator: Option<char> = None;
    let mut decimal_found = false;
    let mut digits = 0usize;
    let chars = compact.chars().collect::<Vec<_>>();

    for index in (0..chars.len()).rev() {
        let ch = chars[index];

        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }

        if !matches!(ch, '.' | ',') {
            return None;
        }

        match (decimal_separator, thousands_separator, ambiguous_separator) {
            (Some(decimal), Some(thousands), _) => {
                if ch == decimal {
                    if thousands_found || decimal_found {
                        return None;
                    }

                    decimal_found = true;
                } else if ch == thousands {
                    if digits != 3 {
                        return None;
                    }

                    thousands_found = true;
                } else {
                    return None;
                }
            }
            (None, None, Some(ambiguous)) => {
                if ch == ambiguous {
                    thousands_separator = Some(ambiguous);
                    thousands_found = true;
                    decimal_separator = Some(if ch == '.' { ',' } else { '.' });
                } else {
                    decimal_separator = Some(ambiguous);
                    decimal_found = true;
                    thousands_separator = Some(ch);
                    thousands_found = true;
                }
            }
            _ => {
                if (index == 1 && chars[0] == '0') || digits != 3 {
                    decimal_separator = Some(ch);
                    decimal_found = true;
                    thousands_separator = Some(if ch == '.' { ',' } else { '.' });
                } else {
                    ambiguous_separator = Some(ch);
                }
            }
        }

        digits = 0;
    }

    if decimal_separator.is_none() && thousands_separator.is_none() {
        decimal_separator = Some('.');
        thousands_separator = Some(',');
    }

    if let Some(thousands) = thousands_separator {
        compact = compact.replace(thousands, "");
    }

    if let Some(decimal) = decimal_separator {
        compact = compact
            .chars()
            .map(|ch| if ch == decimal { '.' } else { ch })
            .collect();
    }

    Some(compact)
}

fn parse_loader_sim_pin_pairs(pins: &str) -> Vec<(String, String)> {
    pins.split_whitespace()
        .filter_map(|token| {
            token.split_once('=').map(|(symbol_pin, model_pin)| {
                (
                    symbol_pin.to_string(),
                    model_pin.trim_matches('"').to_string(),
                )
            })
        })
        .collect()
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

fn hydrate_resolved_sim_library(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &mut Symbol,
    warnings: &mut Vec<String>,
) -> bool {
    let mut modified = false;
    hydrate_current_value_backed_sim_model(symbol);
    let mut resolved_library =
        resolve_symbol_sim_library_from_embedded_files(schematic_path, embedded_files, symbol);
    let mut resolved_model =
        resolve_symbol_sim_model_from_embedded_files(schematic_path, embedded_files, symbol);

    let should_default_resolved_pins = symbol.sim_model.as_ref().is_some_and(|sim_model| {
        matches!(
            sim_model.origin,
            Some(
                crate::model::SimModelOrigin::LibraryReference | crate::model::SimModelOrigin::Ibis
            )
        ) && sim_model.pin_pairs.is_empty()
    });

    if should_default_resolved_pins {
        let source_pins = symbol_source_pin_numbers(symbol);

        if !source_pins.is_empty() {
            let pin_map_value = resolved_model
                .as_ref()
                .filter(|model| source_pins.len() == model.pins.len())
                .map(|model| {
                    source_pins
                        .iter()
                        .zip(model.pins.iter())
                        .map(|(source_pin, model_pin)| format!("{source_pin}={model_pin}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| {
                    source_pins
                        .iter()
                        .enumerate()
                        .map(|(index, source_pin)| format!("{source_pin}={}", index + 1))
                        .collect::<Vec<_>>()
                        .join(" ")
                });

            let mut pin_map_field = symbol
                .properties
                .iter()
                .find(|property| {
                    matches!(
                        property.key.as_str(),
                        "Sim.Name" | "Sim.Library" | "Sim.Device"
                    )
                })
                .cloned()
                .unwrap_or_else(|| {
                    Property::new_named(PropertyKind::User, "", String::new(), false)
                });
            pin_map_field.key = "Sim.Pins".to_string();
            pin_map_field.value = pin_map_value;
            symbol.properties.push(pin_map_field);
            symbol.sync_sim_model_from_properties();
            resolved_library = resolve_symbol_sim_library_from_embedded_files(
                schematic_path,
                embedded_files,
                symbol,
            );
            resolved_model = resolve_symbol_sim_model_from_embedded_files(
                schematic_path,
                embedded_files,
                symbol,
            );
            modified = true;
        }
    }

    if let Some(sim_model) = symbol.sim_model.as_mut() {
        sim_model.resolved_library = resolved_library.clone();
        sim_model.resolved_name = resolved_model
            .as_ref()
            .map(|model| model.name.clone())
            .or_else(|| {
                sim_model.library.as_ref().map(|_| {
                    let name = sim_model.name.as_deref().map(str::trim).unwrap_or_default();

                    if name.is_empty() {
                        "unknown".to_string()
                    } else {
                        name.to_string()
                    }
                })
            });
        sim_model.resolved_kind = resolved_model.as_ref().map(|model| model.kind);
        sim_model.resolved_model_type = resolved_model
            .as_ref()
            .and_then(|model| model.model_type.clone());
        sim_model.resolved_ibis_model_type = resolved_model
            .as_ref()
            .and_then(|model| model.ibis_model_type.clone());
        sim_model.resolved_ibis_diff_pin = resolved_model
            .as_ref()
            .and_then(|model| model.diff_pin.clone());
        sim_model.generated_param_pairs = resolved_model
            .as_ref()
            .map(|model| model.params.clone())
            .unwrap_or_default();
        sim_model.generated_pin_names = resolved_model.map(|model| model.pins).unwrap_or_default();

        if sim_model
            .resolved_library
            .as_ref()
            .is_some_and(|library| library.kind == crate::model::SimLibraryKind::Ibis)
        {
            sim_model.origin = Some(crate::model::SimModelOrigin::Ibis);
        }
    }

    if let Some(warning) = unresolved_sim_model_warning(
        schematic_path,
        embedded_files,
        symbol,
        resolved_library.as_ref(),
    ) {
        if !warnings.iter().any(|existing| existing == &warning) {
            warnings.push(warning);
        }
    }

    if let Some(warning) = unresolved_builtin_sim_model_warning(symbol) {
        if !warnings.iter().any(|existing| existing == &warning) {
            warnings.push(warning);
        }
    }

    if symbol.sim_model.is_none()
        || symbol.sim_model.as_ref().is_some_and(|sim_model| {
            sim_model.origin == Some(crate::model::SimModelOrigin::Fields)
                && sim_model.device.is_none()
                && sim_model.model_type.is_none()
                && sim_model.library.is_none()
                && sim_model.name.is_none()
                && sim_model.params.is_none()
        })
    {
        infer_symbol_sim_model(symbol);
    }

    hydrate_current_raw_spice_value(symbol);

    modified
}

fn unresolved_sim_model_warning(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
    resolved_library: Option<&crate::model::ResolvedSimLibrary>,
) -> Option<String> {
    let sim_model = symbol.sim_model.as_ref()?;
    let library_name = sim_model.library.as_deref()?.trim();

    if library_name.is_empty() {
        return None;
    }

    let name = sim_model.name.as_deref().map(str::trim).unwrap_or_default();

    if name.is_empty() {
        return Some("Error loading simulation model: no 'Sim.Name' field".to_string());
    }

    if load_symbol_sim_library_content_from_embedded_files(schematic_path, embedded_files, symbol)
        .is_none()
    {
        let fallback_location = || {
            let locations = expected_missing_sim_library_locations(schematic_path, library_name);

            match locations.as_slice() {
                [only] => only.display().to_string(),
                [first, second] => format!("{}' or '{}'", first.display(), second.display()),
                _ => library_name.to_string(),
            }
        };
        let location = match resolved_library {
            Some(crate::model::ResolvedSimLibrary {
                source: SimLibrarySource::Filesystem(path),
                ..
            }) if !path.exists() => fallback_location(),
            Some(library) => display_sim_library_source(&library.source),
            None => fallback_location(),
        };
        return Some(format!(
            "Simulation model library not found at '{location}'"
        ));
    }

    if resolve_symbol_sim_model_from_embedded_files(schematic_path, embedded_files, symbol)
        .is_none()
    {
        let location = resolved_library
            .map(|library| display_sim_library_source(&library.source))
            .unwrap_or_else(|| library_name.to_string());
        return Some(format!(
            "Error loading simulation model: could not find base model '{name}' in library '{location}'"
        ));
    }

    None
}

fn display_sim_library_source(source: &SimLibrarySource) -> String {
    match source {
        SimLibrarySource::Filesystem(path) => path.display().to_string(),
        SimLibrarySource::SchematicEmbedded { name }
        | SimLibrarySource::SymbolEmbedded { name } => name.clone(),
    }
}

fn unresolved_builtin_sim_model_warning(symbol: &Symbol) -> Option<String> {
    let sim_model = symbol.sim_model.as_ref()?;

    if sim_model.library.is_some() {
        return None;
    }

    let device = sim_model.device.as_deref()?.trim();
    let model_type = sim_model.model_type.as_deref()?.trim();

    if device.is_empty()
        || model_type.is_empty()
        || is_supported_builtin_sim_type(device, model_type)
    {
        return None;
    }

    let reference = symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolReference)
        .map(|property| property.value.as_str())
        .unwrap_or_default();

    if reference.is_empty() {
        Some("No simulation model definition found.".to_string())
    } else {
        Some(format!(
            "No simulation model definition found for symbol '{}'.",
            reference
        ))
    }
}

fn is_supported_builtin_sim_type(device: &str, model_type: &str) -> bool {
    let device = device.trim();
    let model_type = model_type.trim();

    match (device, model_type) {
        ("R", "" | "POT" | "=") => true,
        ("C" | "L", "" | "=") => true,
        ("K", "") => true,
        ("TLINE", "" | "RLGC") => true,
        ("SW", "V" | "I") => true,
        ("D", "") => true,
        ("NPN" | "PNP", "VBIC" | "GUMMELPOON" | "HICUM2" | "HICUML2") => true,
        ("NJFET" | "PJFET", "SHICHMANHODGES" | "PARKERSKELLERN") => true,
        ("NMES" | "PMES", "STATZ" | "YTTERDAL" | "HFET1" | "HFET2") => true,
        (
            "NMOS" | "PMOS",
            "VDMOS" | "MOS1" | "MOS2" | "MOS3" | "BSIM1" | "BSIM2" | "MOS6" | "BSIM3" | "MOS9"
            | "B4SOI" | "BSIM4" | "B3SOIFD" | "B3SOIDD" | "B3SOIPD" | "HISIM2" | "HISIMHV1"
            | "HISIMHV2",
        ) => true,
        (
            "V" | "I",
            "DC" | "SIN" | "PULSE" | "EXP" | "AM" | "SFFM" | "PWL" | "WHITENOISE" | "PINKNOISE"
            | "BURSTNOISE" | "RANDUNIFORM" | "RANDGAUSSIAN" | "RANDEXP" | "RANDPOISSON" | "TRNOISE"
            | "TRRANDOM" | "=",
        ) => true,
        ("E" | "F" | "G" | "H" | "SUBCKT" | "XSPICE", "") => true,
        _ => false,
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

fn format_sim_param_pair(name: &str, value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\'))
    {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!(r#"{name}="{escaped}""#)
    } else {
        format!("{name}={value}")
    }
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

    if kind.eq_ignore_ascii_case("pwl") {
        return Some(LegacySourceModel {
            kind: "PWL",
            params: vec![("pwl", model[open + 1..close].trim().to_string())],
        });
    }

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
        "WHITENOISE" => Some(("WHITENOISE", &["rms", "dt"][..])),
        "PINKNOISE" => Some(("PINKNOISE", &["rms", "slope", "dt"][..])),
        "BURSTNOISE" => Some(("BURSTNOISE", &["ampl", "tcapt", "temit"][..])),
        "RANDUNIFORM" => Some(("RANDUNIFORM", &["ts", "td", "range", "offset"][..])),
        "RANDGAUSSIAN" => Some(("RANDGAUSSIAN", &["ts", "td", "stddev", "mean"][..])),
        "RANDEXP" => Some(("RANDEXP", &["ts", "td", "mean", "offset"][..])),
        "RANDPOISSON" => Some(("RANDPOISSON", &["ts", "td", "lambda", "offset"][..])),
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

// Upstream parity: local helper for the current-sheet page-state half of reused-screen switching.
// This helper exists because the Rust loader stores selected occurrence page metadata on `Screen`
// and `LoadedSheetPath` instead of KiCad's owning screen/project objects. Remaining divergence is
// blocked on richer reused-screen/current-sheet state beyond the current page fields and variants.
pub(crate) fn refresh_current_screen_page_state(
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
