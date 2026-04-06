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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSimModel {
    pub library: ResolvedSimLibrary,
    pub name: String,
    pub model_type: Option<String>,
    pub pins: Vec<String>,
    pub params: Vec<(String, Option<String>)>,
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

pub fn resolve_symbol_sim_model(
    schematic_path: &Path,
    screen: &Screen,
    symbol: &Symbol,
) -> Option<ResolvedSimModel> {
    resolve_symbol_sim_model_from_embedded_files(schematic_path, &screen.embedded_files, symbol)
}

pub fn resolve_symbol_sim_model_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
) -> Option<ResolvedSimModel> {
    let library =
        resolve_symbol_sim_library_from_embedded_files(schematic_path, embedded_files, symbol)?;
    let name = symbol
        .sim_model
        .as_ref()
        .and_then(|sim_model| sim_model.name.as_deref())?
        .trim()
        .to_string();

    if name.is_empty() {
        return None;
    }

    match library.kind {
        SimLibraryKind::Ibis => {
            let content = load_symbol_sim_library_content_from_embedded_files(
                schematic_path,
                embedded_files,
                symbol,
            )?;
            let model = resolve_ibis_model(&content.text, &name)?;
            Some(ResolvedSimModel {
                library,
                name: model.name,
                model_type: model.model_type,
                pins: model.pins,
                params: model.params,
            })
        }
        SimLibraryKind::Spice => {
            let content = load_symbol_sim_library_content_from_embedded_files(
                schematic_path,
                embedded_files,
                symbol,
            )?;
            let model = resolve_spice_model(&content.text, &name)?;
            Some(ResolvedSimModel {
                library,
                name: model.name,
                model_type: model.model_type,
                pins: model.pins,
                params: model.params,
            })
        }
    }
}

fn classify_sim_library_name(name: &str) -> SimLibraryKind {
    if name.to_ascii_lowercase().ends_with(".ibs") {
        SimLibraryKind::Ibis
    } else {
        SimLibraryKind::Spice
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedSpiceModel {
    name: String,
    model_type: Option<String>,
    pins: Vec<String>,
    params: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedIbisModel {
    name: String,
    model_type: Option<String>,
    pins: Vec<String>,
    params: Vec<(String, Option<String>)>,
}

fn resolve_spice_model(text: &str, wanted_name: &str) -> Option<ResolvedSpiceModel> {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    let mut index = 0;

    while index + 1 < tokens.len() {
        if tokens[index].eq_ignore_ascii_case(".model")
            && tokens[index + 1].eq_ignore_ascii_case(wanted_name)
        {
            return Some(ResolvedSpiceModel {
                name: tokens[index + 1].to_string(),
                model_type: tokens.get(index + 2).map(|token| token.to_string()),
                pins: Vec::new(),
                params: tokens[index + 3..]
                    .iter()
                    .filter_map(|token| parse_spice_model_param_token(token))
                    .collect(),
            });
        }

        if tokens[index].eq_ignore_ascii_case(".subckt")
            && tokens[index + 1].eq_ignore_ascii_case(wanted_name)
        {
            let mut pins = Vec::new();
            let mut params = Vec::new();

            for token in &tokens[index + 2..] {
                if token.starts_with('.') {
                    break;
                }

                if is_spice_subckt_param_token(token) {
                    if let Some(param) = parse_spice_subckt_param_token(token) {
                        params.push(param);
                    }
                    continue;
                }

                if params.is_empty() {
                    pins.push((*token).to_string());
                } else if let Some(param) = parse_spice_subckt_param_token(token) {
                    params.push(param);
                }
            }

            return Some(ResolvedSpiceModel {
                name: tokens[index + 1].to_string(),
                model_type: None,
                pins,
                params,
            });
        }

        index += 1;
    }

    None
}

fn resolve_ibis_model(text: &str, wanted_name: &str) -> Option<ResolvedIbisModel> {
    let mut current_component: Option<String> = None;
    let mut pending_component_name = false;
    let mut collecting_pins = false;
    let mut pins = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('|') {
            continue;
        }

        if pending_component_name {
            current_component = Some(line.to_string());
            pending_component_name = false;
            collecting_pins = false;
            continue;
        }

        if let Some(rest) = line.strip_prefix("[Component]") {
            let rest = rest.trim();
            current_component = if rest.is_empty() {
                pending_component_name = true;
                None
            } else {
                Some(rest.to_string())
            };
            collecting_pins = false;
            continue;
        }

        if line.starts_with('[') {
            collecting_pins = line.eq_ignore_ascii_case("[Pin]")
                && current_component
                    .as_deref()
                    .is_some_and(|component| component.eq_ignore_ascii_case(wanted_name));
            continue;
        }

        if collecting_pins {
            let tokens = line.split_whitespace().collect::<Vec<_>>();

            if tokens.is_empty() || tokens[0].eq_ignore_ascii_case("pin") {
                continue;
            }

            pins.push(tokens[0].to_string());
        }
    }

    if current_component
        .as_deref()
        .is_some_and(|component| component.eq_ignore_ascii_case(wanted_name))
        || !pins.is_empty()
    {
        return Some(ResolvedIbisModel {
            name: wanted_name.to_string(),
            model_type: None,
            pins,
            params: Vec::new(),
        });
    }

    None
}

fn is_spice_subckt_param_token(token: &str) -> bool {
    token.eq_ignore_ascii_case("params:")
        || token.contains('=')
        || token.starts_with("PARAMS:")
        || token.starts_with("params:")
}

fn parse_spice_subckt_param_token(token: &str) -> Option<(String, Option<String>)> {
    if token == "+" || token.eq_ignore_ascii_case("params:") {
        return None;
    }

    let trimmed = token
        .strip_prefix("PARAMS:")
        .or_else(|| token.strip_prefix("params:"))
        .or_else(|| token.strip_prefix('+'))
        .unwrap_or(token);

    if trimmed.is_empty() {
        return None;
    }

    if let Some((name, value)) = trimmed.split_once('=') {
        return Some((name.to_string(), Some(value.to_string())));
    }

    Some((trimmed.to_string(), None))
}

fn parse_spice_model_param_token(token: &str) -> Option<(String, Option<String>)> {
    let trimmed = token.trim_matches(|ch| matches!(ch, '(' | ')'));

    if trimmed.is_empty() {
        return None;
    }

    if let Some((name, value)) = trimmed.split_once('=') {
        return Some((name.to_string(), Some(value.to_string())));
    }

    Some((trimmed.to_string(), None))
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
