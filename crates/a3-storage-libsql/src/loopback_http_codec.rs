//! Bounded canonical storage for one closed loopback HTTP verification recipe.

use a3_domain::{
    AgentRunId, ContentHash, FileRevision, LoopbackExpectedBody, LoopbackFailureExitCode,
    LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpEvidence, LoopbackHttpEvidenceContext,
    LoopbackHttpObservation, LoopbackHttpProcess, LoopbackHttpRecipe, LoopbackMethod,
    LoopbackProbe, LoopbackProcessSettings, LoopbackStart, LoopbackStartupEvidence,
    MAX_LOOPBACK_PROBES, PolicyDecisionId, PolicyResourceId, ProcessOutputLimit, ProcessTimeout,
    RepositoryPath, SnapshotId, TaskStepId, ToolRunId, VerificationRunId, VerificationSpecId,
};
use libsql::{Row, Transaction, Value};
use std::num::NonZeroU16;
#[path = "loopback_mismatch_codec.rs"]
pub(crate) mod mismatch;

pub(crate) const SCHEMA_VERSION: u32 = 56;
pub(crate) const STARTUP_EVIDENCE_SCHEMA_VERSION: u32 = 58;
const V1: u8 = 1;
const STARTUP_COVERAGE_VERSION: u8 = 2;
const RECIPE_DOMAIN_V1: &str = "a3 durable loopback HTTP recipe v1";
const RECIPE_DOMAIN_V2: &str = "a3 durable loopback HTTP recipe v2";
const EVIDENCE_DOMAIN_V1: &str = "a3 durable loopback HTTP evidence v1";
const EVIDENCE_DOMAIN_V2: &str = "a3 durable loopback HTTP evidence v2";
const RECIPE_BOUNDS: std::ops::RangeInclusive<usize> = 52..=133_505;
const EVIDENCE_BOUNDS: std::ops::RangeInclusive<usize> = 321..=131_887;
const MAX_SCRIPT_BYTES: usize = 131_072;
const MAX_PROBE_PATH_BYTES: usize = 256;

pub(crate) const RECIPE_SELECT: &str = ",CASE
    WHEN typeof(loopback_http_recipe)='blob' AND length(loopback_http_recipe) BETWEEN 52 AND 133505
    THEN loopback_http_recipe WHEN loopback_http_recipe IS NOT NULL THEN x'' END";

pub(crate) const EVIDENCE_SELECT: &str = ",CASE
    WHEN typeof(loopback_http_evidence)='blob' AND length(loopback_http_evidence) BETWEEN 321 AND 131887
    THEN loopback_http_evidence WHEN loopback_http_evidence IS NOT NULL THEN x'' END";

#[derive(Debug)]
pub(crate) struct InvalidLoopbackHttp;

pub(crate) async fn supported(tx: &Transaction) -> libsql::Result<bool> {
    crate::migration::read_user_version(tx)
        .await
        .map(|version| version >= SCHEMA_VERSION)
}

pub(crate) fn encode_recipe(recipe: &LoopbackHttpRecipe) -> Vec<u8> {
    let (version, domain, start) = match recipe.default_start_check_index() {
        Some(index) => (STARTUP_COVERAGE_VERSION, RECIPE_DOMAIN_V2, index),
        None => (
            V1,
            RECIPE_DOMAIN_V1,
            match recipe.start() {
                LoopbackStart::Default => 0,
                LoopbackStart::Explicit => 1,
            },
        ),
    };
    let mut payload = vec![version];
    write_script(&mut payload, recipe.script());
    // V2 always starts explicitly. The same byte now selects its default-start check.
    payload.push(start);
    payload.extend_from_slice(&recipe.port().get().to_le_bytes());
    payload.push(recipe.checks().len() as u8);
    for check in recipe.checks() {
        let probe = check.probe();
        payload.push(match probe.method() {
            LoopbackMethod::Get => 0,
            LoopbackMethod::Post => 1,
            LoopbackMethod::Put => 2,
            LoopbackMethod::Delete => 3,
            LoopbackMethod::Patch => 4,
        });
        let path = probe.path().as_bytes();
        payload.extend_from_slice(&(path.len() as u16).to_le_bytes());
        payload.extend_from_slice(path);
        payload.extend_from_slice(&check.status().to_le_bytes());
        payload.push(header_bits(check.headers()));
        match check.body() {
            LoopbackExpectedBody::AnyBounded => payload.push(0),
            LoopbackExpectedBody::Exact { hash, bytes } => {
                payload.push(1);
                payload.extend_from_slice(hash.as_bytes());
                payload.extend_from_slice(&bytes.to_le_bytes());
            }
        }
    }
    seal(payload, domain)
}

