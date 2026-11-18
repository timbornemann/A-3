use super::*;
use crate::{
    AuthorizedPatchAction, EditPhaseOutcome, PatchApplyFailure, PatchApplyFuture,
    PatchPreviewFailure, PatchPreviewFuture, WorkspacePatchProgressError,
};
use a3_domain::{PatchAction, PatchOperation, Progress};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

fn target() -> TestResult<RepositoryPath> {
    Ok(RepositoryPath::try_from_bytes(b"server.py".to_vec())?)
}

fn reply(_phase: &PreparedEditPhase, key: &str, value: Value) -> TestResult<Vec<u8>> {
    Ok(serde_json::to_vec(&json!({(key):value}))?)
}

fn empty(fixture: &Fixture) -> TestResult<PublishedIndex> {
    index_with_files(fixture.published.run().snapshot_id(), Vec::new())
}

fn state<'a>(
    fixture: &'a Fixture,
    published: &'a PublishedIndex,
) -> TestResult<EditPhaseState<'a>> {
    Ok(EditPhaseState::new(
        &fixture.project,
        &fixture.goal,
        &fixture.stored,
        &fixture.run,
        published,
    )?)
}

#[test]
fn empty_add_closes_only_the_unavailable_source_branch_and_preserves_the_active_view() -> TestResult
{
    let fixture = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let empty_index = empty(&fixture)?;
    let empty_state = state(&fixture, &empty_index)?;
    let phase = PreparedEditPhase::add(empty_state, target()?)?;
    assert!(phase.anchor().empty_published_index());
    let input: Value = serde_json::from_str(phase.input_json())?;
    assert_eq!(input["operation"], "add");
    assert_eq!(input["inventory"], "emptyPublishedIndex");
    assert_eq!(input["current"], "");
    let compiled = fixture.compile(&phase, empty_state, None)?;
    let existing_state = fixture.state(&fixture.run)?;
    let existing_add = PreparedEditPhase::add(existing_state, target()?)?;
    let existing = fixture.compile(&existing_add, existing_state, None)?;
    assert!(!existing_add.anchor().empty_published_index());
    assert_eq!(
        compiled.request().messages()[1],
        existing.request().messages()[1]
    );
    let active = compiled.request().messages()[1].content();
    assert!(active.contains(&format!("Rule: {LATE}\n")));
    assert!(active.contains("Non-goal: No package changes.\n"));
    assert!(active.contains("Decision: Use the existing public function.\n"));
    let schema = compiled
        .request()
        .structured_output()
        .ok_or("schema missing")?;
    let variants = schema.value()["oneOf"]
        .as_array()
        .ok_or("variants missing")?;
    assert_eq!(variants.len(), 2);
    assert!(variants[0]["properties"].get("code").is_some());
    assert!(variants[1]["properties"].get("tooLarge").is_some());
    assert!(
        variants
            .iter()
            .all(|variant| variant["properties"].get("need").is_none())
    );
    for request in [
        compiled.for_structural_repair()?,
        compiled.for_truncated_output_repair()?,
    ] {
        assert_eq!(request.job_digest(), compiled.job_digest());
        assert_eq!(request.anchor(), compiled.anchor());
        assert_eq!(
            request.request().structured_output(),
            compiled.request().structured_output()
        );
        assert_eq!(
            &request.request().messages()[1..],
            &compiled.request().messages()[1..]
        );
        assert!(!request.request().messages()[0].content().contains("need/"));
        assert!(
            request.request().messages()[0]
                .content()
                .contains("code=complete file")
        );
        assert!(matches!(
            request.for_structural_repair(),
            Err(EditPhaseRequestFailure::RepairAlreadySelected)
        ));
    }
    assert!(
        compiled.request().messages()[0]
            .content()
            .contains("New file")
    );
    assert!(
        compiled.request().messages()[0]
            .content()
            .contains("code=complete file")
    );
    let regular = existing
        .request()
        .structured_output()
        .ok_or("existing schema missing")?;
    assert_eq!(
        regular.value()["oneOf"]
            .as_array()
            .ok_or("existing variants")?
            .len(),
        2
    );
    assert!(
        serde_json::from_str::<Value>(existing_add.input_json())?
            .get("inventory")
            .is_none()
    );
    Ok(())
}

