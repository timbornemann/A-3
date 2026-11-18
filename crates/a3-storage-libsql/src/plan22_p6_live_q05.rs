//! One explicit local Ask measurement across a controlled Q05 source revision.

use super::super::middle_edit_tests::p4_live_edit::probe_projected_profile;
use super::*;
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Write;

const SCHEMA: &str = "a3.plan22.p6.q05-live.v1";
const TAG: &str = "qwen38-4k:latest";
const OUTPUT_ENV: &str = "A3_PLAN22_P6_Q05_RUN_DIR";
const FOLLOW_UP: &str = "Nach der zuvor beantworteten Frage hat der Supervisor config.py geändert. Welcher DEFAULT_BACKEND gilt jetzt? Verwende den aktuellen Stand und unterscheide ihn vom vorherigen.";

#[test]
#[ignore = "explicit isolated Ollama Ask measurement with a supervisor-owned directory"]
fn live_ollama_4k_ask_reads_both_q05_revisions() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = launch_root()?;
        write_once(
            &root,
            "native-start.json",
            &json!({"schema":SCHEMA,"tag":TAG}),
        )?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_live(&root))
    })
}

fn launch_root() -> TestResult<PathBuf> {
    let path = PathBuf::from(std::env::var_os(OUTPUT_ENV).ok_or("Ask run directory required")?);
    if !path.is_absolute() || !fs::symlink_metadata(&path)?.file_type().is_dir() {
        return Err("Ask run needs an owned absolute directory".into());
    }
    let root = fs::canonicalize(&path)?;
    let base = fs::canonicalize(std::env::current_dir()?.join("target/plan22-p6-runs"))?;
    if root.file_name().and_then(|name| name.to_str()) != Some("native")
        || root.parent().and_then(Path::parent) != Some(base.as_path())
        || fs::read_dir(&root)?.next().is_some()
    {
        return Err("Ask run needs a fresh native directory under its measurement root".into());
    }
    Ok(root)
}

