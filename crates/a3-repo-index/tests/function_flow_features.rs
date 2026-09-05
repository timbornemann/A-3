//! Real grammar tests for evaluation order, local value identity and partial knowledge.
use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    ContentHash, DiscoveredFileRoles, FileRevision, FlowStepKind, FlowValueKind,
    LanguageParseResult, Progress, RepositoryPath,
};
use a3_repo_index::{
    ParserPoolSize, PythonLanguageAdapter, RustLanguageAdapter, TypeScriptJavaScriptLanguageAdapter,
};
use std::error::Error;

#[test]
fn process_targets_require_literal_argv_known_cwd_and_library_identity()
-> Result<(), Box<dyn Error>> {
    let ts = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let py = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    for (adapter, path, source) in [
        (
            &ts as &dyn LanguageAdapter,
            "a.mjs",
            "import {execFileSync as run} from 'node:child_process'; function C(x){run('node',['x.mjs',x],{cwd:import.meta.dirname});}",
        ),
        (
            &py as &dyn LanguageAdapter,
            "a.py",
            "import subprocess\nimport os\ndef C(x):\n    subprocess.run(['python','x.py',x],cwd=os.path.dirname(__file__))\n",
        ),
        (
            &ts as &dyn LanguageAdapter,
            "a.cjs",
            "const {execFileSync:run}=require('node:child_process'); function C(x){run('node',['x.cjs',x],{cwd:__dirname});}",
        ),
    ] {
        let parsed = parse(adapter, path, source)?;
        let owner = parsed
            .symbols()
            .iter()
            .find(|s| s.name().as_str() == "C")
            .ok_or("C")?;
        let flow = parsed
            .function_flows()
            .iter()
            .find(|f| f.owner() == owner.id())
            .ok_or("flow")?;
        let process = flow
            .steps()
            .iter()
            .find_map(|s| s.process.as_ref())
            .ok_or("process")?;
        assert_eq!(process.mode, a3_domain::FlowProcessMode::Wait);
        assert!(
            matches!(process.target, Some(a3_domain::FlowProcessTarget::File(_))),
            "{path}: {process:?}"
        );
        assert_eq!(process.arguments.len(), 1);
        assert!(
            process.arguments[0].values.iter().any(|id| flow
                .values()
                .iter()
                .any(|v| v.id == *id && v.name.as_str() == "x")),
            "{path}: {process:?}"
        );
    }
    for source in [
        "import {spawn} from 'node:child_process'; function C(x){spawn('node',['x.js',x]);}",
        "import {spawn} from 'node:child_process'; function C(x){spawn('node',['../../escape.js',x],{cwd:import.meta.dirname});}",
        "import {spawn} from 'node:child_process'; function C(x){spawn('node',['x.js',x],{cwd:import.meta.dirname,shell:true});}",
        "import {spawn} from 'node:child_process'; function C(spawn){spawn('node',['x.js'],{cwd:import.meta.dirname});}",
        "import {spawn} from 'node:child_process'; spawn=replacement; function C(x){spawn('node',['x.js',x],{cwd:import.meta.dirname});}",
        "import * as cp from 'node:child_process'; cp.spawn=replacement; function C(x){cp.spawn('node',['x.js',x],{cwd:import.meta.dirname});}",
        "import {spawn} from 'node:child_process'; function C(x){spawn('node',['x.js',...x],{cwd:import.meta.dirname});}",
        "import {spawn} from 'node:child_process'; function C(x){spawn('node',['x.js',x],{cwd:import.meta.dirname,cwd:x});}",
    ] {
        let parsed = parse(&ts, "a.mjs", source)?;
        let process = parsed
            .function_flows()
            .iter()
            .flat_map(|f| f.steps())
            .find_map(|s| s.process.as_ref())
            .ok_or("unknown process")?;
        assert_eq!(process.mode, a3_domain::FlowProcessMode::Spawn);
        assert!(process.target.is_none(), "{source}");
    }
    Ok(())
}

