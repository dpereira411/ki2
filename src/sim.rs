use std::path::{Path, PathBuf};

use crate::model::{EmbeddedFileType, Screen, Symbol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimLibrarySource {
    Filesystem(PathBuf),
    SchematicEmbedded { name: String },
    SymbolEmbedded { name: String },
}

pub fn collect_symbol_sim_library_sources(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Vec<SimLibrarySource> {
    let mut sources = Vec::new();

    for file in &screen.embedded_files {
        if file.file_type == Some(EmbeddedFileType::Model) {
            if let Some(name) = file.name.as_ref() {
                sources.push(SimLibrarySource::SchematicEmbedded { name: name.clone() });
            }
        }
    }

    if let Some(lib_symbol) = symbol.lib_symbol.as_ref() {
        for file in &lib_symbol.embedded_files {
            if file.file_type == Some(EmbeddedFileType::Model) {
                if let Some(name) = file.name.as_ref() {
                    sources.push(SimLibrarySource::SymbolEmbedded { name: name.clone() });
                }
            }
        }
    }

    sources.push(SimLibrarySource::Filesystem(resolve_sim_library_path(
        schematic_path,
        symbol
            .sim_model
            .as_ref()
            .and_then(|sim_model| sim_model.library.as_deref())
            .unwrap_or_default(),
    )));

    sources
}

pub fn resolve_symbol_sim_library_source(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Option<SimLibrarySource> {
    let library = symbol
        .sim_model
        .as_ref()
        .and_then(|sim_model| sim_model.library.as_deref())?;

    for source in collect_symbol_sim_library_sources(schematic_path, screen, symbol) {
        match &source {
            SimLibrarySource::SchematicEmbedded { name }
            | SimLibrarySource::SymbolEmbedded { name }
                if name == library =>
            {
                return Some(source);
            }
            SimLibrarySource::Filesystem(path)
                if path == &resolve_sim_library_path(schematic_path, library) =>
            {
                return Some(source);
            }
            _ => {}
        }
    }

    None
}

fn resolve_sim_library_path(schematic_path: &Path, library: &str) -> PathBuf {
    let library_path = PathBuf::from(library);

    if library_path.is_absolute() {
        return library_path;
    }

    schematic_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(library_path)
}
