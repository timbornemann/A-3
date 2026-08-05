//! TypeScript, JavaScript, package, and workspace behavior beyond the shared contract.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    ContentHash, DiscoveredFileRole, DiscoveredFileRoles, FileRevision, LanguageParseResult,
    ParseDiagnosticCode, Progress, RepositoryPath, SymbolKind, SymbolRole, SymbolVisibility,
    SyntaxRelationKind, SyntaxTarget,
};
use a3_repo_index::{ParserPoolSize, TypeScriptJavaScriptLanguageAdapter};
use std::error::Error;

const TYPESCRIPT_SOURCE: &[u8] =
    include_bytes!("../../../fixtures/typescript-monorepo/packages/core/src/index.ts");
const JAVASCRIPT_SOURCE: &[u8] =
    include_bytes!("../../../fixtures/typescript-monorepo/packages/legacy/src/index.cjs");
const TSX_SOURCE: &[u8] =
    include_bytes!("../../../fixtures/typescript-monorepo/packages/web/src/App.tsx");
const INVALID_TYPESCRIPT: &[u8] =
    include_bytes!("../../../fixtures/typescript-monorepo/invalid.ts");
const PACKAGE_MANIFEST: &[u8] =
    include_bytes!("../../../fixtures/typescript-monorepo/package.json");
const PNPM_WORKSPACE: &[u8] =
    include_bytes!("../../../fixtures/typescript-monorepo/pnpm-workspace.yaml");

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

#[test]
fn typescript_fixture_extracts_declarations_exports_calls_tests_and_heritage()
-> Result<(), Box<dyn Error>> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    let result = parse(
        &adapter,
        b"fixtures/typescript-monorepo/packages/core/src/index.ts",
        TYPESCRIPT_SOURCE,
        DiscoveredFileRoles::empty(),
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
            b"fixtures/typescript-monorepo/packages/core/src/index.ts",
            TYPESCRIPT_SOURCE,
            DiscoveredFileRoles::empty(),
        )?
    );

    assert_symbol(&result, "index", SymbolKind::Module)?;
    assert_symbol(&result, "BaseRunner", SymbolKind::Interface)?;
    assert_symbol(&result, "BaseService", SymbolKind::Class)?;
    assert_symbol(&result, "Worker", SymbolKind::Class)?;
    let runner = symbol(&result, "Runner", SymbolKind::Interface)?;
    assert!(runner.documentation_range().is_some());
    assert_eq!(runner.visibility(), SymbolVisibility::Public);
    assert_symbol(&result, "Result", SymbolKind::TypeAlias)?;
    assert_symbol(&result, "State", SymbolKind::Enum)?;
    assert_eq!(
        symbol(&result, "Ready", SymbolKind::Variant)?.visibility(),
        SymbolVisibility::Public
    );
    assert_symbol(&result, "Failed", SymbolKind::Variant)?;
    assert_symbol(&result, "Service", SymbolKind::Class)?;
    assert_eq!(
        symbol(&result, "prefix", SymbolKind::Field)?.visibility(),
        SymbolVisibility::Private
    );
    assert_symbol(&result, "constructor", SymbolKind::Method)?;
    assert_symbol(&result, "makeService", SymbolKind::Function)?;
    assert_symbol(&result, "internalTask", SymbolKind::Function)?;
    assert_symbol(&result, "Tools", SymbolKind::Namespace)?;
    assert_symbol(&result, "configure", SymbolKind::Function)?;
    let suite = symbol(&result, "Service", SymbolKind::Module)?;
    assert!(suite.roles().contains(SymbolRole::Test));
    let test = symbol(&result, "runs", SymbolKind::Function)?;
    assert!(test.roles().contains(SymbolRole::Test));

    let imports = unresolved_targets(&result, SyntaxRelationKind::Imports);
    assert!(imports.contains(&"./config"));
    assert!(imports.contains(&"./helper"));
    assert!(imports.contains(&"./types"));
    let exports = unresolved_targets(&result, SyntaxRelationKind::Exports);
    assert!(exports.contains(&"renamedHelper"));
    assert!(exports.contains(&"./helper"));
    assert!(exports.contains(&"./types"));
    let extends = unresolved_targets(&result, SyntaxRelationKind::Extends);
    assert!(extends.contains(&"BaseRunner"));
    assert!(extends.contains(&"BaseService"));
    assert!(unresolved_targets(&result, SyntaxRelationKind::Implements).contains(&"Runner"));
    let calls = unresolved_targets(&result, SyntaxRelationKind::Calls);
    assert!(calls.contains(&"importedHelper"));
    assert!(calls.contains(&"Worker"));
    assert!(calls.contains(&"new Worker().execute"));
    assert!(calls.contains(&"makeService"));
    assert!(calls.contains(&"describe"));
    assert!(calls.contains(&"it"));
    assert!(calls.contains(&"Service"));
    Ok(())
}

