use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{EmbeddedFile, EmbeddedFileType, Screen, Symbol};

pub use crate::model::{ResolvedSimLibrary, SimLibraryKind, SimLibrarySource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimLibraryContent {
    pub source: SimLibrarySource,
    pub text: String,
}

pub fn collect_symbol_sim_library_sources(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Vec<SimLibrarySource> {
    collect_symbol_sim_library_sources_from_embedded_files(
        schematic_path,
        &screen.embedded_files,
        symbol,
    )
}

pub fn collect_symbol_sim_library_sources_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
) -> Vec<SimLibrarySource> {
    let mut sources = Vec::new();

    for file in embedded_files {
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
    resolve_symbol_sim_library_source_from_embedded_files(
        schematic_path,
        &screen.embedded_files,
        symbol,
    )
}

pub fn resolve_symbol_sim_library_source_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
) -> Option<SimLibrarySource> {
    let library = symbol
        .sim_model
        .as_ref()
        .and_then(|sim_model| sim_model.library.as_deref())?;

    for source in collect_symbol_sim_library_sources_from_embedded_files(
        schematic_path,
        embedded_files,
        symbol,
    ) {
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

pub fn load_symbol_sim_library_content(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Option<SimLibraryContent> {
    load_symbol_sim_library_content_from_embedded_files(
        schematic_path,
        &screen.embedded_files,
        symbol,
    )
}

pub fn load_symbol_sim_library_content_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
) -> Option<SimLibraryContent> {
    let library = symbol
        .sim_model
        .as_ref()
        .and_then(|sim_model| sim_model.library.as_deref())?;

    for file in embedded_files {
        if file.file_type == Some(EmbeddedFileType::Model)
            && file.name.as_deref() == Some(library)
            && file.data.is_some()
        {
            return Some(SimLibraryContent {
                source: SimLibrarySource::SchematicEmbedded {
                    name: library.to_string(),
                },
                text: file.data.clone().unwrap_or_default(),
            });
        }
    }

    if let Some(lib_symbol) = symbol.lib_symbol.as_ref() {
        for file in &lib_symbol.embedded_files {
            if file.file_type == Some(EmbeddedFileType::Model)
                && file.name.as_deref() == Some(library)
                && file.data.is_some()
            {
                return Some(SimLibraryContent {
                    source: SimLibrarySource::SymbolEmbedded {
                        name: library.to_string(),
                    },
                    text: file.data.clone().unwrap_or_default(),
                });
            }
        }
    }

    let path = resolve_sim_library_path(schematic_path, library);
    let text = fs::read_to_string(&path).ok()?;
    Some(SimLibraryContent {
        source: SimLibrarySource::Filesystem(path),
        text,
    })
}

pub fn classify_symbol_sim_library_kind(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Option<SimLibraryKind> {
    classify_symbol_sim_library_kind_from_embedded_files(
        schematic_path,
        &screen.embedded_files,
        symbol,
    )
}

pub fn classify_symbol_sim_library_kind_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
) -> Option<SimLibraryKind> {
    let source = resolve_symbol_sim_library_source_from_embedded_files(
        schematic_path,
        embedded_files,
        symbol,
    )?;
    let name = match source {
        SimLibrarySource::Filesystem(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        SimLibrarySource::SchematicEmbedded { name }
        | SimLibrarySource::SymbolEmbedded { name } => name,
    };

    Some(classify_sim_library_name(&name))
}

pub fn resolve_symbol_sim_library(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Option<ResolvedSimLibrary> {
    resolve_symbol_sim_library_from_embedded_files(schematic_path, &screen.embedded_files, symbol)
}

pub fn resolve_symbol_sim_library_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
) -> Option<ResolvedSimLibrary> {
    let source = resolve_symbol_sim_library_source_from_embedded_files(
        schematic_path,
        embedded_files,
        symbol,
    )?;
    let name = match &source {
        SimLibrarySource::Filesystem(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        SimLibrarySource::SchematicEmbedded { name }
        | SimLibrarySource::SymbolEmbedded { name } => name.clone(),
    };

    Some(ResolvedSimLibrary {
        source,
        kind: classify_sim_library_name(&name),
    })
}

fn classify_sim_library_name(name: &str) -> SimLibraryKind {
    if name.to_ascii_lowercase().ends_with(".ibs") {
        SimLibraryKind::Ibis
    } else {
        SimLibraryKind::Spice
    }
}

fn resolve_sim_library_path(schematic_path: &Path, library: &str) -> PathBuf {
    let library_path = PathBuf::from(library);

    if library_path.is_absolute() {
        return library_path;
    }

    let project_path = schematic_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&library_path);

    if project_path.exists() {
        return project_path;
    }

    if let Some(spice_lib_dir) = env::var_os("SPICE_LIB_DIR") {
        let spice_path = PathBuf::from(spice_lib_dir).join(&library_path);
        if spice_path.exists() {
            return spice_path;
        }
    }

    project_path
}
