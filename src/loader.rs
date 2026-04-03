use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{Schematic, SheetReference};
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

#[derive(Debug)]
pub struct LoadResult {
    pub root_path: PathBuf,
    pub schematics: Vec<Schematic>,
    pub links: Vec<HierarchyLink>,
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
    Ok(LoadResult {
        root_path,
        schematics: loader.schematics,
        links: loader.links,
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
}
