use super::manifest::{ManifestBuilder, dependency_name};
use super::source::diagnostic;
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseResult, ParseDiagnosticCode, SourceRange,
    SymbolRole, SyntaxProvider, SyntaxRelationKind, SyntaxSource,
};

const MAX_SETUP_CFG_BYTES: usize = 256 * 1024;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    if input.source().len() > MAX_SETUP_CFG_BYTES {
        return Err(LanguageParseFailure::InputTooLarge);
    }
    let mut builder = ManifestBuilder::new(input, policy, control)?;
    let source = match std::str::from_utf8(builder.source()) {
        Ok(source) => source.to_owned(),
        Err(error) => {
            let start = error.valid_up_to();
            let end = start.saturating_add(error.error_len().unwrap_or(1));
            builder.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                builder.range(start, end.min(builder.source().len()))?,
                "setup.cfg is not valid UTF-8",
            )?)?;
            return builder.finish(revision);
        }
    };
    let entries = parse_entries(&mut builder, &source)?;
    extract(&mut builder, entries)?;
    builder.finish(revision)
}

fn parse_entries(
    builder: &mut ManifestBuilder<'_>,
    source: &str,
) -> Result<Vec<IniEntry>, LanguageParseFailure> {
    let mut entries = Vec::<IniEntry>::new();
    let mut section = String::new();
    let mut current_entry: Option<usize> = None;
    let mut offset = 0usize;
    for line_with_ending in source.split_inclusive('\n') {
        builder.poll()?;
        let line = line_with_ending.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(['#', ';']) {
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let leading = line.len().saturating_sub(line.trim_start().len());
        if trimmed.starts_with('[') {
            let Some(section_name) = trimmed
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                builder.unsupported(
                    builder.range(offset, offset.saturating_add(line.len()))?,
                    "setup.cfg section header is invalid",
                )?;
                offset = offset.saturating_add(line_with_ending.len());
                continue;
            };
            section = section_name.to_ascii_lowercase();
            current_entry = None;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        if leading > 0 {
            let range = builder.range(
                offset.saturating_add(leading),
                offset.saturating_add(line.len()),
            )?;
            if let Some(index) = current_entry {
                if !trimmed.is_empty() {
                    entries[index].values.push(IniValue {
                        text: trimmed.to_owned(),
                        range,
                    });
                }
            } else {
                builder.unsupported(range, "setup.cfg continuation has no owning key")?;
            }
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let delimiter = line
            .char_indices()
            .find(|(_, character)| matches!(character, '=' | ':'));
        let Some((delimiter_index, delimiter_character)) = delimiter else {
            builder.unsupported(
                builder.range(offset, offset.saturating_add(line.len()))?,
                "setup.cfg line is not a supported key/value entry",
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        };
        let key = line
            .get(..delimiter_index)
            .map(str::trim)
            .unwrap_or_default();
        if key.is_empty() || section.is_empty() {
            builder.unsupported(
                builder.range(offset, offset.saturating_add(line.len()))?,
                "setup.cfg key is outside a supported section",
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let value_start = delimiter_index
            .saturating_add(delimiter_character.len_utf8())
            .saturating_add(
                line.get(delimiter_index.saturating_add(delimiter_character.len_utf8())..)
                    .map(|value| value.len().saturating_sub(value.trim_start().len()))
                    .unwrap_or_default(),
            );
        let value = line
            .get(value_start..)
            .map(str::trim_end)
            .unwrap_or_default();
        let mut values = Vec::new();
        if !value.is_empty() {
            values.push(IniValue {
                text: value.to_owned(),
                range: builder.range(
                    offset.saturating_add(value_start),
                    offset
                        .saturating_add(value_start)
                        .saturating_add(value.len()),
                )?,
            });
        }
        entries.push(IniEntry {
            section: section.clone(),
            key: key.to_ascii_lowercase(),
            values,
        });
        current_entry = entries.len().checked_sub(1);
        offset = offset.saturating_add(line_with_ending.len());
    }
    Ok(entries)
}

fn extract(
    builder: &mut ManifestBuilder<'_>,
    entries: Vec<IniEntry>,
) -> Result<(), LanguageParseFailure> {
    let package_name = entries
        .iter()
        .find(|entry| entry.section == "metadata" && entry.key == "name")
        .and_then(|entry| entry.values.first());
    let selection = package_name.map_or(builder.range(0, 0)?, |value| value.range);
    let name = package_name.map_or("setup", |value| value.text.as_str());
    let has_entrypoints = entries
        .iter()
        .any(|entry| entry.section == "options.entry_points" && !entry.values.is_empty());
    let roles = if has_entrypoints {
        [SymbolRole::Entrypoint].as_slice()
    } else {
        [].as_slice()
    };
    let root = builder.add_root_module(name, selection, "Python setup.cfg package", roles)?;
    for entry in entries {
        builder.poll()?;
        match entry.section.as_str() {
            "options" => match entry.key.as_str() {
                "install_requires" => {
                    add_requirements(builder, root, entry.values, SyntaxRelationKind::Imports)?
                }
                "tests_require" => {
                    add_requirements(builder, root, entry.values, SyntaxRelationKind::Tests)?
                }
                "setup_requires" => {
                    add_requirements(builder, root, entry.values, SyntaxRelationKind::Builds)?
                }
                "packages" | "py_modules" => {
                    for value in entry.values {
                        builder.push_reference(
                            SyntaxSource::Symbol(root),
                            &format!("package:{}", value.text),
                            SyntaxRelationKind::Builds,
                            value.range,
                            SyntaxProvider::Manifest,
                            Confidence::certain(),
                        )?;
                    }
                }
                _ => {}
            },
            "options.extras_require" => add_requirements(
                builder,
                root,
                entry.values,
                dependency_group_kind(&entry.key),
            )?,
            "options.entry_points" => {
                for value in entry.values {
                    let Some((name, target)) = value.text.split_once('=') else {
                        builder.unsupported(
                            value.range,
                            "setup.cfg entry point must contain a static name and target",
                        )?;
                        continue;
                    };
                    let name = name.trim();
                    let target = target.trim();
                    if name.is_empty() || target.is_empty() {
                        builder
                            .unsupported(value.range, "setup.cfg entry point must not be empty")?;
                        continue;
                    }
                    builder.add_entrypoint(root, name, value.range, target, value.range)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn add_requirements(
    builder: &mut ManifestBuilder<'_>,
    root: a3_domain::LocalSymbolId,
    values: Vec<IniValue>,
    kind: SyntaxRelationKind,
) -> Result<(), LanguageParseFailure> {
    for value in values {
        builder.poll()?;
        let Some(name) = dependency_name(&value.text) else {
            builder.unsupported(
                value.range,
                "setup.cfg dependency has no static package name",
            )?;
            continue;
        };
        builder.push_reference(
            SyntaxSource::Symbol(root),
            name,
            kind,
            value.range,
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )?;
    }
    Ok(())
}

fn dependency_group_kind(group: &str) -> SyntaxRelationKind {
    if group.contains("test") || group.contains("dev") {
        SyntaxRelationKind::Tests
    } else {
        SyntaxRelationKind::Imports
    }
}

struct IniEntry {
    section: String,
    key: String,
    values: Vec<IniValue>,
}

struct IniValue {
    text: String,
    range: SourceRange,
}
