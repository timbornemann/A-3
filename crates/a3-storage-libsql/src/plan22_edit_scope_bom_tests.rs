//! The real SourceReader/adapter can qualify a first callable while BOM delivery stays strict.
use super::*;

#[test]
fn edit_scope_frame_bom_keeps_actual_edit_span_and_never_relaxes_source_delivery() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let text = format!("\u{feff}{SINGLE}\ndef other(value):\n    return value + 1\n");
            let end = 3 + SINGLE.len();
            let span =
                SourceRange::new(0, end, SourcePosition::new(0, 0), SourcePosition::new(2, 0))?;
            let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
            let (goal, step) = path_edit_goal(
                &path,
                b"Edit the first current function and preserve all other bytes.\n",
            )?;
            let f = TargetFixture::start_using_span(
                &text,
                target_profile()?,
                goal,
                step,
                Fixture::new()?,
                Some(span),
            )
            .await?;
            assert_eq!(f.page.range(), span);
            assert!(f.page.text().starts_with('\u{feff}'));
            let phase = PreparedEditPhase::update(f.state()?, &f.page)?;
            let mut compiled = build_edit_phase_request(
                &f.goal,
                &f.stored,
                &phase,
                f.state()?,
                f.profile.clone(),
                None,
                EditPhasePrerequisites::none(),
            )?;
            let input: Value = serde_json::from_str(phase.input_json())?;
            assert_eq!(input["current"].as_str(), Some(SINGLE));
            let window = phase.no_change_window().ok_or("replace window")?;
            assert_eq!(window.start_byte(), 3);
            assert_eq!(window.end_byte(), u32::try_from(end)?);
            let before = compiled.request_digest();
            let patch = WorkspacePatchAdapter::with_change_window_preview();
            let phase = Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
                &f.compiler,
                phase,
                &mut compiled,
                f.state()?,
                at(22)?,
                &patch,
                &ActiveControl,
            ))
            .await?;
            assert!(compiled.scope_frame_digest().is_some());
            assert_eq!(compiled.scope_frame_index(), Some(f.published.run().id()));
            assert_ne!(compiled.request_digest(), before);
            phase.require_scope_binding(&compiled)?;
            // Core qualification does not pretend the BOM was delivered in the model window.
            assert!(matches!(
                build_edit_phase_request(
                    &f.goal,
                    &f.stored,
                    &phase,
                    f.state()?,
                    f.profile.clone(),
                    None,
                    EditPhasePrerequisites::none().with_required_source(
                        EditEvidenceNeed::Definition,
                        &f.bound,
                        Some((&f.page, &f.acquired))
                    )
                ),
                Err(EditPhaseRequestFailure::InvalidTargetSource)
            ));
            assert!(matches!(
                phase.bind_reply(br#"{"code":"pass"}"#),
                Err(EditPhaseFailure::WrongReplyBinding)
            ));
            assert_eq!(
                f.store
                    .load_edit_phase_usage(&f.fixture.project, f.run.id())
                    .await?
                    .calls,
                0
            );
            assert_eq!(
                fs::read(f.fixture.root.join("worktree/subject.py"))?,
                text.as_bytes()
            );
            drop((phase, compiled));
            f.cleanup()
        })
    })
}
