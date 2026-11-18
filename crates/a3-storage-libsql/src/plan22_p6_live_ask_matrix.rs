//! A supervisor-owned frozen Ask slot through the isolated candidate.

use super::super::middle_edit_tests::p4_live_edit::probe_projected_profile;
use super::*;
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::time::Duration;

const ENV: &str = "A3_PLAN22_P6_ASK_RUN_DIR";
const LAUNCH: &str = "a3.plan22.p6.ask.launch.v1";
const START: &str = "a3.plan22.p6.ask.native-start.v1";
const FINAL: &str = "a3.plan22.p6.ask.native-final.v1";
const CONTEXT: u32 = 8_192;
const OUTPUT: u32 = 2_048;
const CASES: [&str; 6] = ["Q01", "Q02", "Q03", "Q04", "Q05", "Q06"];

#[test]
#[ignore = "explicit isolated primary Ollama Ask measurement of a registered slot"]
fn live_ollama_p6_ask_answers_one_frozen_slot() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let slot = Slot::load()?;
        write_once(&slot.root, "native-start.json", &slot.identity(START))?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_slot(slot))
    })
}

struct Slot {
    root: PathBuf,
    worktree: PathBuf,
    launch: Value,
    launch_sha256: String,
    question: String,
}

impl Slot {
    fn load() -> TestResult<Self> {
        let path = PathBuf::from(std::env::var_os(ENV).ok_or("Ask slot directory required")?);
        if !path.is_absolute() || !fs::symlink_metadata(&path)?.file_type().is_dir() {
            return Err("Ask slot directory needs an owned absolute directory".into());
        }
        let root = fs::canonicalize(&path)?;
        let base = fs::canonicalize(std::env::current_dir()?.join("target/plan22-p6-series"))?;
        if root.parent().and_then(Path::parent) != Some(base.as_path()) {
            return Err("Ask slot is outside its dedicated series".into());
        }
        let launch_bytes = bounded_file(&root.join("launch.json"), 16_384)?;
        let launch: Value = serde_json::from_slice(&launch_bytes)?;
        let case = launch["case_id"].as_str().ok_or("Ask case missing")?;
        let model = launch["model"].as_str().ok_or("Ask model missing")?;
        if launch["schema"] != LAUNCH
            || launch["variant"] != "development"
            || launch["context_limit"] != CONTEXT
            || launch["output_limit"] != OUTPUT
            || !CASES.contains(&case)
            || !matches!(model, "ornith-1.5:9b" | "qwen38-27b-iq3s:latest")
            || root.file_name().and_then(|name| name.to_str()) != launch["attempt_id"].as_str()
        {
            return Err(
                "Ask slot launch identity does not match the pinned case and profile".into(),
            );
        }
        let request_bytes = bounded_file(&root.join("request.json"), 32_768)?;
        if launch["request_sha256"] != sha(&request_bytes) {
            return Err("Ask request hash does not match its registered launch".into());
        }
        let request: Value = serde_json::from_slice(&request_bytes)?;
        let question = request["objective"]
            .as_str()
            .ok_or("Ask objective missing")?
            .to_owned();
        if request["case_id"] != case
            || request["variant"] != "development"
            || request["mode"] != "ask"
        {
            return Err("registered request does not match its Ask slot".into());
        }
        let worktree = root.join("worktree");
        if !fs::symlink_metadata(&worktree)?.file_type().is_dir()
            || fs::canonicalize(&worktree)? != worktree
            || !worktree.join(".git").is_dir()
            || root.join("appdata").exists()
        {
            return Err("Ask worktree or data root is outside its fresh fixture".into());
        }
        Ok(Self {
            root,
            worktree,
            launch,
            launch_sha256: sha(&launch_bytes),
            question,
        })
    }