#[test]
fn javascript_tsx_and_anonymous_default_exports_use_the_correct_grammars()
-> Result<(), Box<dyn Error>> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let javascript = parse(
        &adapter,
        b"fixtures/typescript-monorepo/packages/legacy/src/index.cjs",
        JAVASCRIPT_SOURCE,
        DiscoveredFileRoles::empty(),
    )?;
    assert!(
        javascript.coverage().is_complete(),
        "{:?}",
        javascript.diagnostics()
    );
    assert_symbol(&javascript, "LegacyService", SymbolKind::Class)?;
    assert!(unresolved_targets(&javascript, SyntaxRelationKind::Imports).contains(&"./helper"));
    assert!(unresolved_targets(&javascript, SyntaxRelationKind::Calls).contains(&"require"));
    assert!(
        unresolved_targets(&javascript, SyntaxRelationKind::Exports).contains(&"module.exports")
    );

    let tsx = parse(
        &adapter,
        b"fixtures/typescript-monorepo/packages/web/src/App.tsx",
        TSX_SOURCE,
        DiscoveredFileRoles::empty(),
    )?;
    assert!(tsx.coverage().is_complete(), "{:?}", tsx.diagnostics());
    assert_symbol(&tsx, "App", SymbolKind::Function)?;
    assert!(unresolved_targets(&tsx, SyntaxRelationKind::Calls).contains(&"activate"));

    let default_export = parse(
        &adapter,
        b"src/default.ts",
        b"export default () => answer();\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(default_export.coverage().is_complete());
    let symbol = symbol(&default_export, "default", SymbolKind::Function)?;
    assert_eq!(symbol.visibility(), SymbolVisibility::Public);
    assert!(unresolved_targets(&default_export, SyntaxRelationKind::Calls).contains(&"answer"));
    Ok(())
}

#[test]
fn syntax_errors_dynamic_calls_and_test_file_roles_are_visible_and_isolated()
-> Result<(), Box<dyn Error>> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let invalid = parse(
        &adapter,
        b"src/invalid.ts",
        INVALID_TYPESCRIPT,
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!invalid.coverage().is_complete());
    assert!(!invalid.diagnostics().is_empty());

    let dynamic = parse(
        &adapter,
        b"src/dynamic.ts",
        b"getFactory()();\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!dynamic.coverage().is_complete());
    assert!(
        dynamic
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == ParseDiagnosticCode::UnsupportedSyntax)
    );

    let dynamic_module = parse(
        &adapter,
        b"src/dynamic-module.cjs",
        b"const moduleName = './module';\nrequire(moduleName);\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!dynamic_module.coverage().is_complete());
    assert!(unresolved_targets(&dynamic_module, SyntaxRelationKind::Calls).contains(&"require"));

    let destructured = parse(
        &adapter,
        b"src/destructured.ts",
        b"const { value } = source;\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!destructured.coverage().is_complete());
    assert!(
        !destructured
            .symbols()
            .iter()
            .any(|symbol| symbol.name().as_str() == "{ value }")
    );

    let roles = DiscoveredFileRoles::empty().with(DiscoveredFileRole::Test);
    let test_file = parse(
        &adapter,
        b"src/feature.test.ts",
        b"export function test_feature() {}\n",
        roles,
    )?;
    assert!(
        symbol(&test_file, "feature.test", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Test)
    );
    assert!(
        symbol(&test_file, "test_feature", SymbolKind::Function)?
            .roles()
            .contains(SymbolRole::Test)
    );

    assert!(
        parse(
            &adapter,
            b"src/index.ts",
            b"export function recovered() {}\n",
            DiscoveredFileRoles::empty(),
        )?
        .coverage()
        .is_complete()
    );
    Ok(())
}

#[test]
fn package_manifest_extracts_entrypoints_dependencies_scripts_and_workspaces()
-> Result<(), Box<dyn Error>> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let roles = DiscoveredFileRoles::empty().with(DiscoveredFileRole::Manifest);
    let result = parse(
        &adapter,
        b"fixtures/typescript-monorepo/package.json",
        PACKAGE_MANIFEST,
        roles,
    )?;
    assert!(
        result.coverage().is_complete(),
        "{:?}",
        result.diagnostics()
    );
    let package = symbol(&result, "@a3-fixture/root", SymbolKind::Module)?;
    assert!(package.roles().contains(SymbolRole::Entrypoint));
    let bin = symbol(&result, "a3-fixture", SymbolKind::Module)?;
    assert!(bin.roles().contains(SymbolRole::Entrypoint));

    assert!(unresolved_targets(&result, SyntaxRelationKind::Imports).contains(&"nanoid"));
    let tests = unresolved_targets(&result, SyntaxRelationKind::Tests);
    assert!(tests.contains(&"vitest"));
    assert!(tests.contains(&"script:test"));
    let builds = unresolved_targets(&result, SyntaxRelationKind::Builds);
    assert!(builds.contains(&"packages/*"));
    assert!(builds.contains(&"script:build"));
    assert!(unresolved_targets(&result, SyntaxRelationKind::Configures).contains(&"script:lint"));

    let built_files = file_targets(&result, SyntaxRelationKind::Builds);
    assert!(built_files.contains(&"fixtures/typescript-monorepo/dist/index.js"));
    assert!(built_files.contains(&"fixtures/typescript-monorepo/dist/index.mjs"));
    assert!(built_files.contains(&"fixtures/typescript-monorepo/dist/index.d.ts"));
    assert!(built_files.contains(&"fixtures/typescript-monorepo/dist/index.cjs"));
    assert!(built_files.contains(&"fixtures/typescript-monorepo/bin/cli.js"));

    let pattern = parse(
        &adapter,
        b"packages/pattern/package.json",
        br#"{"name":"pattern","exports":"./dist/*.js"}"#,
        roles,
    )?;
    assert!(
        pattern.coverage().is_complete(),
        "{:?}",
        pattern.diagnostics()
    );
    assert!(unresolved_targets(&pattern, SyntaxRelationKind::Builds).contains(&"./dist/*.js"));

    let browser_map = parse(
        &adapter,
        b"packages/browser/package.json",
        br#"{"name":"browser","browser":{"./server.js":false,"./node.js":"./web.js"}}"#,
        roles,
    )?;
    assert!(
        browser_map.coverage().is_complete(),
        "{:?}",
        browser_map.diagnostics()
    );
    assert!(
        file_targets(&browser_map, SyntaxRelationKind::Builds).contains(&"packages/browser/web.js")
    );

    let invalid_dependency = parse(
        &adapter,
        b"packages/invalid/package.json",
        br#"{"name":"invalid","dependencies":{"not-a-version":1}}"#,
        roles,
    )?;
    assert!(!invalid_dependency.coverage().is_complete());
    assert!(
        !unresolved_targets(&invalid_dependency, SyntaxRelationKind::Imports)
            .contains(&"not-a-version")
    );
    Ok(())
}

