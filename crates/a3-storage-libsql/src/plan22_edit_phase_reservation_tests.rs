use super::*;
#[path = "plan22_edit_need_boundary_tests.rs"]
mod edit_need_boundary;

#[path = "plan22_edit_planned_addition_tests.rs"]
mod planned_addition;
use a3_application::{
    AdvanceAgentController,
    EditPhaseCallKind::{Initial, StructuralRepair},
    EditPhaseObservation, EditPhaseObservationOutcome as Outcome,
    EditPhaseReservationFailure as Failure, EditPhaseReservationStore, EditPhaseReservedUsage,
    EditPhaseWireCost, KnowledgeIndexStore,
};
use a3_domain::{
    AgentBudgetDimension, FileRevision, GoalRevisionReason, IndexPublication, LinkedGraph,
    ModulePolicyVersion, ModuleProjection, ModuleSymbolSet, PublishedIndex, RankProjection,
    RepositoryCard,
};

struct ActiveEdit {
    fixture: Fixture,
    goal: GoalContract,
    path: RepositoryPath,
    run: AgentRun,
    ledger: StoredTaskLedger,
    published: PublishedIndex,
    store: Arc<LibsqlKnowledgeStore>,
}

impl ActiveEdit {
    async fn start() -> TestResult<Self> {
        Self::start_with_notes(true).await
    }

    async fn start_with_notes(notes_present: bool) -> TestResult<Self> {
        Self::start_with_scope(notes_present, false).await
    }

    async fn start_with_scope(notes_present: bool, planned: bool) -> TestResult<Self> {
        Self::start_with_model(notes_present, planned, &profile()?).await
    }

    async fn start_with_model(
        notes_present: bool,
        planned: bool,
        model: &ModelProfile,
    ) -> TestResult<Self> {
        let mut fixture = Fixture::new()?;
        let worktree = fixture.root.join("worktree");
        initialize_git(&worktree)?;
        if notes_present {
            fs::write(worktree.join("notes.txt"), OTHER_FILE)?;
        }
        fixture.project = RepositoryInspector::new().inspect(&worktree)?;
        let path = RepositoryPath::try_from_bytes(SUBJECT_PATH.as_bytes().to_vec())?;
        let (goal, step) = path_edit_goal(&path, b"Create only subject.py.\n")?;
        let step = if planned {
            step.with_planned_file_addition(a3_domain::PlannedFileAddition::new(path.clone()))?
        } else {
            step
        };
        let (store, _, _, published) = index_fixture(&fixture).await?;
        let (run, ledger, _) = start_fixture_step(
            &fixture,
            &store,
            &goal,
            model,
            published.run().snapshot_id(),
            vec![step],
        )
        .await?;
        Ok(Self {
            fixture,
            goal,
            path,
            run,
            ledger,
            published,
            store,
        })
    }

    fn phase(&self, path: &RepositoryPath) -> TestResult<PreparedEditPhase> {
        let state = EditPhaseState::new(
            &self.fixture.project,
            &self.goal,
            &self.ledger,
            &self.run,
            &self.published,
        )?;
        Ok(PreparedEditPhase::add(state, path.clone())?)
    }

    async fn reopen(self) -> TestResult<Self> {
        let Self {
            fixture,
            goal,
            path,
            run,
            store,
            ..
        } = self;
        let run_id = run.id();
        drop(store);
        let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
        let project = &fixture.project;
        let run = store
            .load_agent_run(project, run_id)
            .await?
            .ok_or("run missing")?;
        let ledger = store
            .load_task_ledger(project, goal.reference().task_id())
            .await?
            .ok_or("ledger missing")?;
        let published = store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("index missing")?;
        Ok(Self {
            fixture,
            goal,
            path,
            run,
            ledger,
            published,
            store,
        })
    }

    async fn usage(&self) -> TestResult<EditPhaseReservedUsage> {
        Ok(self
            .store
            .load_edit_phase_usage(&self.fixture.project, self.run.id())
            .await?)
    }