pub(crate) fn decode_recipe(bytes: &[u8]) -> Result<LoopbackHttpRecipe, InvalidLoopbackHttp> {
    let version = bytes.first().copied().ok_or(InvalidLoopbackHttp)?;
    let domain = match version {
        V1 => RECIPE_DOMAIN_V1,
        STARTUP_COVERAGE_VERSION => RECIPE_DOMAIN_V2,
        _ => return Err(InvalidLoopbackHttp),
    };
    let mut reader = open(bytes, RECIPE_BOUNDS, version, domain)?;
    let script = reader.script()?;
    let (start, default_check) = match (version, reader.byte()?) {
        (V1, 0) => (LoopbackStart::Default, None),
        (V1, 1) => (LoopbackStart::Explicit, None),
        (STARTUP_COVERAGE_VERSION, index) => (LoopbackStart::Explicit, Some(index)),
        _ => return Err(InvalidLoopbackHttp),
    };
    let port = NonZeroU16::new(reader.u16()?).ok_or(InvalidLoopbackHttp)?;
    let count = reader.count()?;
    let mut checks = Vec::with_capacity(count);
    for _ in 0..count {
        let method = match reader.byte()? {
            0 => LoopbackMethod::Get,
            1 => LoopbackMethod::Post,
            2 => LoopbackMethod::Put,
            3 => LoopbackMethod::Delete,
            4 => LoopbackMethod::Patch,
            _ => return Err(InvalidLoopbackHttp),
        };
        let length = usize::from(reader.u16()?);
        if !(1..=MAX_PROBE_PATH_BYTES).contains(&length) {
            return Err(InvalidLoopbackHttp);
        }
        let path = std::str::from_utf8(reader.bytes(length)?).map_err(|_| InvalidLoopbackHttp)?;
        let probe = LoopbackProbe::new(method, path.to_owned()).map_err(|_| InvalidLoopbackHttp)?;
        let status = reader.u16()?;
        let headers = reader.headers()?;
        let body = match reader.byte()? {
            0 => LoopbackExpectedBody::AnyBounded,
            1 => LoopbackExpectedBody::Exact {
                hash: ContentHash::from_bytes(reader.take()?),
                bytes: reader.u32()?,
            },
            _ => return Err(InvalidLoopbackHttp),
        };
        checks.push(
            LoopbackHttpCheck::new(probe, status, body, headers)
                .map_err(|_| InvalidLoopbackHttp)?,
        );
    }
    reader.finish()?;
    let recipe =
        LoopbackHttpRecipe::new(script, start, port, checks).map_err(|_| InvalidLoopbackHttp)?;
    match default_check {
        Some(index) => recipe
            .with_startup_coverage(index)
            .map_err(|_| InvalidLoopbackHttp),
        None => Ok(recipe),
    }
}

pub(crate) fn read_recipe(
    row: &Row,
    column: i32,
) -> Result<Option<LoopbackHttpRecipe>, InvalidLoopbackHttp> {
    optional_blob(row, column)?
        .map(|bytes| decode_recipe(&bytes))
        .transpose()
}

