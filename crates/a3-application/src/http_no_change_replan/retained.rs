//! V6 content-free retention, deliberately separate from the two-field model wire.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Retained {
    version: u8,
    job: String,
    first: Option<u32>,
    last: Option<u32>,
    intent: Option<String>,
    range: Option<[u32; 6]>,
    #[serde(rename = "textDigest")]
    text_digest: Option<String>,
    halt: Option<HaltWire>,
    #[serde(rename = "noProgress")]
    no_progress: Option<bool>,
}
impl HttpNoChangeReplanDecision {
    /// Canonical Core-owned V6 metadata, without raw source or executable authority.
    pub fn canonical_retained_bytes(
        &self,
        job: ContentHash,
    ) -> Result<Vec<u8>, HttpNoChangeReplanFailure> {
        let value = match self {
            Self::Window {
                first,
                last,
                original,
                text_digest,
                intent,
            } => {
                let r = original.range();
                json!({"version":6,"job":hex(job),"first":first.get(),"last":last.get(),"intent":intent.as_str(),
                    "range":[r.start_byte(),r.end_byte(),r.start_position().row(),r.start_position().column(),r.end_position().row(),r.end_position().column()],
                    "textDigest":hex(*text_digest)})
            }
            Self::NoProgress {
                first,
                last,
                intent,
            } => {
                json!({"version":6,"job":hex(job),"first":first.get(),"last":last.get(),"intent":intent.as_str(),"noProgress":true})
            }
            Self::Halt(halt) => json!({"version":6,"job":hex(job),"halt":match halt {
                HttpNoChangeReplanHalt::NeedEvidence=>"needEvidence", HttpNoChangeReplanHalt::TooLarge=>"tooLarge",HttpNoChangeReplanHalt::Refuse=>"refuse"
            }}),
        };
        let bytes =
            serde_json::to_vec(&value).map_err(|_| HttpNoChangeReplanFailure::InvalidReply)?;
        if bytes.len() > MAX_REPLY_BYTES {
            return Err(HttpNoChangeReplanFailure::Capacity);
        }
        Ok(bytes)
    }
    /// Shape-only retained reconstruction. Store proves rows; Prepared proves current bytes.
    pub fn decode_retained(
        job: ContentHash,
        anchor: &HttpNoChangeReplanAnchor,
        bytes: &[u8],
    ) -> Result<Self, HttpNoChangeReplanFailure> {
        use HttpNoChangeReplanFailure as E;
        if bytes.len() > MAX_REPLY_BYTES {
            return Err(E::InvalidReply);
        }
        let crate::phase_json::Object(raw): crate::phase_json::Object<Retained> =
            serde_json::from_slice(bytes).map_err(|_| E::InvalidReply)?;
        if raw.version != 6 || raw.job != hex(job) {
            return Err(E::InvalidReply);
        }
        let value = if let Some(halt) = raw.halt {
            if raw.first.is_some()
                || raw.last.is_some()
                || raw.intent.is_some()
                || raw.range.is_some()
                || raw.text_digest.is_some()
                || raw.no_progress.is_some()
            {
                return Err(E::InvalidReply);
            }
            Self::Halt(halt.into())
        } else {
            let first = AgentFileStartLine::new(raw.first.ok_or(E::InvalidReply)?)
                .map_err(|_| E::InvalidReply)?;
            let last = AgentFileStartLine::new(raw.last.ok_or(E::InvalidReply)?)
                .map_err(|_| E::InvalidReply)?;
            let intent = checked_intent(raw.intent.ok_or(E::InvalidReply)?)?;
            let old = anchor.window.range();
            let old_first = old
                .start_position()
                .row()
                .checked_add(1)
                .ok_or(E::Binding)?;
            let old_last = old
                .end_position()
                .row()
                .checked_add(u32::from(old.end_position().column() > 0))
                .ok_or(E::Binding)?;
            if first > last || first.get() < old_first || last.get() > old_last {
                return Err(E::InvalidReply);
            }
            if raw.no_progress == Some(true) {
                if first.get() != old_first
                    || last.get() != old_last
                    || raw.range.is_some()
                    || raw.text_digest.is_some()
                {
                    return Err(E::InvalidReply);
                }
                Self::NoProgress {
                    first,
                    last,
                    intent,
                }
            } else {
                if raw.no_progress.is_some() {
                    return Err(E::InvalidReply);
                }
                let [start, end, sr, sc, er, ec] = raw.range.ok_or(E::InvalidReply)?;
                let range = SourceRange::new(
                    usize::try_from(start).map_err(|_| E::InvalidReply)?,
                    usize::try_from(end).map_err(|_| E::InvalidReply)?,
                    SourcePosition::new(sr, sc),
                    SourcePosition::new(er, ec),
                )
                .map_err(|_| E::InvalidReply)?;
                let range_last = er.checked_add(u32::from(ec > 0)).ok_or(E::InvalidReply)?;
                if range.is_empty()
                    || !old.contains(range)
                    || range.len() >= old.len()
                    || sc != 0
                    || sr.checked_add(1) != Some(first.get())
                    || range_last != last.get()
                    || (ec > 0
                        && (range.end_byte() != old.end_byte()
                            || range.end_position() != old.end_position()))
                {
                    return Err(E::InvalidReply);
                }
                let offered = line_offers(
                    AgentFileStartLine::new(old_first).map_err(|_| E::InvalidReply)?,
                    AgentFileStartLine::new(old_last).map_err(|_| E::InvalidReply)?,
                )
                .map_err(|_| E::InvalidReply)?;
                if !offered.contains(&WindowOffer { first, last }) {
                    return Err(E::InvalidReply);
                }
                Self::Window {
                    first,
                    last,
                    original: EvidenceRef::new(anchor.window.revision().clone(), range),
                    text_digest: parse_hash(&raw.text_digest.ok_or(E::InvalidReply)?)?,
                    intent,
                }
            }
        };
        if value.canonical_retained_bytes(job)? != bytes {
            return Err(E::InvalidReply);
        }
        Ok(value)
    }
}
pub(super) fn checked_intent(
    text: String,
) -> Result<HttpCorrectionPlanIntent, HttpNoChangeReplanFailure> {
    HttpCorrectionPlanIntent::try_from_string(text).map_err(|e| match e {
        crate::HttpCorrectionPlanFailure::UnsafeContent => HttpNoChangeReplanFailure::UnsafeContent,
        _ => HttpNoChangeReplanFailure::InvalidReply,
    })
}
fn hex(hash: ContentHash) -> String {
    blake3::Hash::from_bytes(*hash.as_bytes())
        .to_hex()
        .to_string()
}
fn parse_hash(text: &str) -> Result<ContentHash, HttpNoChangeReplanFailure> {
    use HttpNoChangeReplanFailure as E;
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(E::InvalidReply);
    }
    let mut result = [0u8; 32];
    for (i, slot) in result.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(i * 2..i * 2 + 2).ok_or(E::InvalidReply)?, 16)
            .map_err(|_| E::InvalidReply)?;
    }
    Ok(ContentHash::from_bytes(result))
}
