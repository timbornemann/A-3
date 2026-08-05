//! Python source and packaging behavior beyond the shared adapter contract.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    ContentHash, DiscoveredFileRoles, FileRevision, LanguageParseResult, ParseDiagnosticCode,
    Progress, RepositoryPath, SymbolKind, SymbolRole, SymbolVisibility, SyntaxRelationKind,
    SyntaxTarget,
};
use a3_repo_index::{ParserPoolSize, PythonLanguageAdapter};
use std::error::Error;

const SERVICE: &[u8] = include_bytes!("../../../fixtures/python-package/src/sample/service.py");
const TEST_SERVICE: &[u8] =
    include_bytes!("../../../fixtures/python-package/tests/test_service.py");
const CLI: &[u8] = include_bytes!("../../../fixtures/python-package/src/sample/cli.py");
const INVALID: &[u8] = include_bytes!("../../../fixtures/python-package/invalid.py");
const PYPROJECT: &[u8] = include_bytes!("../../../fixtures/python-package/pyproject.toml");
const SETUP_PY: &[u8] = include_bytes!("../../../fixtures/python-package/setup.py");
const SETUP_CFG: &[u8] = include_bytes!("../../../fixtures/python-package/setup.cfg");
const REQUIREMENTS: &[u8] = include_bytes!("../../../fixtures/python-package/requirements.txt");
const DEV_REQUIREMENTS: &[u8] =
    include_bytes!("../../../fixtures/python-package/requirements-dev.txt");

#[derive(Debug)]
struct SilentControl;

impl LanguageParseControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct CancelledControl;

impl LanguageParseControl for CancelledControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct RejectProgressControl;

impl LanguageParseControl for RejectProgressControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Err(LanguageParseControlError::Unavailable)
    }
}

#[test]
fn source_fixture_extracts_symbols_imports_exports_bases_and_uncertain_calls()
-> Result<(), Box<dyn Error>> {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    let result = parse(
        &adapter,
        b"fixtures/python-package/src/sample/service.py",
        SERVICE,
    )?;
    assert!(
        result.coverage().is_complete(),
        "{:?}",
        result.diagnostics()
    );
    assert!(result.diagnostics().is_empty());
    assert_eq!(
        result,
        parse(
            &adapter,
            b"fixtures/python-package/src/sample/service.py",
            SERVICE,
        )?
    );

    let module = symbol(&result, "service", SymbolKind::Module)?;
    assert!(module.documentation_range().is_some());
    let service = symbol(&result, "Service", SymbolKind::Class)?;
    assert_eq!(service.visibility(), SymbolVisibility::Public);
    assert!(service.documentation_range().is_some());
    assert_eq!(
        symbol(&result, "__init__", SymbolKind::Method)?.visibility(),
        SymbolVisibility::Public
    );
    assert!(
        symbol(&result, "run", SymbolKind::Method)?
            .documentation_range()
            .is_some()
    );
    assert_eq!(
        symbol(&result, "_helper", SymbolKind::Method)?.visibility(),
        SymbolVisibility::Protected
    );
    assert_eq!(
        symbol(&result, "__private", SymbolKind::Method)?.visibility(),
        SymbolVisibility::Private
    );
    assert_eq!(
        symbol(&result, "_internal_task", SymbolKind::Function)?.visibility(),
        SymbolVisibility::Internal
    );
    assert_symbol(&result, "build_service", SymbolKind::Function)?;

    let imports = unresolved_targets(&result, SyntaxRelationKind::Imports);
    assert!(imports.contains(&"__future__.annotations"));
    assert!(imports.contains(&"json"));
    assert!(imports.contains(&"pathlib"));
    assert!(imports.contains(&"collections.abc.Callable"), "{imports:?}");
    assert!(imports.contains(&".base.BaseService"), "{imports:?}");
    assert!(imports.contains(&".helpers.helper"), "{imports:?}");
    let exports = unresolved_targets(&result, SyntaxRelationKind::Exports);
    assert!(exports.contains(&"Service"));
    assert!(exports.contains(&"build_service"));
    assert!(unresolved_targets(&result, SyntaxRelationKind::Extends).contains(&"BaseService"));

    assert_call_confidence(&result, "Service", 7_000)?;
    assert_call_confidence(&result, "json.loads", 6_000)?;
    assert_call_confidence(&result, "self._callback", 6_000)?;
    assert_call_confidence(&result, "notify", 7_000)?;
    Ok(())
}