#[test]
fn pnpm_workspace_and_manifest_failures_remain_bounded_and_explicit() -> Result<(), Box<dyn Error>>
{
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let workspace = parse(
        &adapter,
        b"fixtures/typescript-monorepo/pnpm-workspace.yaml",
        PNPM_WORKSPACE,
        DiscoveredFileRoles::empty(),
    )?;
    assert!(
        workspace.coverage().is_complete(),
        "{:?}",
        workspace.diagnostics()
    );
    let packages = unresolved_targets(&workspace, SyntaxRelationKind::Builds);
    assert!(packages.contains(&"packages/*"));
    assert!(packages.contains(&"tools/*"));

    let inline = parse(
        &adapter,
        b"pnpm-workspace.yaml",
        b"packages: [packages/*]\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!inline.coverage().is_complete());

    let empty = parse(
        &adapter,
        b"pnpm-workspace.yaml",
        b"packages:\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!empty.coverage().is_complete());

    let invalid_encoding = parse(
        &adapter,
        b"package.json",
        b"{\"name\":\"bad\xff\"}\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!invalid_encoding.coverage().is_complete());
    assert!(
        invalid_encoding
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == ParseDiagnosticCode::InvalidEncoding)
    );

    let traversal = parse(
        &adapter,
        b"package.json",
        b"{\"name\":\"unsafe\",\"main\":\"../outside.js\"}\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!traversal.coverage().is_complete());

    let oversized = vec![b' '; 512 * 1024 + 1];
    let revision = revision(b"package.json", &oversized)?;
    assert_eq!(
        adapter.parse(
            LanguageParseInput::new(&revision, &oversized, DiscoveredFileRoles::empty()),
            LanguageParsePolicy::v1(),
            &SilentControl,
        ),
        Err(LanguageParseFailure::InputTooLarge)
    );
    Ok(())
}

fn parse(
    adapter: &TypeScriptJavaScriptLanguageAdapter,
    path: &[u8],
    source: &[u8],
    roles: DiscoveredFileRoles,
) -> Result<LanguageParseResult, Box<dyn Error>> {
    let revision = revision(path, source)?;
    Ok(adapter.parse(
        LanguageParseInput::new(&revision, source, roles),
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
