use a3_application::LanguageParseFailure;
use a3_domain::{
    DiagnosticMessage, ParseDiagnostic, ParseDiagnosticCode, ParseDiagnosticSeverity,
    SourcePosition, SourceRange,
};
use tree_sitter::Node;

pub(super) fn node_text<'a>(source: &'a [u8], node: Node<'_>) -> Option<&'a str> {
    source
        .get(node.byte_range())
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
}

pub(super) fn normalized_node_text(source: &[u8], node: Node<'_>) -> Option<String> {
    node_text(source, node).map(normalize_layout)
}

pub(super) fn normalize_layout(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn range_for_offsets(
    source: &[u8],
    start: usize,
    end: usize,
) -> Result<SourceRange, LanguageParseFailure> {
    if start > end || end > source.len() {
        return Err(LanguageParseFailure::InvalidResult);
    }
    SourceRange::new(
        start,
        end,
        position_at(source, start)?,
        position_at(source, end)?,
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)
}

pub(super) fn diagnostic(
    code: ParseDiagnosticCode,
    range: SourceRange,
    message: &'static str,
) -> Result<ParseDiagnostic, LanguageParseFailure> {
    diagnostic_with_severity(code, ParseDiagnosticSeverity::Error, range, message)
}

pub(super) fn warning(
    code: ParseDiagnosticCode,
    range: SourceRange,
    message: &'static str,
) -> Result<ParseDiagnostic, LanguageParseFailure> {
    diagnostic_with_severity(code, ParseDiagnosticSeverity::Warning, range, message)
}

fn diagnostic_with_severity(
    code: ParseDiagnosticCode,
    severity: ParseDiagnosticSeverity,
    range: SourceRange,
    message: &'static str,
) -> Result<ParseDiagnostic, LanguageParseFailure> {
    let message = DiagnosticMessage::try_from_string(message.to_owned())
        .map_err(|_| LanguageParseFailure::InvalidResult)?;
    Ok(ParseDiagnostic::new(code, severity, range, message))
}

fn position_at(source: &[u8], offset: usize) -> Result<SourcePosition, LanguageParseFailure> {
    let prefix = source
        .get(..offset)
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let row = prefix.iter().filter(|byte| **byte == b'\n').count();
    let line_start = prefix
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index.saturating_add(1));
    let column = offset.saturating_sub(line_start);
    Ok(SourcePosition::new(
        u32::try_from(row).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
        u32::try_from(column).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
    ))
}