#[test]
fn empty_add_unoffered_need_is_a_nonmutating_semantic_halt() -> TestResult {
    let fixture = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let empty_index = empty(&fixture)?;
    let current = state(&fixture, &empty_index)?;
    let tools = NoEffects::default();
    for need in ["definition", "caller", "test"] {
        let phase = PreparedEditPhase::add(current, target()?)?;
        let bytes = reply(&phase, "need", json!(need))?;
        assert!(matches!(
            futures::executor::block_on(complete(phase, &bytes, current, &tools)),
            Ok(EditPhaseOutcome::NeedEvidence(_))
        ));
    }
    let phase = PreparedEditPhase::add(current, target()?)?;
    let bytes = reply(&phase, "tooLarge", json!(true))?;
    assert!(matches!(
        futures::executor::block_on(complete(phase, &bytes, current, &tools)),
        Ok(EditPhaseOutcome::TooLarge)
    ));
    let phase = PreparedEditPhase::add(current, target()?)?;
    let bytes = reply(&phase, "code", json!("import argparse\n"))?;
    let result = futures::executor::block_on(complete(phase, &bytes, current, &tools))?;
    let EditPhaseOutcome::Proposal(a3_domain::AgentAction::ApplyPatch(action)) = result else {
        return Err("expected a bounded Add proposal".into());
    };
    let [PatchOperation::Add(add)] = action.operations() else {
        return Err("expected exactly one Add".into());
    };
    assert_eq!(add.path(), &target()?);
    assert_eq!(add.content().as_bytes(), b"import argparse\n");
    let phase = PreparedEditPhase::add(current, target()?)?;
    assert!(matches!(
        futures::executor::block_on(complete(
            phase,
            br#"{"version":1,"window":"Wforeign","need":"test"}"#,
            current,
            &tools
        )),
        Err(EditPhaseFailure::InvalidReply)
    ));
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn existing_add_and_update_keep_every_legitimate_need_variant() -> TestResult {
    let fixture = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let current = fixture.state(&fixture.run)?;
    let tools = NoEffects::default();
    for update in [false, true] {
        for (label, expected) in [
            ("definition", EditEvidenceNeed::Definition),
            ("caller", EditEvidenceNeed::Caller),
            ("test", EditEvidenceNeed::Test),
        ] {
            let phase = if update {
                PreparedEditPhase::update(current, &fixture.page)?
            } else {
                PreparedEditPhase::add(current, target()?)?
            };
            let compiled = fixture.compile(&phase, current, None)?;
            assert_eq!(
                compiled
                    .request()
                    .structured_output()
                    .ok_or("schema missing")?
                    .value()["oneOf"]
                    .as_array()
                    .ok_or("variants missing")?
                    .len(),
                2
            );
            let bytes = reply(&phase, "need", json!(label))?;
            assert!(matches!(
                futures::executor::block_on(complete(phase, &bytes, current, &tools)),
                Ok(EditPhaseOutcome::NeedEvidence(actual)) if actual == expected
            ));
        }
    }
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn opposite_inventory_with_the_same_snapshot_cannot_rebind_an_edit_job() -> TestResult {
    let fixture = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let empty_index = empty(&fixture)?;
    assert_eq!(empty_index.run(), fixture.published.run());
    let empty_state = state(&fixture, &empty_index)?;
    let existing_state = fixture.state(&fixture.run)?;
    let first = PreparedEditPhase::add(empty_state, target()?)?;
    let second = PreparedEditPhase::add(existing_state, target()?)?;
    assert_ne!(first.anchor(), second.anchor());
    assert_ne!(first.job_digest(), second.job_digest());
    let first_input: Value = serde_json::from_str(first.input_json())?;
    let second_input: Value = serde_json::from_str(second.input_json())?;
    assert_eq!(first_input["version"], 3);
    assert_eq!(second_input["version"], 3);
    assert_eq!(first_input["inventory"], "emptyPublishedIndex");
    assert!(second_input.get("inventory").is_none());
    assert!(first_input.get("window").is_none());
    assert!(second_input.get("window").is_none());
    let tools = NoEffects::default();
    for (prepared_state, changed_state) in
        [(empty_state, existing_state), (existing_state, empty_state)]
    {
        let phase = PreparedEditPhase::add(prepared_state, target()?)?;
        assert!(matches!(
            fixture.compile(&phase, changed_state, None),
            Err(EditPhaseRequestFailure::Stale)
        ));
        assert!(matches!(
            futures::executor::block_on(phase.revalidate(changed_state, &tools, &tools)),
            Err(EditPhaseFailure::InactiveOrStale)
        ));
        let bytes = reply(&phase, "code", json!("import argparse\n"))?;
        assert!(matches!(
            futures::executor::block_on(complete(phase, &bytes, changed_state, &tools)),
            Err(EditPhaseFailure::InactiveOrStale)
        ));
    }
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[derive(Debug, Default)]
struct NoEffects {
    calls: AtomicUsize,
}

impl WorkspacePatchControl for NoEffects {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), WorkspacePatchProgressError> {
        Ok(())
    }
}

impl WorkspacePatchTool for NoEffects {
    fn preview<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a PublishedIndex,
        _: &'a PatchAction,
        _: &'a dyn WorkspacePatchControl,
    ) -> PatchPreviewFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(Err(PatchPreviewFailure::Denied)))
    }
    fn apply<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a PublishedIndex,
        _: AuthorizedPatchAction,
        _: &'a dyn WorkspacePatchControl,
    ) -> PatchApplyFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(Err(PatchApplyFailure::Denied)))
    }
}

async fn complete(
    phase: PreparedEditPhase,
    bytes: &[u8],
    current: EditPhaseState<'_>,
    tools: &NoEffects,
) -> Result<EditPhaseOutcome, EditPhaseFailure> {
    let reply = phase.bind_reply(bytes)?;
    phase.complete(reply, current, tools, tools).await
}
