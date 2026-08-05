use a3_application::LanguageParseFailure;
use a3_domain::{
    DiagnosticMessage, ParseDiagnostic, ParseDiagnosticCode, ParseDiagnosticSeverity,
    SourcePosition, SourceRange,
};
use std::iter::Peekable;
use std::str::Chars;
use tree_sitter::{Node, Point};

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

#[derive(Debug, Clone)]
pub(super) struct SourceLocator {
    source_len: usize,
    line_starts: Vec<usize>,
}

impl SourceLocator {
    pub(super) fn new(source: &[u8]) -> Self {
        let mut line_starts = Vec::with_capacity(
            source
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                .saturating_add(1),
        );
        line_starts.push(0);
        for (index, byte) in source.iter().enumerate() {
            if *byte == b'\n' && index.saturating_add(1) <= source.len() {
                line_starts.push(index.saturating_add(1));
            }
        }
        Self {
            source_len: source.len(),
            line_starts,
        }
    }

    pub(super) fn range(
        &self,
        start: usize,
        end: usize,
    ) -> Result<SourceRange, LanguageParseFailure> {
        if start > end || end > self.source_len {
            return Err(LanguageParseFailure::InvalidResult);
        }
        SourceRange::new(start, end, self.position(start)?, self.position(end)?)
            .map_err(|_| LanguageParseFailure::InvalidResult)
    }

    fn position(&self, offset: usize) -> Result<SourcePosition, LanguageParseFailure> {
        let row = self
            .line_starts
            .partition_point(|line_start| *line_start <= offset)
            .saturating_sub(1);
        let line_start = *self
            .line_starts
            .get(row)
            .ok_or(LanguageParseFailure::InvalidResult)?;
        Ok(SourcePosition::new(
            u32::try_from(row).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
            u32::try_from(offset.saturating_sub(line_start))
                .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaticString {
    pub(super) value: String,
    pub(super) range: SourceRange,
}

pub(super) fn static_string(
    source: &[u8],
    node: Node<'_>,
) -> Result<Option<StaticString>, LanguageParseFailure> {
    if node.kind() == "concatenated_string" {
        let mut value = String::new();
        for index in 0..node.named_child_count() {
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = node
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            let Some(part) = static_string(source, child)? else {
                return Ok(None);
            };
            value.push_str(&part.value);
        }
        return Ok(Some(StaticString {
            value,
            range: crate::source_range_for_node(node)?,
        }));
    }
    if node.kind() != "string" || node.named_child_count() < 2 {
        return Ok(None);
    }
    let start = node
        .named_child(0)
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let end_index = node
        .named_child_count()
        .checked_sub(1)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let end = node
        .named_child(end_index)
        .ok_or(LanguageParseFailure::InvalidResult)?;
    if start.kind() != "string_start"
        || end.kind() != "string_end"
        || (0..node.named_child_count()).any(|index| {
            u32::try_from(index)
                .ok()
                .and_then(|index| node.named_child(index))
                .is_some_and(|child| child.kind() == "interpolation")
        })
    {
        return Ok(None);
    }
    let Some(start_text) = node_text(source, start) else {
        return Ok(None);
    };
    let prefix_end = start_text
        .find(['\'', '"'])
        .ok_or(LanguageParseFailure::InvalidResult)?;
    let prefix = start_text
        .get(..prefix_end)
        .ok_or(LanguageParseFailure::InvalidResult)?;
    if prefix
        .chars()
        .any(|character| matches!(character, 'b' | 'B' | 'f' | 'F'))
    {
        return Ok(None);
    }
    let raw = prefix
        .chars()
        .any(|character| matches!(character, 'r' | 'R'));
    let content = source
        .get(start.end_byte()..end.start_byte())
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    let Some(content) = content else {
        return Ok(None);
    };
    let Some(value) = decode_python_string(content, raw) else {
        return Ok(None);
    };
    Ok(Some(StaticString {
        value,
        range: range_from_points(
            start.end_byte(),
            end.start_byte(),
            start.end_position(),
            end.start_position(),
        )?,
    }))
}

fn range_from_points(
    start: usize,
    end: usize,
    start_point: Point,
    end_point: Point,
) -> Result<SourceRange, LanguageParseFailure> {
    SourceRange::new(
        start,
        end,
        position_from_point(start_point)?,
        position_from_point(end_point)?,
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)
}

fn position_from_point(point: Point) -> Result<SourcePosition, LanguageParseFailure> {
    Ok(SourcePosition::new(
        u32::try_from(point.row).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
        u32::try_from(point.column).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
    ))
}

fn decode_python_string(value: &str, raw: bool) -> Option<String> {
    if raw {
        return Some(value.to_owned());
    }
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = characters.next()?;
        match escaped {
            '\\' => output.push('\\'),
            '\'' => output.push('\''),
            '"' => output.push('"'),
            'a' => output.push('\u{0007}'),
            'b' => output.push('\u{0008}'),
            'f' => output.push('\u{000c}'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            'v' => output.push('\u{000b}'),
            '\n' => {}
            '\r' if characters.peek() == Some(&'\n') => {
                let _ = characters.next();
            }
            'x' => output.push(char::from_u32(read_radix(&mut characters, 2, 16)?)?),
            'u' => output.push(char::from_u32(read_radix(&mut characters, 4, 16)?)?),
            'U' => output.push(char::from_u32(read_radix(&mut characters, 8, 16)?)?),
            'N' => return None,
            digit if digit.is_digit(8) => {
                let mut octal = String::from(digit);
                for _ in 0..2 {
                    if characters.peek().is_some_and(|next| next.is_digit(8)) {
                        if let Some(next) = characters.next() {
                            octal.push(next);
                        }
                    } else {
                        break;
                    }
                }
                output.push(char::from_u32(u32::from_str_radix(&octal, 8).ok()?)?);
            }
            other => {
                output.push('\\');
                output.push(other);
            }
        }
    }
    Some(output)
}

fn read_radix(characters: &mut Peekable<Chars<'_>>, count: usize, radix: u32) -> Option<u32> {
    let mut value = String::with_capacity(count);
    for _ in 0..count {
        value.push(characters.next()?);
    }
    u32::from_str_radix(&value, radix).ok()
}
