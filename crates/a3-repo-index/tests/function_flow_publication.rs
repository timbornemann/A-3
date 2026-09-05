//! Real compiler + durable Fast Index tests, including revision invalidation.
mod support;
use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeStore,
    RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryRescanReason,
};
use a3_domain::{FlowStepKind, Progress};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::{error::Error, sync::Arc};
use support::{TempDirectory, run_libsql_test};

#[test]
fn both_c_contexts_cross_a_real_script_boundary_without_mixing_arguments()
-> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write(
            "package.json",
            r#"{"name":"flow","scripts":{"start":"node a.mjs"}}"#,
        )?;
        repository.write("a.mjs","import {execFileSync as run} from 'node:child_process'; function A(left,right){B(left);C(right);} function B(y){C(y);} function C(z){run('node',['x.mjs',z],{cwd:import.meta.dirname});}")?;
        repository.write("x.mjs", "import {D} from './d.mjs'; D(process.argv[2]);")?;
        repository.write("d.mjs", "export function D(input){ return input; }")?;
        repository.write("a.py","import subprocess\nimport os\ndef A(left,right):\n    B(left)\n    C(right)\ndef B(y):\n    C(y)\ndef C(z):\n    subprocess.run(['python','x.py',z],cwd=os.path.dirname(__file__))\n")?;
        repository.write("x.py", "import sys\nfrom d import D\nD(sys.argv[1])\n")?;
        repository.write("d.py", "def D(input):\n    return input\n")?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        let batch = RepositoryChangeBatch::full_rescan(
            Vec::new(),
            RepositoryRescanReason::InitialObservation,
        )?;
        refresh
            .execute(&project, &batch, &mut compiler, &Control)
            .await?;
        let published = store
            .latest_published_index(&project, &Control)
            .await?
            .ok_or("index")?;
        let explorer = a3_application::ExploreFunctionFlows::new(store.clone());
        for path in [b"a.mjs".as_slice(), b"a.py".as_slice()] {
            let root = published
                .publication()
                .graph()
                .symbols()
                .iter()
                .find(|s| {
                    s.revision().path().as_bytes() == path && s.parsed().name().as_str() == "A"
                })
                .ok_or("A")?;
            for first in ["B", "C"] {
                let mut selection = a3_application::FunctionFlowSelection {
                    run_id: published.run().id(),
                    root: root.id(),
                    call_path: Vec::new(),
                };
                let mut next = Some(first);
                for _ in 0..5 {
                    let inspection = explorer
                        .inspect(&project, &selection, &Control)
                        .await?
                        .ok_or("inspection")?;
                    let frame = inspection.frames.last().ok_or("frame")?;
                    if frame.owner.parsed().name().as_str() == "D" {
                        break;
                    }
                    let call = frame
                        .flow
                        .analysis()
                        .steps()
                        .iter()
                        .find(|s| match next {
                            Some(name) => s.name.as_ref().is_some_and(|n| n.as_str() == name),
                            None => s.kind == FlowStepKind::Process,
                        })
                        .ok_or("call")?;
                    assert!(
                        frame
                            .flow
                            .calls()
                            .iter()
                            .any(|c| c.step == call.id && c.target.is_some()),
                        "{path:?} {:?}",
                        frame.flow
                    );
                    selection.call_path.push(call.id);
                    next = match next {
                        Some("B") => Some("C"),
                        Some("C") => None,
                        None => Some("D"),
                        _ => None,
                    };
                }
                let inspection = explorer
                    .inspect(&project, &selection, &Control)
                    .await?
                    .ok_or("D inspection")?;
                let frame = inspection.frames.last().ok_or("D frame")?;
                assert_eq!(frame.owner.parsed().name().as_str(), "D");
                let parameter = frame
                    .flow
                    .analysis()
                    .values()
                    .iter()
                    .find(|v| v.kind == a3_domain::FlowValueKind::Parameter)
                    .ok_or("parameter")?;
                let origins = explorer
                    .trace_value(
                        &project,
                        &selection,
                        parameter.id,
                        a3_application::FlowTraceDirection::Origins,
                        &Control,
                    )
                    .await?
                    .ok_or("origins")?;
                let expected = if first == "B" { "left" } else { "right" };
                let forbidden = if first == "B" { "right" } else { "left" };
                assert!(
                    origins
                        .nodes
                        .iter()
                        .any(|n| n.name == expected && n.address.call_path.is_empty()),
                    "{path:?} {first}: {origins:?}"
                );
                assert!(
                    !origins.nodes.iter().any(|n| n.name == forbidden),
                    "{path:?} {first}: {origins:?}"
                );
                assert!(
                    origins.evidence.iter().any(|e| e
                        .revision()
                        .path()
                        .as_bytes()
                        .starts_with(b"x."))
                );
                let document = explorer
                    .read_document(
                        &project,
                        selection.run_id,
                        &a3_domain::FunctionFlowReadRequest::new(
                            selection.root,
                            selection.call_path.clone(),
                            a3_domain::FunctionFlowReadView::Origins(parameter.id),
                        )?,
                        &Control,
                    )
                    .await?
                    .ok_or("document")?;
                assert!(document.text.contains("FUNCTION_FLOW_V1"));
                assert!(
                    document.evidence.iter().any(|e| e
                        .revision()
                        .path()
                        .as_bytes()
                        .starts_with(b"x."))
                );
                if first == "B" {
                    let root_selection = a3_application::FunctionFlowSelection {
                        run_id: selection.run_id,
                        root: selection.root,
                        call_path: Vec::new(),
                    };
                    let root_view = explorer
                        .inspect(&project, &root_selection, &Control)
                        .await?
                        .ok_or("root")?;
                    let input = root_view.frames[0]
                        .flow
                        .analysis()
                        .values()
                        .iter()
                        .find(|v| v.name.as_str() == "left")
                        .ok_or("left")?;
                    let uses = explorer
                        .trace_value(
                            &project,
                            &root_selection,
                            input.id,
                            a3_application::FlowTraceDirection::Uses,
                            &Control,
                        )
                        .await?
                        .ok_or("uses")?;
                    assert!(
                        uses.nodes.iter().any(|n| n.function_name == "D"),
                        "{path:?}: {uses:?}"
                    );
                    assert!(!uses.nodes.iter().any(|n| n.name == "right"));
                }
            }
        }
        repository.write("x.mjs", "import {D} from './d.mjs'; D('changed');")?;
        refresh
            .execute(&project, &batch, &mut compiler, &Control)
            .await?;
        let owner = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|s| s.parsed().name().as_str() == "A")
            .ok_or("old A")?;
        assert!(
            store
                .read_function_flow(&project, published.run().id(), owner, &Control)
                .await?
                .is_none()
        );
        Ok(())
    })
}

