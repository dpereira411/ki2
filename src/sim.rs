use std::env;
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};

use crate::model::{EmbeddedFile, EmbeddedFileType, ResolvedSimModelKind, Screen, Symbol};

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
    pub kind: ResolvedSimModelKind,
    pub model_type: Option<String>,
    pub ibis_model_type: Option<String>,
    pub diff_pin: Option<String>,
    pub pins: Vec<String>,
    pub params: Vec<(String, Option<String>)>,
}

fn ibis_kind_override(model_type: Option<&str>) -> Option<ResolvedSimModelKind> {
    match model_type?.trim().to_ascii_uppercase().as_str() {
        "DEVICE" => Some(ResolvedSimModelKind::IbisComponent),
        "DCDRIVER" => Some(ResolvedSimModelKind::IbisDriverDc),
        "RECTDRIVER" => Some(ResolvedSimModelKind::IbisDriverRect),
        "PRBSDRIVER" => Some(ResolvedSimModelKind::IbisDriverPrbs),
        _ => None,
    }
}

fn sim_library_source_key(source: &SimLibrarySource) -> String {
    match source {
        SimLibrarySource::Filesystem(path) => format!("fs:{}", path.display()),
        SimLibrarySource::SchematicEmbedded { name } => format!("sch:{name}"),
        SimLibrarySource::SymbolEmbedded { name } => format!("sym:{name}"),
    }
}

fn normalize_embedded_library_name(name: &str) -> String {
    let mut normalized = PathBuf::new();

    for component in Path::new(name).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {}
        }
    }

    normalized.to_string_lossy().replace('\\', "/")
}