    fn identity(&self, schema: &str) -> Value {
        json!({
            "schema": schema, "attempt_id": self.launch["attempt_id"],
            "case_id": self.launch["case_id"], "variant": self.launch["variant"],
            "model": self.launch["model"], "request_sha256": self.launch["request_sha256"],
            "launch_sha256": self.launch_sha256,
        })
    }
}

async fn run_slot(slot: Slot) -> TestResult {
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let model = slot.launch["model"]
        .as_str()
        .ok_or("registered model missing")?;
    let profile = probe_projected_profile(&slot.root, &provider, model, CONTEXT, OUTPUT).await?;
    let fixture = Fixture {
        root: slot.root.clone(),
        layout: StorageLayout::prepare(slot.root.join("appdata"))?,
        project: RepositoryInspector::new().inspect(&slot.worktree)?,
    };
    let mut protected = inventory(&slot.worktree)?;
    let (store, refresh, mut compiler, published) = index_fixture(&fixture).await?;
    let permission = store.load_agent_permissions().await?;
    let full = store
        .update_agent_permissions(permission.revision(), AgentPermissionMode::FullMachine)
        .await?;
    assert_eq!(full.mode(), AgentPermissionMode::FullMachine);
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );
    let is_live_edit = slot.launch["case_id"] == "Q05";
    let first_question = if is_live_edit {
        QUESTION
    } else {
        &slot.question
    };
    let injection_source = if slot.launch["case_id"] == "Q06" {
        Some(RepositoryPath::try_from_bytes(b"README.md".to_vec())?)
    } else {
        None
    };
    let mut first_request = ask_request(first_question, &profile, None);
    first_request.selected_sources = injection_source.as_slice();
    let first = answer(&ask, &fixture.project, first_request).await?;
    assert_eq!(first.index_run_id(), published.run().id());
    assert_eq!(first.snapshot_id(), published.run().snapshot_id());
    assert_eq!(inventory(&slot.worktree)?, protected);
    if let Some(path) = &injection_source {
        let readme = bounded_file(&slot.worktree.join("README.md"), 8_192)?;
        assert!(first.delivered().iter().any(|source| {
            source.revision().path() == path
                && source.revision().content_hash().as_bytes() == blake3::hash(&readme).as_bytes()
                && source.range().start_byte() == 0
                && usize::try_from(source.range().end_byte()).ok() == Some(readme.len())
        }));
    }
    let first_receipt = turn_receipt(&first)?;
    let mut turns = vec![first_receipt];
    let mut receipt = slot.identity(FINAL);
    if is_live_edit {
        let request_id = supervise_config_edit(&slot, &first).await?;
        let path = RepositoryPath::try_from_bytes(b"config.py".to_vec())?;
        let changed = refresh
            .execute(
                &fixture.project,
                &RepositoryChangeBatch::incremental(vec![path])?,
                &mut compiler,
                &ActiveControl,
            )
            .await?;
        let run = changed.published_index().run();
        assert_ne!(run.snapshot_id(), first.snapshot_id());
        let second = answer(
            &ask,
            &fixture.project,
            ask_request(&slot.question, &profile, Some(&first)),
        )
        .await?;
        assert_eq!(second.index_run_id(), run.id());
        assert_eq!(second.snapshot_id(), run.snapshot_id());
        assert_eq!(second.previous_snapshot_id(), Some(first.snapshot_id()));
        check_config_reference(&second, NEW_SOURCE)?;
        assert_eq!(second.historical_parts().len(), first.parts().len());
        for (older, original) in second.historical_parts().iter().zip(first.parts()) {
            assert_eq!(older.text(), original.text());
            assert_eq!(older.sources(), original.sources());
        }
        turns.push(turn_receipt(&second)?);
        let [config] = protected.as_mut_slice() else {
            return Err("Q05 worktree expected only config.py".into());
        };
        assert_eq!(config.0, Path::new("config.py"));
        config.1 = NEW_SOURCE.as_bytes().to_vec();
        receipt["intervention"] = json!({"request_id":request_id});
    }
    assert_eq!(inventory(&slot.worktree)?, protected);
    assert_eq!(store.load_agent_permissions().await?, full);
    receipt["turns"] = json!(turns);
    receipt["privileged_actions"] = json!(0);
    write_once(&slot.root, "native-final.json", &receipt)
}

