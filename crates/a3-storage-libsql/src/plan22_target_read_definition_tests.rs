//! Existing API regression: a real acquired current target must compile as Definition.
use super::*;

#[test]
fn target_read_definition_compiles_after_real_acquisition() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            use a3_application::{AcquiredSourceRead, SourceReadStore};
            use a3_domain::EvidenceRef;
            let mut fixture = Fixture::new()?;
            let root = fixture.root.join("worktree");
            initialize_git(&root)?;
            fs::write(root.join("subject.py"), TARGET)?;
            fixture.project = RepositoryInspector::new().inspect(&root)?;
            let project = &fixture.project;
            let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
            let (goal, step) = path_edit_goal(
                &path,
                b"Change only the selected subject.py implementation.\n",
            )?;
            let profile = profile()?;
            let (store, refresh, compiler, published) = index_fixture(&fixture).await?;
            let snapshot = published.run().snapshot_id();
            let (mut run, stored, _) = start_fixture_step(
                &fixture,
                store.as_ref(),
                &goal,
                &profile,
                snapshot,
                vec![step],
            )
            .await?;
            let symbol = published
                .publication()
                .graph()
                .symbols()
                .iter()
                .find(|symbol| {
                    symbol.revision().path() == &path && symbol.parsed().name().as_str() == "target"
                })
                .ok_or("target missing")?;
            let required = EvidenceRef::new(
                symbol.revision().clone(),
                symbol.parsed().declaration_range(),
            );
            let seed = TaskLensSeedText::try_from_string(
                "Read the selected current implementation".to_owned(),
            )?;
            let lens = CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref())
                .execute(
                    project,
                    TaskLensSeedSet::new(
                        seed.clone(),
                        seed,
                        vec![TaskLensSeed::ExplicitPath(path)],
                    )?,
                    TaskLensTokenBudget::new(4096)?,
                    &ActiveControl,
                )
                .await?;
            let bound = BoundEditSource::for_file_span(project, &published, &lens, &required)?;
            let anchor = EditPhaseState::new(project, &goal, &stored, &run, &published)?.anchor();
            let tool = ToolRunId::from_bytes([192; 32]);
            store
                .begin_source_read(project, anchor, &bound, tool, at(20)?)
                .await?;
            let reader = DeterministicAgentReadTools::new(
                store.as_ref(),
                store.as_ref(),
                store.as_ref(),
                &WorkspaceAgentSourceReader,
            );
            let mut read = reader
                .execute(
                    project,
                    snapshot,
                    tool,
                    &bound.read_action(project, &published)?,
                    AgentReadTimeout::DEFAULT,
                    &ActiveControl,
                )
                .await?;
            let page = bound.take_original(project, &published, tool, &mut read)?;
            let sequence = run.last_event_sequence();
            let recorded = read.record(&mut run, RunEventId::from_bytes([192; 32]), at(21)?)?;
            AppendAgentRead::new(store.as_ref())
                .execute(project, sequence, &run, &recorded)
                .await?;
            let current = EditPhaseState::new(project, &goal, &stored, &run, &published)?;
            let history = store
                .load_source_read_state(project, current.anchor())
                .await?;
            let acquired: &AcquiredSourceRead =
                history.acquired().ok_or("target acquisition missing")?;
            let phase = PreparedEditPhase::update(current, &page)?;
            // Every API/variant here already exists before the fix. The current implementation
            // returns UnsupportedSourceNeed(Definition); the fixed target path must return Ok.
            let result = build_edit_phase_request(
                &goal,
                &stored,
                &phase,
                current,
                profile,
                None,
                EditPhasePrerequisites::none().with_required_source(
                    EditEvidenceNeed::Definition,
                    &bound,
                    Some((&page, acquired)),
                ),
            );
            assert!(
                result.is_ok(),
                "current acquired target Definition must compile through the existing API"
            );
            drop((store, refresh, compiler));
            fixture.cleanup()
        })
    })
}
