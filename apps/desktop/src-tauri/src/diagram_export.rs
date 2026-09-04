use a3_protocol::AgentDiagramExportFormatV1;
use base64::Engine;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;
const MAX_PNG_BYTES: usize = 8 * 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagramExportFailure {
    InvalidPayload,
    Unavailable,
}

pub(crate) fn validate_rendered_payload(
    format: AgentDiagramExportFormatV1,
    payload: &str,
) -> Result<Vec<u8>, DiagramExportFailure> {
    match format {
        AgentDiagramExportFormatV1::Svg => validate_svg(payload),
        AgentDiagramExportFormatV1::Png => validate_png(payload),
    }
}

pub(crate) fn safe_file_name(title: &str, extension: &str) -> String {
    let mut stem = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while stem.contains("--") {
        stem = stem.replace("--", "-");
    }
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "a3-diagram" } else { stem };
    format!(
        "{}.{}",
        stem.chars().take(80).collect::<String>(),
        extension
    )
}

pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), DiagramExportFailure> {
    let parent = path.parent().ok_or(DiagramExportFailure::Unavailable)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(DiagramExportFailure::Unavailable)?;
    let temporary = private_sibling(parent, file_name, "tmp")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| DiagramExportFailure::Unavailable)?;
    let write_result = file.write_all(bytes).and_then(|()| file.sync_all());
    if write_result.is_err() {
        let _ignored = fs::remove_file(&temporary);
        return Err(DiagramExportFailure::Unavailable);
    }
    drop(file);

    if !path.exists() {
        return fs::rename(&temporary, path).map_err(|_| {
            let _ignored = fs::remove_file(&temporary);
            DiagramExportFailure::Unavailable
        });
    }

    let backup = private_sibling(parent, file_name, "backup")?;
    fs::rename(path, &backup).map_err(|_| {
        let _ignored = fs::remove_file(&temporary);
        DiagramExportFailure::Unavailable
    })?;
    if fs::rename(&temporary, path).is_err() {
        let _ignored = fs::rename(&backup, path);
        let _ignored = fs::remove_file(&temporary);
        return Err(DiagramExportFailure::Unavailable);
    }
    let _ignored = fs::remove_file(backup);
    Ok(())
}

fn private_sibling(
    parent: &Path,
    file_name: &str,
    suffix: &str,
) -> Result<PathBuf, DiagramExportFailure> {
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).map_err(|_| DiagramExportFailure::Unavailable)?;
    let random = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(parent.join(format!(".{file_name}.a3-{random}.{suffix}")))
}

fn validate_svg(payload: &str) -> Result<Vec<u8>, DiagramExportFailure> {
    let bytes = payload.as_bytes();
    let lower = payload.to_ascii_lowercase();
    if bytes.is_empty()
        || bytes.len() > MAX_SVG_BYTES
        || !payload.trim_start().starts_with("<svg")
        || !payload.trim_end().ends_with("</svg>")
        || [
            "<script",
            "<foreignobject",
            "<iframe",
            "<object",
            "<embed",
            "<a ",
            "<animate",
            "<set",
            "<image",
            "<use",
            "<audio",
            "<video",
            "<canvas",
            "javascript:",
            "data:",
            "expression(",
            "@import",
            "<!doctype",
            "<!entity",
            "<?xml-stylesheet",
            " xml:base=",
            " href=",
            " xlink:href=",
        ]
        .iter()
        .any(|forbidden| lower.contains(forbidden))
        || contains_event_attribute(&lower)
        || contains_external_css_url(&lower)
    {
        return Err(DiagramExportFailure::InvalidPayload);
    }
    Ok(bytes.to_vec())
}