#[test]
fn pytest_unittest_and_main_guard_roles_are_evidence_bound() -> Result<(), Box<dyn Error>> {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let result = parse(
        &adapter,
        b"fixtures/python-package/tests/test_service.py",
        TEST_SERVICE,
    )?;
    assert!(
        result.coverage().is_complete(),
        "{:?}",
        result.diagnostics()
    );
    assert!(
        symbol(&result, "test_service", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Test)
    );
    for (name, kind) in [
        ("service", SymbolKind::Function),
        ("test_build", SymbolKind::Function),
        ("ServiceTests", SymbolKind::Class),
        ("test_run", SymbolKind::Method),
        ("TestPytestStyle", SymbolKind::Class),
        ("test_method", SymbolKind::Method),
    ] {
        assert!(
            symbol(&result, name, kind)?
                .roles()
                .contains(SymbolRole::Test)
        );
    }
    let tests = unresolved_targets(&result, SyntaxRelationKind::Tests);
    assert!(tests.contains(&"pytest"));
    assert!(tests.contains(&"unittest"));

    let cli = parse(
        &adapter,
        b"fixtures/python-package/src/sample/runner.py",
        CLI,
    )?;
    assert!(
        symbol(&cli, "runner", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Entrypoint)
    );
    Ok(())
}

#[test]
fn syntax_errors_wildcards_and_dynamic_calls_are_partial_and_isolated() -> Result<(), Box<dyn Error>>
{
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let invalid = parse(&adapter, b"src/invalid.py", INVALID)?;
    assert!(!invalid.coverage().is_complete());
    assert!(!invalid.diagnostics().is_empty());

    let wildcard = parse(&adapter, b"src/wildcard.py", b"from package import *\n")?;
    assert!(!wildcard.coverage().is_complete());
    assert!(unresolved_targets(&wildcard, SyntaxRelationKind::Imports).contains(&"package.*"));

    let dynamic = parse(&adapter, b"src/dynamic.py", b"factory()()\n")?;
    assert!(!dynamic.coverage().is_complete());
    assert!(unresolved_targets(&dynamic, SyntaxRelationKind::Calls).contains(&"factory"));
    assert!(
        dynamic
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == ParseDiagnosticCode::UnsupportedSyntax)
    );

    let recovered = parse(
        &adapter,
        b"src/recovered.py",
        b"def recovered():\n    return 1\n",
    )?;
    assert!(recovered.coverage().is_complete());
    Ok(())
}

#[test]
fn pyproject_extracts_pep621_build_poetry_and_pytest_metadata() -> Result<(), Box<dyn Error>> {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let result = parse(
        &adapter,
        b"fixtures/python-package/pyproject.toml",
        PYPROJECT,
    )?;
    assert!(
        result.coverage().is_complete(),
        "{:?}",
        result.diagnostics()
    );
    assert!(
        symbol(&result, "sample-python", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Entrypoint)
    );
    assert_symbol(&result, "default", SymbolKind::Module)?;

    let imports = unresolved_targets(&result, SyntaxRelationKind::Imports);
    assert!(imports.contains(&"requests"));
    assert!(imports.contains(&"typing-extensions"));
    assert!(imports.contains(&"sphinx"));
    let tests = unresolved_targets(&result, SyntaxRelationKind::Tests);
    assert!(tests.contains(&"pytest"));
    assert!(tests.contains(&"coverage"));
    assert!(tests.contains(&"pytest:testpath:tests"));
    let builds = unresolved_targets(&result, SyntaxRelationKind::Builds);
    assert!(builds.contains(&"setuptools"));
    assert!(builds.contains(&"wheel"));
    assert!(builds.contains(&"setuptools.build_meta"));
    let configures = unresolved_targets(&result, SyntaxRelationKind::Configures);
    assert!(configures.contains(&"sample.cli:main"));
    assert!(configures.contains(&"sample.service:Service"));
    assert!(configures.contains(&"pytest:python-file:test_*.py"));

    let poetry = parse(
        &adapter,
        b"pyproject.toml",
        br#"[tool.poetry]
name = "poetry-sample"
[tool.poetry.dependencies]
python = "^3.12"
httpx = "^0.28"
[tool.poetry.group.test.dependencies]
pytest = "^8"
[tool.poetry.scripts]
poetry-sample = "sample.cli:main"
"#,
    )?;
    assert!(
        poetry.coverage().is_complete(),
        "{:?}",
        poetry.diagnostics()
    );
    assert!(unresolved_targets(&poetry, SyntaxRelationKind::Imports).contains(&"httpx"));
    assert!(unresolved_targets(&poetry, SyntaxRelationKind::Tests).contains(&"pytest"));
    Ok(())
}