#[test]
fn imported_aliases_resolve_but_parameters_and_out_of_scope_functions_do_not()
-> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write("package.json", br#"{"name":"flows","private":true}"#)?;
        repository.write("Cargo.toml", "[package]\nname='flows'\nversion='0.1.0'\n")?;
        repository.write("a.ts","import { D as imported } from './b';\nfunction A(x) { return imported(x); }\nfunction S(imported) { return imported(); }\nfunction outer() { { function hidden() {} hidden(); } hidden(); }\nfunction Same(Same){return Same();}\nfunction Rebound(){Rebound=other;Rebound();}\nconst Arrow=(x)=>x; function ArrowCaller(x){return Arrow(x);}")?;
        repository.write("b.ts", "export function D(x) { return x; }")?;
        repository.write("a.py","from b import D as imported\ndef A(x):\n    return imported(x)\ndef S(imported):\n    return imported()\n")?;
        repository.write("b.py", "def D(x):\n    return x\n")?;
        repository.write("src/lib.rs","mod b; use crate::b::D as imported; fn A(x:i32)->i32 { imported(x) } fn S(imported:fn()->i32)->i32 { imported() }")?;
        repository.write("src/b.rs", "pub fn D(x:i32)->i32 { x }")?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &Control,
            )
            .await?;
        let published = store
            .latest_published_index(&project, &Control)
            .await?
            .ok_or("missing index")?;
        for owner in published.publication().graph().symbols() {
            let name = owner.parsed().name().as_str();
            if !matches!(
                name,
                "A" | "S" | "outer" | "Same" | "Rebound" | "ArrowCaller"
            ) {
                continue;
            }
            let flow = store
                .read_function_flow(&project, published.run().id(), owner, &Control)
                .await?
                .ok_or("missing flow")?;
            let targets = flow
                .calls()
                .iter()
                .map(|c| c.target.is_some())
                .collect::<Vec<_>>();
            assert_eq!(
                targets,
                match name {
                    "A" | "ArrowCaller" => vec![true],
                    "S" | "Same" | "Rebound" => vec![false],
                    _ => vec![true, false],
                },
                "{:?}",
                owner.revision().path()
            );
        }
        Ok(())
    })
}