pub(crate) fn encode_evidence(evidence: &LoopbackHttpEvidence) -> Vec<u8> {
    let context = evidence.context();
    let (version, domain) = if evidence.startup().is_some() {
        (STARTUP_COVERAGE_VERSION, EVIDENCE_DOMAIN_V2)
    } else {
        (V1, EVIDENCE_DOMAIN_V1)
    };
    let mut payload = vec![version];
    payload.extend_from_slice(context.tool_run_id.as_bytes());
    payload.extend_from_slice(context.step_id.as_bytes());
    payload.extend_from_slice(evidence.recipe_id().as_bytes());
    if version == V1 {
        write_script(&mut payload, evidence.script().path());
    }
    payload.extend_from_slice(evidence.script().content_hash().as_bytes());
    payload.extend_from_slice(evidence.process_specification().as_bytes());
    let settings = evidence.process().settings();
    payload.extend_from_slice(&settings.timeout.as_millis().to_le_bytes());
    payload.extend_from_slice(&settings.stdout_limit.get().to_le_bytes());
    payload.extend_from_slice(&settings.stderr_limit.get().to_le_bytes());
    payload.push(u8::from(settings.system_root));
    payload.extend_from_slice(evidence.start_decision().as_bytes());
    payload.push(evidence.observations().len() as u8);
    for observation in evidence.observations() {
        write_observation(&mut payload, *observation);
    }
    if let Some(startup) = evidence.startup() {
        payload.extend_from_slice(startup.reserve_decision().as_bytes());
        payload.extend_from_slice(startup.held_explicit_decision().as_bytes());
        payload.extend_from_slice(startup.default_start_decision().as_bytes());
        payload.extend_from_slice(startup.invalid_port_decision().as_bytes());
        payload.push(startup.held_exit().get());
        payload.push(startup.invalid_exit().get());
        write_observation(&mut payload, startup.default_observation());
    }
    seal(payload, domain)
}

/// Reads only the sealed coordinates needed to recover the immutable V2 script path.
pub(crate) fn read_startup_step(
    row: &Row,
    column: i32,
) -> Result<Option<TaskStepId>, InvalidLoopbackHttp> {
    let Some(bytes) = optional_blob(row, column)? else {
        return Ok(None);
    };
    match bytes.first().copied() {
        Some(V1) => Ok(None),
        Some(STARTUP_COVERAGE_VERSION) => {
            let mut reader = open(
                &bytes,
                EVIDENCE_BOUNDS,
                STARTUP_COVERAGE_VERSION,
                EVIDENCE_DOMAIN_V2,
            )?;
            let _: [u8; 32] = reader.take()?;
            Ok(Some(TaskStepId::from_bytes(reader.take()?)))
        }
        _ => Err(InvalidLoopbackHttp),
    }
}

pub(crate) fn read_evidence(
    row: &Row,
    column: i32,
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    recipe: Option<&LoopbackHttpRecipe>,
) -> Result<Option<LoopbackHttpEvidence>, InvalidLoopbackHttp> {
    optional_blob(row, column)?
        .map(|bytes| {
            decode_evidence(
                &bytes,
                verification_run_id,
                spec_id,
                run_id,
                snapshot_id,
                recipe,
            )
        })
        .transpose()
}