#[test]
fn setup_py_and_setup_cfg_extract_static_package_metadata() -> Result<(), Box<dyn Error>> {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let setup_py = parse(&adapter, b"fixtures/python-package/setup.py", SETUP_PY)?;
    assert!(
        setup_py.coverage().is_complete(),
        "{:?}",
        setup_py.diagnostics()
    );
    assert!(
        symbol(&setup_py, "legacy-sample", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Entrypoint)
    );
    assert_symbol(&setup_py, "legacy-sample", SymbolKind::Module)?;
    assert!(unresolved_targets(&setup_py, SyntaxRelationKind::Imports).contains(&"requests"));
    assert!(
        unresolved_targets(&setup_py, SyntaxRelationKind::Imports).contains(&"typing-extensions")
    );
    assert!(unresolved_targets(&setup_py, SyntaxRelationKind::Tests).contains(&"pytest"));
    assert!(unresolved_targets(&setup_py, SyntaxRelationKind::Builds).contains(&"package:sample"));
    assert!(
        unresolved_targets(&setup_py, SyntaxRelationKind::Configures).contains(&"sample.cli:main")
    );

    let setup_cfg = parse(&adapter, b"fixtures/python-package/setup.cfg", SETUP_CFG)?;
    assert!(
        setup_cfg.coverage().is_complete(),
        "{:?}",
        setup_cfg.diagnostics()
    );
    assert_symbol(&setup_cfg, "configured-sample", SymbolKind::Module)?;
    assert!(unresolved_targets(&setup_cfg, SyntaxRelationKind::Imports).contains(&"requests"));
    assert!(unresolved_targets(&setup_cfg, SyntaxRelationKind::Imports).contains(&"sphinx"));
    let tests = unresolved_targets(&setup_cfg, SyntaxRelationKind::Tests);
    assert!(tests.contains(&"pytest"));
    assert!(tests.contains(&"pytest-cov"));
    assert!(
        unresolved_targets(&setup_cfg, SyntaxRelationKind::Configures).contains(&"sample.cli:main")
    );

    let escaped = parse(
        &adapter,
        b"setup.py",
        b"from setuptools import setup\nsetup(name='escaped\\x2dname', packages=('pkg',))\n",
    )?;
    assert!(
        escaped.coverage().is_complete(),
        "{:?}",
        escaped.diagnostics()
    );
    assert_symbol(&escaped, "escaped-name", SymbolKind::Module)?;

    let dynamic = parse(
        &adapter,
        b"setup.py",
        b"from setuptools import setup\nsetup(name=PACKAGE_NAME, install_requires=DEPENDENCIES)\n",
    )?;
    assert!(!dynamic.coverage().is_complete());
    Ok(())
}

