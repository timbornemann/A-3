use super::source::{diagnostic, range_for_offsets, warning};
use crate::normalize_parse_diagnostics;
use a3_application::{
    LanguageParseControl, LanguageParseControlError, LanguageParseFailure, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseArtifacts, LanguageParseResult,
    LocalSymbolId, ParseDiagnostic, ParseDiagnosticCode, ParsedSymbol, Progress, SymbolKind,
    SymbolName, SymbolReference, SymbolSignature, SymbolVisibility, SyntaxProvider, SyntaxRelation,
    SyntaxRelationKind, SyntaxSource, SyntaxTarget,
};
use std::time::Instant;

const MAX_PNPM_WORKSPACE_BYTES: usize = 256 * 1024;
const LINE_POLL_INTERVAL: usize = 64;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    if input.source().len() > MAX_PNPM_WORKSPACE_BYTES {
        return Err(LanguageParseFailure::InputTooLarge);
    }
    ensure_active(control)?;
    let total = u64::try_from(input.source().len())
        .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?
        .max(1);
    report_progress(control, 0, total)?;
    let started = Instant::now();
    let mut artifacts = LanguageParseArtifacts::default();
    let source = match std::str::from_utf8(input.source()) {
        Ok(source) => source,
        Err(error) => {
            let start = error.valid_up_to();
            let end = start.saturating_add(error.error_len().unwrap_or(1));
            push_diagnostic(
                &mut artifacts,
                &policy,
                diagnostic(
                    ParseDiagnosticCode::InvalidEncoding,
                    range_for_offsets(input.source(), start, end.min(input.source().len()))?,
                    "pnpm workspace manifest is not valid UTF-8",
                )?,
            )?;
            return finish(input, policy, control, revision, total, artifacts);
        }
    };

    let full_range = range_for_offsets(input.source(), 0, input.source().len())?;
    let root_id = LocalSymbolId::new(1).map_err(|_| LanguageParseFailure::InvalidResult)?;
    let root = ParsedSymbol::new(
        root_id,
        SymbolKind::Module,
        SymbolName::try_from_string("pnpm-workspace".to_owned())
            .map_err(|_| LanguageParseFailure::InvalidResult)?,
        full_range,
        range_for_offsets(input.source(), 0, 0)?,
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)?
    .with_visibility(SymbolVisibility::Internal)
    .with_signature(
        SymbolSignature::try_from_string("pnpm workspace".to_owned())
            .map_err(|_| LanguageParseFailure::InvalidResult)?,
    );
    artifacts.symbols.push(root);
    artifacts.relations.push(SyntaxRelation::new(
        SyntaxSource::File,
        SyntaxTarget::Symbol(root_id),
        SyntaxRelationKind::Defines,
        SyntaxProvider::Manifest,
        Confidence::certain(),
        full_range,
    ));

    let mut packages_found = false;
    let mut package_count = 0usize;
    let mut in_packages = false;
    let mut offset = 0usize;
    for (line_index, line_with_ending) in source.split_inclusive('\n').enumerate() {
        if line_index.is_multiple_of(LINE_POLL_INTERVAL) {
            ensure_active(control)?;
            if started.elapsed() >= policy.parse_timeout() {
                return Err(LanguageParseFailure::TimedOut);
            }
        }
        let line = line_with_ending.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let indentation = line.len().saturating_sub(line.trim_start().len());
        if indentation == 0 {
            in_packages = false;
            if let Some(rest) = trimmed.strip_prefix("packages:") {
                packages_found = true;
                in_packages = true;
                if !rest.trim().is_empty() {
                    push_diagnostic(
                        &mut artifacts,
                        &policy,
                        warning(
                            ParseDiagnosticCode::UnsupportedSyntax,
                            range_for_offsets(
                                input.source(),
                                offset,
                                offset.saturating_add(line.len()),
                            )?,
                            "Inline pnpm package lists are outside the bounded workspace subset",
                        )?,
                    )?;
                    in_packages = false;
                }
            }
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        if !in_packages {
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        let content = line.trim_start();
        let Some(raw_value) = content.strip_prefix('-') else {
            push_diagnostic(
                &mut artifacts,
                &policy,
                warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    range_for_offsets(input.source(), offset, offset.saturating_add(line.len()))?,
                    "pnpm packages entries must be scalar sequence items",
                )?,
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        };
        let leading_after_dash = raw_value.len().saturating_sub(raw_value.trim_start().len());
        let value = raw_value.trim_start();
        let value_in_line = indentation
            .saturating_add(1)
            .saturating_add(leading_after_dash);
        let Some((pattern, leading_quote, trailing_quote, consumed)) = parse_yaml_scalar(value)
        else {
            push_diagnostic(
                &mut artifacts,
                &policy,
                warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    range_for_offsets(
                        input.source(),
                        offset.saturating_add(value_in_line),
                        offset.saturating_add(line.len()),
                    )?,
                    "pnpm package pattern is not a supported scalar",
                )?,
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        };
        if pattern.is_empty() {
            push_diagnostic(
                &mut artifacts,
                &policy,
                warning(
                    ParseDiagnosticCode::UnsupportedSyntax,
                    range_for_offsets(
                        input.source(),
                        offset.saturating_add(value_in_line),
                        offset
                            .saturating_add(value_in_line)
                            .saturating_add(consumed),
                    )?,
                    "pnpm package pattern must not be empty",
                )?,
            )?;
            offset = offset.saturating_add(line_with_ending.len());
            continue;
        }
        if artifacts.relations.len() >= policy.max_relations() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        let start = offset
            .saturating_add(value_in_line)
            .saturating_add(leading_quote);
        let end = offset
            .saturating_add(value_in_line)
            .saturating_add(consumed)
            .saturating_sub(trailing_quote);
        let range = range_for_offsets(input.source(), start, end)?;
        let reference = match SymbolReference::try_from_string(pattern) {
            Ok(reference) => reference,
            Err(_) => {
                push_diagnostic(
                    &mut artifacts,
                    &policy,
                    warning(
                        ParseDiagnosticCode::OutputTruncated,
                        range,
                        "pnpm package pattern exceeds the adapter contract",
                    )?,
                )?;
                offset = offset.saturating_add(line_with_ending.len());
                continue;
            }
        };
        let relation = SyntaxRelation::new(
            SyntaxSource::Symbol(root_id),
            SyntaxTarget::Unresolved(reference),
            SyntaxRelationKind::Builds,
            SyntaxProvider::Manifest,
            Confidence::certain(),
            range,
        );
        if !artifacts.relations.contains(&relation) {
            artifacts.relations.push(relation);
            package_count = package_count.saturating_add(1);
        }
        offset = offset.saturating_add(line_with_ending.len());
    }
    if !packages_found || package_count == 0 {
        push_diagnostic(
            &mut artifacts,
            &policy,
            warning(
                ParseDiagnosticCode::UnsupportedSyntax,
                full_range,
                "pnpm workspace manifest contains no supported package patterns",
            )?,
        )?;
    }
    if started.elapsed() >= policy.parse_timeout() {
        return Err(LanguageParseFailure::TimedOut);
    }
    finish(input, policy, control, revision, total, artifacts)
}

fn finish(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
    progress_total: u64,
    artifacts: LanguageParseArtifacts,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    let (coverage, diagnostics) = normalize_parse_diagnostics(
        input.source().len(),
        policy.max_diagnostics(),
        artifacts.diagnostics,
    )?;
    let result = LanguageParseResult::new(
        input.revision().clone(),
        revision.clone(),
        policy.contract_version(),
        coverage,
        LanguageParseArtifacts {
            diagnostics,
            ..artifacts
        },
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)?;
    ensure_active(control)?;
    report_progress(control, progress_total, progress_total)?;
    Ok(result)
}

fn parse_yaml_scalar(value: &str) -> Option<(String, usize, usize, usize)> {
    if let Some(rest) = value.strip_prefix('"') {
        let end = find_unescaped_double_quote(rest)?;
        let consumed = end.saturating_add(2);
        let encoded = value.get(..consumed)?;
        let decoded = serde_json::from_str::<String>(encoded).ok()?;
        let trailing = value.get(consumed..)?.trim();
        if !trailing.is_empty() && !trailing.starts_with('#') {
            return None;
        }
        return Some((decoded, 1, 1, consumed));
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let mut decoded = String::new();
        let mut chars = rest.char_indices().peekable();
        while let Some((index, character)) = chars.next() {
            if character == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    let _ = chars.next();
                    decoded.push('\'');
                    continue;
                }
                let consumed = index.saturating_add(2);
                let trailing = value.get(consumed..)?.trim();
                if !trailing.is_empty() && !trailing.starts_with('#') {
                    return None;
                }
                return Some((decoded, 1, 1, consumed));
            }
            decoded.push(character);
        }
        return None;
    }
    let scalar = value
        .split_once(" #")
        .map_or(value, |(before_comment, _)| before_comment)
        .trim_end();
    if scalar.is_empty()
        || scalar.starts_with(['[', '{', '&', '*', '!', '|', '>'])
        || scalar.contains(['\r', '\n', '\0'])
    {
        return None;
    }
    Some((scalar.to_owned(), 0, 0, scalar.len()))
}

