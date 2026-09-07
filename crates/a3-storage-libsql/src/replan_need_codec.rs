//! Strict adapter-only V38 navigation metadata; never stores original bytes.
use a3_application::{ReplanEvidenceNeed, ResearchEvidenceNeed};
use a3_domain::{
    AskResearchSourceId, ContentHash, FileRevision, RepositoryPath, ResearchQuestionId,
    ResearchResultSource, SourcePosition, SourceRange,
};
use serde::{Deserialize, Serialize};

const LIMIT: usize = 65_536;

#[derive(Debug)]
pub(super) struct InvalidNeed;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Need {
    version: u8,
    question: u16,
    targets: Vec<String>,
    originals: Vec<Original>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Original {
    source: [u8; 32],
    path: Vec<u8>,
    hash: [u8; 32],
    range: [u32; 6],
}

pub(super) fn encode(need: &ReplanEvidenceNeed) -> Result<String, InvalidNeed> {
    let data = Need {
        version: 1,
        question: need.need().question().get(),
        targets: need.need().targets().to_vec(),
        originals: need
            .originals()
            .iter()
            .map(|s| Original {
                source: *s.source_id.as_bytes(),
                path: s.revision.path().as_bytes().to_vec(),
                hash: *s.revision.content_hash().as_bytes(),
                range: [
                    s.range.start_byte(),
                    s.range.end_byte(),
                    s.range.start_position().row(),
                    s.range.start_position().column(),
                    s.range.end_position().row(),
                    s.range.end_position().column(),
                ],
            })
            .collect(),
    };
    let text = serde_json::to_string(&data).map_err(|_| InvalidNeed)?;
    if text.len() > LIMIT {
        return Err(InvalidNeed);
    }
    Ok(text)
}

pub(super) fn decode(text: &str) -> Result<ReplanEvidenceNeed, InvalidNeed> {
    if text.len() > LIMIT {
        return Err(InvalidNeed);
    }
    let data: Need = serde_json::from_str(text).map_err(|_| InvalidNeed)?;
    if data.version != 1 {
        return Err(InvalidNeed);
    }
    let need = ResearchEvidenceNeed::new(
        ResearchQuestionId::new(data.question).map_err(|_| InvalidNeed)?,
        data.targets,
    )
    .map_err(|_| InvalidNeed)?;
    let originals = data
        .originals
        .into_iter()
        .map(|s| {
            Ok(ResearchResultSource {
                source_id: AskResearchSourceId::from_bytes(s.source),
                revision: FileRevision::new(
                    RepositoryPath::try_from_bytes(s.path).map_err(|_| InvalidNeed)?,
                    ContentHash::from_bytes(s.hash),
                ),
                range: SourceRange::new(
                    usize::try_from(s.range[0]).map_err(|_| InvalidNeed)?,
                    usize::try_from(s.range[1]).map_err(|_| InvalidNeed)?,
                    SourcePosition::new(s.range[2], s.range[3]),
                    SourcePosition::new(s.range[4], s.range[5]),
                )
                .map_err(|_| InvalidNeed)?,
            })
        })
        .collect::<Result<Vec<_>, InvalidNeed>>()?;
    ReplanEvidenceNeed::new(need, originals).map_err(|_| InvalidNeed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn need_codec_is_bounded_strict_and_preserves_only_navigation_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let need = ReplanEvidenceNeed::new(
            ResearchEvidenceNeed::new(ResearchQuestionId::FIRST, vec!["helper_b".to_owned()])?,
            vec![ResearchResultSource {
                source_id: AskResearchSourceId::from_bytes([1; 32]),
                revision: FileRevision::new(
                    RepositoryPath::try_from_bytes(b"a.py".to_vec())?,
                    ContentHash::from_bytes([2; 32]),
                ),
                range: SourceRange::new(
                    0,
                    18,
                    SourcePosition::new(0, 0),
                    SourcePosition::new(1, 0),
                )?,
            }],
        )?;
        let text = encode(&need).map_err(|_| "encode")?;
        assert_eq!(decode(&text).map_err(|_| "decode")?, need);
        assert!(!text.contains("return helper_b()"));
        assert!(!format!("{need:?}").contains("helper_b"));
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let mut variants = Vec::new();
        for (key, replacement) in [
            ("version", serde_json::json!(2)),
            ("question", serde_json::json!(2)),
            ("targets", serde_json::json!([])),
            ("targets", serde_json::json!(["../secret"])),
            ("originals", serde_json::json!([])),
            ("source_text", serde_json::json!("private")),
        ] {
            let mut invalid = value.clone();
            invalid[key] = replacement;
            variants.push(invalid);
        }
        let mut extra_original = value.clone();
        extra_original["originals"][0]["raw"] = serde_json::json!("private");
        variants.push(extra_original);
        let mut duplicated = value.clone();
        duplicated["originals"] = serde_json::json!([value["originals"][0], value["originals"][0]]);
        variants.push(duplicated);
        for invalid in variants {
            assert!(decode(&invalid.to_string()).is_err());
        }
        assert!(decode(&" ".repeat(LIMIT + 1)).is_err());
        Ok(())
    }
}