    async fn assert_run_and_ledger_unchanged(&self) -> TestResult {
        let run = self
            .store
            .load_agent_run(&self.fixture.project, self.run.id())
            .await?
            .ok_or("run missing")?;
        assert_eq!(run, self.run);
        let ledger = self
            .store
            .load_task_ledger(&self.fixture.project, self.goal.reference().task_id())
            .await?
            .ok_or("ledger missing")?;
        assert_eq!(ledger.version(), self.ledger.version());
        assert_eq!(ledger.ledger(), self.ledger.ledger());
        Ok(())
    }

    fn cleanup(self) -> TestResult {
        drop(self.store);
        self.fixture.cleanup()
    }
}

fn wire(byte: u8) -> TestResult<EditPhaseWireCost> {
    Ok(EditPhaseWireCost::new(
        ContentHash::from_bytes([byte; 32]),
        1200,
        400,
        200,
    )?)
}

fn observation(outcome: Outcome, time: u64) -> TestResult<EditPhaseObservation> {
    Ok(EditPhaseObservation::new(
        outcome,
        Some(377),
        Some(41),
        at(time)?,
    ))
}

fn forged_inventory(published: &PublishedIndex, empty: bool) -> TestResult<PublishedIndex> {
    let snapshot = published.run().snapshot_id();
    let files = if empty {
        Vec::new()
    } else {
        vec![FileRevision::new(
            RepositoryPath::try_from_bytes(b"notes.txt".to_vec())?,
            ContentHash::from_bytes(*blake3::hash(OTHER_FILE).as_bytes()),
        )]
    };
    let count = u32::try_from(files.len())?;
    let graph = LinkedGraph::new(snapshot, files, Vec::new(), Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(
        snapshot,
        published.run().ranking_policy_version(),
        Vec::new(),
    )?;
    let policy = ModulePolicyVersion::v1();
    let card = RepositoryCard::new(
        snapshot,
        policy,
        Vec::new(),
        Vec::new(),
        ModuleSymbolSet::empty(),
        count,
        0,
    )?;
    let modules = ModuleProjection::new(snapshot, policy, Vec::new(), Vec::new(), card)?;
    Ok(PublishedIndex::new(
        published.run(),
        IndexPublication::new(graph, ranking, Vec::new(), modules)?,
    )?)
}

#[test]
fn same_snapshot_forged_empty_inventory_cannot_reserve_or_leave_an_unknown_slot() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for notes_present in [true, false] {
                let active = ActiveEdit::start_with_notes(notes_present).await?;
                let correct = active.phase(&active.path)?;
                assert_eq!(correct.anchor().empty_published_index(), !notes_present);
                assert_eq!(
                    active.published.publication().graph().files().len(),
                    usize::from(notes_present)
                );
                let forged = forged_inventory(&active.published, notes_present)?;
                assert_eq!(forged.run(), active.published.run());
                let state = EditPhaseState::new(
                    &active.fixture.project,
                    &active.goal,
                    &active.ledger,
                    &active.run,
                    &forged,
                )?;
                let phase = PreparedEditPhase::add(state, active.path.clone())?;
                assert_eq!(phase.anchor().run(), correct.anchor().run());
                assert_eq!(phase.anchor().snapshot(), correct.anchor().snapshot());
                assert_eq!(phase.anchor().sequence(), correct.anchor().sequence());
                assert_eq!(phase.anchor().ledger(), correct.anchor().ledger());
                assert_eq!(phase.anchor().version(), correct.anchor().version());
                assert_eq!(phase.anchor().step(), correct.anchor().step());
                assert_eq!(phase.anchor().attempt(), correct.anchor().attempt());
                assert_eq!(
                    phase.anchor().verification(),
                    correct.anchor().verification()
                );
                assert_ne!(
                    phase.anchor().empty_published_index(),
                    correct.anchor().empty_published_index()
                );
                assert_ne!(phase.job_digest(), correct.job_digest());
                for kind in [Initial, StructuralRepair] {
                    assert!(matches!(
                        active
                            .store
                            .reserve_edit_phase(
                                &active.fixture.project,
                                &phase,
                                kind,
                                wire(1)?,
                                at(8)?,
                            )
                            .await,
                        Err(Failure::StaleAnchor)
                    ));
                }
                assert_eq!(active.usage().await?, EditPhaseReservedUsage::default());
                active.assert_run_and_ledger_unchanged().await?;
                let active = active.reopen().await?;
                assert_eq!(active.usage().await?, EditPhaseReservedUsage::default());
                let phase = active.phase(&active.path)?;
                assert_eq!(phase.job_digest(), correct.job_digest());
                let permit = active
                    .store
                    .reserve_edit_phase(&active.fixture.project, &phase, Initial, wire(1)?, at(8)?)
                    .await?;
                active
                    .store
                    .finish_edit_phase(
                        &active.fixture.project,
                        permit,
                        observation(Outcome::AcceptedReply, 9)?,
                    )
                    .await?;
                let usage = active.usage().await?;
                assert_eq!(
                    (
                        usage.calls,
                        usage.repairs,
                        usage.unobserved_calls,
                        usage.unknown_calls
                    ),
                    (1, 0, 0, 0)
                );
                active.assert_run_and_ledger_unchanged().await?;
                active.cleanup()?;
            }
            Ok(())
        })
    })
}

