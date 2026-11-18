//! Current-run Plan intent only. Fixture admission is not automatic HTTP correction admission.
use super::*;
use a3_application::*;
use a3_domain::*;
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use a3_workspace::WorkspaceAgentSourceReader;
use serde_json::json;
#[path = "plan22_http_correction_apply_tests.rs"]
mod apply;
#[path = "plan22_http_context_intent_tests.rs"]
mod context_intent;
#[path = "plan22_http_correction_edit_tests.rs"]
mod edit;
#[path = "plan22_http_correction_plan_live.rs"]
mod live;
#[path = "plan22_http_origin_codec_tests.rs"]
mod origin_codec;

const FROZEN_SOURCE: &str = include_str!("../../../fixtures/plan22-p7/http-correction-source.py");

async fn offered_context(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    published: &PublishedIndex,
    script: &FileRevision,
) -> TestResult<HttpCorrectionPlanSourceOffer> {
    let seed = TaskLensSeedText::try_from_string("Additional current script context".to_owned())?;
    let lens = CompileTaskLens::new(store, store, store)
        .execute(
            project,
            TaskLensSeedSet::new(
                seed.clone(),
                seed,
                vec![TaskLensSeed::ExplicitPath(script.path().clone())],
            )?,
            TaskLensTokenBudget::new(4096)?,
            &ActiveControl,
        )
        .await?;
    let end = FROZEN_SOURCE
        .split_inclusive('\n')
        .take(21)
        .map(str::len)
        .sum();
    let required = EvidenceRef::new(
        script.clone(),
        SourceRange::new(
            0,
            end,
            SourcePosition::new(0, 0),
            SourcePosition::new(21, 0),
        )?,
    );
    let bound = BoundEditSource::for_file_span(project, published, &lens, &required)?;
    Ok(HttpCorrectionPlanSourceOffer::from_bound_source(
        project, published, &bound,
    )?)
}

fn ollama_profile(model: &str) -> TestResult<ModelProfile> {
    ollama_profile_with_context(model, HttpFixtureContext::FourK)
}

// Separate immutable run profiles; the historical 4k fixture remains the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpFixtureContext {
    FourK,
    SixteenK,
}
impl HttpFixtureContext {
    fn from_setting(value: Option<&str>) -> TestResult<Self> {
        match value {
            None | Some("4096") => Ok(Self::FourK),
            Some("16384") => Ok(Self::SixteenK),
            Some(_) => Err("HTTP fixture context must be exactly 4096 or 16384".into()),
        }
    }

    const fn tokens(self) -> u32 {
        match self {
            Self::FourK => 4096,
            Self::SixteenK => 16384,
        }
    }
}

fn ollama_profile_with_context(
    model: &str,
    context: HttpFixtureContext,
) -> TestResult<ModelProfile> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string(model.to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(context.tokens())?,
            ModelOutputLimit::new(1024)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::Disabled,
        ),
    ))
}
fn a04_goal() -> TestResult<GoalContract> {
    let suite: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/local-agent-eval-v1/suite.json"
    ))?;
    let text = suite["cases"]
        .as_array()
        .ok_or("cases missing")?
        .iter()
        .find(|c| c["case_id"] == "A04" && c["variant"] == "development")
        .and_then(|c| c["objective"].as_str())
        .ok_or("original A04 missing")?;
    assert_eq!(text.len(), 1207);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(TaskId::from_bytes([70; 32])),
        text.as_bytes().to_vec(),
    )?;
    let pending = PendingGoalIntake::new(original, GoalContractTimestamp::from_unix_millis(1)?)?;
    let original = pending.original();
    let mut findings = Vec::new();
    for (index, bounds) in [0, 402, 475, 639, 1207].windows(2).enumerate() {
        let range = original.range(bounds[0], bounds[1])?;
        findings.push(GoalIntakeFinding::new(
            if index == 0 {
                GoalIntakeFindingKind::Outcome {
                    requirement: AcceptanceCriterionRequirement::Must,
                    evidence_requirement: CriterionEvidenceRequirement::LoopbackHttp(a04_recipe()?),
                }
            } else {
                GoalIntakeFindingKind::Constraint
            },
            range,
            original.resolve(range)?.to_owned(),
        )?);
    }
    let review = GoalIntakeReview::begin(
        &pending,
        GoalIntakeReviewBatchProposal::new(original.range(0, 1207)?, findings, Vec::new())?,
    )?;
    Ok(prepare_goal_from_intake_review(&pending, &review)?)
}
fn a04_recipe() -> TestResult<LoopbackHttpRecipe> {
    let mut checks = vec![
        LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/".to_owned())?,
            200,
            LoopbackExpectedBody::Exact {
                hash: hash(b"Hello, world!\n"),
                bytes: 14,
            },
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?,
        LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/missing".to_owned())?,
            404,
            LoopbackExpectedBody::AnyBounded,
            LoopbackHeaderChecks::default(),
        )?,
    ];
    for method in [
        LoopbackMethod::Post,
        LoopbackMethod::Put,
        LoopbackMethod::Delete,
        LoopbackMethod::Patch,
    ] {
        checks.push(LoopbackHttpCheck::new(
            LoopbackProbe::new(method, "/".to_owned())?,
            405,
            LoopbackExpectedBody::AnyBounded,
            LoopbackHeaderChecks {
                plain_utf8: false,
                allow_get: true,
            },
        )?);
    }
    Ok(LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Explicit,
        std::num::NonZeroU16::try_from(18765)?,
        checks,
    )?
    .with_startup_coverage(0)?)
}

// Full original and complete current handler, with a fresh candidate DB and fresh grants.
async fn full_fixture(model: ModelProfile, physical: bool) -> TestResult<CurrentPlanFixture> {
    full_fixture_using(model, physical, Fixture::new()?).await
}

async fn full_fixture_using(
    model: ModelProfile,
    physical: bool,
    fixture: Fixture,
) -> TestResult<CurrentPlanFixture> {
    full_fixture_with_goal(model, physical, fixture, a04_goal()?).await
}

