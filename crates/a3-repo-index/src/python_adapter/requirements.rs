use super::manifest::{ManifestBuilder, dependency_name};
use super::source::diagnostic;
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseResult, ParseDiagnosticCode, SymbolRole,
    SyntaxProvider, SyntaxRelationKind, SyntaxSource,
};

const MAX_REQUIREMENTS_BYTES: usize = 512 * 1024;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    if input.source().len() > MAX_REQUIREMENTS_BYTES {
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
                "Python requirements file is not valid UTF-8",
            )?)?;
            return builder.finish(revision);
        }
    };
    let name = requirements_name(builder.path());
    let is_test =
        name.to_ascii_lowercase().contains("test") || name.to_ascii_lowercase().contains("dev");
    let roles = if is_test {
        [SymbolRole::Test].as_slice()
    } else {
        [].as_slice()
    };
    let root = builder.add_root_module(
        &name,
        builder.range(0, 0)?,
        "Python requirements manifest",
        roles,
    )?;
    let dependency_kind = if is_test {
        SyntaxRelationKind::Tests
    } else {
        SyntaxRelationKind::Imports
    };
    let mut offset = 0usize;
    for line_with_ending in source.split_inclusive('\n') {
        builder.poll()?;
        let line = line_with_ending.trim_end_matches(['\r', '\n']);
        let trimmed_start = line.len().saturating_sub(line.trim_start().len());
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let content = strip_requirement_comment(trimmed).trim_end();
        let start = offset.saturating_add(trimmed_start);
        let end = start.saturating_add(content.len());
        let range = builder.range(start, end)?;
        if content.ends_with('\\') {
            builder.unsupported(
                range,
                "Continued Python requirement lines are not statically combined",
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        if content.starts_with("--hash=") {
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        if let Some(include) = requirement_include(content) {
            builder.push_file(
                SyntaxSource::Symbol(root),
                include,
                SyntaxRelationKind::Builds,
                range,
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        if content.starts_with('-') && !content.starts_with("-e ") {
            builder.unsupported(
                range,
                "Python requirement option is intentionally not retained",
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let editable = content
            .strip_prefix("-e ")
            .map(str::trim)
            .unwrap_or(content);
        if editable.starts_with("./") || editable.starts_with("../") {
            builder.push_file(
                SyntaxSource::Symbol(root),
                editable,
                SyntaxRelationKind::Builds,
                range,
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let static_name = dependency_name(editable).or_else(|| egg_name(editable));
        let Some(name) = static_name else {
            builder.unsupported(range, "Python requirement has no static package name")?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        };
        builder.push_reference(
            SyntaxSource::Symbol(root),
            name,
            dependency_kind,
            range,
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )?;
        offset = offset.saturating_add(line_with_ending.len());
    }
    builder.finish(revision)
}

fn requirements_name(path: &a3_domain::RepositoryPath) -> String {
    let basename = path
        .as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    [b".txt".as_slice(), b".in"]
        .iter()
        .find_map(|extension| basename.strip_suffix(*extension))
        .and_then(|stem| std::str::from_utf8(stem).ok())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("requirements")
        .to_owned()
}

fn strip_requirement_comment(value: &str) -> &str {
    value
        .split_once(" #")
        .map_or(value, |(before_comment, _)| before_comment)
}

fn requirement_include(value: &str) -> Option<&str> {
    ["-r", "--requirement", "-c", "--constraint"]
        .iter()
        .find_map(|prefix| {
            value
                .strip_prefix(prefix)
                .filter(|rest| rest.starts_with(char::is_whitespace))
                .map(str::trim)
                .filter(|rest| !rest.is_empty())
        })
}

fn egg_name(value: &str) -> Option<&str> {
    value
        .split_once("#egg=")
        .map(|(_, name)| name)
        .and_then(dependency_name)
}