fn push_diagnostic(
    artifacts: &mut LanguageParseArtifacts,
    policy: &LanguageParsePolicy,
    diagnostic: ParseDiagnostic,
) -> Result<(), LanguageParseFailure> {
    if artifacts.diagnostics.len() >= policy.max_diagnostics() {
        return Err(LanguageParseFailure::ResourceLimitExceeded);
    }
    artifacts.diagnostics.push(diagnostic);
    Ok(())
}

fn find_unescaped_double_quote(value: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if character == '"' && !escaped {
            return Some(index);
        }
        if character == '\\' {
            escaped = !escaped;
        } else {
            escaped = false;
        }
    }
    None
}

fn ensure_active(control: &dyn LanguageParseControl) -> Result<(), LanguageParseFailure> {
    if control.is_cancelled() {
        return Err(LanguageParseFailure::Cancelled);
    }
    Ok(())
}

fn report_progress(
    control: &dyn LanguageParseControl,
    completed: u64,
    total: u64,
) -> Result<(), LanguageParseFailure> {
    let progress =
        Progress::determinate(completed, total).map_err(|_| LanguageParseFailure::InvalidResult)?;
    control
        .report_progress(progress)
        .map_err(|LanguageParseControlError::Unavailable| LanguageParseFailure::ProgressUnavailable)
}
