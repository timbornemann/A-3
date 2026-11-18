use super::*;
use a3_domain::{
    AgentRunId, CanonicalDirectory, ContentHash, FileRevision, GitHead, GitReferenceName,
    LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpRecipe,
    LoopbackProbe, LoopbackProcessSettings, LoopbackStart, ProcessOutputLimit, ProcessTimeout,
    RepositoryId, RepositoryIdentity, SnapshotId, TaskStepId, VerificationSpecId, WorktreeAnchorId,
    WorktreeIdentity,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

struct Fixture {
    buffer: AgentApprovalBuffer,
    project: ProjectIdentity,
    context: AgentInspectionContext,
    prepared: PreparedLoopbackVerification,
}

impl Fixture {
    fn startup_prepared(&self) -> TestResult<PreparedLoopbackStartupCoverage> {
        let mut checks = self.prepared.recipe().checks().to_vec();
        let check = checks.first_mut().ok_or("missing GET check")?;
        *check = LoopbackHttpCheck::new(
            check.probe().clone(),
            check.status(),
            LoopbackExpectedBody::Exact {
                hash: ContentHash::from_bytes(*blake3::hash(b"Ready\n").as_bytes()),
                bytes: 6,
            },
            check.headers(),
        )?;
        let recipe = LoopbackHttpRecipe::new(
            self.prepared.recipe().script().clone(),
            LoopbackStart::Explicit,
            NonZeroU16::try_from(18_765)?,
            checks,
        )?
        .with_startup_coverage(0)?;
        Ok(PreparedLoopbackStartupCoverage::new(
            self.context.run_id(),
            self.project.worktree().id(),
            self.context.step_id(),
            self.context.snapshot_id(),
            self.prepared.script().clone(),
            recipe,
            a3_domain::LoopbackHttpProcess::from_spec(self.prepared.start())?.settings(),
        )?)
    }

    fn new() -> TestResult<Self> {
        let run = AgentRunId::from_bytes([1; 32]);
        let worktree = WorktreeId::from_bytes([2; 32]);
        let step = TaskStepId::from_bytes([3; 32]);
        let context = AgentInspectionContext::new(
            TaskId::from_bytes([4; 32]),
            run,
            step,
            VerificationSpecId::from_bytes([5; 32]),
            SnapshotId::from_bytes([6; 32]),
        );
        let repository = RepositoryId::from_bytes([7; 32]);
        let root = CanonicalDirectory::from_canonicalized(std::env::temp_dir())?;
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(repository, root.clone(), None),
            WorktreeIdentity::new(
                worktree,
                WorktreeAnchorId::from_bytes([8; 32]),
                repository,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let checks = [(LoopbackMethod::Get, 200), (LoopbackMethod::Post, 405)]
            .into_iter()
            .map(|(method, status)| {
                LoopbackHttpCheck::new(
                    LoopbackProbe::new(method, "/health".to_owned())?,
                    status,
                    LoopbackExpectedBody::AnyBounded,
                    LoopbackHeaderChecks::default(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let recipe = LoopbackHttpRecipe::new(
            path.clone(),
            LoopbackStart::Default,
            NonZeroU16::try_from(8765)?,
            checks,
        )?;
        let limit = ProcessOutputLimit::new(1024)?;
        let process = recipe.process_spec(
            run,
            worktree,
            step,
            LoopbackProcessSettings {
                timeout: ProcessTimeout::from_millis(4000)?,
                stdout_limit: limit,
                stderr_limit: limit,
                system_root: false,
            },
        )?;
        let prepared = PreparedLoopbackVerification::new(
            process,
            FileRevision::new(path, ContentHash::from_bytes([9; 32])),
            context.snapshot_id(),
            recipe,
        )?;
        let buffer = AgentApprovalBuffer::new();
        buffer.activate_project(&project);
        Ok(Self {
            buffer,
            project,
            context,
            prepared,
        })
    }

    fn request(&self, id: u8, probe: Option<usize>) -> TestResult<ApprovalRequest> {
        let action = match probe {
            None => self.prepared.start().policy_action(),
            Some(index) => self
                .prepared
                .probe_policy_action(index)
                .ok_or("missing probe")?,
        };
        Ok(ApprovalRequest::new(
            ApprovalRequestId::from_bytes([id; 32]),
            self.context.run_id(),
            &action,
            AgentRunTimestamp::from_unix_millis(100)?,
            AgentRunTimestamp::from_unix_millis(200)?,
        )?)
    }

    fn record(
        &self,
        request: &ApprovalRequest,
        probe: Option<usize>,
        previous: Option<(AgentApprovalRevision, ApprovalId)>,
    ) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure> {
        self.buffer.record_loopback_request(
            &self.project,
            self.context,
            request,
            PolicyDecisionReason::SystemApprovalRequired,
            &self.prepared,
            probe,
            previous,
        )
    }

    fn presentation(&self) -> TestResult<AgentApprovalPresentation> {
        Ok(self
            .buffer
            .presentation(&self.project, self.context.task_id())?
            .ok_or("missing presentation")?)
    }

    fn grant(
        &self,
        revision: AgentApprovalRevision,
        approval: ApprovalId,
    ) -> TestResult<AgentApprovalRevision> {
        Ok(self
            .buffer
            .mark_granted(&self.project, self.context.task_id(), revision, approval)?)
    }
}

#[test]
fn loopback_presents_default_start_and_exact_ordered_get_and_post() -> TestResult {
    let fixture = Fixture::new()?;
    let start = fixture.request(10, None)?;
    let get = fixture.request(11, Some(0))?;
    let post = fixture.request(12, Some(1))?;
    let start_revision = fixture.record(&start, None, None)?;
    let current = fixture.presentation()?;
    let AgentApprovalAction::LoopbackStart { process, port } = current.action() else {
        return Err("expected exact loopback start".into());
    };
    assert_eq!(process.arguments(), ["-B", "server.py"]);
    assert_eq!(port.get(), 8765);
    assert_eq!(current.revision(), start_revision);
    assert_eq!(
        fixture.record(&start, None, None),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );

    let start_grant = ApprovalId::from_bytes([13; 32]);
    assert_eq!(
        fixture.record(&get, Some(0), Some((start_revision, start_grant))),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    let granted_start = fixture.grant(start_revision, start_grant)?;
    let continuation = LoopbackApprovalContinuation {
        worktree_id: fixture.project.worktree().id(),
        revision: granted_start,
        context: Box::new(fixture.context),
        request_id: start.id(),
        approval_id: start_grant,
        ledger_revision: 1,
        ledger_store_version: TaskLedgerStoreVersion::INITIAL,
    };
    fixture
        .buffer
        .validate_loopback_continuation(&fixture.project, &continuation)?;
    assert_eq!(
        fixture.record(&post, Some(1), Some((granted_start, start_grant))),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    let get_revision = fixture.record(&get, Some(0), Some((granted_start, start_grant)))?;
    assert_eq!(
        fixture
            .buffer
            .validate_loopback_continuation(&fixture.project, &continuation),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    assert_eq!(
        fixture.presentation()?.action(),
        &AgentApprovalAction::LoopbackProbe {
            method: LoopbackMethod::Get,
            url: "http://127.0.0.1:8765/health".to_owned(),
        }
    );

    let get_grant = ApprovalId::from_bytes([14; 32]);
    let granted_get = fixture.grant(get_revision, get_grant)?;
    assert_eq!(
        fixture.record(&get, Some(1), Some((granted_get, get_grant))),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    fixture.record(&post, Some(1), Some((granted_get, get_grant)))?;
    assert_eq!(
        fixture.presentation()?.action(),
        &AgentApprovalAction::LoopbackProbe {
            method: LoopbackMethod::Post,
            url: "http://127.0.0.1:8765/health".to_owned(),
        }
    );
    Ok(())
}

#[test]
fn loopback_refuses_a_different_snapshot_or_grant_and_retains_the_slot() -> TestResult {
    let fixture = Fixture::new()?;
    let start = fixture.request(10, None)?;
    let get = fixture.request(11, Some(0))?;
    let revision = fixture.record(&start, None, None)?;
    let grant = ApprovalId::from_bytes([12; 32]);
    let granted = fixture.grant(revision, grant)?;
    let stale_context = AgentInspectionContext::new(
        fixture.context.task_id(),
        fixture.context.run_id(),
        fixture.context.step_id(),
        fixture.context.verification_spec_id(),
        SnapshotId::from_bytes([13; 32]),
    );
    assert_eq!(
        fixture.buffer.record_loopback_request(
            &fixture.project,
            stale_context,
            &get,
            PolicyDecisionReason::SystemApprovalRequired,
            &fixture.prepared,
            Some(0),
            Some((granted, grant)),
        ),
        Err(AgentApprovalSinkFailure::AnchorMismatch),
    );
    let wrong_grant = ApprovalId::from_bytes([14; 32]);
    assert_eq!(
        fixture.record(&get, Some(0), Some((granted, wrong_grant))),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    let retained = fixture.presentation()?;
    assert_eq!(retained.request_id(), start.id());
    assert_eq!(retained.revision(), granted);
    assert_eq!(
        retained.state(),
        AgentApprovalPresentationState::Granted(grant)
    );
    Ok(())
}

#[test]
fn loopback_discard_only_removes_the_exact_pending_slot() -> TestResult {
    let fixture = Fixture::new()?;
    let start = fixture.request(10, None)?;
    let replacement = fixture.request(11, None)?;
    let revision = fixture.record(&start, None, None)?;
    let wrong_revision = AgentApprovalRevision::new(revision.get() + 1)?;
    for (request_id, revision) in [(start.id(), wrong_revision), (replacement.id(), revision)] {
        fixture.buffer.discard_loopback_request(
            &fixture.project,
            fixture.context,
            request_id,
            revision,
        );
        assert_eq!(fixture.presentation()?.request_id(), start.id());
    }

    fixture.buffer.discard_loopback_request(
        &fixture.project,
        fixture.context,
        start.id(),
        revision,
    );
    assert!(
        fixture
            .buffer
            .presentation(&fixture.project, fixture.context.task_id())?
            .is_none()
    );
    let replacement_revision = fixture.record(&replacement, None, None)?;
    assert!(replacement_revision > revision);
    let grant = ApprovalId::from_bytes([12; 32]);
    let granted_revision = fixture.grant(replacement_revision, grant)?;
    fixture.buffer.discard_loopback_request(
        &fixture.project,
        fixture.context,
        replacement.id(),
        granted_revision,
    );
    assert_eq!(
        fixture.presentation()?.state(),
        AgentApprovalPresentationState::Granted(grant)
    );
    Ok(())
}

fn startup_request(
    fixture: &Fixture,
    prepared: &PreparedLoopbackStartupCoverage,
    stage: usize,
    id: u8,
) -> TestResult<ApprovalRequest> {
    let actions = prepared.policy_actions();
    let action = actions.get(stage).ok_or("missing startup stage")?;
    Ok(ApprovalRequest::new(
        ApprovalRequestId::from_bytes([id; 32]),
        fixture.context.run_id(),
        action,
        AgentRunTimestamp::from_unix_millis(100)?,
        AgentRunTimestamp::from_unix_millis(200)?,
    )?)
}

fn record_startup(
    fixture: &Fixture,
    prepared: &PreparedLoopbackStartupCoverage,
    request: &ApprovalRequest,
    stage: usize,
    previous: Option<(AgentApprovalRevision, ApprovalId)>,
) -> Result<AgentApprovalRevision, AgentApprovalSinkFailure> {
    fixture.buffer.record_startup_request(
        &fixture.project,
        fixture.context,
        request,
        PolicyDecisionReason::SystemApprovalRequired,
        prepared,
        stage,
        previous,
    )
}

fn startup_continuation(
    fixture: &Fixture,
    request: &ApprovalRequest,
    revision: AgentApprovalRevision,
    approval_id: ApprovalId,
) -> LoopbackApprovalContinuation {
    LoopbackApprovalContinuation {
        worktree_id: fixture.project.worktree().id(),
        revision,
        context: Box::new(fixture.context),
        request_id: request.id(),
        approval_id,
        ledger_revision: 1,
        ledger_store_version: TaskLedgerStoreVersion::INITIAL,
    }
}

#[test]
fn startup_presents_every_exact_phase_and_requires_a_fresh_continuation() -> TestResult {
    let fixture = Fixture::new()?;
    let prepared = fixture.startup_prepared()?;
    let actions = prepared.policy_actions();
    assert_eq!(actions.len(), 8);
    assert_ne!(actions[1].fingerprint(), actions[2].fingerprint());
    let mut previous = None;
    let mut previous_continuation = None;
    for (stage, action) in actions.iter().enumerate() {
        let request = startup_request(&fixture, &prepared, stage, 20 + u8::try_from(stage)?)?;
        let revision = record_startup(&fixture, &prepared, &request, stage, previous)?;
        if let Some(continuation) = previous_continuation.as_ref() {
            assert_eq!(
                fixture
                    .buffer
                    .validate_loopback_continuation(&fixture.project, continuation),
                Err(AgentApprovalSinkFailure::AnchorMismatch)
            );
        }
        let current = fixture.presentation()?;
        assert_eq!(current.state(), AgentApprovalPresentationState::Pending);
        assert_eq!(current.action_fingerprint(), action.fingerprint());
        assert_eq!(current.scope_digest(), action.scope_digest());
        match (stage, current.action()) {
            (
                0,
                AgentApprovalAction::LoopbackStartup {
                    phase,
                    process,
                    port,
                },
            ) => {
                assert_eq!(*phase, LoopbackStartupPhase::ReserveEndpoint);
                assert!(process.is_none());
                assert_eq!(port.get(), 18_765);
            }
            (
                1 | 2 | 5 | 7,
                AgentApprovalAction::LoopbackStartup {
                    phase,
                    process,
                    port,
                },
            ) => {
                let (expected_phase, argv, expected_port): (_, &[&str], _) = match stage {
                    1 => (
                        LoopbackStartupPhase::HeldExplicit,
                        &["-B", "server.py", "--host", "127.0.0.1", "--port", "18765"],
                        18_765,
                    ),
                    2 => (
                        LoopbackStartupPhase::FreeExplicit,
                        &["-B", "server.py", "--host", "127.0.0.1", "--port", "18765"],
                        18_765,
                    ),
                    5 => (LoopbackStartupPhase::Default, &["-B", "server.py"], 8765),
                    7 => (
                        LoopbackStartupPhase::InvalidPort,
                        &["-B", "server.py", "--port", "invalid"],
                        18_765,
                    ),
                    _ => return Err("unexpected process phase".into()),
                };
                assert_eq!(*phase, expected_phase);
                let process = process.as_ref().ok_or("startup process omitted")?;
                assert_eq!(process.arguments(), argv);
                assert_eq!(port.get(), expected_port);
                assert_eq!(
                    process.working_directory(),
                    &AgentApprovalWorkingDirectory::Root
                );
                assert_eq!(process.timeout_millis(), 4000);
                assert_eq!(process.stdout_limit(), 1024);
                assert_eq!(process.stderr_limit(), 1024);
            }
            (3 | 4 | 6, AgentApprovalAction::LoopbackProbe { method, url }) => {
                assert_eq!(
                    *method,
                    if stage == 4 {
                        LoopbackMethod::Post
                    } else {
                        LoopbackMethod::Get
                    }
                );
                let port = if stage == 6 { 8765 } else { 18_765 };
                assert_eq!(url, &format!("http://127.0.0.1:{port}/health"));
            }
            _ => return Err("unexpected startup approval projection".into()),
        }
        let approval = ApprovalId::from_bytes([100 + u8::try_from(stage)?; 32]);
        let pending = startup_continuation(&fixture, &request, revision, approval);
        assert_eq!(
            fixture
                .buffer
                .validate_loopback_continuation(&fixture.project, &pending),
            Err(AgentApprovalSinkFailure::AnchorMismatch)
        );
        let granted = fixture.grant(revision, approval)?;
        let continuation = startup_continuation(&fixture, &request, granted, approval);
        fixture
            .buffer
            .validate_loopback_continuation(&fixture.project, &continuation)?;
        previous = Some((granted, approval));
        previous_continuation = Some(continuation);
    }
    Ok(())
}

#[test]
fn startup_refuses_skips_stale_anchors_and_mismatched_grants_without_replacing_the_slot()
-> TestResult {
    let fixture = Fixture::new()?;
    let prepared = fixture.startup_prepared()?;
    let reserve = startup_request(&fixture, &prepared, 0, 20)?;
    let held = startup_request(&fixture, &prepared, 1, 21)?;
    let free = startup_request(&fixture, &prepared, 2, 22)?;
    let initial = record_startup(&fixture, &prepared, &reserve, 0, None)?;
    let approval = ApprovalId::from_bytes([100; 32]);
    assert_eq!(
        record_startup(&fixture, &prepared, &held, 1, Some((initial, approval))),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    let granted = fixture.grant(initial, approval)?;
    let retained = fixture.presentation()?;
    let wrong_approval = ApprovalId::from_bytes([101; 32]);
    for (request, stage, previous) in [
        (&reserve, 0, None),
        (&reserve, 0, Some((granted, approval))),
        (&held, 1, None),
        (&held, 1, Some((initial, approval))),
        (&held, 1, Some((granted, wrong_approval))),
        (&free, 1, Some((granted, approval))),
        (&free, 2, Some((granted, approval))),
        (&held, usize::MAX, Some((granted, approval))),
    ] {
        assert_eq!(
            record_startup(&fixture, &prepared, request, stage, previous),
            Err(AgentApprovalSinkFailure::AnchorMismatch)
        );
        assert_eq!(fixture.presentation()?, retained);
    }
    for changed in 0..5 {
        let context = fixture.context;
        let stale = AgentInspectionContext::new(
            if changed == 0 {
                TaskId::from_bytes([40; 32])
            } else {
                context.task_id()
            },
            if changed == 1 {
                AgentRunId::from_bytes([41; 32])
            } else {
                context.run_id()
            },
            if changed == 2 {
                TaskStepId::from_bytes([42; 32])
            } else {
                context.step_id()
            },
            if changed == 3 {
                VerificationSpecId::from_bytes([43; 32])
            } else {
                context.verification_spec_id()
            },
            if changed == 4 {
                SnapshotId::from_bytes([44; 32])
            } else {
                context.snapshot_id()
            },
        );
        assert_eq!(
            fixture.buffer.record_startup_request(
                &fixture.project,
                stale,
                &held,
                PolicyDecisionReason::SystemApprovalRequired,
                &prepared,
                1,
                Some((granted, approval)),
            ),
            Err(AgentApprovalSinkFailure::AnchorMismatch)
        );
        assert_eq!(fixture.presentation()?, retained);
    }
    let next = record_startup(&fixture, &prepared, &held, 1, Some((granted, approval)))?;
    assert!(next > granted);
    assert_eq!(
        fixture.presentation()?.state(),
        AgentApprovalPresentationState::Pending
    );
    Ok(())
}

#[test]
fn startup_discard_and_restart_do_not_create_continuation_authority() -> TestResult {
    let fixture = Fixture::new()?;
    let prepared = fixture.startup_prepared()?;
    let reserve = startup_request(&fixture, &prepared, 0, 20)?;
    let held = startup_request(&fixture, &prepared, 1, 21)?;
    let revision = record_startup(&fixture, &prepared, &reserve, 0, None)?;
    fixture
        .buffer
        .discard_loopback_request(&fixture.project, fixture.context, held.id(), revision);
    assert_eq!(fixture.presentation()?.request_id(), reserve.id());
    fixture.buffer.discard_loopback_request(
        &fixture.project,
        fixture.context,
        reserve.id(),
        revision,
    );
    assert!(
        fixture
            .buffer
            .presentation(&fixture.project, fixture.context.task_id())?
            .is_none()
    );
    let replacement = record_startup(&fixture, &prepared, &reserve, 0, None)?;
    let approval = ApprovalId::from_bytes([100; 32]);
    let granted = fixture.grant(replacement, approval)?;
    let continuation = startup_continuation(&fixture, &reserve, granted, approval);
    fixture.buffer.discard_loopback_request(
        &fixture.project,
        fixture.context,
        reserve.id(),
        granted,
    );
    fixture
        .buffer
        .validate_loopback_continuation(&fixture.project, &continuation)?;
    let reopened = AgentApprovalBuffer::new();
    assert_eq!(
        reopened.record_startup_request(
            &fixture.project,
            fixture.context,
            &reserve,
            PolicyDecisionReason::SystemApprovalRequired,
            &prepared,
            0,
            None,
        ),
        Err(AgentApprovalSinkFailure::InactiveProject)
    );
    reopened.activate_project(&fixture.project);
    assert_eq!(
        reopened.validate_loopback_continuation(&fixture.project, &continuation),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    assert_eq!(
        reopened.record_startup_request(
            &fixture.project,
            fixture.context,
            &held,
            PolicyDecisionReason::SystemApprovalRequired,
            &prepared,
            1,
            Some((granted, approval)),
        ),
        Err(AgentApprovalSinkFailure::AnchorMismatch)
    );
    Ok(())
}