#[derive(Debug)]
struct Control;
impl RepositoryIndexControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}
impl IndexPersistenceControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}
#[test]
fn published_flows_round_trip_and_old_runs_are_unreadable_after_refresh()
-> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write("package.json", br#"{"name":"flows","private":true}"#)?;
        repository.write("a.ts","function A(left,right) { const one = B(left); C(right); return one; }\nfunction B(y) { return C(y); }\nfunction C(z) { return z; }")?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let data = TempDirectory::new()?;
        let layout = StorageLayout::prepare(data.path().join("app-data"))?;
        let store = Arc::new(LibsqlKnowledgeStore::open(&layout).await?);
        store.record_opened_project(&project).await?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        let batch = RepositoryChangeBatch::full_rescan(
            Vec::new(),
            RepositoryRescanReason::InitialObservation,
        )?;
        refresh
            .execute(&project, &batch, &mut compiler, &Control)
            .await?;
        let published = store
            .latest_published_index(&project, &Control)
            .await?
            .ok_or("missing publication")?;
        let owner = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|s| s.parsed().name().as_str() == "A")
            .ok_or("missing A")?;
        let flow = store
            .read_function_flow(&project, published.run().id(), owner, &Control)
            .await?
            .ok_or("missing flow")?;
        let deep_map = a3_application::PublishedIndexDeepMapReadTools::new(store.clone());
        let observation = a3_application::DeepMapReadTools::inspect(
            &deep_map,
            &project,
            published.run().snapshot_id(),
            &a3_domain::ExploreTarget::Symbol(owner.id()),
            a3_application::DeepMapReadTimeout::DEFAULT,
            &Control,
        )
        .await?;
        assert!(observation.preview().contains("FUNCTION_FLOW_V1"));
        assert!(observation.evidence_ids().contains(
            &a3_domain::ModuleCardEvidenceId::for_file_revision_v1(owner.revision())
        ));
        assert_eq!(
            flow.analysis()
                .steps()
                .iter()
                .filter(|s| s.kind == FlowStepKind::Call)
                .count(),
            2
        );
        assert!(
            flow.calls().iter().all(|c| c.target.is_some()),
            "call occurrences must use the existing linker"
        );
        let explorer = a3_application::ExploreFunctionFlows::new(store.clone());
        let selection = a3_application::FunctionFlowSelection {
            run_id: published.run().id(),
            root: owner.id(),
            call_path: Vec::new(),
        };
        let returned = flow
            .analysis()
            .steps()
            .iter()
            .find(|s| s.kind == FlowStepKind::Return)
            .and_then(|s| s.inputs.first())
            .copied()
            .ok_or("missing return value")?;
        let trace = explorer
            .trace_value(
                &project,
                &selection,
                returned,
                a3_application::FlowTraceDirection::Origins,
                &Control,
            )
            .await?
            .ok_or("missing trace")?;
        assert!(!trace.truncated);
        assert!(
            trace
                .nodes
                .iter()
                .any(|n| n.address.call_path.is_empty() && n.name == "left")
        );
        assert!(
            !trace
                .nodes
                .iter()
                .any(|n| n.address.call_path.is_empty() && n.name == "right")
        );
        assert!(
            trace
                .nodes
                .iter()
                .any(|n| n.function_name == "C" && n.address.call_path.len() == 2)
        );
        let direct_c = flow
            .analysis()
            .steps()
            .iter()
            .find(|s| {
                s.kind == FlowStepKind::Call && s.name.as_ref().is_some_and(|n| n.as_str() == "C")
            })
            .and_then(|s| s.outputs.first())
            .copied()
            .ok_or("missing C result")?;
        let direct = explorer
            .trace_value(
                &project,
                &selection,
                direct_c,
                a3_application::FlowTraceDirection::Origins,
                &Control,
            )
            .await?
            .ok_or("missing direct trace")?;
        assert!(
            direct
                .nodes
                .iter()
                .any(|n| n.address.call_path.is_empty() && n.name == "right")
        );
        assert!(!direct.nodes.iter().any(|n| n.name == "left"));
        assert!(
            direct
                .nodes
                .iter()
                .any(|n| n.function_name == "C" && n.address.call_path.len() == 1)
        );
        let reopened = LibsqlKnowledgeStore::open(&layout).await?;
        assert_eq!(
            reopened
                .read_function_flow(&project, published.run().id(), owner, &Control)
                .await?,
            Some(flow)
        );
        repository.write("a.ts", "function A(x) { return x; }")?;
        refresh
            .execute(&project, &batch, &mut compiler, &Control)
            .await?;
        assert_eq!(
            store
                .read_function_flow(&project, published.run().id(), owner, &Control)
                .await?,
            None
        );
        store.rebuild_regenerable_index(&project, &Control).await?;
        assert_eq!(
            store
                .read_function_flow(&project, published.run().id(), owner, &Control)
                .await?,
            None
        );
        Ok(())
    })
}

impl a3_application::DeepMapReadControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }
}
