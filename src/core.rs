use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::loader::{HierarchyLink, LoadResult};
use crate::model::Schematic;

#[derive(Debug)]
pub struct SchematicProject {
    pub root_path: PathBuf,
    pub schematics: Vec<Schematic>,
    pub links: Vec<HierarchyLink>,
    by_path: HashMap<PathBuf, usize>,
}

impl SchematicProject {
    pub fn from_load_result(load: LoadResult) -> Self {
        let by_path = load
            .schematics
            .iter()
            .enumerate()
            .map(|(index, schematic)| (schematic.path.clone(), index))
            .collect();

        Self {
            root_path: load.root_path,
            schematics: load.schematics,
            links: load.links,
            by_path,
        }
    }

    pub fn root(&self) -> Option<&Schematic> {
        self.schematic(&self.root_path)
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
}
