use super::{MAX_OFFERS, MAX_PLAN_PHASE_REPLY_BYTES, PlanEvidenceKind, PlanPhaseFailure};
use crate::{StructuredOutputSchema, phase_json::Object};
use serde::{Deserialize, Deserializer};
use serde_json::json;
use std::collections::BTreeSet;

// The caption is repeated in subsequent edit requests beside their mandatory original view.
pub(super) const MAX_OUTCOME_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub(super) enum OfferAlias {
    P1,
    P2,
    P3,
}

impl OfferAlias {
    pub(super) const ALL: [Self; MAX_OFFERS] = [Self::P1, Self::P2, Self::P3];

    pub(super) const fn index(self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 1,
            Self::P3 => 2,
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
        }
    }
}

pub(super) fn reply_schema(
    job: &str,
    offers: usize,
    empty_inventory: bool,
) -> Result<StructuredOutputSchema, PlanPhaseFailure> {
    let aliases = OfferAlias::ALL
        .iter()
        .take(offers)
        .map(|alias| alias.label())
        .collect::<Vec<_>>();
    let alias = json!({"enum":aliases});
    let step = json!({
        "type":"object","additionalProperties":false,"required":["offer","outcome"],
        "properties":{
            "offer":alias,
            "outcome":{
                "type":"string","minLength":1,"maxLength":MAX_OUTCOME_BYTES,
                // Keep the generated JSON string free of escape sequences. The runtime
                // independently checks the decoded caption and its UTF-8 byte bound.
                "pattern":format!(r#"^[^"\\\x00-\x1F\x7F-\x9F]{{1,{MAX_OUTCOME_BYTES}}}$"#),
            },
            "after":{"type":"array","maxItems":offers.saturating_sub(1),"uniqueItems":true,"items":alias},
        },
    });
    let variant = |key: &str, payload: serde_json::Value| {
        json!({
            "type":"object","additionalProperties":false,"required":["version","job",key],
            "properties":{"version":{"const":1},"job":{"const":job},(key):payload},
        })
    };
    let mut variants = vec![variant(
        "steps",
        json!({"type":"array","minItems":1,"maxItems":offers,"items":step}),
    )];
    if !empty_inventory {
        variants.push(variant(
            "need",
            json!({
                "type":"object","additionalProperties":false,"required":["offer","kind"],
                "properties":{"offer":alias,"kind":{"enum":["source","callers","verification"]}},
            }),
        ));
    }
    variants.push(variant("tooLarge", json!({"const":true})));
    StructuredOutputSchema::new(json!({"oneOf":variants})).map_err(|_| PlanPhaseFailure::Capacity)
}

pub(super) enum Reply {
    Steps(Vec<Step>),
    Need(Need),
    TooLarge,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    job: String,
    #[serde(default, deserialize_with = "present")]
    steps: Option<Vec<Object<Step>>>,
    #[serde(default, deserialize_with = "present")]
    need: Option<Object<Need>>,
    #[serde(rename = "tooLarge", default, deserialize_with = "present")]
    too_large: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    pub(super) offer: OfferAlias,
    pub(super) outcome: String,
    #[serde(default)]
    pub(super) after: Vec<OfferAlias>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Need {
    pub(super) offer: OfferAlias,
    pub(super) kind: PlanEvidenceKind,
}

pub(super) fn decode(bytes: &[u8], job: &str) -> Result<Reply, PlanPhaseFailure> {
    if bytes.len() > MAX_PLAN_PHASE_REPLY_BYTES {
        return Err(PlanPhaseFailure::Capacity);
    }
    let Object(reply): Object<Envelope> =
        serde_json::from_slice(bytes).map_err(|_| PlanPhaseFailure::InvalidReply)?;
    if reply.version != 1 || reply.job != job {
        return Err(PlanPhaseFailure::InvalidReply);
    }
    match (reply.steps, reply.need, reply.too_large) {
        (Some(steps), None, None) if !steps.is_empty() && steps.len() <= MAX_OFFERS => {
            let steps = steps
                .into_iter()
                .map(|Object(step)| step)
                .collect::<Vec<_>>();
            if steps.iter().any(|step| {
                step.after.len() > MAX_OFFERS - 1
                    || step.after.iter().collect::<BTreeSet<_>>().len() != step.after.len()
            }) {
                return Err(PlanPhaseFailure::InvalidDependency);
            }
            Ok(Reply::Steps(steps))
        }
        (None, Some(Object(need)), None) => Ok(Reply::Need(need)),
        (None, None, Some(true)) => Ok(Reply::TooLarge),
        _ => Err(PlanPhaseFailure::InvalidReply),
    }
}

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

pub(super) fn addition_reply_schema(job: &str) -> Result<StructuredOutputSchema, PlanPhaseFailure> {
    // A shared envelope keeps all decisions available after any fixed property prefix.
    // Separate complete-object alternatives can commit a grammar to an outcome at its key.
    StructuredOutputSchema::new(json!({
        "type":"object","additionalProperties":false,
        "required":["decision","job","version"],
        "properties":{
            "decision":{"enum":["add","refuse","clarify","tooLarge"]},
            "job":{"const":job},"version":{"const":3},
        },
    }))
    .map_err(|_| PlanPhaseFailure::Capacity)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdditionEnvelope {
    version: u8,
    job: String,
    decision: AdditionReply,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum AdditionReply {
    Add,
    Refuse,
    Clarify,
    TooLarge,
}

pub(super) fn decode_addition(bytes: &[u8], job: &str) -> Result<AdditionReply, PlanPhaseFailure> {
    if bytes.len() > MAX_PLAN_PHASE_REPLY_BYTES {
        return Err(PlanPhaseFailure::Capacity);
    }
    let Object(reply): Object<AdditionEnvelope> =
        serde_json::from_slice(bytes).map_err(|_| PlanPhaseFailure::InvalidReply)?;
    if reply.version != 3 || reply.job != job {
        return Err(PlanPhaseFailure::InvalidReply);
    }
    Ok(reply.decision)
}