fn contains_event_attribute(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0_usize;
    while index + 2 < bytes.len() {
        let boundary = index == 0
            || bytes[index - 1].is_ascii_whitespace()
            || matches!(bytes[index - 1], b'<' | b'/');
        if boundary && bytes[index] == b'o' && bytes[index + 1] == b'n' {
            let mut cursor = index + 2;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric()
                    || matches!(bytes[cursor], b'-' | b'_' | b':'))
            {
                cursor += 1;
            }
            if cursor > index + 2 {
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if bytes.get(cursor) == Some(&b'=') {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

fn contains_external_css_url(value: &str) -> bool {
    let mut remainder = value;
    while let Some(index) = remainder.find("url(") {
        remainder = &remainder[index + 4..];
        let candidate = remainder.trim_start_matches([' ', '\'', '"']);
        if !candidate.starts_with('#') {
            return true;
        }
    }
    false
}

fn validate_png(payload: &str) -> Result<Vec<u8>, DiagramExportFailure> {
    let encoded = payload
        .strip_prefix("data:image/png;base64,")
        .ok_or(DiagramExportFailure::InvalidPayload)?;
    if encoded.len() > MAX_PNG_BYTES.saturating_mul(2) {
        return Err(DiagramExportFailure::InvalidPayload);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| DiagramExportFailure::InvalidPayload)?;
    if bytes.len() > MAX_PNG_BYTES || !valid_png_structure(&bytes) {
        return Err(DiagramExportFailure::InvalidPayload);
    }
    Ok(bytes)
}

fn valid_png_structure(bytes: &[u8]) -> bool {
    if bytes.len() < 33 || bytes.get(..8) != Some(PNG_SIGNATURE.as_slice()) {
        return false;
    }
    let mut offset = 8_usize;
    let mut first = true;
    let mut seen_idat = false;
    while offset.saturating_add(12) <= bytes.len() {
        let Some(length_bytes) = bytes.get(offset..offset + 4) else {
            return false;
        };
        let Ok(length_bytes) = <[u8; 4]>::try_from(length_bytes) else {
            return false;
        };
        let length = usize::try_from(u32::from_be_bytes(length_bytes)).unwrap_or(usize::MAX);
        let end = offset.saturating_add(12).saturating_add(length);
        if end > bytes.len() {
            return false;
        }
        let kind = &bytes[offset + 4..offset + 8];
        if first {
            if kind != b"IHDR" || length != 13 {
                return false;
            }
            let width =
                u32::from_be_bytes(bytes[offset + 8..offset + 12].try_into().unwrap_or([0; 4]));
            let height =
                u32::from_be_bytes(bytes[offset + 12..offset + 16].try_into().unwrap_or([0; 4]));
            if width == 0
                || height == 0
                || width > 8_192
                || height > 8_192
                || u64::from(width).saturating_mul(u64::from(height)) > 16_777_216
            {
                return false;
            }
            first = false;
        } else if kind == b"IHDR" {
            return false;
        }
        if kind == b"IDAT" {
            seen_idat = true;
        }
        offset = end;
        if kind == b"IEND" {
            return seen_idat && length == 0 && offset == bytes.len();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_rejects_active_content_events_links_and_external_css() {
        assert!(validate_svg(r#"<svg><path d="M0 0"/></svg>"#).is_ok());
        for payload in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><path onload="x"/></svg>"#,
            r#"<svg><path onload = "x"/></svg>"#,
            r#"<svg><a href="https://example.com"></a></svg>"#,
            r#"<svg><animate attributeName="x"/></svg>"#,
            r##"<svg><image href="#local"/></svg>"##,
            r##"<svg><use href="#local"/></svg>"##,
            r#"<svg><style>path{fill:url(https://example.com)}</style></svg>"#,
        ] {
            assert_eq!(
                validate_svg(payload),
                Err(DiagramExportFailure::InvalidPayload)
            );
        }
    }

    #[test]
    fn png_requires_bounded_dimensions_and_complete_iend() {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&13_u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&1_u32.to_be_bytes());
        png.extend_from_slice(&1_u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0; 4]);
        png.extend_from_slice(&0_u32.to_be_bytes());
        png.extend_from_slice(b"IDAT");
        png.extend_from_slice(&[0; 4]);
        png.extend_from_slice(&0_u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0; 4]);
        let payload = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        assert!(validate_png(&payload).is_ok());
        assert_eq!(
            validate_png("data:image/png;base64,AAAA"),
            Err(DiagramExportFailure::InvalidPayload)
        );
    }
}