fn resolve_relative_embedded_library_name(base_name: &str, relative: &str) -> String {
    let base_path = Path::new(base_name);
    let joined = base_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(relative);
    normalize_embedded_library_name(&joined.to_string_lossy())
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

fn load_sim_library_content_by_source_from_embedded_files(
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
    source: &SimLibrarySource,
) -> Option<SimLibraryContent> {
    match source {
        SimLibrarySource::SchematicEmbedded { name } => embedded_files
            .iter()
            .find(|file| {
                file.file_type == Some(EmbeddedFileType::Model)
                    && file.name.as_deref() == Some(name.as_str())
                    && file.data.is_some()
            })
            .map(|file| SimLibraryContent {
                source: source.clone(),
                text: file.data.clone().unwrap_or_default(),
            }),
        SimLibrarySource::SymbolEmbedded { name } => {
            symbol.lib_symbol.as_ref().and_then(|lib_symbol| {
                lib_symbol
                    .embedded_files
                    .iter()
                    .find(|file| {
                        file.file_type == Some(EmbeddedFileType::Model)
                            && file.name.as_deref() == Some(name.as_str())
                            && file.data.is_some()
                    })
                    .map(|file| SimLibraryContent {
                        source: source.clone(),
                        text: file.data.clone().unwrap_or_default(),
                    })
            })
        }
        SimLibrarySource::Filesystem(path) => {
            fs::read_to_string(path).ok().map(|text| SimLibraryContent {
                source: source.clone(),
                text,
            })
        }
    }
}

fn resolve_relative_sim_library_source_from_embedded_files(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
    base_source: &SimLibrarySource,
    relative_lib: &str,
) -> Option<SimLibrarySource> {
    let relative_lib = relative_lib.trim_matches('"');

    if relative_lib.is_empty() {
        return None;
    }

    match base_source {
        SimLibrarySource::Filesystem(path) => {
            let resolved = if Path::new(relative_lib).is_absolute() {
                PathBuf::from(relative_lib)
            } else {
                path.parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(relative_lib)
            };

            if resolved.exists() {
                Some(SimLibrarySource::Filesystem(resolved))
            } else {
                None
            }
        }
        SimLibrarySource::SchematicEmbedded { name } => {
            let resolved_name = resolve_relative_embedded_library_name(name, relative_lib);

            if embedded_files.iter().any(|file| {
                file.file_type == Some(EmbeddedFileType::Model)
                    && file.name.as_deref() == Some(resolved_name.as_str())
            }) {
                Some(SimLibrarySource::SchematicEmbedded {
                    name: resolved_name,
                })
            } else {
                let fs_path = resolve_sim_library_path(schematic_path, &resolved_name);
                fs_path
                    .exists()
                    .then_some(SimLibrarySource::Filesystem(fs_path))
            }
        }
        SimLibrarySource::SymbolEmbedded { name } => {
            let resolved_name = resolve_relative_embedded_library_name(name, relative_lib);

            if symbol.lib_symbol.as_ref().is_some_and(|lib_symbol| {
                lib_symbol.embedded_files.iter().any(|file| {
                    file.file_type == Some(EmbeddedFileType::Model)
                        && file.name.as_deref() == Some(resolved_name.as_str())
                })
            }) {
                Some(SimLibrarySource::SymbolEmbedded {
                    name: resolved_name,
                })
            } else if embedded_files.iter().any(|file| {
                file.file_type == Some(EmbeddedFileType::Model)
                    && file.name.as_deref() == Some(resolved_name.as_str())
            }) {
                Some(SimLibrarySource::SchematicEmbedded {
                    name: resolved_name,
                })
            } else {
                let fs_path = resolve_sim_library_path(schematic_path, &resolved_name);
                fs_path
                    .exists()
                    .then_some(SimLibrarySource::Filesystem(fs_path))
            }
        }
    }
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
    let raw_name = symbol
        .sim_model
        .as_ref()
        .and_then(|sim_model| sim_model.name.as_deref())?
        .trim()
        .to_string();

    if raw_name.is_empty() {
        return None;
    }

    let name = raw_name
        .split_once(' ')
        .map(|(name, _)| name)
        .unwrap_or(raw_name.as_str())
        .to_string();

    if name.is_empty() {
        return None;
    }

    match library.kind {
        SimLibraryKind::Ibis => {
            let selected_pin = symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.ibis_pin.as_deref());
            let selected_model = symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.ibis_model.as_deref());
            let type_override = symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| ibis_kind_override(sim_model.model_type.as_deref()));
            let content = load_symbol_sim_library_content_from_embedded_files(
                schematic_path,
                embedded_files,
                symbol,
            )?;
            let model = resolve_ibis_model(&content.text, &name, selected_pin, selected_model)?;
            Some(ResolvedSimModel {
                library,
                name: model.name,
                kind: type_override.unwrap_or(model.kind),
                model_type: model.model_type,
                ibis_model_type: model.ibis_model_type,
                diff_pin: model.diff_pin,
                pins: model.pins,
                params: model.params,
            })
        }
        SimLibraryKind::Spice => {
            let model = resolve_spice_model_from_source(
                schematic_path,
                embedded_files,
                symbol,
                &library.source,
                &name,
                &mut Vec::new(),
            )?;
            Some(ResolvedSimModel {
                library,
                name: model.name,
                kind: model.kind,
                model_type: model.model_type,
                ibis_model_type: None,
                diff_pin: None,
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
    kind: ResolvedSimModelKind,
    model_type: Option<String>,
    pins: Vec<String>,
    params: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedIbisModel {
    name: String,
    kind: ResolvedSimModelKind,
    model_type: Option<String>,
    ibis_model_type: Option<String>,
    diff_pin: Option<String>,
    pins: Vec<String>,
    params: Vec<(String, Option<String>)>,
}

fn resolve_spice_model(text: &str, wanted_name: &str) -> Option<ResolvedSpiceModel> {
    for statement in collect_spice_statements(text) {
        let tokens = statement.split_whitespace().collect::<Vec<_>>();

        if tokens.len() < 2 {
            continue;
        }

        if tokens[0].eq_ignore_ascii_case(".model") && tokens[1].eq_ignore_ascii_case(wanted_name) {
            return Some(ResolvedSpiceModel {
                name: tokens[1].to_string(),
                kind: ResolvedSimModelKind::SpiceModel,
                model_type: tokens.get(2).map(|token| token.to_string()),
                pins: Vec::new(),
                params: tokens[3..]
                    .iter()
                    .filter_map(|token| parse_spice_model_param_token(token))
                    .collect(),
            });
        }

        if tokens[0].eq_ignore_ascii_case(".subckt") && tokens[1].eq_ignore_ascii_case(wanted_name)
        {
            let mut pins = Vec::new();
            let mut params = Vec::new();

            for token in &tokens[2..] {
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
                name: tokens[1].to_string(),
                kind: ResolvedSimModelKind::SpiceSubckt,
                model_type: None,
                pins,
                params,
            });
        }
    }

    None
}

fn collect_spice_statements(text: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('*') {
            continue;
        }

        if let Some(continuation) = line.strip_prefix('+') {
            let continuation = continuation.trim();

            if !continuation.is_empty() {
                if !current.is_empty() {
                    current.push(' ');
                }

                current.push_str(continuation);
            }

            continue;
        }

        if !current.is_empty() {
            statements.push(current);
            current = String::new();
        }

        current.push_str(line);
    }

    if !current.is_empty() {
        statements.push(current);
    }

    statements
}

fn collect_spice_include_paths(text: &str) -> Vec<String> {
    collect_spice_statements(text)
        .into_iter()
        .filter_map(|statement| {
            let mut parts = statement.split_whitespace();
            let head = parts.next()?;

            if !head.eq_ignore_ascii_case(".include") {
                return None;
            }

            let rest = statement[head.len()..].trim();

            if rest.is_empty() {
                return None;
            }

            let include = if let Some(stripped) = rest.strip_prefix('"') {
                stripped
                    .split_once('"')
                    .map(|(path, _)| path)
                    .unwrap_or(stripped)
                    .to_string()
            } else {
                rest.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string()
            };

            (!include.is_empty()).then_some(include)
        })
        .collect()
}

fn resolve_spice_model_from_source(
    schematic_path: &Path,
    embedded_files: &[EmbeddedFile],
    symbol: &Symbol,
    source: &SimLibrarySource,
    wanted_name: &str,
    visited: &mut Vec<String>,
) -> Option<ResolvedSpiceModel> {
    let source_key = sim_library_source_key(source);

    if visited.contains(&source_key) {
        return None;
    }

    visited.push(source_key);

    let content =
        load_sim_library_content_by_source_from_embedded_files(embedded_files, symbol, source)?;

    if let Some(model) = resolve_spice_model(&content.text, wanted_name) {
        return Some(model);
    }

    for include in collect_spice_include_paths(&content.text) {
        let Some(include_source) = resolve_relative_sim_library_source_from_embedded_files(
            schematic_path,
            embedded_files,
            symbol,
            source,
            &include,
        ) else {
            continue;
        };

        if let Some(model) = resolve_spice_model_from_source(
            schematic_path,
            embedded_files,
            symbol,
            &include_source,
            wanted_name,
            visited,
        ) {
            return Some(model);
        }
    }

    None
}

fn resolve_ibis_model(
    text: &str,
    wanted_name: &str,
    selected_pin: Option<&str>,
    selected_model: Option<&str>,
) -> Option<ResolvedIbisModel> {
    fn matches_selected_ibis_model(
        explicit_model: Option<&str>,
        resolved_model: Option<&str>,
        candidate: &str,
    ) -> bool {
        explicit_model
            .or(resolved_model)
            .is_some_and(|selected| selected.eq_ignore_ascii_case(candidate))
    }

    let mut current_component: Option<String> = None;
    let mut pending_component_name = false;
    let mut pending_model_name = false;
    let mut collecting_pins = false;
    let mut collecting_diff_pins = false;
    let mut collecting_model = false;
    let mut pins = Vec::new();
    let mut resolved_model_type = None;
    let mut resolved_ibis_model_type = None;
    let mut resolved_diff_pin = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('|') {
            continue;
        }

        if pending_component_name {
            current_component = Some(line.to_string());
            pending_component_name = false;
            collecting_pins = false;
            collecting_diff_pins = false;
            collecting_model = false;
            continue;
        }

        if pending_model_name {
            pending_model_name = false;
            collecting_model =
                matches_selected_ibis_model(selected_model, resolved_model_type.as_deref(), line);
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
            collecting_diff_pins = false;
            collecting_model = false;
            continue;
        }

        if let Some(rest) = line.strip_prefix("[Model]") {
            let rest = rest.trim();
            pending_model_name = rest.is_empty();
            collecting_model = !pending_model_name
                && matches_selected_ibis_model(
                    selected_model,
                    resolved_model_type.as_deref(),
                    rest,
                );
            collecting_pins = false;
            collecting_diff_pins = false;
            continue;
        }

        if line.starts_with('[') {
            collecting_pins = line.eq_ignore_ascii_case("[Pin]")
                && current_component
                    .as_deref()
                    .is_some_and(|component| component.eq_ignore_ascii_case(wanted_name));
            collecting_diff_pins = line.eq_ignore_ascii_case("[Diff Pin]")
                && current_component
                    .as_deref()
                    .is_some_and(|component| component.eq_ignore_ascii_case(wanted_name));
            collecting_model = false;
            continue;
        }

        if collecting_pins {
            let tokens = line.split_whitespace().collect::<Vec<_>>();

            if tokens.is_empty() || tokens[0].eq_ignore_ascii_case("pin") {
                continue;
            }

            pins.push(tokens[0].to_string());

            if selected_pin.is_some_and(|pin| pin.eq_ignore_ascii_case(tokens[0])) {
                if let Some(model_name) = selected_model {
                    if tokens
                        .get(2)
                        .is_some_and(|token| token.eq_ignore_ascii_case(model_name))
                    {
                        resolved_model_type = Some(model_name.to_string());
                    }
                }

                if resolved_model_type.is_none() {
                    resolved_model_type = tokens.get(2).map(|token| token.to_string());
                }
            }
        }

        if collecting_diff_pins {
            let tokens = line.split_whitespace().collect::<Vec<_>>();

            if tokens.len() < 2 || tokens[0].eq_ignore_ascii_case("pin") {
                continue;
            }

            if selected_pin.is_some_and(|pin| pin.eq_ignore_ascii_case(tokens[0])) {
                resolved_diff_pin = Some(tokens[1].to_string());
            } else if selected_pin.is_some_and(|pin| pin.eq_ignore_ascii_case(tokens[1])) {
                resolved_diff_pin = Some(tokens[0].to_string());
            }
        }

        if collecting_model {
            let tokens = line.split_whitespace().collect::<Vec<_>>();

            if tokens.len() >= 2 && tokens[0].eq_ignore_ascii_case("Model_type") {
                resolved_ibis_model_type = Some(tokens[1].to_string());
            }
        }
    }

    if current_component
        .as_deref()
        .is_some_and(|component| component.eq_ignore_ascii_case(wanted_name))
        || !pins.is_empty()
    {
        return Some(ResolvedIbisModel {
            name: wanted_name.to_string(),
            kind: ResolvedSimModelKind::IbisComponent,
            model_type: resolved_model_type,
            ibis_model_type: resolved_ibis_model_type,
            diff_pin: resolved_diff_pin,
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