#[test]
fn reservation_debits_before_io_and_allows_only_one_repair_across_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            let project = &active.fixture.project;
            let phase = active.phase(&active.path)?;
            let job = phase.job_digest();
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, StructuralRepair, wire(1)?, at(8)?)
                    .await,
                Err(Failure::RepairNotAvailable)
            ));
            let first = active
                .store
                .reserve_edit_phase(project, &phase, Initial, wire(1)?, at(8)?)
                .await?;
            let deadline = first.deadline();
            assert_eq!(first.anchor(), phase.anchor());
            assert_eq!(first.job_digest(), job);
            assert_eq!(
                active.usage().await?,
                EditPhaseReservedUsage {
                    calls: 1,
                    repairs: 0,
                    prompt_tokens: 400,
                    output_tokens: 200,
                    unobserved_calls: 1,
                    unknown_calls: 0,
                }
            );
            active
                .store
                .finish_edit_phase(project, first, observation(Outcome::InvalidStructure, 9)?)
                .await?;
            active.assert_run_and_ledger_unchanged().await?;

            let active = active.reopen().await?;
            let project = &active.fixture.project;
            let phase = active.phase(&active.path)?;
            assert_eq!(phase.job_digest(), job);
            let alternative =
                active.phase(&RepositoryPath::try_from_bytes(b"alternate.py".to_vec())?)?;
            assert_ne!(alternative.job_digest(), job);
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &alternative, Initial, wire(2)?, at(10)?)
                    .await,
                Err(Failure::StructuralFailurePending)
            ));
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, wire(2)?, at(10)?)
                    .await,
                Err(Failure::AlreadyReserved)
            ));
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, StructuralRepair, wire(1)?, at(10)?)
                    .await,
                Err(Failure::UnchangedRepairWire)
            ));
            let repair = active
                .store
                .reserve_edit_phase(project, &phase, StructuralRepair, wire(2)?, at(10)?)
                .await?;
            assert_eq!(repair.deadline(), deadline);
            active
                .store
                .finish_edit_phase(project, repair, observation(Outcome::AcceptedReply, 11)?)
                .await?;

            let active = active.reopen().await?;
            let phase = active.phase(&active.path)?;
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(
                        &active.fixture.project,
                        &phase,
                        StructuralRepair,
                        wire(3)?,
                        at(12)?
                    )
                    .await,
                Err(Failure::AlreadyReserved)
            ));
            assert_eq!(
                active.usage().await?,
                EditPhaseReservedUsage {
                    calls: 2,
                    repairs: 1,
                    prompt_tokens: 800,
                    output_tokens: 400,
                    unobserved_calls: 0,
                    unknown_calls: 0,
                }
            );
            active.assert_run_and_ledger_unchanged().await?;
            active.cleanup()
        })
    })
}