fn decode_evidence(
    bytes: &[u8],
    verification_run_id: VerificationRunId,
    spec_id: VerificationSpecId,
    run_id: AgentRunId,
    snapshot_id: SnapshotId,
    bound_recipe: Option<&LoopbackHttpRecipe>,
) -> Result<LoopbackHttpEvidence, InvalidLoopbackHttp> {
    let version = bytes.first().copied().ok_or(InvalidLoopbackHttp)?;
    let domain = match version {
        V1 => EVIDENCE_DOMAIN_V1,
        STARTUP_COVERAGE_VERSION => EVIDENCE_DOMAIN_V2,
        _ => return Err(InvalidLoopbackHttp),
    };
    let mut reader = open(bytes, EVIDENCE_BOUNDS, version, domain)?;
    let context = LoopbackHttpEvidenceContext {
        verification_run_id,
        spec_id,
        run_id,
        tool_run_id: ToolRunId::from_bytes(reader.take()?),
        step_id: TaskStepId::from_bytes(reader.take()?),
        snapshot_id,
    };
    let recipe = PolicyResourceId::from_bytes(reader.take()?);
    let path = if version == V1 {
        reader.script()?
    } else {
        let bound = bound_recipe.ok_or(InvalidLoopbackHttp)?;
        if bound.default_start_check_index().is_none() || bound.id() != recipe {
            return Err(InvalidLoopbackHttp);
        }
        bound.script().clone()
    };
    let script = FileRevision::new(path, ContentHash::from_bytes(reader.take()?));
    let specification = PolicyResourceId::from_bytes(reader.take()?);
    let settings = LoopbackProcessSettings {
        timeout: ProcessTimeout::from_millis(reader.u64()?).map_err(|_| InvalidLoopbackHttp)?,
        stdout_limit: ProcessOutputLimit::new(reader.u32()?).map_err(|_| InvalidLoopbackHttp)?,
        stderr_limit: ProcessOutputLimit::new(reader.u32()?).map_err(|_| InvalidLoopbackHttp)?,
        system_root: match reader.byte()? {
            0 => false,
            1 => true,
            _ => return Err(InvalidLoopbackHttp),
        },
    };
    let process = LoopbackHttpProcess::new(specification, settings);
    let start = PolicyDecisionId::from_bytes(reader.take()?);
    let count = reader.count()?;
    let mut observations = Vec::with_capacity(count);
    for _ in 0..count {
        observations.push(read_observation(&mut reader)?);
    }
    if version == STARTUP_COVERAGE_VERSION {
        let startup = LoopbackStartupEvidence::new(
            PolicyDecisionId::from_bytes(reader.take()?),
            PolicyDecisionId::from_bytes(reader.take()?),
            PolicyDecisionId::from_bytes(reader.take()?),
            PolicyDecisionId::from_bytes(reader.take()?),
            LoopbackFailureExitCode::new(u32::from(reader.byte()?))
                .map_err(|_| InvalidLoopbackHttp)?,
            LoopbackFailureExitCode::new(u32::from(reader.byte()?))
                .map_err(|_| InvalidLoopbackHttp)?,
            read_observation(&mut reader)?,
        )
        .map_err(|_| InvalidLoopbackHttp)?;
        reader.finish()?;
        return LoopbackHttpEvidence::new_startup(
            context,
            recipe,
            script,
            process,
            start,
            observations,
            startup,
        )
        .map_err(|_| InvalidLoopbackHttp);
    }
    reader.finish()?;
    LoopbackHttpEvidence::new(context, recipe, script, process, start, observations)
        .map_err(|_| InvalidLoopbackHttp)
}

fn write_observation(payload: &mut Vec<u8>, observation: LoopbackHttpObservation) {
    payload.extend_from_slice(observation.decision().as_bytes());
    payload.extend_from_slice(&observation.status().to_le_bytes());
    payload.extend_from_slice(observation.body_hash().as_bytes());
    payload.extend_from_slice(&observation.body_bytes().to_le_bytes());
    payload.push(header_bits(observation.headers()));
}

fn read_observation(
    reader: &mut Reader<'_>,
) -> Result<LoopbackHttpObservation, InvalidLoopbackHttp> {
    LoopbackHttpObservation::new(
        PolicyDecisionId::from_bytes(reader.take()?),
        reader.u16()?,
        ContentHash::from_bytes(reader.take()?),
        reader.u32()?,
        reader.headers()?,
    )
    .map_err(|_| InvalidLoopbackHttp)
}

fn write_script(payload: &mut Vec<u8>, script: &RepositoryPath) {
    let bytes = script.as_bytes();
    payload.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    payload.extend_from_slice(bytes);
}

fn optional_blob(row: &Row, column: i32) -> Result<Option<Vec<u8>>, InvalidLoopbackHttp> {
    match row.get_value(column).map_err(|_| InvalidLoopbackHttp)? {
        Value::Null => Ok(None),
        Value::Blob(bytes) => Ok(Some(bytes)),
        _ => Err(InvalidLoopbackHttp),
    }
}

fn header_bits(headers: LoopbackHeaderChecks) -> u8 {
    u8::from(headers.plain_utf8) | (u8::from(headers.allow_get) << 1)
}

fn seal(mut payload: Vec<u8>, domain: &str) -> Vec<u8> {
    let digest = blake3::derive_key(domain, &payload);
    payload.extend_from_slice(&digest);
    payload
}