#[test]
fn requirements_and_manifest_failures_remain_bounded_and_safe() -> Result<(), Box<dyn Error>> {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let requirements = parse(
        &adapter,
        b"fixtures/python-package/requirements.txt",
        REQUIREMENTS,
    )?;
    assert!(
        requirements.coverage().is_complete(),
        "{:?}",
        requirements.diagnostics()
    );
    let imports = unresolved_targets(&requirements, SyntaxRelationKind::Imports);
    assert!(imports.contains(&"requests"));
    assert!(imports.contains(&"typing-extensions"));
    assert!(imports.contains(&"local-package"));
    assert!(
        file_targets(&requirements, SyntaxRelationKind::Builds)
            .contains(&"fixtures/python-package/requirements-dev.txt")
    );

    let development = parse(
        &adapter,
        b"fixtures/python-package/requirements-dev.txt",
        DEV_REQUIREMENTS,
    )?;
    assert!(
        symbol(&development, "requirements-dev", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Test)
    );
    assert!(unresolved_targets(&development, SyntaxRelationKind::Tests).contains(&"pytest"));

    let unsafe_include = parse(&adapter, b"requirements.txt", b"-r ../outside.txt\n")?;
    assert!(!unsafe_include.coverage().is_complete());
    assert!(file_targets(&unsafe_include, SyntaxRelationKind::Builds).is_empty());

    let option = parse(
        &adapter,
        b"requirements.txt",
        b"--index-url https://user:secret@example.invalid/simple\n",
    )?;
    assert!(!option.coverage().is_complete());
    assert!(unresolved_targets(&option, SyntaxRelationKind::Imports).is_empty());

    let invalid_encoding = parse(&adapter, b"setup.cfg", b"[metadata]\nname=bad\xff\n")?;
    assert!(!invalid_encoding.coverage().is_complete());
    assert!(
        invalid_encoding
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == ParseDiagnosticCode::InvalidEncoding)
    );

    let invalid_toml = parse(&adapter, b"pyproject.toml", b"[project\nname='bad'\n")?;
    assert!(!invalid_toml.coverage().is_complete());

    let oversized = vec![b' '; 512 * 1024 + 1];
    let oversized_requirements_revision = revision(b"requirements.txt", &oversized)?;
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &oversized_requirements_revision,
                &oversized,
                DiscoveredFileRoles::empty(),
            ),
            LanguageParsePolicy::v1(),
            &SilentControl,
        ),
        Err(LanguageParseFailure::InputTooLarge)
    );

    let oversized_setup = vec![b' '; 256 * 1024 + 1];
    let oversized_setup_revision = revision(b"setup.cfg", &oversized_setup)?;
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &oversized_setup_revision,
                &oversized_setup,
                DiscoveredFileRoles::empty(),
            ),
            LanguageParsePolicy::v1(),
            &SilentControl,
        ),
        Err(LanguageParseFailure::InputTooLarge)
    );

    let pyproject_revision = revision(b"pyproject.toml", PYPROJECT)?;
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(&pyproject_revision, PYPROJECT, DiscoveredFileRoles::empty(),),
            LanguageParsePolicy::v1(),
            &CancelledControl,
        ),
        Err(LanguageParseFailure::Cancelled)
    );

    let requirements_revision = revision(b"requirements.txt", REQUIREMENTS)?;
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(
                &requirements_revision,
                REQUIREMENTS,
                DiscoveredFileRoles::empty(),
            ),
            LanguageParsePolicy::v1(),
            &RejectProgressControl,
        ),
        Err(LanguageParseFailure::ProgressUnavailable)
    );
    Ok(())
}

fn parse(
    adapter: &PythonLanguageAdapter,
    path: &[u8],
    source: &[u8],
) -> Result<LanguageParseResult, Box<dyn Error>> {
    let revision = revision(path, source)?;
    Ok(adapter.parse(
        LanguageParseInput::new(&revision, source, DiscoveredFileRoles::empty()),
        LanguageParsePolicy::v1(),
        &SilentControl,
    )?)
}

fn revision(path: &[u8], source: &[u8]) -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(*blake3::hash(source).as_bytes()),
    ))
}

fn assert_symbol(
    result: &LanguageParseResult,
    name: &str,
    kind: SymbolKind,
) -> Result<(), Box<dyn Error>> {
    let _symbol = symbol(result, name, kind)?;
    Ok(())
}

fn symbol<'a>(
    result: &'a LanguageParseResult,
    name: &str,
    kind: SymbolKind,
) -> Result<&'a a3_domain::ParsedSymbol, Box<dyn Error>> {
    result
        .symbols()
        .iter()
        .find(|symbol| symbol.name().as_str() == name && symbol.kind() == kind)
        .ok_or_else(|| {
            format!(
                "missing {kind:?} symbol {name}; observed: {:?}",
                result.symbols()
            )
            .into()
        })
}

fn assert_call_confidence(
    result: &LanguageParseResult,
    target: &str,
    confidence: u16,
) -> Result<(), Box<dyn Error>> {
    let relation = result
        .relations()
        .iter()
        .find(|relation| {
            relation.kind() == SyntaxRelationKind::Calls
                && matches!(
                    relation.target(),
                    SyntaxTarget::Unresolved(reference) if reference.as_str() == target
                )
        })
        .ok_or_else(|| format!("missing call target {target}"))?;
    assert_eq!(relation.confidence().basis_points(), confidence);
    Ok(())
}

fn unresolved_targets(result: &LanguageParseResult, kind: SyntaxRelationKind) -> Vec<&str> {
    result
        .relations()
        .iter()
        .filter(|relation| relation.kind() == kind)
        .filter_map(|relation| match relation.target() {
            SyntaxTarget::Unresolved(reference) => Some(reference.as_str()),
            _ => None,
        })
        .collect()
}

fn file_targets(result: &LanguageParseResult, kind: SyntaxRelationKind) -> Vec<&str> {
    result
        .relations()
        .iter()
        .filter(|relation| relation.kind() == kind)
        .filter_map(|relation| match relation.target() {
            SyntaxTarget::File(path) => std::str::from_utf8(path.as_bytes()).ok(),
            _ => None,
        })
        .collect()
}