#[test]
fn committed_but_unobserved_call_blocks_redispatch_after_cold_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            {
                let phase = active.phase(&active.path)?;
                let _lost = active
                    .store
                    .reserve_edit_phase(&active.fixture.project, &phase, Initial, wire(1)?, at(8)?)
                    .await?;
            }
            let active = active.reopen().await?;
            for path in [
                &active.path,
                &RepositoryPath::try_from_bytes(b"new.py".to_vec())?,
            ] {
                let phase = active.phase(path)?;
                assert!(matches!(
                    active
                        .store
                        .reserve_edit_phase(
                            &active.fixture.project,
                            &phase,
                            Initial,
                            wire(2)?,
                            at(9)?
                        )
                        .await,
                    Err(Failure::UncertainProviderCall)
                ));
            }
            assert_eq!(
                active.usage().await?,
                EditPhaseReservedUsage {
                    calls: 1,
                    repairs: 0,
                    prompt_tokens: 400,
                    output_tokens: 200,
                    unobserved_calls: 1,
                    unknown_calls: 0,
                }
            );
            active.assert_run_and_ledger_unchanged().await?;
            active.cleanup()
        })
    })
}

#[test]
fn ledger_cas_change_leaves_history_and_makes_reply_unusable() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            let project = &active.fixture.project;
            let phase = active.phase(&active.path)?;
            let reservation = active
                .store
                .reserve_edit_phase(project, &phase, Initial, wire(1)?, at(8)?)
                .await?;
            // Replacing the real ledger exercises the delete/rewrite of the exact active attempt.
            let new_version = active
                .store
                .replace_task_ledger(project, active.ledger.version(), active.ledger.ledger())
                .await?;
            assert_ne!(new_version, active.ledger.version());
            assert_eq!(
                active
                    .store
                    .finish_edit_phase(
                        project,
                        reservation,
                        observation(Outcome::InvalidStructure, 9)?,
                    )
                    .await,
                Err(Failure::StaleAnchor)
            );
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, wire(2)?, at(10)?)
                    .await,
                Err(Failure::StaleAnchor)
            ));
            let active = active.reopen().await?;
            assert_eq!(active.ledger.version(), new_version);
            let fresh = active.phase(&active.path)?;
            assert_ne!(phase.job_digest(), fresh.job_digest());
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(
                        &active.fixture.project,
                        &fresh,
                        StructuralRepair,
                        wire(2)?,
                        at(10)?
                    )
                    .await,
                Err(Failure::RepairNotAvailable)
            ));
            assert_eq!(
                active.usage().await?,
                EditPhaseReservedUsage {
                    calls: 1,
                    repairs: 0,
                    prompt_tokens: 400,
                    output_tokens: 200,
                    unobserved_calls: 0,
                    unknown_calls: 0,
                }
            );
            active.cleanup()
        })
    })
}

#[test]
fn changed_published_snapshot_rejects_reserve_and_usable_finish() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            let project = &active.fixture.project;
            let phase = active.phase(&active.path)?;
            let reservation = active
                .store
                .reserve_edit_phase(project, &phase, Initial, wire(1)?, at(8)?)
                .await?;
            fs::write(
                active.fixture.root.join("worktree/notes.txt"),
                b"external change\n",
            )?;
            let (_, _, _, updated) = index_fixture(&active.fixture).await?;
            assert_ne!(
                updated.run().snapshot_id(),
                active.run.current_snapshot_id()
            );
            assert_eq!(
                active
                    .store
                    .finish_edit_phase(
                        project,
                        reservation,
                        observation(Outcome::AcceptedReply, 9)?
                    )
                    .await,
                Err(Failure::StaleAnchor)
            );
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, wire(2)?, at(10)?)
                    .await,
                Err(Failure::StaleAnchor)
            ));
            assert_eq!(active.usage().await?.unobserved_calls, 0);
            assert_eq!(active.usage().await?.calls, 1);
            active.assert_run_and_ledger_unchanged().await?;
            active.cleanup()
        })
    })
}