async fn full_fixture_with_goal(
    model: ModelProfile,
    physical: bool,
    mut fixture: Fixture,
    goal: GoalContract,
) -> TestResult<CurrentPlanFixture> {
    let root = fixture.root.join("worktree");
    initialize_git(&root)?;
    fs::write(root.join("server.py"), FROZEN_SOURCE)?;
    fixture.project = RepositoryInspector::new().inspect(&root)?;
    let before = inventory(&root)?;
    let recipe = a04_recipe()?;
    let step = TaskStepId::from_bytes([STEP; 32]);
    let spec = VerificationSpec::loopback_http(
        VerificationSpecId::from_bytes([213; 32]),
        VerificationRequirement::try_from_string(
            "Prove the original HTTP and startup requirements".to_owned(),
        )?,
        recipe.clone(),
    );
    let definition = TaskStepDefinition::new(
        step,
        None,
        TaskStepOutcome::try_from_string("Verify the current HTTP service".to_owned())?,
        TaskStepRationale::try_from_string("Current original-bound proof".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Complete owned HTTP recipe".to_owned(),
        )?],
        spec.clone(),
    )?
    .with_criterion_coverage(Vec::new(), vec![goal.draft().acceptance_criteria()[0].id()])?;
    let (store, refresh, compiler, published) = index_fixture(&fixture).await?;
    let snapshot = published.run().snapshot_id();
    let script = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|f| f.path() == recipe.script())
        .ok_or("source missing")?
        .clone();
    let budget = AgentRunBudget::new(
        AgentTurnLimit::new(12)?,
        AgentTokenLimit::new(45000)?,
        AgentTokenLimit::new(24000)?,
        AgentActionLimit::new(8)?,
        AgentRunDurationLimit::from_millis(900000)?,
        AgentRepairLimit::new(2)?,
    );
    let (mut run, stored, _) = start_fixture_step_with_budget(
        &fixture,
        &store,
        &goal,
        &model,
        snapshot,
        vec![definition],
        (budget, at(3)?),
    )
    .await?;
    let settings = LoopbackProcessSettings {
        timeout: ProcessTimeout::from_millis(8000)?,
        stdout_limit: ProcessOutputLimit::new(1024)?,
        stderr_limit: ProcessOutputLimit::new(1024)?,
        system_root: cfg!(windows),
    };
    let prepared = PreparedLoopbackStartupCoverage::new(
        run.id(),
        fixture.project.worktree().id(),
        step,
        snapshot,
        script.clone(),
        recipe.clone(),
        settings,
    )?;
    let mut decisions = Vec::new();
    for (index, action) in prepared.policy_actions().iter().enumerate() {
        decisions.push(
            super::super::super::approve(
                &fixture,
                &store,
                &mut run,
                action,
                10 + 5 * u8::try_from(index)?,
            )
            .await?,
        );
    }
    let scope = MachineEffectScope::new(
        AgentMutationKind::MachineProcess,
        step,
        prepared.effect_resource(),
        "python".to_owned(),
        Some(script.content_hash()),
        None,
    )?;
    let attempt = store
        .begin_machine_mutation_attempt(
            &fixture.project,
            run.id(),
            snapshot,
            TOOL,
            prepared.effect_fingerprint(),
            &scope,
            at(70)?,
        )
        .await?;
    let observed_clock = Instant::now();
    let mismatch = if physical {
        #[cfg(any(windows, target_os = "linux"))]
        {
            let mut grants = Vec::new();
            for id in &decisions {
                grants.push(
                    store
                        .load_policy_decision(&fixture.project, *id)
                        .await?
                        .ok_or("decision missing")?,
                );
            }
            let fresh = PreparedLoopbackStartupCoverage::new(
                run.id(),
                fixture.project.worktree().id(),
                step,
                snapshot,
                script.clone(),
                recipe.clone(),
                settings,
            )?;
            match native_http_runner()?
                .verify_startup(
                    &fixture.project,
                    AuthorizedLoopbackStartupCoverage::new(fresh, &grants)?,
                    &ActiveControl,
                )
                .await
            {
                LoopbackStartupVerificationOutcome::Mismatched { cleanup, mismatch } => {
                    #[cfg(windows)]
                    assert_eq!(
                        cleanup,
                        LoopbackStartupCleanupProof::WindowsJobsAndEndpoints
                    );
                    #[cfg(target_os = "linux")]
                    assert_eq!(
                        cleanup,
                        LoopbackStartupCleanupProof::ProcessGroupsAndEndpoints
                    );
                    mismatch
                }
                other => {
                    return Err(format!(
                        "physical source did not produce a cleaned mismatch: {other:?}"
                    )
                    .into());
                }
            }
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            return Err("owned physical startup is unsupported here".into());
        }
    } else {
        LoopbackHttpMismatch::new(
            &recipe,
            LoopbackStartupPhase::FreeExplicit,
            0,
            LoopbackHttpObservation::new(
                decisions[3],
                200,
                hash(b"Hello, world!"),
                13,
                LoopbackHeaderChecks {
                    plain_utf8: true,
                    allow_get: false,
                },
            )?,
        )?
    };
    let d = LoopbackHttpMismatchDiagnostic::new(
        LoopbackHttpEvidenceContext {
            verification_run_id: VerificationRunId::from_bytes([230; 32]),
            spec_id: spec.id(),
            run_id: run.id(),
            tool_run_id: TOOL,
            step_id: step,
            snapshot_id: snapshot,
        },
        attempt.tool_attempt().attempt(),
        fixture.project.worktree().id(),
        &recipe,
        script.clone(),
        LoopbackHttpProcess::from_spec(prepared.start())?,
        mismatch,
        decisions,
    )?;
    let observed_at = at(80 + u64::try_from(observed_clock.elapsed().as_millis())?)?;
    let expected = run.last_event_sequence();
    let event = run.record(
        RunEventId::from_bytes([231; 32]),
        RunEventKind::ToolAction,
        RunEventPayload::new(
            RunEventCode::VerificationFailure,
            Some(RunEventOutcome::Failed),
            None,
        ),
        snapshot,
        Some(RunEventSubject::Tool(TOOL)),
        observed_at,
    )?;
    store
        .complete_loopback_mismatch(
            &fixture.project,
            expected,
            &run,
            &event,
            &d,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let (mut ledger, version) = stored.into_parts();
    ledger.fail_step(
        step,
        run.id(),
        TaskStepFailureReason::try_from_string("Observed HTTP mismatch".to_owned())?,
        TaskLedgerTimestamp::from_unix_millis(observed_at.unix_millis())?,
    )?;
    let expected = run.last_event_sequence();
    let advance = AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::ExecutionVerificationFailed {
            correction_allowed: true,
        },
        RunEventId::from_bytes([239; 32]),
        snapshot,
        observed_at,
        false,
    )?;
    let version = store
        .commit_ledger_action(
            &fixture.project,
            version,
            expected,
            &ledger,
            &run,
            advance.event(),
        )
        .await?;
    let stored = StoredTaskLedger::new(ledger, version);
    let page = WorkspaceAgentSourceReader
        .read_page(
            &fixture.project,
            &script,
            &AgentFileInspection::new(
                script.path().clone(),
                AgentFileStartLine::new(22)?,
                AgentFileLineCount::new(10)?,
            ),
            &ActiveControl,
        )
        .await?;
    assert_eq!(page.text().len(), 370);
    let phase = PreparedHttpCorrectionPlan::new(
        &fixture.project,
        &goal,
        &stored,
        &run,
        &published,
        model.clone(),
        &d,
        &page,
        Some(offered_context(store.as_ref(), &fixture.project, &published, &script).await?),
    )?;
    let http = PreparedHttpFixture {
        fixture,
        root,
        before,
        goal,
        spec,
        step,
        store,
        refresh,
        compiler,
        model,
        run: run.clone(),
        stored: stored.clone(),
        settings,
        prepared,
    };
    Ok(CurrentPlanFixture {
        http,
        phase,
        run,
        stored,
    })
}