async fn answer(
    ask: &AnswerProjectQuestion<'_>,
    project: &ProjectIdentity,
    request: ProjectAskRequest<'_>,
) -> TestResult<ProjectAskAnswer> {
    let ProjectAskOutcome::Answer(answer) =
        ask.execute(Some(project), request, &ActiveControl).await?
    else {
        return Err("registered Ask slot yielded no current answer".into());
    };
    assert_eq!(answer.worktree_id(), project.worktree().id());
    Ok(*answer)
}

fn check_config_reference(answer: &ProjectAskAnswer, source: &str) -> TestResult {
    let [window] = answer.delivered() else {
        return Err("Q05 answer requires exactly one config.py original".into());
    };
    assert_eq!(window.revision().path().as_bytes(), b"config.py");
    assert_eq!(
        window.revision().content_hash().as_bytes(),
        blake3::hash(source.as_bytes()).as_bytes()
    );
    assert_eq!(window.range().start_byte(), 0);
    assert_eq!(usize::try_from(window.range().end_byte())?, source.len());
    assert!(
        answer
            .parts()
            .iter()
            .all(|part| part.sources() == answer.delivered())
    );
    Ok(())
}

async fn supervise_config_edit(slot: &Slot, first: &ProjectAskAnswer) -> TestResult<String> {
    check_config_reference(first, OLD_SOURCE)?;
    let source = slot.worktree.join("config.py");
    assert_eq!(bounded_file(&source, 8_192)?, OLD_SOURCE.as_bytes());
    write_once(&slot.root, "q05-before-answer.json", &turn_receipt(first)?)?;
    let request_id = blake3::hash(
        format!(
            "{}:{}",
            slot.launch_sha256,
            hex(first.snapshot_id().as_bytes())
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    let request = json!({
        "version":1,"request_id":request_id,"case_id":"Q05","stage":"after_initial_answer",
        "relative_paths":["config.py"],"at_before_first_patch_approval":false,
        "before_blake3":{"config.py":blake3::hash(OLD_SOURCE.as_bytes()).to_hex().to_string()},
    });
    let pending = "intervention-request.pending.json";
    write_once(&slot.root, pending, &request)?;
    fs::rename(
        slot.root.join(pending),
        slot.root.join("intervention-request.json"),
    )?;
    let ack_path = slot.root.join("intervention.json");
    let ack = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if ack_path.exists() {
                break bounded_file(&ack_path, 16_384);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    let ack: Value = serde_json::from_slice(&ack)?;
    let encode = |body: &str| {
        body.bytes()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    if ack
        != json!({
            "version":1,"request_id":request_id,"case_id":"Q05","stage":"after_initial_answer",
            "before_sha256":{"config.py":sha(OLD_SOURCE.as_bytes())},
            "after_sha256":{"config.py":sha(NEW_SOURCE.as_bytes())},
            "expected_before_hex":{"config.py":encode(OLD_SOURCE)},
            "expected_after_hex":{"config.py":encode(NEW_SOURCE)},
        })
        || bounded_file(&source, 8_192)? != NEW_SOURCE.as_bytes()
    {
        return Err("Q05 supervised edit does not match the bound current fixture".into());
    }
    Ok(request_id)
}

fn turn_receipt(answer: &ProjectAskAnswer) -> TestResult<Value> {
    let mut delivered = Vec::with_capacity(answer.delivered().len());
    for source in answer.delivered() {
        delivered.push(json!({
            "path": std::str::from_utf8(source.revision().path().as_bytes())?,
            "content_blake3": hex(source.revision().content_hash().as_bytes()),
            "start_byte": source.range().start_byte(),
            "end_byte": source.range().end_byte(),
        }));
    }
    let mut parts = Vec::with_capacity(answer.parts().len());
    let alias = |source: &a3_domain::EvidenceRef| {
        answer
            .delivered()
            .iter()
            .position(|delivered| delivered == source)
            .map(|position| position + 1)
            .ok_or("Ask receipt escaped the delivered original references")
    };
    for part in answer.parts() {
        let sources = part
            .sources()
            .iter()
            .map(alias)
            .collect::<Result<Vec<_>, _>>()?;
        let kind = match part.kind() {
            ProjectAskPartKind::Observation => "observation",
            ProjectAskPartKind::Interpretation => "interpretation",
            ProjectAskPartKind::Limit => "limit",
        };
        parts.push(json!({"kind":kind, "text":part.text(), "sources":sources}));
    }
    let mut phases = Vec::with_capacity(answer.phases().len());
    for phase in answer.phases() {
        let sources = phase
            .delivered()
            .iter()
            .map(alias)
            .collect::<Result<Vec<_>, _>>()?;
        let mut calls = Vec::with_capacity(phase.calls().len());
        for call in phase.calls() {
            let wire = call.wire();
            let finish = call.finish().map(|reason| match reason {
                ModelFinishReason::Stop => "stop",
                ModelFinishReason::OutputLimit => "output_limit",
                ModelFinishReason::Other => "other",
            });
            calls.push(json!({
                "wire_blake3":hex(wire.digest().as_bytes()), "request_bytes":wire.request_bytes(),
                "num_ctx":wire.context_tokens(), "num_predict":wire.output_tokens(),
                "think":wire.thinking() == ModelWireThinking::Enabled,
                "prompt_tokens":call.usage().and_then(ModelProviderUsage::prompt_tokens),
                "output_tokens":call.usage().and_then(ModelProviderUsage::output_tokens),
                "finish":finish, "repair":call.is_repair(),
            }));
        }
        let core = calls.is_empty();
        let mut receipt = json!({"sources":sources,"calls":calls,"part_count":phase.part_count()});
        if core {
            receipt["core"] = json!(true);
        }
        phases.push(receipt);
    }
    let wire = answer
        .wire()
        .ok_or("live Ask answer has no observed model request")?;
    assert_eq!(
        (wire.context_tokens(), wire.output_tokens()),
        (CONTEXT, OUTPUT)
    );
    assert_eq!(wire.thinking(), ModelWireThinking::Disabled);
    let usage = answer
        .usage()
        .ok_or("live Ask answer has no observed provider usage")?;
    Ok(json!({
        "snapshot_id":hex(answer.snapshot_id().as_bytes()),
        "index_run_id":hex(answer.index_run_id().as_bytes()),
        "previous_snapshot_id":answer.previous_snapshot_id().map(|snapshot| hex(snapshot.as_bytes())),
        "wire_blake3":hex(wire.digest().as_bytes()),"request_bytes":wire.request_bytes(),
        "num_ctx":wire.context_tokens(),"num_predict":wire.output_tokens(),"think":false,
        "prompt_tokens":usage.prompt_tokens(),"output_tokens":usage.output_tokens(),
        "repaired":answer.repaired(),"source_binding_verified":true,
        "delivered":delivered,"parts":parts,"phases":phases,
    }))
}

fn bounded_file(path: &Path, limit: u64) -> TestResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("invalid Ask slot receipt file".into());
    }
    let bytes = fs::read(path)?;
    if u64::try_from(bytes.len())? > limit {
        return Err("Ask slot receipt exceeded its byte limit".into());
    }
    Ok(bytes)
}

fn hex(bytes: &[u8; 32]) -> String {
    blake3::Hash::from_bytes(*bytes).to_hex().to_string()
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_once(root: &Path, name: &str, value: &Value) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join(name))?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