#[test]
fn terminal_run_records_unknown_without_reopening_or_refunding() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut active = ActiveEdit::start().await?;
            let project = &active.fixture.project;
            let phase = active.phase(&active.path)?;
            let reservation = active
                .store
                .reserve_edit_phase(project, &phase, Initial, wire(1)?, at(8)?)
                .await?;
            let sequence = active.run.last_event_sequence();
            let snapshot = active.run.current_snapshot_id();
            let cancelled = AdvanceAgentController.execute(
                &mut active.run,
                AgentControllerSignal::CancelRequested,
                RunEventId::from_bytes([191; 32]),
                snapshot,
                at(9)?,
                false,
            )?;
            AppendRunEvent::new(active.store.as_ref())
                .execute(project, sequence, &active.run, cancelled.event())
                .await?;
            active
                .store
                .finish_edit_phase(project, reservation, observation(Outcome::Unknown, 10)?)
                .await?;
            let active = active.reopen().await?;
            assert_eq!(active.run.state(), AgentControllerState::Cancelled);
            assert_eq!(active.run.usage(), a3_domain::AgentRunUsage::ZERO);
            assert_eq!(
                active.usage().await?,
                EditPhaseReservedUsage {
                    calls: 1,
                    repairs: 0,
                    prompt_tokens: 400,
                    output_tokens: 200,
                    unobserved_calls: 0,
                    unknown_calls: 1,
                }
            );
            active.cleanup()
        })
    })
}

#[test]
fn exact_budget_is_accepted_but_reported_overrun_is_durable_and_unusable() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            let phase = active.phase(&active.path)?;
            let project = &active.fixture.project;
            let limit = u32::try_from(active.run.budget().prompt_token_limit().get())?;
            let exact = EditPhaseWireCost::new(ContentHash::from_bytes([1; 32]), 1000, limit, 200)?;
            let deadline = active.run.created_at().unix_millis()
                + active.run.budget().duration_limit().millis();
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, exact, at(deadline)?)
                    .await,
                Err(Failure::DeadlineReached)
            ));
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, exact, at(5)?)
                    .await,
                Err(Failure::TimestampRegressed)
            ));
            assert_eq!(active.usage().await?, EditPhaseReservedUsage::default());
            let permit = active
                .store
                .reserve_edit_phase(project, &phase, Initial, exact, at(8)?)
                .await?;
            assert_eq!(permit.deadline(), at(deadline)?);
            assert_eq!(
                active
                    .store
                    .finish_edit_phase(
                        project,
                        permit,
                        EditPhaseObservation::new(
                            Outcome::AcceptedReply,
                            Some(limit + 1),
                            Some(41),
                            at(9)?
                        ),
                    )
                    .await,
                Err(Failure::BudgetExhausted(AgentBudgetDimension::PromptTokens))
            );
            let active = active.reopen().await?;
            assert_eq!(active.usage().await?.prompt_tokens, u64::from(limit + 1));
            let next = active.phase(&RepositoryPath::try_from_bytes(b"next.py".to_vec())?)?;
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(&active.fixture.project, &next, Initial, wire(2)?, at(10)?)
                    .await,
                Err(Failure::BudgetExhausted(AgentBudgetDimension::PromptTokens))
            ));
            active.assert_run_and_ledger_unchanged().await?;
            active.cleanup()
        })
    })
}

#[test]
fn production_store_does_not_open_the_reservation_contract() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            let other_layout = StorageLayout::prepare(active.fixture.root.join("old-appdata"))?;
            let old = LibsqlKnowledgeStore::open(&other_layout).await?;
            let phase = active.phase(&active.path)?;
            assert!(matches!(
                old.reserve_edit_phase(&active.fixture.project, &phase, Initial, wire(1)?, at(8)?)
                    .await,
                Err(Failure::UnsupportedSchema)
            ));
            drop(old);
            active.cleanup()
        })
    })
}