#[test]
fn dynamic_imports_options_and_script_globals_do_not_claim_known_execution()
-> Result<(), Box<dyn Error>> {
    let ts = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let py = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    for (adapter, path, source) in [
        (
            &ts as &dyn LanguageAdapter,
            "a.cjs",
            "const require=replacement; const cp=require('child_process'); cp.spawn('node',['x.cjs'],{cwd:__dirname});",
        ),
        (
            &ts as &dyn LanguageAdapter,
            "a.mjs",
            "const process={argv:[]}; function D(){return process.argv[2];}",
        ),
        (
            &py as &dyn LanguageAdapter,
            "a.py",
            "import sys\nsys.argv=[]\ndef D():\n    return sys.argv[1]\n",
        ),
        (
            &py as &dyn LanguageAdapter,
            "a.py",
            "import subprocess\nimport os\nos=replacement\nsubprocess.run(['python','x.py'],cwd=os.path.dirname(__file__))\n",
        ),
        (
            &py as &dyn LanguageAdapter,
            "a.py",
            "import subprocess\nimport os\nsubprocess.run(['python','x.py'],cwd=os.path.dirname(__file__),**options)\n",
        ),
    ] {
        let parsed = parse(adapter, path, source)?;
        assert!(
            !parsed
                .function_flows()
                .iter()
                .flat_map(|f| f.steps())
                .filter_map(|s| s.process.as_ref())
                .any(|p| p.target.is_some()),
            "{source}"
        );
        assert!(
            !parsed
                .function_flows()
                .iter()
                .flat_map(|f| f.values())
                .any(|v| v.script_argument.is_some()),
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn handlers_are_separate_and_finally_survives_return() -> Result<(), Box<dyn Error>> {
    let ts = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let py = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let rust = RustLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    for (adapter, path, source, names) in [
        (
            &ts as &dyn LanguageAdapter,
            "a.ts",
            "function A(){try{return B();}catch(e){C();}finally{D();}}",
            vec!["B", "C", "D"],
        ),
        (
            &py as &dyn LanguageAdapter,
            "a.py",
            "def A():\n    try:\n        return B()\n    except Exception:\n        C()\n    finally:\n        D()\n",
            vec!["B", "C", "D"],
        ),
        (
            &rust as &dyn LanguageAdapter,
            "a.rs",
            "fn A(x:i32){match x {0=>B(),_=>C()};}",
            vec!["B", "C"],
        ),
        (
            &ts as &dyn LanguageAdapter,
            "a.ts",
            "function A(x){switch(x){case 0:B();break;default:C();}}",
            vec!["B", "C"],
        ),
    ] {
        let parsed = parse(adapter, path, source)?;
        let owner = parsed
            .symbols()
            .iter()
            .find(|s| s.name().as_str() == "A")
            .ok_or("A")?;
        let flow = parsed
            .function_flows()
            .iter()
            .find(|f| f.owner() == owner.id())
            .ok_or("flow")?;
        let calls = flow
            .steps()
            .iter()
            .filter(|s| s.kind == FlowStepKind::Call)
            .collect::<Vec<_>>();
        assert_eq!(
            calls
                .iter()
                .filter_map(|s| s.name.as_ref().map(|n| n.as_str()))
                .collect::<Vec<_>>(),
            names,
            "{path}"
        );
        assert_ne!(calls[0].parent, calls[1].parent, "alternatives in {path}");
        assert!(!flow.gaps().is_empty());
    }
    Ok(())
}

#[test]
fn manifest_chains_are_success_conditional_and_compile_is_not_execution()
-> Result<(), Box<dyn Error>> {
    let parsed = parse(
        &TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?,
        "package.json",
        r#"{"scripts":{"start":"tsc && node x.js && pnpm run next","next":"python d.py","unsafe":"node x.js | sh","dynamic":"node $TARGET"}}"#,
    )?;
    for name in ["start", "next", "unsafe", "dynamic"] {
        let owner = parsed
            .symbols()
            .iter()
            .find(|s| s.name().as_str() == format!("scripts:{name}"))
            .ok_or("script")?;
        let flow = parsed
            .function_flows()
            .iter()
            .find(|f| f.owner() == owner.id())
            .ok_or("flow")?;
        match name {
            "start" => {
                assert_eq!(flow.steps().len(), 5);
                assert_eq!(
                    flow.steps()[0].process.as_ref().map(|p| p.mode),
                    Some(a3_domain::FlowProcessMode::CompileOnly)
                );
                assert_eq!(flow.steps()[2].parent, Some(flow.steps()[1].id));
                assert_eq!(flow.steps()[4].parent, Some(flow.steps()[3].id));
            }
            "next" => assert_eq!(flow.steps().len(), 1),
            _ => {
                assert!(flow.steps().is_empty());
                assert!(!flow.gaps().is_empty());
            }
        }
    }
    Ok(())
}

#[test]
fn rust_tail_return_uses_only_the_final_expression() -> Result<(), Box<dyn Error>> {
    let parsed = parse(
        &RustLanguageAdapter::new(ParserPoolSize::new(1)?)?,
        "a.rs",
        "fn a(first:i32,last:i32)->i32 { noise(first); last }",
    )?;
    let owner = parsed
        .symbols()
        .iter()
        .find(|s| s.name().as_str() == "a" && s.kind() == a3_domain::SymbolKind::Function)
        .ok_or("missing a")?;
    let flow = parsed
        .function_flows()
        .iter()
        .find(|f| f.owner() == owner.id())
        .ok_or("missing flow")?;
    let returned = flow
        .steps()
        .iter()
        .find(|s| s.kind == FlowStepKind::Return)
        .ok_or("missing tail")?;
    let names = returned
        .inputs
        .iter()
        .filter_map(|id| flow.values().iter().find(|v| v.id == *id))
        .map(|v| v.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["last"]);
    Ok(())
}
#[test]
fn branches_stop_after_definite_exit_and_for_updates_follow_body() -> Result<(), Box<dyn Error>> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    for (source, expected) in [
        (
            "function a(x) { if(x) {return left();} else {return right();} unreachable(); }",
            vec!["left", "right"],
        ),
        (
            "function a() { for(initial(); condition(); update()) { body(); } }",
            vec!["initial", "condition", "body", "update"],
        ),
    ] {
        let parsed = parse(&adapter, "a.ts", source)?;
        let owner = parsed
            .symbols()
            .iter()
            .find(|s| s.name().as_str() == "a")
            .ok_or("missing a")?;
        let flow = parsed
            .function_flows()
            .iter()
            .find(|f| f.owner() == owner.id())
            .ok_or("missing flow")?;
        let calls = flow
            .steps()
            .iter()
            .filter(|s| s.kind == FlowStepKind::Call)
            .filter_map(|s| s.name.as_ref().map(|n| n.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(calls, expected);
    }
    Ok(())
}
#[test]
fn async_bodies_and_analysis_limits_remain_explicit() -> Result<(), Box<dyn Error>> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let parsed = parse(
        &adapter,
        "a.ts",
        "async function a(x) { return await B(x); }",
    )?;
    let owner = parsed
        .symbols()
        .iter()
        .find(|s| s.name().as_str() == "a")
        .ok_or("missing a")?;
    let flow = parsed
        .function_flows()
        .iter()
        .find(|f| f.owner() == owner.id())
        .ok_or("missing flow")?;
    assert_eq!(
        flow.steps().first().map(|s| s.kind),
        Some(FlowStepKind::Deferred)
    );
    assert!(flow.steps().iter().any(|s| s.kind == FlowStepKind::Await));
    let source = format!("function a() {{ B({}); }}", vec!["0"; 5000].join(","));
    let parsed = parse(&adapter, "a.ts", &source)?;
    let owner = parsed
        .symbols()
        .iter()
        .find(|s| s.name().as_str() == "a")
        .ok_or("missing a")?;
    let flow = parsed
        .function_flows()
        .iter()
        .find(|f| f.owner() == owner.id())
        .ok_or("missing flow")?;
    assert!(
        flow.gaps()
            .iter()
            .any(|g| g.kind == a3_domain::FlowGapKind::Limit)
    );
    assert!(flow.element_count() <= a3_domain::MAX_FUNCTION_FLOW_ELEMENTS);
    Ok(())
}

#[derive(Debug)]
struct Control;
impl LanguageParseControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}
fn parse(
    adapter: &dyn LanguageAdapter,
    path: &str,
    source: &str,
) -> Result<LanguageParseResult, Box<dyn Error>> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes(*blake3::hash(source.as_bytes()).as_bytes()),
    );
    Ok(adapter.parse(
        LanguageParseInput::new(&revision, source.as_bytes(), DiscoveredFileRoles::empty()),
        LanguageParsePolicy::v1(),
        &Control,
    )?)
}
#[test]
fn nested_calls_follow_evaluation_order_in_all_languages() -> Result<(), Box<dyn Error>> {
    let size = ParserPoolSize::new(1)?;
    let cases: Vec<(Box<dyn LanguageAdapter>, &str, &str)> = vec![
        (
            Box::new(TypeScriptJavaScriptLanguageAdapter::new(size)?),
            "a.ts",
            "function a(x) { C(B(x)); C(x); return x; never(); }",
        ),
        (
            Box::new(PythonLanguageAdapter::new(size)?),
            "a.py",
            "def a(x):\n    C(B(x))\n    C(x)\n    return x\n    never()\n",
        ),
        (
            Box::new(RustLanguageAdapter::new(size)?),
            "a.rs",
            "fn a(x: i32) { C(B(x)); C(x); return x; never(); }",
        ),
    ];
    for (adapter, path, source) in cases {
        let parsed = parse(adapter.as_ref(), path, source)?;
        let owner = parsed
            .symbols()
            .iter()
            .find(|s| s.name().as_str() == "a" && s.kind() == a3_domain::SymbolKind::Function)
            .ok_or("missing a")?;
        let flow = parsed
            .function_flows()
            .iter()
            .find(|f| f.owner() == owner.id())
            .ok_or("missing flow")?;
        let names = flow
            .steps()
            .iter()
            .filter(|s| s.kind == FlowStepKind::Call)
            .filter_map(|s| s.name.as_ref().map(|n| n.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["B", "C", "C"], "{path}");
        assert_eq!(
            flow.values()
                .iter()
                .filter(|v| v.kind == FlowValueKind::Parameter)
                .count(),
            1,
            "{path}"
        );
        assert_eq!(parsed, parse(adapter.as_ref(), path, source)?);
    }
    Ok(())
}
#[test]
fn shadowing_and_reassignment_keep_distinct_value_versions() -> Result<(), Box<dyn Error>> {
    let parsed = parse(
        &TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?,
        "a.ts",
        "function a(x) { let v = x; { let v = B(x); C(v); } v = D(x); return v; }",
    )?;
    let flow = parsed
        .function_flows()
        .iter()
        .find(|f| f.values().iter().any(|v| v.name.as_str() == "v"))
        .ok_or("missing values")?;
    let versions = flow
        .values()
        .iter()
        .filter(|v| v.name.as_str() == "v")
        .collect::<Vec<_>>();
    assert_eq!(versions.len(), 3);
    assert_ne!(versions[0].id, versions[1].id);
    let returned = flow
        .steps()
        .iter()
        .find(|s| s.kind == FlowStepKind::Return)
        .ok_or("missing return")?;
    assert_eq!(returned.inputs, vec![versions[2].id]);
    Ok(())
}
#[test]
fn declarations_do_not_execute_nested_callback_bodies() -> Result<(), Box<dyn Error>> {
    let parsed = parse(
        &TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?,
        "a.ts",
        "function a() { const later = () => hidden(); visible(); }",
    )?;
    let owner = parsed
        .symbols()
        .iter()
        .find(|s| s.name().as_str() == "a" && s.kind() == a3_domain::SymbolKind::Function)
        .ok_or("missing a")?;
    let flow = parsed
        .function_flows()
        .iter()
        .find(|f| f.owner() == owner.id())
        .ok_or("missing flow")?;
    assert_eq!(
        flow.steps()
            .iter()
            .filter(|s| s.kind == FlowStepKind::Call)
            .filter_map(|s| s.name.as_ref().map(|n| n.as_str()))
            .collect::<Vec<_>>(),
        vec!["visible"]
    );
    Ok(())
}