#[test]
fn http_correction_plan_actual_ollama_initial_and_repair_fit_four_k_with_complete_a04() -> TestResult
{
    crate::run_native_libsql_test(async {
        let provider = OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?;
        for tag in ["qwen38-4k:latest", "ornith-1.5:9b"] {
            let f = full_fixture(ollama_profile(tag)?, false).await?;
            let original = f.http.goal.original().ok_or("original missing")?;
            let input = f.phase.request().messages()[1].content();
            let original_bytes = original.as_bytes().to_vec();
            assert!(input.contains(original.resolve(original.range(0, 402)?)?));
            for constraint in f.http.goal.draft().constraints() {
                assert!(input.contains(constraint.as_str()));
            }
            assert_eq!(original.as_bytes(), original_bytes);
            assert_eq!(f.phase.anchor().original, original.digest());
            let raw = FROZEN_SOURCE
                .split_inclusive('\n')
                .skip(21)
                .take(10)
                .collect::<String>();
            assert_eq!(raw.len(), 370);
            assert_eq!(f.phase.anchor().source_text, hash(raw.as_bytes()));
            assert_eq!(f.phase.anchor().source_range.len(), 370);
            for (offset, line) in raw.split_inclusive('\n').enumerate() {
                assert!(input.contains(&format!("L{}: {line}", 22 + offset)));
            }
            let schema = f
                .phase
                .request()
                .structured_output()
                .ok_or("schema missing")?
                .value();
            assert_eq!(schema["properties"]["version"]["const"], 4);
            assert_eq!(
                schema["required"]
                    .as_array()
                    .ok_or("required missing")?
                    .len(),
                4
            );
            assert!(input.contains("proof unverified"));
            assert!(input.ends_with("Read offer S1: server.py 1-21.\n"));
            assert!(!input.contains("Task:"));
            let system = f.phase.request().messages()[0].content();
            assert!(system.contains("Plan next code change"));
            assert!(system.contains("intent=short change outcome"));
            assert!(system.contains(
                "decision=responsible line label, S1 for context, needEvidence, tooLarge or refuse"
            ));
            assert!(matches!(
                f.phase.decode_proposal(&reply(&f.phase, "S1")?)?.decision(),
                HttpCorrectionPlanDecision::NeedOfferedSource(_)
            ));
            assert!(!input.contains("open1"));
            for kind in [
                EditPhaseCallKind::Initial,
                EditPhaseCallKind::StructuralRepair,
            ] {
                let request = f.phase.request_for(kind)?;
                let projection = provider.project_wire_request(&request)?;
                println!(
                    "actual current Plan wire {tag} {kind:?}: {} bytes",
                    projection.request_bytes()
                );
                PhaseWireBudget::fit(projection, request.profile())?;
                assert_eq!(
                    (projection.context_tokens(), projection.output_tokens()),
                    (4096, 1024)
                );
            }
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

fn long_projection_goal(large_mandatory: bool) -> TestResult<GoalContract> {
    let base = a04_goal()?;
    let mut texts =
        vec!["Serve GET / as 200 with plain UTF-8 'Hello, world!' followed by LF.\n".to_owned()];
    for index in 1..30 {
        texts.push(format!("UNRELATED_{index:02}: change notes/part_{index:02}.txt as a separate path-change obligation. This item belongs to a later work step, outside the HTTP service; it makes no runtime claim and must not receive completion credit from this check.\n"));
    }
    let mut constraints = base
        .draft()
        .constraints()
        .iter()
        .map(|v| v.as_str().to_owned())
        .collect::<Vec<_>>();
    if large_mandatory {
        constraints.extend([
            format!(
                "Mandatory boundary A: {}",
                "preserve this complete rule; ".repeat(50)
            ),
            format!(
                "Mandatory boundary B: {}",
                "retain this complete restriction; ".repeat(45)
            ),
        ]);
    }
    let constraint_start = texts.len();
    texts.extend(constraints.iter().map(|value| format!("{value}\n")));
    let non_goal_start = texts.len();
    texts.push("Do not create a new user interface.\n".to_owned());
    texts.push("Keep the existing Python standard-library layout.\n".to_owned());
    let mut offset = 0;
    let bounds = texts
        .iter()
        .map(|text| {
            let start = offset;
            offset += text.len();
            (start, offset)
        })
        .collect::<Vec<_>>();
    let raw = texts.concat();
    assert!(raw.len() > 4096);
    let task = TaskId::from_bytes([70; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        raw.into_bytes(),
    )?;
    let mut criteria = Vec::new();
    for index in 0..30 {
        let range = original.range(bounds[index].0, bounds[index].1)?;
        let evidence = if index == 0 {
            CriterionEvidenceRequirement::LoopbackHttp(a04_recipe()?)
        } else {
            CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![RepositoryPath::try_from_bytes(
                    format!("notes/part_{index:02}.txt").into_bytes(),
                )?],
            )?)
        };
        let statement = if index == 0 {
            "Preserve the required GET / response in the existing implementation.".to_owned()
        } else {
            texts[index].clone()
        };
        criteria.push(
            AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([1 + u8::try_from(index)?; 32]),
                AcceptanceCriterionStatement::try_from_string(statement)?,
            )
            .with_origin(CriterionOrigin::new(range, evidence)),
        );
    }
    let constraints = constraints
        .into_iter()
        .enumerate()
        .map(|(index, text)| -> TestResult<GoalConstraint> {
            Ok(
                GoalConstraint::try_from_string(text)?.with_origin(original.range(
                    bounds[constraint_start + index].0,
                    bounds[constraint_start + index].1,
                )?),
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Verify one active HTTP requirement".to_owned())?,
        criteria,
        constraints,
        vec![
            NonGoal::try_from_string(texts[non_goal_start].clone())?
                .with_origin(original.range(bounds[non_goal_start].0, bounds[non_goal_start].1)?),
        ],
        vec![
            UserDecision::try_from_string(texts[non_goal_start + 1].clone())?.with_origin(
                original.range(bounds[non_goal_start + 1].0, bounds[non_goal_start + 1].1)?,
            ),
        ],
        SuccessVerification::try_from_string(
            "Require the matching current evidence for each criterion".to_owned(),
        )?,
    )?;
    Ok(
        GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
            .with_original(original)?,
    )
}

#[test]
fn http_correction_plan_active_projection_retains_authority_and_all_outside_rules() -> TestResult {
    crate::run_native_libsql_test(async {
        let provider = OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?;
        for large in [false, true] {
            let goal = long_projection_goal(large)?;
            let original = goal.original().ok_or("long original missing")?.clone();
            let f = full_fixture_with_goal(
                ollama_profile("qwen38-4k:latest")?,
                false,
                Fixture::new()?,
                goal,
            )
            .await?;
            let view = f.phase.request().messages()[1].content();
            assert!(!view.contains("UNRELATED_"));
            assert!(view.contains("Criteria30 active1 outside29 unassessed; proof unverified."));
            let active = f.http.goal.draft().acceptance_criteria()[0]
                .origin()
                .ok_or("active origin missing")?
                .original_range();
            assert!(view.contains(original.resolve(active)?));
            assert!(
                view.contains(
                    f.http.goal.draft().acceptance_criteria()[0]
                        .statement()
                        .as_str()
                )
            );
            for text in f
                .http
                .goal
                .draft()
                .constraints()
                .iter()
                .map(GoalConstraint::as_str)
                .chain(f.http.goal.draft().non_goals().iter().map(NonGoal::as_str))
                .chain(
                    f.http
                        .goal
                        .draft()
                        .user_decisions()
                        .iter()
                        .map(UserDecision::as_str),
                )
            {
                assert!(view.contains(text));
            }
            assert!(view.contains("Rules:\n"));
            assert!(view.contains("Non-goals:\n"));
            assert!(view.contains("Decisions:\n"));
            assert_eq!(f.phase.anchor().original, original.digest());
            let persisted = f
                .http
                .store
                .load_current_goal_contract(&f.http.fixture.project, f.http.goal.task_id())
                .await?
                .ok_or("durable goal missing")?;
            assert_eq!(persisted.original(), Some(&original));
            for kind in [
                EditPhaseCallKind::Initial,
                EditPhaseCallKind::StructuralRepair,
            ] {
                let request = f.phase.request_for(kind)?;
                let projection = provider.project_wire_request(&request)?;
                println!(
                    "active projection long30 large={large} {kind:?}: {} bytes",
                    projection.request_bytes()
                );
                if large {
                    assert_eq!(
                        PhaseWireBudget::fit(projection, request.profile()),
                        Err(PhaseWireBudgetError::Capacity)
                    );
                } else {
                    PhaseWireBudget::fit(projection, request.profile())?;
                }
            }
            assert_eq!(
                f.http
                    .store
                    .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                    .await?
                    .calls,
                0
            );
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

struct CurrentPlanFixture {
    http: PreparedHttpFixture<PreparedLoopbackStartupCoverage>,
    phase: PreparedHttpCorrectionPlan,
    run: AgentRun,
    stored: StoredTaskLedger,
}

async fn current_fixture() -> TestResult<CurrentPlanFixture> {
    let (f, d, mut run, event, expected) = prepared_mismatch().await?;
    f.store
        .complete_loopback_mismatch(
            &f.fixture.project,
            expected,
            &run,
            &event,
            &d,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    // Test-only existing signal: production mismatch remains terminal until a later slice.
    let (mut ledger, version) = f.stored.clone().into_parts();
    ledger.fail_step(
        f.step,
        run.id(),
        TaskStepFailureReason::try_from_string("Observed HTTP mismatch".to_owned())?,
        TaskLedgerTimestamp::from_unix_millis(80)?,
    )?;
    let expected = run.last_event_sequence();
    let advance = AdvanceAgentController.execute(
        &mut run,
        AgentControllerSignal::ExecutionVerificationFailed {
            correction_allowed: true,
        },
        RunEventId::from_bytes([239; 32]),
        d.context().snapshot_id,
        at(80)?,
        false,
    )?;
    let version = f
        .store
        .commit_ledger_action(
            &f.fixture.project,
            version,
            expected,
            &ledger,
            &run,
            advance.event(),
        )
        .await?;
    let stored = StoredTaskLedger::new(ledger, version);
    let published = f
        .store
        .latest_published_index(&f.fixture.project, &ActiveControl)
        .await?
        .ok_or("published index missing")?;
    let page = WorkspaceAgentSourceReader
        .read_page(
            &f.fixture.project,
            d.script(),
            &AgentFileInspection::new(
                d.script().path().clone(),
                AgentFileStartLine::new(5)?,
                AgentFileLineCount::new(9)?,
            ),
            &ActiveControl,
        )
        .await?;
    let phase = PreparedHttpCorrectionPlan::new(
        &f.fixture.project,
        &f.goal,
        &stored,
        &run,
        &published,
        f.model.clone(),
        &d,
        &page,
        None,
    )?;
    Ok(CurrentPlanFixture {
        http: f,
        phase,
        run,
        stored,
    })
}

fn wire(
    phase: &PreparedHttpCorrectionPlan,
    kind: EditPhaseCallKind,
) -> TestResult<EditPhaseWireCost> {
    // Exact deterministic test transport; actual Ollama wire is separately regression-tested.
    let request = phase.request_for(kind)?;
    let body = serde_json::to_vec(
        &json!({"messages":request.messages().iter().map(|m|m.content()).collect::<Vec<_>>(),"format":request.structured_output().ok_or("missing schema")?.value()}),
    )?;
    Ok(EditPhaseWireCost::new(
        hash(&body),
        u32::try_from(body.len())?,
        u32::try_from(body.len())?,
        1024,
    )?)
}
fn reply(phase: &PreparedHttpCorrectionPlan, decision: &str) -> TestResult<Vec<u8>> {
    reply_with_intent(
        phase,
        decision,
        &HttpCorrectionPlanIntent::try_from_string(
            "Adjust the selected HTTP handler for the observed response mismatch.".to_owned(),
        )?,
    )
}
fn reply_with_intent(
    phase: &PreparedHttpCorrectionPlan,
    decision: &str,
    intent: &HttpCorrectionPlanIntent,
) -> TestResult<Vec<u8>> {
    let schema = phase
        .request()
        .structured_output()
        .ok_or("missing reply schema")?;
    Ok(serde_json::to_vec(
        &json!({"version":4,"job":schema.value()["properties"]["job"]["const"],"intent":intent.as_str(),"decision":decision}),
    )?)
}
fn observation(
    outcome: EditPhaseObservationOutcome,
    at_millis: u64,
) -> TestResult<EditPhaseObservation> {
    Ok(EditPhaseObservation::new(
        outcome,
        Some(64),
        Some(16),
        at(at_millis)?,
    ))
}

#[test]
fn http_correction_plan_accepts_only_required_strict_bounded_decisions() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let good = reply(&f.phase, "L6")?;
        let proposal = f.phase.decode_proposal(&good)?;
        assert_eq!(
            proposal.decision(),
            HttpCorrectionPlanDecision::FocusLine(AgentFileStartLine::new(6)?)
        );
        assert_eq!(
            f.phase
                .decode_proposal(&f.phase.encode_proposal(&proposal)?)?,
            proposal
        );
        for decision in [
            "correct", "L0", "L4", "L14", "L022", "L06", "L6other", "invent", "S1", "S2",
        ] {
            assert_eq!(
                f.phase.decode_proposal(&reply(&f.phase, decision)?),
                Err(HttpCorrectionPlanFailure::InvalidReply)
            );
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(&reply(&f.phase, "needEvidence")?)?;
        value
            .as_object_mut()
            .ok_or("object missing")?
            .remove("decision");
        assert!(
            f.phase
                .decode_proposal(&serde_json::to_vec(&value)?)
                .is_err()
        );
        for field in ["job", "version", "basis", "unexpected"] {
            let mut value: serde_json::Value = serde_json::from_slice(&good)?;
            value[field] = json!("foreign");
            assert!(
                f.phase
                    .decode_proposal(&serde_json::to_vec(&value)?)
                    .is_err()
            );
        }
        let duplicate = String::from_utf8(good.clone())?.replacen('{', "{\"decision\":\"L6\",", 1);
        assert!(f.phase.decode_proposal(duplicate.as_bytes()).is_err());
        let mut old: serde_json::Value = serde_json::from_slice(&good)?;
        for version in [1, 2, 3] {
            old["version"] = json!(version);
            assert!(f.phase.decode_proposal(&serde_json::to_vec(&old)?).is_err());
        }
        for decision in ["L6", "needEvidence", "tooLarge", "refuse"] {
            for range in [json!(null), json!([6, 6])] {
                let mut coupled: serde_json::Value =
                    serde_json::from_slice(&reply(&f.phase, decision)?)?;
                coupled["range"] = range;
                assert!(
                    f.phase
                        .decode_proposal(&serde_json::to_vec(&coupled)?)
                        .is_err()
                );
            }
        }
        for line in 5..=13 {
            assert_eq!(
                f.phase
                    .decode_proposal(&reply(&f.phase, &format!("L{line}"))?)?
                    .decision(),
                HttpCorrectionPlanDecision::FocusLine(AgentFileStartLine::new(line)?)
            );
        }
        for decision in ["needEvidence", "tooLarge", "refuse"] {
            f.phase.decode_proposal(&reply(&f.phase, decision)?)?;
        }
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_reserves_before_result_and_cold_loads_without_ledger_authority()
-> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let store = &f.http.store;
        let project = &f.http.fixture.project;
        let permit = store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                wire(&f.phase, EditPhaseCallKind::Initial)?,
                at(81)?,
            )
            .await?;
        let usage = store.load_edit_phase_usage(project, f.run.id()).await?;
        assert_eq!((usage.calls, usage.unobserved_calls), (1, 1));
        assert!(
            store
                .load_http_correction_plan(project, &f.phase)
                .await?
                .is_none()
        );
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
        assert_eq!(
            store
                .finish_http_correction_plan(
                    project,
                    &f.phase,
                    permit,
                    observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                    Some(proposal.clone())
                )
                .await?,
            Some(proposal.clone())
        );
        assert_eq!(
            store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored.clone())
        );
        assert_eq!(
            store.load_agent_run(project, f.run.id()).await?,
            Some(f.run.clone())
        );
        let usage = store.load_edit_phase_usage(project, f.run.id()).await?;
        assert_eq!(
            (usage.calls, usage.unobserved_calls, usage.unknown_calls),
            (1, 0, 0)
        );
        assert_eq!(f.run.usage().turn_count(), 0);
        drop((f.http.compiler, f.http.refresh, f.http.store));
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
        assert_eq!(
            reopened
                .load_http_correction_plan(&f.http.fixture.project, &f.phase)
                .await?,
            Some(proposal)
        );
        assert_eq!(
            reopened
                .load_task_ledger(&f.http.fixture.project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        drop(reopened);
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_lost_dispatch_is_charged_and_blocks_any_second_call() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let project = &f.http.fixture.project;
        let store = &f.http.store;
        {
            let _permit = store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?,
                )
                .await?;
        }
        for kind in [
            EditPhaseCallKind::Initial,
            EditPhaseCallKind::StructuralRepair,
        ] {
            assert!(matches!(
                store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        kind,
                        wire(&f.phase, kind)?,
                        at(82)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::UncertainProviderCall)
            ));
        }
        let db = f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        let tx = conn
            .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
            .await?;
        let (usage, latest) = crate::edit_phase_reservation_repository::read_run_usage(
            &tx,
            project.worktree().id(),
            &f.run,
        )
        .await?;
        assert_eq!(
            (usage.calls, usage.unobserved_calls, latest),
            (1, 1, Some(at(81)?))
        );
        tx.commit().await?;
        drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
        assert_eq!(
            reopened
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?
                .unobserved_calls,
            1
        );
        drop(reopened);
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_allows_exactly_one_different_wire_repair_after_invalid_structure()
-> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let project = &f.http.fixture.project;
        let store = &f.http.store;
        let first = wire(&f.phase, EditPhaseCallKind::Initial)?;
        assert!(matches!(
            store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::StructuralRepair,
                    wire(&f.phase, EditPhaseCallKind::StructuralRepair)?,
                    at(81)?
                )
                .await,
            Err(EditPhaseReservationFailure::RepairNotAvailable)
        ));
        let permit = store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                first,
                at(81)?,
            )
            .await?;
        store
            .finish_http_correction_plan(
                project,
                &f.phase,
                permit,
                observation(EditPhaseObservationOutcome::InvalidStructure, 82)?,
                None,
            )
            .await?;
        assert!(matches!(
            store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::StructuralRepair,
                    first,
                    at(83)?
                )
                .await,
            Err(EditPhaseReservationFailure::UnchangedRepairWire)
        ));
        let permit = store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::StructuralRepair,
                wire(&f.phase, EditPhaseCallKind::StructuralRepair)?,
                at(83)?,
            )
            .await?;
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "needEvidence")?)?;
        store
            .finish_http_correction_plan(
                project,
                &f.phase,
                permit,
                observation(EditPhaseObservationOutcome::AcceptedReply, 84)?,
                Some(proposal),
            )
            .await?;
        for kind in [
            EditPhaseCallKind::Initial,
            EditPhaseCallKind::StructuralRepair,
        ] {
            assert!(matches!(
                store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        kind,
                        wire(&f.phase, kind)?,
                        at(85)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::AlreadyReserved)
            ));
        }
        let usage = store.load_edit_phase_usage(project, f.run.id()).await?;
        assert_eq!((usage.calls, usage.repairs), (2, 1));
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_reservation_and_observation_roll_back_without_free_dispatch() -> TestResult
{
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let project = &f.http.fixture.project;
        let store = &f.http.store;
        let db = f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        conn.execute_batch("CREATE TRIGGER reject_plan BEFORE INSERT ON http_correction_plan_reservations BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;").await?;
        assert!(matches!(
            store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?
                )
                .await,
            Err(EditPhaseReservationFailure::Unavailable)
        ));
        assert_eq!(
            store
                .load_edit_phase_usage(project, f.run.id())
                .await?
                .calls,
            0
        );
        conn.execute_batch("DROP TRIGGER reject_plan; CREATE TRIGGER reject_plan_finish BEFORE UPDATE ON http_correction_plan_reservations BEGIN SELECT RAISE(ABORT,'fixture finish rollback'); END;").await?;
        let permit = store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                wire(&f.phase, EditPhaseCallKind::Initial)?,
                at(81)?,
            )
            .await?;
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
        assert_eq!(
            store
                .finish_http_correction_plan(
                    project,
                    &f.phase,
                    permit,
                    observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                    Some(proposal)
                )
                .await,
            Err(EditPhaseReservationFailure::Unavailable)
        );
        assert_eq!(
            store
                .load_edit_phase_usage(project, f.run.id())
                .await?
                .unobserved_calls,
            1
        );
        assert!(
            store
                .load_http_correction_plan(project, &f.phase)
                .await?
                .is_none()
        );
        assert_eq!(
            store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_finish_requires_the_exact_wire_and_reservation_cas() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let project = &f.http.fixture.project;
        let store = &f.http.store;
        let mut permit = store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                wire(&f.phase, EditPhaseCallKind::Initial)?,
                at(81)?,
            )
            .await?;
        permit.wire = EditPhaseWireCost::new(hash(b"foreign wire"), 100, 100, 100)?;
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
        assert_eq!(
            store
                .finish_http_correction_plan(
                    project,
                    &f.phase,
                    permit,
                    observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                    Some(proposal)
                )
                .await,
            Err(EditPhaseReservationFailure::ObservationConflict)
        );
        assert_eq!(
            store
                .load_edit_phase_usage(project, f.run.id())
                .await?
                .unobserved_calls,
            1
        );
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_validates_canonical_causal_tail_diagnosis_and_current_failed_attempt()
-> TestResult {
    crate::run_native_libsql_test(async {
        for kind in 0..8 {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let store = &f.http.store;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            match kind {
                0 => {
                    conn.execute(
                        "UPDATE run_events SET payload_digest=?1 WHERE event_id=?2",
                        libsql::params![vec![99_u8; 32], vec![239_u8; 32]],
                    )
                    .await?;
                }
                1 => {
                    let payload = RunEventPayload::new(
                        RunEventCode::VerificationFailure,
                        Some(RunEventOutcome::Failed),
                        Some(RunEventRedaction::new(
                            RunEventRedactionSource::ToolOutput,
                            1,
                            false,
                        )),
                    );
                    conn.execute("UPDATE run_events SET redaction_source='tool_output',redaction_observed_bytes=1,redaction_source_truncated=0,payload_digest=?1 WHERE event_id=?2",libsql::params![payload.digest().as_bytes().to_vec(),vec![239_u8;32]]).await?;
                }
                2 => {
                    conn.execute("UPDATE run_events SET event_kind='model_interaction',state_from=NULL,state_to=NULL,turn_prompt_tokens=1,turn_output_tokens=1,turn_repair_used=0 WHERE event_id=?1",libsql::params![vec![239_u8;32]]).await?;
                }
                3 => {
                    conn.execute_batch("DROP TRIGGER loopback_mismatch_immutable;")
                        .await?;
                    conn.execute(
                        "UPDATE loopback_mismatch_diagnostics SET digest=?1",
                        libsql::params![vec![99_u8; 32]],
                    )
                    .await?;
                }
                4 => {
                    conn.execute(
                        "UPDATE task_step_attempts SET started_at_unix_millis=71 WHERE task_id=?1",
                        libsql::params![f.http.goal.task_id().as_bytes().to_vec()],
                    )
                    .await?;
                }
                5 => {
                    assert!(
                        conn.execute(
                            "UPDATE task_steps SET verification_spec_id=?1 WHERE task_id=?2",
                            libsql::params![
                                vec![99_u8; 32],
                                f.http.goal.task_id().as_bytes().to_vec()
                            ]
                        )
                        .await
                        .is_err()
                    );
                }
                6 => {
                    conn.execute(
                        "UPDATE file_revisions SET content_hash=?1 WHERE repository_path=?2",
                        libsql::params![vec![99_u8; 32], b"server.py".to_vec()],
                    )
                    .await?;
                }
                7 => {
                    conn.execute(
                        "UPDATE run_events SET occurred_at_unix_millis=79 WHERE event_id=?1",
                        libsql::params![vec![231_u8; 32]],
                    )
                    .await?;
                }
                _ => unreachable!(),
            }
            if kind != 5 {
                let expected = if kind == 0 {
                    EditPhaseReservationFailure::InvalidStoredData
                } else {
                    EditPhaseReservationFailure::StaleAnchor
                };
                let result = store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::Initial,
                        wire(&f.phase, EditPhaseCallKind::Initial)?,
                        at(82)?,
                    )
                    .await;
                assert!(
                    matches!(result,Err(error) if error==expected),
                    "case {kind}: {:?}",
                    result.err()
                );
            }
            assert_eq!(
                store
                    .load_edit_phase_usage(project, f.run.id())
                    .await?
                    .calls,
                0
            );
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_terminal_run_and_changed_source_deny_dispatch() -> TestResult {
    crate::run_native_libsql_test(async {
        for terminal in [false, true] {
            let mut f = current_fixture().await?;
            let project = &f.http.fixture.project;
            if terminal {
                let expected = f.run.last_event_sequence();
                let snapshot = f.run.current_snapshot_id();
                let advance = AdvanceAgentController.execute(
                    &mut f.run,
                    AgentControllerSignal::FatalFailure,
                    RunEventId::from_bytes([240; 32]),
                    snapshot,
                    at(81)?,
                    false,
                )?;
                f.http
                    .store
                    .append_run_event(project, expected, &f.run, advance.event())
                    .await?;
                assert_eq!(f.run.state(), AgentControllerState::Failed);
            } else {
                fs::write(
                    f.http.root.join("server.py"),
                    format!("{STARTUP_SERVER}\n# user change\n"),
                )?;
                assert_eq!(
                    f.phase
                        .revalidate_source(project, &WorkspaceAgentSourceReader, &ActiveControl)
                        .await,
                    Err(AgentSourceReadFailure::Stale)
                );
                f.http
                    .refresh
                    .execute(
                        project,
                        &RepositoryChangeBatch::full_rescan(
                            Vec::new(),
                            RepositoryRescanReason::Explicit,
                        )?,
                        &mut f.http.compiler,
                        &ActiveControl,
                    )
                    .await?;
            }
            assert!(matches!(
                f.http
                    .store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::Initial,
                        wire(&f.phase, EditPhaseCallKind::Initial)?,
                        at(82)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::StaleAnchor)
            ));
            assert_eq!(
                f.http
                    .store
                    .load_edit_phase_usage(project, f.run.id())
                    .await?
                    .calls,
                0
            );
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_deadline_usage_overrun_and_unknown_remain_durable() -> TestResult {
    crate::run_native_libsql_test(async {
        for kind in 0..4 {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let store = &f.http.store;
            let deadline =
                f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis();
            assert!(matches!(
                store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::Initial,
                        wire(&f.phase, EditPhaseCallKind::Initial)?,
                        at(deadline)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::DeadlineReached)
            ));
            let permit = store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?,
                )
                .await?;
            let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
            let (observed, expected) = match kind {
                0 => (
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::AcceptedReply,
                        Some(64),
                        Some(16),
                        at(deadline)?,
                    ),
                    EditPhaseReservationFailure::DeadlineReached,
                ),
                1 => (
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::AcceptedReply,
                        Some(permit.wire.prompt_estimate() + 1),
                        Some(16),
                        at(82)?,
                    ),
                    EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::PromptTokens,
                    ),
                ),
                2 => (
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::AcceptedReply,
                        Some(64),
                        Some(permit.wire.output_reserve() + 1),
                        at(82)?,
                    ),
                    EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::OutputTokens,
                    ),
                ),
                _ => (
                    EditPhaseObservation::new(
                        EditPhaseObservationOutcome::AcceptedReply,
                        None,
                        None,
                        at(82)?,
                    ),
                    EditPhaseReservationFailure::IncompleteProviderUsage,
                ),
            };
            assert_eq!(
                store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        permit,
                        observed,
                        Some(proposal)
                    )
                    .await,
                Err(expected)
            );
            let usage = store.load_edit_phase_usage(project, f.run.id()).await?;
            assert_eq!((usage.calls, usage.unobserved_calls), (1, 0));
            assert_eq!(usage.unknown_calls, u32::from(kind != 0));
            assert!(
                store
                    .load_http_correction_plan(project, &f.phase)
                    .await?
                    .is_none()
            );
            if kind != 0 {
                assert!(matches!(
                    store
                        .reserve_http_correction_plan(
                            project,
                            &f.phase,
                            EditPhaseCallKind::StructuralRepair,
                            wire(&f.phase, EditPhaseCallKind::StructuralRepair)?,
                            at(83)?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::UncertainProviderCall)
                ));
            }
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_sql_lifecycle_is_insert_unobserved_then_one_immutable_observation()
-> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let project = &f.http.fixture.project;
        let store = &f.http.store;
        let permit = store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                wire(&f.phase, EditPhaseCallKind::Initial)?,
                at(81)?,
            )
            .await?;
        let db = f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        {
            let mut rows = conn
                .query(
                    "SELECT protocol_version FROM http_correction_plan_reservations",
                    (),
                )
                .await?;
            let row = rows.next().await?.ok_or("protocol row missing")?;
            assert_eq!(row.get::<i64>(0)?, 4);
        }
        assert!(conn.execute("INSERT INTO http_correction_plan_reservations SELECT worktree_id,run_id,diagnostic_digest,tool_run_id,tool_attempt,job_digest,'structural_repair',anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,NULL,NULL,NULL,NULL,NULL,NULL,1 FROM http_correction_plan_reservations",()).await.is_err());
        assert!(
            conn.execute(
                "UPDATE http_correction_plan_reservations SET reported_prompt=1",
                ()
            )
            .await
            .is_err()
        );
        assert!(conn.execute("UPDATE http_correction_plan_reservations SET proposal=x'01',proposal_digest=zeroblob(32)",()).await.is_err());
        assert!(conn.execute("INSERT INTO http_correction_plan_reservations SELECT worktree_id,run_id,diagnostic_digest,tool_run_id,tool_attempt,job_digest,'structural_repair',anchor,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,'unknown',82,NULL,NULL,NULL,NULL,2 FROM http_correction_plan_reservations",()).await.is_err());
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
        store
            .finish_http_correction_plan(
                project,
                &f.phase,
                permit,
                observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                Some(proposal),
            )
            .await?;
        assert!(conn.execute("UPDATE http_correction_plan_reservations SET outcome='unknown',proposal=NULL,proposal_digest=NULL",()).await.is_err());
        assert!(
            conn.execute("DELETE FROM http_correction_plan_reservations", ())
                .await
                .is_err()
        );
        drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_counts_edit_phase_uncertainty_usage_and_latest_time() -> TestResult {
    crate::run_native_libsql_test(async {
        for unknown in [false, true] {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let store = &f.http.store;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            let a = f.phase.anchor();
            // A prior Execute-phase debit retained in the same owning run. No phase admission is weakened.
            conn.execute("INSERT INTO edit_phase_reservations(worktree_id,run_id,job_digest,call_kind,task_id,goal_revision,ledger_revision,ledger_store_version,run_sequence,step_id,attempt_number,verification_id,snapshot_id,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline) VALUES(?1,?2,?3,'initial',?4,?5,?6,?7,?8,?9,1,?10,?11,?3,100,100,50,79,900003)",libsql::params![a.worktree.as_bytes().to_vec(),a.run.as_bytes().to_vec(),vec![91_u8;32],a.goal.task_id().as_bytes().to_vec(),i64::from(a.goal.revision().get()),i64::from(a.ledger.get()),i64::try_from(a.version.get())?,i64::try_from(a.sequence.get())?-2,a.step.as_bytes().to_vec(),a.verification.as_bytes().to_vec(),a.snapshot.as_bytes().to_vec()]).await?;
            if unknown {
                conn.execute(
                    "UPDATE edit_phase_reservations SET outcome='unknown',finished_at=81",
                    (),
                )
                .await?;
            }
            let tx = conn
                .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
                .await?;
            let (usage, latest) = crate::edit_phase_reservation_repository::read_run_usage(
                &tx,
                project.worktree().id(),
                &f.run,
            )
            .await?;
            assert_eq!(
                (usage.calls, usage.prompt_tokens, usage.output_tokens),
                (1, 100, 50)
            );
            assert_eq!(
                (usage.unobserved_calls, usage.unknown_calls),
                (u32::from(!unknown), u32::from(unknown))
            );
            assert_eq!(latest, Some(at(if unknown { 81 } else { 79 })?));
            tx.commit().await?;
            assert!(matches!(
                store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::Initial,
                        wire(&f.phase, EditPhaseCallKind::Initial)?,
                        at(82)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::UncertainProviderCall)
            ));
            // Once a known old observation settles, its full debit still reduces this Plan budget.
            if !unknown {
                conn.execute("UPDATE edit_phase_reservations SET outcome='accepted_reply',finished_at=83,reported_prompt=200,reported_output=75",()).await?;
                assert!(matches!(
                    store
                        .reserve_http_correction_plan(
                            project,
                            &f.phase,
                            EditPhaseCallKind::Initial,
                            wire(&f.phase, EditPhaseCallKind::Initial)?,
                            at(82)?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::TimestampRegressed)
                ));
                let costly = EditPhaseWireCost::new(
                    hash(b"over shared budget"),
                    1,
                    u32::try_from(f.run.budget().prompt_token_limit().get())?,
                    1,
                )?;
                assert!(matches!(
                    store
                        .reserve_http_correction_plan(
                            project,
                            &f.phase,
                            EditPhaseCallKind::Initial,
                            costly,
                            at(84)?
                        )
                        .await,
                    Err(EditPhaseReservationFailure::BudgetExhausted(
                        AgentBudgetDimension::PromptTokens
                    ))
                ));
            }
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[derive(Debug)]
struct ObservedPlanProvider<'a> {
    inner: OllamaModelProvider,
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    run: AgentRunId,
    invalid_initial: bool,
    unsafe_intent: bool,
    calls: AtomicUsize,
}
impl ModelProvider for ObservedPlanProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        ModelProvider::provider_id(&self.inner)
    }
    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async { Err(ModelProviderFailure::Rejected) })
    }
    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.inner.project_wire_request(request)
    }
    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let usage = self
                .store
                .load_edit_phase_usage(self.project, self.run)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if usage.unobserved_calls != 1 || self.project_wire_request(request)? != projection {
                return Err(ModelProviderFailure::Rejected);
            }
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let text = if self.invalid_initial && call == 0 {
                "{}".to_owned()
            } else {
                json!({"version":4,"job":request.structured_output().ok_or(ModelProviderFailure::Rejected)?.value()["properties"]["job"]["const"],"intent":if self.unsafe_intent { "password=fixture-secret-value" } else { "Adjust the selected HTTP handler for the observed response mismatch." },"decision":"needEvidence"}).to_string()
            };
            let events = futures::stream::iter([
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(text)
                        .map_err(|_| ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(64), Some(16)),
                ))),
            ]);
            Ok(Box::pin(events) as ProviderEventStream<'a>)
        })
    }
}