fn open<'a>(
    bytes: &'a [u8],
    bounds: std::ops::RangeInclusive<usize>,
    version: u8,
    domain: &str,
) -> Result<Reader<'a>, InvalidLoopbackHttp> {
    if !bounds.contains(&bytes.len()) {
        return Err(InvalidLoopbackHttp);
    }
    let (payload, digest) = bytes.split_at(bytes.len() - 32);
    if blake3::derive_key(domain, payload) != digest {
        return Err(InvalidLoopbackHttp);
    }
    let mut reader = Reader(payload);
    if reader.byte()? != version {
        return Err(InvalidLoopbackHttp);
    }
    Ok(reader)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn bytes(&mut self, length: usize) -> Result<&'a [u8], InvalidLoopbackHttp> {
        let (bytes, remaining) = self.0.split_at_checked(length).ok_or(InvalidLoopbackHttp)?;
        self.0 = remaining;
        Ok(bytes)
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], InvalidLoopbackHttp> {
        self.bytes(N)?.try_into().map_err(|_| InvalidLoopbackHttp)
    }

    fn byte(&mut self) -> Result<u8, InvalidLoopbackHttp> {
        let [value] = self.take()?;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, InvalidLoopbackHttp> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32, InvalidLoopbackHttp> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, InvalidLoopbackHttp> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    fn count(&mut self) -> Result<usize, InvalidLoopbackHttp> {
        let count = usize::from(self.byte()?);
        (1..=MAX_LOOPBACK_PROBES)
            .contains(&count)
            .then_some(count)
            .ok_or(InvalidLoopbackHttp)
    }

    fn script(&mut self) -> Result<RepositoryPath, InvalidLoopbackHttp> {
        let length = self.u32()? as usize;
        if !(3..=MAX_SCRIPT_BYTES).contains(&length) {
            return Err(InvalidLoopbackHttp);
        }
        RepositoryPath::try_from_bytes(self.bytes(length)?.to_vec())
            .map_err(|_| InvalidLoopbackHttp)
    }

    fn headers(&mut self) -> Result<LoopbackHeaderChecks, InvalidLoopbackHttp> {
        let value = self.byte()?;
        if value > 3 {
            return Err(InvalidLoopbackHttp);
        }
        Ok(LoopbackHeaderChecks {
            plain_utf8: value & 1 != 0,
            allow_get: value & 2 != 0,
        })
    }

    fn finish(self) -> Result<(), InvalidLoopbackHttp> {
        self.0.is_empty().then_some(()).ok_or(InvalidLoopbackHttp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn legacy_recipe() -> Result<LoopbackHttpRecipe, Box<dyn Error>> {
        Ok(LoopbackHttpRecipe::new(
            RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
            LoopbackStart::Explicit,
            NonZeroU16::try_from(18_765_u16)?,
            vec![
                LoopbackHttpCheck::new(
                    LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
                    200,
                    LoopbackExpectedBody::Exact {
                        hash: ContentHash::from_bytes(*blake3::hash(b"Ready\n").as_bytes()),
                        bytes: 6,
                    },
                    LoopbackHeaderChecks {
                        plain_utf8: true,
                        allow_get: false,
                    },
                )?,
                LoopbackHttpCheck::new(
                    LoopbackProbe::new(LoopbackMethod::Delete, "/health".to_owned())?,
                    405,
                    LoopbackExpectedBody::AnyBounded,
                    LoopbackHeaderChecks {
                        plain_utf8: false,
                        allow_get: true,
                    },
                )?,
            ],
        )?)
    }

    #[test]
    fn recipe_roundtrips_and_rejects_corruption_or_validly_sealed_trailing_data()
    -> Result<(), Box<dyn Error>> {
        let recipe = legacy_recipe()?;
        let payload = encode_recipe(&recipe);
        let mut legacy =
            b"\x01\x09\0\0\0server.py\x01\x4d\x49\x02\0\x07\0/health\xc8\0\x01\x01".to_vec();
        legacy.extend_from_slice(blake3::hash(b"Ready\n").as_bytes());
        legacy.extend_from_slice(b"\x06\0\0\0\x03\x07\0/health\x95\x01\x02\0");
        assert_eq!(payload, seal(legacy, "a3 durable loopback HTTP recipe v1"));
        assert_eq!(
            decode_recipe(&payload).map_err(|_| "recipe did not decode")?,
            recipe
        );
        let mut changed = payload.clone();
        changed[1] ^= 1;
        assert!(decode_recipe(&changed).is_err());
        let mut trailing = payload[..payload.len() - 32].to_vec();
        trailing.push(0);
        assert!(decode_recipe(&seal(trailing, RECIPE_DOMAIN_V1)).is_err());
        Ok(())
    }

    #[test]
    fn startup_recipe_uses_the_same_bound_and_rejects_invalid_default_indices()
    -> Result<(), Box<dyn Error>> {
        let legacy = legacy_recipe()?;
        let legacy = LoopbackHttpRecipe::new(
            legacy.script().clone(),
            legacy.start(),
            legacy.port(),
            legacy.checks().iter().rev().cloned().collect(),
        )?;
        let legacy_bytes = encode_recipe(&legacy);
        let aggregate = legacy.with_startup_coverage(1)?;
        let encoded = encode_recipe(&aggregate);
        assert_eq!(encoded.len(), legacy_bytes.len());
        assert_eq!(encoded[0], STARTUP_COVERAGE_VERSION);
        assert_eq!(
            decode_recipe(&encoded).map_err(|_| "startup recipe did not decode")?,
            aggregate
        );

        let payload = &encoded[..encoded.len() - 32];
        let default_check_offset = 1 + 4 + aggregate.script().as_bytes().len();
        for index in [0, 2, u8::MAX] {
            let mut invalid = payload.to_vec();
            invalid[default_check_offset] = index;
            assert!(decode_recipe(&seal(invalid, RECIPE_DOMAIN_V2)).is_err());
        }
        let mut trailing = payload.to_vec();
        trailing.push(0);
        assert!(decode_recipe(&seal(trailing, RECIPE_DOMAIN_V2)).is_err());
        assert!(decode_recipe(&seal(payload.to_vec(), RECIPE_DOMAIN_V1)).is_err());
        Ok(())
    }

    fn evidence_fixture(
        recipe: &LoopbackHttpRecipe,
    ) -> Result<LoopbackHttpEvidence, Box<dyn Error>> {
        let context = LoopbackHttpEvidenceContext {
            verification_run_id: VerificationRunId::from_bytes([1; 32]),
            spec_id: VerificationSpecId::from_bytes([2; 32]),
            run_id: AgentRunId::from_bytes([3; 32]),
            tool_run_id: ToolRunId::from_bytes([4; 32]),
            step_id: TaskStepId::from_bytes([5; 32]),
            snapshot_id: SnapshotId::from_bytes([6; 32]),
        };
        let limit = ProcessOutputLimit::new(1024)?;
        let settings = LoopbackProcessSettings {
            timeout: ProcessTimeout::from_millis(8000)?,
            stdout_limit: limit,
            stderr_limit: limit,
            system_root: false,
        };
        let process = LoopbackHttpProcess::from_spec(&recipe.process_spec(
            context.run_id,
            a3_domain::WorktreeId::from_bytes([7; 32]),
            context.step_id,
            settings,
        )?)?;
        let positive = LoopbackHttpObservation::new(
            PolicyDecisionId::from_bytes([22; 32]),
            200,
            ContentHash::from_bytes(*blake3::hash(b"Ready\n").as_bytes()),
            6,
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?;
        let negative = LoopbackHttpObservation::new(
            PolicyDecisionId::from_bytes([23; 32]),
            405,
            ContentHash::from_bytes(*blake3::hash(b"").as_bytes()),
            0,
            LoopbackHeaderChecks {
                plain_utf8: false,
                allow_get: true,
            },
        )?;
        let script = FileRevision::new(recipe.script().clone(), ContentHash::from_bytes([8; 32]));
        let start = PolicyDecisionId::from_bytes([21; 32]);
        if recipe.default_start_check_index().is_some() {
            let default_observation = LoopbackHttpObservation::new(
                PolicyDecisionId::from_bytes([24; 32]),
                positive.status(),
                positive.body_hash(),
                positive.body_bytes(),
                positive.headers(),
            )?;
            let startup = LoopbackStartupEvidence::new(
                PolicyDecisionId::from_bytes([25; 32]),
                PolicyDecisionId::from_bytes([26; 32]),
                PolicyDecisionId::from_bytes([27; 32]),
                PolicyDecisionId::from_bytes([28; 32]),
                LoopbackFailureExitCode::new(1)?,
                LoopbackFailureExitCode::new(2)?,
                default_observation,
            )?;
            Ok(LoopbackHttpEvidence::new_startup(
                context,
                recipe.id(),
                script,
                process,
                start,
                vec![positive, negative],
                startup,
            )?)
        } else {
            Ok(LoopbackHttpEvidence::new(
                context,
                recipe.id(),
                script,
                process,
                start,
                vec![positive, negative],
            )?)
        }
    }

    #[test]
    fn startup_evidence_roundtrips_only_with_the_bound_recipe_and_complete_observations()
    -> Result<(), Box<dyn Error>> {
        let recipe = legacy_recipe()?.with_startup_coverage(0)?;
        let evidence = evidence_fixture(&recipe)?;
        let context = evidence.context();
        let encoded = encode_evidence(&evidence);
        assert_eq!(encoded[0], STARTUP_COVERAGE_VERSION);
        assert!(encoded.len() < 2048);
        let decode = |bytes: &[u8], recipe: Option<&LoopbackHttpRecipe>| {
            decode_evidence(
                bytes,
                context.verification_run_id,
                context.spec_id,
                context.run_id,
                context.snapshot_id,
                recipe,
            )
        };
        assert_eq!(
            decode(&encoded, Some(&recipe)).map_err(|_| "startup receipt did not decode")?,
            evidence
        );
        assert!(decode(&encoded, None).is_err());
        assert!(decode(&encoded, Some(&legacy_recipe()?)).is_err());

        let payload = &encoded[..encoded.len() - 32];
        let default_observation_bytes = 32 + 2 + 32 + 4 + 1;
        let exits = payload.len() - default_observation_bytes - 2;
        for offset in [exits, exits + 1] {
            let mut zero_exit = payload.to_vec();
            zero_exit[offset] = 0;
            assert!(decode(&seal(zero_exit, EVIDENCE_DOMAIN_V2), Some(&recipe)).is_err());
        }
        let mut reused_decision = payload.to_vec();
        let default_decision = payload.len() - default_observation_bytes;
        reused_decision[default_decision..default_decision + 32]
            .copy_from_slice(evidence.start_decision().as_bytes());
        assert!(decode(&seal(reused_decision, EVIDENCE_DOMAIN_V2), Some(&recipe)).is_err());
        let missing = payload[..payload.len() - default_observation_bytes].to_vec();
        assert!(decode(&seal(missing, EVIDENCE_DOMAIN_V2), Some(&recipe)).is_err());
        let mut trailing = payload.to_vec();
        trailing.push(0);
        assert!(decode(&seal(trailing, EVIDENCE_DOMAIN_V2), Some(&recipe)).is_err());
        assert!(decode(&seal(payload.to_vec(), EVIDENCE_DOMAIN_V1), Some(&recipe)).is_err());
        Ok(())
    }

    #[test]
    fn single_start_evidence_keeps_its_v1_codec_without_a_bound_path_lookup()
    -> Result<(), Box<dyn Error>> {
        let evidence = evidence_fixture(&legacy_recipe()?)?;
        let context = evidence.context();
        let bytes = encode_evidence(&evidence);
        assert_eq!(bytes[0], V1);
        assert_eq!(
            decode_evidence(
                &bytes,
                context.verification_run_id,
                context.spec_id,
                context.run_id,
                context.snapshot_id,
                None
            )
            .map_err(|_| "V1 receipt did not decode")?,
            evidence
        );
        Ok(())
    }
}