#[test]
fn a_new_current_goal_makes_an_old_run_and_ledger_binding_unusable() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let active = ActiveEdit::start().await?;
            let project = &active.fixture.project;
            let phase = active.phase(&active.path)?;
            let reservation = active
                .store
                .reserve_edit_phase(project, &phase, Initial, wire(1)?, at(8)?)
                .await?;

            let old = &active.goal;
            let original = GoalOriginalRequest::try_from_bytes(
                old.next_reference()?,
                old.original()
                    .ok_or("initial Goal original missing")?
                    .as_bytes()
                    .to_vec(),
            )?;
            let previous_criterion = &old.draft().acceptance_criteria()[0];
            let requirement = previous_criterion
                .origin()
                .ok_or("initial criterion origin missing")?;
            let rebound = previous_criterion.clone().with_origin(CriterionOrigin::new(
                original.range(0, original.as_bytes().len())?,
                requirement.evidence_requirement().clone(),
            ));
            let revised = old.revise_with_original(
                GoalContractDraft::new(
                    GoalObjective::try_from_string(
                        "Create the specified new file only".to_owned(),
                    )?,
                    vec![rebound],
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    old.draft().success_verification().clone(),
                )?,
                GoalRevisionReason::try_from_string(
                    "Clarify the file-creation boundary".to_owned(),
                )?,
                GoalContractTimestamp::from_unix_millis(9)?,
                original,
            )?;
            active
                .store
                .append_goal_contract_revision(project, &revised)
                .await?;
            active.assert_run_and_ledger_unchanged().await?;
            assert_eq!(
                active
                    .store
                    .finish_edit_phase(
                        project,
                        reservation,
                        observation(Outcome::AcceptedReply, 10)?,
                    )
                    .await,
                Err(Failure::StaleAnchor)
            );
            assert!(matches!(
                active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, wire(2)?, at(11)?)
                    .await,
                Err(Failure::StaleAnchor)
            ));
            assert_eq!(active.usage().await?.calls, 1);
            assert_eq!(active.usage().await?.unobserved_calls, 0);
            active.cleanup()
        })
    })
}

#[test]
fn incomplete_or_zero_usage_cannot_enable_repair_or_new_dispatch_after_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for (outcome, prompt, output) in [
                (Outcome::InvalidStructure, Some(390), None),
                (Outcome::InvalidStructure, Some(390), Some(0)),
                (Outcome::AcceptedReply, Some(0), Some(32)),
            ] {
                let active = ActiveEdit::start().await?;
                let project = &active.fixture.project;
                let phase = active.phase(&active.path)?;
                let reservation = active
                    .store
                    .reserve_edit_phase(project, &phase, Initial, wire(1)?, at(8)?)
                    .await?;
                assert_eq!(
                    active
                        .store
                        .finish_edit_phase(
                            project,
                            reservation,
                            EditPhaseObservation::new(outcome, prompt, output, at(9)?),
                        )
                        .await,
                    Err(Failure::IncompleteProviderUsage)
                );
                let active = active.reopen().await?;
                let phase = active.phase(&active.path)?;
                assert_eq!(
                    active.usage().await?,
                    EditPhaseReservedUsage {
                        calls: 1,
                        repairs: 0,
                        prompt_tokens: 400,
                        output_tokens: 200,
                        unobserved_calls: 0,
                        unknown_calls: 1,
                    }
                );
                for kind in [Initial, StructuralRepair] {
                    assert!(matches!(
                        active
                            .store
                            .reserve_edit_phase(
                                &active.fixture.project,
                                &phase,
                                kind,
                                wire(2)?,
                                at(10)?,
                            )
                            .await,
                        Err(Failure::UncertainProviderCall)
                    ));
                }
                active.cleanup()?;
            }
            Ok(())
        })
    })
}