#[test]
fn http_correction_plan_executor_debits_before_io_and_settles_before_exposing_intent() -> TestResult
{
    crate::run_native_libsql_test(async {
        for invalid_initial in [false, true] {
            let f = full_fixture(ollama_profile("qwen38-4k:latest")?, false).await?;
            let project = &f.http.fixture.project;
            let provider = ObservedPlanProvider {
                inner: OllamaModelProvider::for_projected_wire(
                    OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                    Arc::new(LocalOnlyOllamaEndpointPolicy),
                )?,
                store: &f.http.store,
                project,
                run: f.run.id(),
                invalid_initial,
                unsafe_intent: false,
                calls: AtomicUsize::new(0),
            };
            let now = f.run.updated_at().unix_millis() + 1;
            let elapsed = Instant::now();
            let first = ExecuteHttpCorrectionPlan::new(
                &provider,
                &WorkspaceAgentSourceReader,
                f.http.store.as_ref(),
            )
            .execute(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                at(now)?,
                ModelRequestTimeout::from_millis(10000)?,
                &ActiveControl,
            )
            .await;
            let report = if invalid_initial {
                assert!(matches!(
                    first,
                    Err(HttpCorrectionPlanExecutionFailure::InvalidReply)
                ));
                ExecuteHttpCorrectionPlan::new(
                    &provider,
                    &WorkspaceAgentSourceReader,
                    f.http.store.as_ref(),
                )
                .execute(
                    project,
                    &f.phase,
                    EditPhaseCallKind::StructuralRepair,
                    at(now + u64::try_from(elapsed.elapsed().as_millis())? + 1)?,
                    ModelRequestTimeout::from_millis(10000)?,
                    &ActiveControl,
                )
                .await?
            } else {
                first?
            };
            assert_eq!(
                report.proposal.decision(),
                HttpCorrectionPlanDecision::NeedEvidence
            );
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                1 + usize::from(invalid_initial)
            );
            assert_eq!(
                f.http
                    .store
                    .load_http_correction_plan(project, &f.phase)
                    .await?,
                Some(report.proposal)
            );
            assert_eq!(
                f.http
                    .store
                    .load_task_ledger(project, f.http.goal.task_id())
                    .await?,
                Some(f.stored)
            );
            assert_eq!(
                f.http.store.load_agent_run(project, f.run.id()).await?,
                Some(f.run)
            );
            drop(provider);
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_plan_physical_mismatch_cleanup_is_a_fresh_non_evidence_plan_input() -> TestResult
{
    crate::run_native_libsql_test(async {
        let f = full_fixture(ollama_profile("qwen38-4k:latest")?, true).await?;
        let d = f
            .http
            .store
            .load_loopback_mismatch(
                &f.http.fixture.project,
                f.run.id(),
                TOOL,
                AgentToolAttemptNumber::FIRST,
            )
            .await?
            .ok_or("actual diagnosis missing")?;
        assert_eq!(
            (
                d.mismatch().phase(),
                d.mismatch().check_index(),
                d.mismatch().observation().status(),
                d.mismatch().observation().body_bytes()
            ),
            (LoopbackStartupPhase::FreeExplicit, 0, 200, 13)
        );
        assert!(
            f.http
                .store
                .load_settled_loopback_evidence_id(
                    &f.http.fixture.project,
                    f.run.id(),
                    TOOL,
                    AgentToolAttemptNumber::FIRST
                )
                .await?
                .is_none()
        );
        assert_eq!(f.run.state(), AgentControllerState::Replan);
        assert!(
            f.http
                .store
                .load_http_correction_plan(&f.http.fixture.project, &f.phase)
                .await?
                .is_none()
        );
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_ports_reject_production42_and_leave_baseline_unchanged() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let baseline = Fixture::new()?;
        let layout = baseline
            .layout
            .prepare_project(baseline.project.worktree())?;
        let db = KnowledgeDatabase::open(&layout, &baseline.project).await?;
        let conn = db.connection_for_operation().await?;
        conn.execute_batch("CREATE TABLE retained_fixture(value TEXT NOT NULL); INSERT INTO retained_fixture VALUES('unchanged baseline');").await?;
        let production = LibsqlKnowledgeStore::open(&baseline.layout).await?;
        assert!(matches!(
            production
                .reserve_http_correction_plan(
                    &baseline.project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?
                )
                .await,
            Err(EditPhaseReservationFailure::UnsupportedSchema)
        ));
        assert_eq!(
            production
                .load_http_correction_plan(&baseline.project, &f.phase)
                .await,
            Err(EditPhaseReservationFailure::UnsupportedSchema)
        );
        let permit = HttpCorrectionPlanReservation {
            job: f.phase.job_digest(),
            kind: EditPhaseCallKind::Initial,
            wire: wire(&f.phase, EditPhaseCallKind::Initial)?,
            reserved_at: at(81)?,
            deadline: at(100)?,
        };
        assert_eq!(
            production
                .finish_http_correction_plan(
                    &baseline.project,
                    &f.phase,
                    permit,
                    observation(EditPhaseObservationOutcome::Unknown, 82)?,
                    None
                )
                .await,
            Err(EditPhaseReservationFailure::UnsupportedSchema)
        );
        let mut rows=conn.query("SELECT value,(SELECT COUNT(*) FROM sqlite_master WHERE name='http_correction_plan_reservations'),(SELECT user_version FROM pragma_user_version) FROM retained_fixture",()).await?;
        let row = rows.next().await?.ok_or("baseline missing")?;
        assert_eq!(row.get::<String>(0)?, "unchanged baseline");
        assert_eq!((row.get::<i64>(1)?, row.get::<i64>(2)?), (0, 42));
        drop((row, rows, conn, db, production));
        baseline.cleanup()?;
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_cold_loader_independently_rejects_tampered_proposal_and_anchor()
-> TestResult {
    crate::run_native_libsql_test(async {
        for kind in 0..5 {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let store = &f.http.store;
            let permit = store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?,
                )
                .await?;
            let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
            store
                .finish_http_correction_plan(
                    project,
                    &f.phase,
                    permit,
                    observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                    Some(proposal),
                )
                .await?;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute_batch("DROP TRIGGER http_correction_plan_update_guard;")
                .await?;
            match kind {
                0 => {
                    conn.execute(
                        "UPDATE http_correction_plan_reservations SET job_digest=?1",
                        libsql::params![vec![99_u8; 32]],
                    )
                    .await?;
                }
                1 => {
                    conn.execute(
                        "UPDATE http_correction_plan_reservations SET anchor=zeroblob(512)",
                        (),
                    )
                    .await?;
                }
                2 => {
                    conn.execute(
                        "UPDATE http_correction_plan_reservations SET proposal_digest=?1",
                        libsql::params![vec![99_u8; 32]],
                    )
                    .await?;
                }
                3 => {
                    let mut bytes = reply(&f.phase, "L6")?;
                    bytes.push(b'!');
                    conn.execute("UPDATE http_correction_plan_reservations SET proposal=?1,proposal_digest=?2",libsql::params![bytes.clone(),blake3::hash(&bytes).as_bytes().to_vec()]).await?;
                }
                _ => {
                    let mut old: serde_json::Value =
                        serde_json::from_slice(&reply(&f.phase, "L6")?)?;
                    old["version"] = json!(1);
                    old["decision"] = json!("correct");
                    old["range"] = json!([6, 6]);
                    let bytes = serde_json::to_vec(&old)?;
                    conn.execute("UPDATE http_correction_plan_reservations SET proposal=?1,proposal_digest=?2",libsql::params![bytes.clone(),blake3::hash(&bytes).as_bytes().to_vec()]).await?;
                }
            }
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            let reopened =
                LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            assert_eq!(
                reopened
                    .load_http_correction_plan(&f.http.fixture.project, &f.phase)
                    .await,
                Err(EditPhaseReservationFailure::InvalidStoredData)
            );
            drop(reopened);
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_ports_deny_v61_through_v65_without_relabeling_or_reopening_old_jobs()
-> TestResult {
    crate::run_native_libsql_test(async {
        for version in [61, 62, 63, 64, 65] {
            let f = current_fixture().await?;
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            // Owned negative fixture only: reconstruct preceding schemas, never an importer.
            conn.execute_batch(
                "ALTER TABLE http_correction_plan_reservations DROP COLUMN protocol_version;",
            )
            .await?;
            if version >= 62 {
                let protocol = match version {
                    62 => 2,
                    65 => 4,
                    _ => 3,
                };
                conn.execute_batch(&format!("ALTER TABLE http_correction_plan_reservations ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT {protocol} CHECK(protocol_version={protocol});")).await?;
            }
            conn.execute_batch(&format!("DELETE FROM schema_migrations WHERE version>{version}; PRAGMA user_version={version}; PRAGMA wal_checkpoint(TRUNCATE);")).await?;
            let path = db.path().to_path_buf();
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            let before = fs::read(&path)?;
            let reopened =
                LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            let project = &f.http.fixture.project;
            assert!(matches!(
                reopened
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::Initial,
                        wire(&f.phase, EditPhaseCallKind::Initial)?,
                        at(81)?
                    )
                    .await,
                Err(EditPhaseReservationFailure::Unavailable)
            ));
            assert_eq!(
                reopened.load_http_correction_plan(project, &f.phase).await,
                Err(EditPhaseReservationFailure::Unavailable)
            );
            let permit = HttpCorrectionPlanReservation {
                job: f.phase.job_digest(),
                kind: EditPhaseCallKind::Initial,
                wire: wire(&f.phase, EditPhaseCallKind::Initial)?,
                reserved_at: at(81)?,
                deadline: at(100)?,
            };
            assert_eq!(
                reopened
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        permit,
                        observation(EditPhaseObservationOutcome::Unknown, 82)?,
                        None
                    )
                    .await,
                Err(EditPhaseReservationFailure::Unavailable)
            );
            drop(reopened);
            assert_eq!(fs::read(&path)?, before);
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_protocol_tamper_is_rejected_warm_and_on_cold_open() -> TestResult {
    crate::run_native_libsql_test(async {
        for predecessor in [false, true] {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let first = f
                .http
                .store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?,
                )
                .await?;
            let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
            if predecessor {
                f.http
                    .store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        first,
                        observation(EditPhaseObservationOutcome::InvalidStructure, 82)?,
                        None,
                    )
                    .await?;
                let repair = f
                    .http
                    .store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::StructuralRepair,
                        wire(&f.phase, EditPhaseCallKind::StructuralRepair)?,
                        at(83)?,
                    )
                    .await?;
                f.http
                    .store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        repair,
                        observation(EditPhaseObservationOutcome::AcceptedReply, 84)?,
                        Some(proposal),
                    )
                    .await?;
            } else {
                f.http
                    .store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        first,
                        observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                        Some(proposal),
                    )
                    .await?;
            }
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute_batch("DROP TRIGGER http_correction_plan_update_guard; PRAGMA ignore_check_constraints=ON;").await?;
            conn.execute("UPDATE http_correction_plan_reservations SET protocol_version=1 WHERE call_kind='initial'",()).await?;
            conn.execute_batch("PRAGMA ignore_check_constraints=OFF;")
                .await?;
            assert_eq!(
                f.http
                    .store
                    .load_http_correction_plan(project, &f.phase)
                    .await,
                Err(EditPhaseReservationFailure::InvalidStoredData)
            );
            assert_eq!(
                f.http
                    .store
                    .load_edit_phase_usage(project, f.run.id())
                    .await,
                Err(EditPhaseReservationFailure::InvalidStoredData)
            );
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            let reopened =
                LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            // Native cold integrity rejects the violated SQL CHECK before the phase loader runs.
            assert_eq!(
                reopened
                    .load_http_correction_plan(&f.http.fixture.project, &f.phase)
                    .await,
                Err(EditPhaseReservationFailure::Unavailable)
            );
            drop(reopened);
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_cold_loader_rejects_orphaned_or_forged_repair_lineage() -> TestResult {
    crate::run_native_libsql_test(async {
        for kind in 0..5 {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let initial = f
                .http
                .store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?,
                )
                .await?;
            f.http
                .store
                .finish_http_correction_plan(
                    project,
                    &f.phase,
                    initial,
                    observation(EditPhaseObservationOutcome::InvalidStructure, 82)?,
                    None,
                )
                .await?;
            let repair = f
                .http
                .store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::StructuralRepair,
                    wire(&f.phase, EditPhaseCallKind::StructuralRepair)?,
                    at(83)?,
                )
                .await?;
            let repair_wire = repair.wire;
            let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
            f.http
                .store
                .finish_http_correction_plan(
                    project,
                    &f.phase,
                    repair,
                    observation(EditPhaseObservationOutcome::AcceptedReply, 84)?,
                    Some(proposal.clone()),
                )
                .await?;
            assert_eq!(
                f.http
                    .store
                    .load_http_correction_plan(project, &f.phase)
                    .await?,
                Some(proposal)
            );
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute_batch("DROP TRIGGER http_correction_plan_update_guard; DROP TRIGGER http_correction_plan_delete_guard;").await?;
            match kind {
                0 => {
                    conn.execute(
                        "DELETE FROM http_correction_plan_reservations WHERE call_kind='initial'",
                        (),
                    )
                    .await?;
                }
                1 => {
                    conn.execute("UPDATE http_correction_plan_reservations SET outcome='rejected_reply' WHERE call_kind='initial'", ()).await?;
                }
                2 => {
                    conn.execute("UPDATE http_correction_plan_reservations SET wire_digest=?1 WHERE call_kind='initial'", libsql::params![repair_wire.digest().as_bytes().to_vec()]).await?;
                }
                3 => {
                    conn.execute("UPDATE http_correction_plan_reservations SET deadline=deadline+1 WHERE call_kind='initial'", ()).await?;
                }
                _ => {
                    conn.execute("UPDATE http_correction_plan_reservations SET worktree_id=?1 WHERE call_kind='structural_repair'", libsql::params![vec![99_u8;32]]).await?;
                }
            }
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            let reopened =
                LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            assert_eq!(
                reopened
                    .load_http_correction_plan(&f.http.fixture.project, &f.phase)
                    .await,
                Err(EditPhaseReservationFailure::InvalidStoredData)
            );
            drop(reopened);
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_plan_cold_loader_binds_original_run_deadline_and_reservation_floor() -> TestResult
{
    crate::run_native_libsql_test(async {
        let mut unexpected = Vec::new();
        for kind in 0..5 {
            let f = current_fixture().await?;
            let project = &f.http.fixture.project;
            let initial = f
                .http
                .store
                .reserve_http_correction_plan(
                    project,
                    &f.phase,
                    EditPhaseCallKind::Initial,
                    wire(&f.phase, EditPhaseCallKind::Initial)?,
                    at(81)?,
                )
                .await?;
            let repaired = matches!(kind, 1 | 3 | 4);
            let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
            if repaired {
                f.http
                    .store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        initial,
                        observation(EditPhaseObservationOutcome::InvalidStructure, 82)?,
                        None,
                    )
                    .await?;
                let repair = f
                    .http
                    .store
                    .reserve_http_correction_plan(
                        project,
                        &f.phase,
                        EditPhaseCallKind::StructuralRepair,
                        wire(&f.phase, EditPhaseCallKind::StructuralRepair)?,
                        at(83)?,
                    )
                    .await?;
                f.http
                    .store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        repair,
                        observation(EditPhaseObservationOutcome::AcceptedReply, 84)?,
                        Some(proposal.clone()),
                    )
                    .await?;
            } else {
                f.http
                    .store
                    .finish_http_correction_plan(
                        project,
                        &f.phase,
                        initial,
                        observation(EditPhaseObservationOutcome::AcceptedReply, 82)?,
                        Some(proposal.clone()),
                    )
                    .await?;
            }
            assert_eq!(
                f.http
                    .store
                    .load_http_correction_plan(project, &f.phase)
                    .await?,
                Some(proposal)
            );
            let db = f.http.fixture.candidate_database().await?;
            let conn = db.connection_for_operation().await?;
            conn.execute_batch("DROP TRIGGER http_correction_plan_update_guard;")
                .await?;
            match kind {
                0 | 1 => {
                    // Both initial and repair retain the same forged deadline, so equality alone is insufficient.
                    conn.execute(
                        "UPDATE http_correction_plan_reservations SET deadline=deadline+1",
                        (),
                    )
                    .await?;
                }
                2 | 3 => {
                    // The predecessor is ordered internally but predates the causal Replan tail at 80.
                    conn.execute("UPDATE http_correction_plan_reservations SET reserved_at=79 WHERE call_kind='initial'", ()).await?;
                }
                _ => {
                    conn.execute("UPDATE http_correction_plan_reservations SET reserved_at=77,finished_at=78 WHERE call_kind='initial'", ()).await?;
                    conn.execute("UPDATE http_correction_plan_reservations SET reserved_at=79 WHERE call_kind='structural_repair'", ()).await?;
                }
            }
            drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
            let reopened =
                LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
            let loaded = reopened
                .load_http_correction_plan(&f.http.fixture.project, &f.phase)
                .await;
            if loaded != Err(EditPhaseReservationFailure::InvalidStoredData) {
                unexpected.push((kind, format!("{loaded:?}")));
            }
            drop(reopened);
            f.http.fixture.cleanup()?;
        }
        assert!(
            unexpected.is_empty(),
            "cold temporal bindings accepted: {unexpected:?}"
        );
        Ok(())
    })
}

#[test]
fn http_correction_plan_intent_is_bounded_unverified_and_key_order_independent() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let mut value: serde_json::Value =
            serde_json::from_slice(&reply(&f.phase, "needEvidence")?)?;
        for intent in [
            "x".repeat(96),
            "\u{e9}".repeat(48),
            "\"".repeat(96),
            "All tests passed.".to_owned(),
        ] {
            value["intent"] = json!(intent);
            let proposal = f.phase.decode_proposal(&serde_json::to_vec(&value)?)?;
            assert_eq!(proposal.intent().as_str(), intent);
            assert_eq!(
                proposal.decision(),
                HttpCorrectionPlanDecision::NeedEvidence
            );
            assert_eq!(
                f.phase
                    .decode_proposal(&f.phase.encode_proposal(&proposal)?)?,
                proposal
            );
            assert!(!format!("{proposal:?}").contains(&intent));
        }
        for intent in [
            "".to_owned(),
            "  ".to_owned(),
            "x".repeat(97),
            "\u{e9}".repeat(49),
            "\u{1}".repeat(90),
            "Keep the first line\nthen add another".to_owned(),
            "Keep the first line\rthen add another".to_owned(),
            "Keep\tthe indentation".to_owned(),
        ] {
            value["intent"] = json!(intent);
            assert_eq!(
                f.phase.decode_proposal(&serde_json::to_vec(&value)?),
                Err(HttpCorrectionPlanFailure::InvalidReply)
            );
        }
        value["intent"] = json!("password=fixture-secret-value");
        let error = f
            .phase
            .decode_proposal(&serde_json::to_vec(&value)?)
            .err()
            .ok_or("unsafe intent accepted")?;
        assert_eq!(error, HttpCorrectionPlanFailure::UnsafeContent);
        assert!(!format!("{error:?}").contains("fixture-secret-value"));
        for intent in [json!(null), json!(1), json!([])] {
            value["intent"] = intent;
            assert_eq!(
                f.phase.decode_proposal(&serde_json::to_vec(&value)?),
                Err(HttpCorrectionPlanFailure::InvalidReply)
            );
        }
        value["intent"] = json!("Unverified public fixture intent.");
        let entries = [
            format!("\"intent\":{}", serde_json::to_string(&value["intent"])?),
            format!(
                "\"decision\":{}",
                serde_json::to_string(&value["decision"])?
            ),
            format!("\"job\":{}", serde_json::to_string(&value["job"])?),
            "\"version\":4".to_owned(),
        ];
        let mut count = 0;
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        if [a, b, c, d]
                            .into_iter()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            != 4
                        {
                            continue;
                        }
                        let raw = format!(
                            "{{{}}}",
                            [a, b, c, d].map(|i| entries[i].as_str()).join(",")
                        );
                        assert_eq!(
                            f.phase.decode_proposal(raw.as_bytes())?.intent().as_str(),
                            "Unverified public fixture intent."
                        );
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(count, 24);
        assert_eq!(
            f.http
                .store
                .load_task_ledger(&f.http.fixture.project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        assert_eq!(f.run.state(), AgentControllerState::Replan);
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_offered_context_is_bound_retained_and_never_read() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = full_fixture(ollama_profile("qwen38-4k:latest")?, false).await?;
        let offer = f
            .phase
            .anchor()
            .source_offer
            .as_ref()
            .ok_or("offer missing")?
            .clone();
        assert_eq!(offer.revision(), &f.phase.anchor().source);
        assert_eq!(offer.inspection().start_line().get(), 1);
        assert_eq!(offer.inspection().line_count().get(), 21);
        assert_eq!(offer.range().end_byte(), 719);
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "S1")?)?;
        assert_eq!(
            proposal.decision(),
            HttpCorrectionPlanDecision::NeedOfferedSource(Box::new(offer))
        );
        let project = &f.http.fixture.project;
        let now = f.run.updated_at().unix_millis() + 1;
        let permit = f
            .http
            .store
            .reserve_http_correction_plan(
                project,
                &f.phase,
                EditPhaseCallKind::Initial,
                wire(&f.phase, EditPhaseCallKind::Initial)?,
                at(now)?,
            )
            .await?;
        f.http
            .store
            .finish_http_correction_plan(
                project,
                &f.phase,
                permit,
                observation(EditPhaseObservationOutcome::AcceptedReply, now + 1)?,
                Some(proposal.clone()),
            )
            .await?;
        let db = f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        {
            let mut rows = conn
                .query("SELECT COUNT(*) FROM source_read_acquisitions", ())
                .await?;
            assert_eq!(rows.next().await?.ok_or("source count")?.get::<i64>(0)?, 0);
        }
        assert_eq!(
            f.http.store.load_agent_run(project, f.run.id()).await?,
            Some(f.run.clone())
        );
        assert_eq!(
            f.http
                .store
                .load_task_ledger(project, f.http.goal.task_id())
                .await?,
            Some(f.stored)
        );
        drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&f.http.fixture.layout).await?;
        assert_eq!(
            cold.load_http_correction_plan(&f.http.fixture.project, &f.phase)
                .await?,
            Some(proposal)
        );
        drop(cold);
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_executor_rejects_unsafe_public_intent_without_semantic_repair() -> TestResult
{
    crate::run_native_libsql_test(async {
        let f = full_fixture(ollama_profile("qwen38-4k:latest")?, false).await?;
        let project = &f.http.fixture.project;
        let provider = ObservedPlanProvider {
            inner: OllamaModelProvider::for_projected_wire(
                OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                Arc::new(LocalOnlyOllamaEndpointPolicy),
            )?,
            store: &f.http.store,
            project,
            run: f.run.id(),
            invalid_initial: false,
            unsafe_intent: true,
            calls: AtomicUsize::new(0),
        };
        let now = f.run.updated_at().unix_millis() + 1;
        let started = Instant::now();
        let result = ExecuteHttpCorrectionPlan::new(
            &provider,
            &WorkspaceAgentSourceReader,
            f.http.store.as_ref(),
        )
        .execute(
            project,
            &f.phase,
            EditPhaseCallKind::Initial,
            at(now)?,
            ModelRequestTimeout::from_millis(10000)?,
            &ActiveControl,
        )
        .await;
        assert!(matches!(
            result,
            Err(HttpCorrectionPlanExecutionFailure::UnsafeReply)
        ));
        assert!(
            f.http
                .store
                .load_http_correction_plan(project, &f.phase)
                .await?
                .is_none()
        );
        let repair = ExecuteHttpCorrectionPlan::new(
            &provider,
            &WorkspaceAgentSourceReader,
            f.http.store.as_ref(),
        )
        .execute(
            project,
            &f.phase,
            EditPhaseCallKind::StructuralRepair,
            at(now + u64::try_from(started.elapsed().as_millis())? + 1)?,
            ModelRequestTimeout::from_millis(10000)?,
            &ActiveControl,
        )
        .await;
        assert!(matches!(
            repair,
            Err(HttpCorrectionPlanExecutionFailure::Reservation(
                EditPhaseReservationFailure::RepairNotAvailable
            ))
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let db = f.http.fixture.candidate_database().await?;
        let conn = db.connection_for_operation().await?;
        {
            let mut rows=conn.query("SELECT outcome,proposal,proposal_digest FROM http_correction_plan_reservations",()).await?;
            let row = rows.next().await?.ok_or("settled row")?;
            assert_eq!(row.get::<String>(0)?, "rejected_reply");
            assert_eq!(row.get::<Option<Vec<u8>>>(1)?, None);
            assert_eq!(row.get::<Option<Vec<u8>>>(2)?, None);
        }
        let usage = f
            .http
            .store
            .load_edit_phase_usage(project, f.run.id())
            .await?;
        assert_eq!(
            (usage.calls, usage.repairs, usage.unobserved_calls),
            (1, 0, 0)
        );
        drop(provider);
        drop((conn, db, f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}

#[test]
fn http_correction_plan_offer_cannot_rebind_foreign_scope_or_repeat_the_supplied_page() -> TestResult
{
    crate::run_native_libsql_test(async {
        let f = full_fixture(ollama_profile("qwen38-4k:latest")?, false).await?;
        let foreign = full_fixture(ollama_profile("qwen38-4k:latest")?, false).await?;
        let project = &f.http.fixture.project;
        let published = f
            .http
            .store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("publication missing")?;
        let diagnostic = f
            .http
            .store
            .load_loopback_mismatch(project, f.run.id(), TOOL, AgentToolAttemptNumber::FIRST)
            .await?
            .ok_or("diagnostic missing")?;
        let page = WorkspaceAgentSourceReader
            .read_page(
                project,
                diagnostic.script(),
                &AgentFileInspection::new(
                    diagnostic.script().path().clone(),
                    AgentFileStartLine::new(22)?,
                    AgentFileLineCount::new(10)?,
                ),
                &ActiveControl,
            )
            .await?;
        let foreign_offer = foreign
            .phase
            .anchor()
            .source_offer
            .clone()
            .ok_or("foreign offer missing")?;
        assert!(matches!(
            PreparedHttpCorrectionPlan::new(
                project,
                &f.http.goal,
                &f.stored,
                &f.run,
                &published,
                f.http.model.clone(),
                &diagnostic,
                &page,
                Some(foreign_offer)
            ),
            Err(HttpCorrectionPlanFailure::Binding)
        ));
        let seed = TaskLensSeedText::try_from_string("Current script context".to_owned())?;
        let lens = CompileTaskLens::new(
            f.http.store.as_ref(),
            f.http.store.as_ref(),
            f.http.store.as_ref(),
        )
        .execute(
            project,
            TaskLensSeedSet::new(
                seed.clone(),
                seed,
                vec![TaskLensSeed::ExplicitPath(
                    diagnostic.script().path().clone(),
                )],
            )?,
            TaskLensTokenBudget::new(4096)?,
            &ActiveControl,
        )
        .await?;
        let bound = BoundEditSource::for_file_span(
            project,
            &published,
            &lens,
            &EvidenceRef::new(page.revision().clone(), page.range()),
        )?;
        let repeated =
            HttpCorrectionPlanSourceOffer::from_bound_source(project, &published, &bound)?;
        assert!(matches!(
            PreparedHttpCorrectionPlan::new(
                project,
                &f.http.goal,
                &f.stored,
                &f.run,
                &published,
                f.http.model.clone(),
                &diagnostic,
                &page,
                Some(repeated)
            ),
            Err(HttpCorrectionPlanFailure::Binding)
        ));
        drop((
            foreign.http.compiler,
            foreign.http.refresh,
            foreign.http.store,
        ));
        foreign.http.fixture.cleanup()?;
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}