async fn run_live(root: &Path) -> TestResult {
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_projected_profile(root, &provider, TAG, 4_096, 1_024).await?;
    let worktree = root.join("worktree");
    fs::create_dir(&worktree)?;
    initialize_git(&worktree)?;
    let source_path = worktree.join("config.py");
    fs::write(&source_path, OLD_SOURCE)?;
    let fixture = Fixture {
        root: root.to_path_buf(),
        layout: StorageLayout::prepare(root.join("appdata"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let project = &fixture.project;
    let (store, refresh, mut compiler, initial) = index_fixture(&fixture).await?;
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );
    let first = answer(&ask, project, ask_request(QUESTION, &profile, None)).await?;
    let first_receipt = turn_receipt(&first, project, initial.run(), None, OLD_SOURCE)?;
    assert_only_source(&worktree, OLD_SOURCE)?;
    write_once(root, "before.json", &first_receipt)?;

    fs::write(&source_path, NEW_SOURCE)?;
    let path = RepositoryPath::try_from_bytes(b"config.py".to_vec())?;
    let changed = refresh
        .execute(
            project,
            &RepositoryChangeBatch::incremental(vec![path])?,
            &mut compiler,
            &ActiveControl,
        )
        .await?;
    let updated_run = changed.published_index().run();
    assert_ne!(initial.run().snapshot_id(), updated_run.snapshot_id());
    let second = answer(
        &ask,
        project,
        ask_request(FOLLOW_UP, &profile, Some(&first)),
    )
    .await?;
    let second_receipt = turn_receipt(
        &second,
        project,
        updated_run,
        Some(initial.run().snapshot_id()),
        NEW_SOURCE,
    )?;
    assert_only_source(&worktree, NEW_SOURCE)?;
    assert_eq!(second.historical_parts().len(), first.parts().len());
    for (older, first_part) in second.historical_parts().iter().zip(first.parts()) {
        assert_eq!(older.text(), first_part.text());
        assert_eq!(older.sources(), first_part.sources());
    }
    write_comparison(root, &second)?;
    write_once(
        root,
        "native-final.json",
        &json!({
            "schema":SCHEMA, "before":first_receipt, "after":second_receipt,
            "controlled_source_edit":true, "privileged_actions":0,
        }),
    )
}

async fn answer(
    ask: &AnswerProjectQuestion<'_>,
    project: &ProjectIdentity,
    request: ProjectAskRequest<'_>,
) -> TestResult<ProjectAskAnswer> {
    let ProjectAskOutcome::Answer(answer) =
        ask.execute(Some(project), request, &ActiveControl).await?
    else {
        return Err("live Ask received no source-bound answer".into());
    };
    Ok(*answer)
}

fn turn_receipt(
    answer: &ProjectAskAnswer,
    project: &ProjectIdentity,
    run: a3_domain::IndexRunRecord,
    previous: Option<SnapshotId>,
    source: &str,
) -> TestResult<Value> {
    assert_eq!(answer.worktree_id(), project.worktree().id());
    assert_eq!(answer.index_run_id(), run.id());
    assert_eq!(answer.snapshot_id(), run.snapshot_id());
    assert_eq!(answer.previous_snapshot_id(), previous);
    let [original] = answer.delivered() else {
        return Err("live Ask needs one exact delivered original".into());
    };
    assert_eq!(original.revision().path().as_bytes(), b"config.py");
    let source_blake3 = blake3::hash(source.as_bytes());
    assert_eq!(
        original.revision().content_hash().as_bytes(),
        source_blake3.as_bytes()
    );
    let range = original.range();
    assert_eq!(range.start_byte(), 0);
    assert_eq!(usize::try_from(range.end_byte())?, source.len());
    assert!(!answer.parts().is_empty());
    let mut parts = Vec::with_capacity(answer.parts().len());
    for part in answer.parts() {
        assert_eq!(part.sources(), answer.delivered());
        let kind = match part.kind() {
            ProjectAskPartKind::Observation => "observation",
            ProjectAskPartKind::Interpretation => "interpretation",
            ProjectAskPartKind::Limit => "limit",
        };
        parts.push(json!({"kind":kind, "text":part.text()}));
    }
    let wire = answer
        .wire()
        .ok_or("live Ask answer has no observed model request")?;
    assert_eq!((wire.context_tokens(), wire.output_tokens()), (4096, 1024));
    assert_eq!(wire.thinking(), ModelWireThinking::Disabled);
    let usage = answer
        .usage()
        .ok_or("live Ask answer has no observed provider usage")?;
    Ok(json!({
        "snapshot_id": hex(run.snapshot_id().as_bytes()), "index_run_id": hex(run.id().as_bytes()),
        "previous_snapshot_id": previous.map(|snapshot| hex(snapshot.as_bytes())),
        "source_sha256": format!("{:x}", Sha256::digest(source.as_bytes())),
        "source_blake3": source_blake3.to_hex().to_string(),
        "range_start":range.start_byte(),"range_end":range.end_byte(),
        "wire_blake3":hex(wire.digest().as_bytes()),"request_bytes":wire.request_bytes(),
        "num_ctx":wire.context_tokens(),"num_predict":wire.output_tokens(),"think":false,
        "prompt_tokens":usage.prompt_tokens(),"output_tokens":usage.output_tokens(),
        "repaired":answer.repaired(),"parts":parts,
    }))
}

fn hex(bytes: &[u8; 32]) -> String {
    blake3::Hash::from_bytes(*bytes).to_hex().to_string()
}

fn write_comparison(root: &Path, answer: &ProjectAskAnswer) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("answer.txt"))?;
    for (label, parts) in [
        ("Aktueller Stand", answer.parts()),
        (
            "Vorheriger Stand (historische Quelle)",
            answer.historical_parts(),
        ),
    ] {
        writeln!(file, "{label}")?;
        for part in parts {
            writeln!(file, "- {} [config.py, Zeile 1]", part.text())?;
        }
    }
    file.sync_all()?;
    Ok(())
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
