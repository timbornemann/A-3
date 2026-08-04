//! Rust syntax and Cargo-manifest feature fixtures beyond the shared contract.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    ContentHash, DiscoveredFileRole, DiscoveredFileRoles, FileRevision, LanguageParseResult,
    Progress, RepositoryPath, SymbolKind, SymbolRole, SyntaxRelationKind, SyntaxTarget,
};
use a3_repo_index::{ParserPoolSize, RustLanguageAdapter};
use std::error::Error;

const RUST_SOURCE: &[u8] = include_bytes!("../../../fixtures/rust-adapter/src/main.rs");
const INVALID_RUST: &[u8] = include_bytes!("../../../fixtures/rust-adapter/invalid.rs");
const CARGO_MANIFEST: &[u8] = include_bytes!("../../../fixtures/rust-adapter/Cargo.toml");

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
fn rust_fixture_extracts_symbols_roles_modules_reexports_and_calls() -> Result<(), Box<dyn Error>> {
    let adapter = RustLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    let result = parse(
        &adapter,
        b"fixtures/rust-adapter/src/main.rs",
        RUST_SOURCE,
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!result.coverage().is_complete());
    assert!(
        result.diagnostics().iter().any(
            |diagnostic| diagnostic.code() == a3_domain::ParseDiagnosticCode::UnsupportedSyntax
        )
    );
    assert_eq!(
        result,
        parse(
            &adapter,
            b"fixtures/rust-adapter/src/main.rs",
            RUST_SOURCE,
            DiscoveredFileRoles::empty(),
        )?
    );

    assert_symbol(&result, "main", SymbolKind::Module)?;
    assert_symbol(&result, "Model", SymbolKind::Struct)?;
    assert_symbol(&result, "State", SymbolKind::Enum)?;
    assert_symbol(&result, "Ready", SymbolKind::Variant)?;
    assert_symbol(&result, "code", SymbolKind::Field)?;
    assert_symbol(&result, "Runner", SymbolKind::Trait)?;
    assert_symbol(&result, "Runner for Model", SymbolKind::Implementation)?;
    assert_symbol(&result, "run", SymbolKind::Method)?;
    assert_symbol(&result, "Count", SymbolKind::TypeAlias)?;
    assert_symbol(&result, "LIMIT", SymbolKind::Constant)?;
    assert_symbol(&result, "LABEL", SymbolKind::Static)?;
    assert_symbol(&result, "nested", SymbolKind::Module)?;
    assert_symbol(&result, "external", SymbolKind::Module)?;

    let model = symbol(&result, "Model", SymbolKind::Struct)?;
    assert!(model.documentation_range().is_some());
    assert_eq!(model.visibility(), a3_domain::SymbolVisibility::Public);
    assert_eq!(
        symbol(&result, "Ready", SymbolKind::Variant)?.visibility(),
        a3_domain::SymbolVisibility::Public
    );
    let test = symbol(&result, "model_runs", SymbolKind::Function)?;
    assert!(test.roles().contains(SymbolRole::Test));
    let main = symbol(&result, "main", SymbolKind::Function)?;
    assert!(main.roles().contains(SymbolRole::Entrypoint));

    let implements = unresolved_targets(&result, SyntaxRelationKind::Implements);
    assert!(implements.contains(&"Runner"));
    let extends = unresolved_targets(&result, SyntaxRelationKind::Extends);
    assert!(extends.contains(&"Send"));
    assert!(extends.contains(&"Sync"));
    let imports = unresolved_targets(&result, SyntaxRelationKind::Imports);
    assert!(imports.contains(&"external"));
    assert!(imports.contains(&"nested::helper as exported_helper"));
    let exports = unresolved_targets(&result, SyntaxRelationKind::Exports);
    assert!(exports.contains(&"nested::helper as exported_helper"));
    let calls = unresolved_targets(&result, SyntaxRelationKind::Calls);
    assert!(calls.contains(&"helper"));
    assert!(calls.contains(&"exported_helper"));
    assert!(calls.contains(&"assert_eq!"));
    assert!(calls.contains(&"println!"));
    assert!(calls.contains(&"model.run"), "observed calls: {calls:?}");
    Ok(())
}

#[test]
fn syntax_error_is_partial_and_does_not_poison_the_rust_adapter() -> Result<(), Box<dyn Error>> {
    let adapter = RustLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let invalid = parse(
        &adapter,
        b"src/invalid.rs",
        INVALID_RUST,
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!invalid.coverage().is_complete());
    assert!(!invalid.diagnostics().is_empty());
    assert!(
        parse(
            &adapter,
            b"src/lib.rs",
            b"pub fn recovered() {}\n",
            DiscoveredFileRoles::empty(),
        )?
        .coverage()
        .is_complete()
    );
    Ok(())
}

#[test]
fn rust_entrypoint_and_cfg_test_roles_follow_file_and_attribute_semantics()
-> Result<(), Box<dyn Error>> {
    let adapter = RustLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let support = parse(
        &adapter,
        b"src/bin/tool/support.rs",
        b"fn helper() {}\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(
        !symbol(&support, "support", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Entrypoint)
    );

    let nested_binary = parse(
        &adapter,
        b"src/bin/tool/main.rs",
        b"fn main() {}\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(
        symbol(&nested_binary, "main", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Entrypoint)
    );
    assert!(
        symbol(&nested_binary, "main", SymbolKind::Function)?
            .roles()
            .contains(SymbolRole::Entrypoint)
    );

    let cfg_test = parse(
        &adapter,
        b"src/lib.rs",
        b"#[cfg(test)]\nmod tests {\n    fn smoke() {}\n}\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(
        symbol(&cfg_test, "tests", SymbolKind::Module)?
            .roles()
            .contains(SymbolRole::Test)
    );
    Ok(())
}

#[test]
fn cargo_manifest_extracts_package_targets_dependencies_and_workspace_members()
-> Result<(), Box<dyn Error>> {
    let adapter = RustLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let roles = DiscoveredFileRoles::empty().with(DiscoveredFileRole::Manifest);
    let result = parse(
        &adapter,
        b"fixtures/rust-adapter/Cargo.toml",
        CARGO_MANIFEST,
        roles,
    )?;
    assert!(result.coverage().is_complete());
    assert_symbol(&result, "rust-fixture", SymbolKind::Module)?;
    let library = symbol(&result, "fixture_core", SymbolKind::Module)?;
    assert!(library.roles().contains(SymbolRole::Entrypoint));
    let binary = symbol(&result, "fixture-cli", SymbolKind::Module)?;
    assert!(binary.roles().contains(SymbolRole::Entrypoint));
    let test = symbol(&result, "integration", SymbolKind::Module)?;
    assert!(test.roles().contains(SymbolRole::Test));

    assert!(unresolved_targets(&result, SyntaxRelationKind::Imports).contains(&"serde"));
    assert!(unresolved_targets(&result, SyntaxRelationKind::Tests).contains(&"proptest"));
    assert!(unresolved_targets(&result, SyntaxRelationKind::Builds).contains(&"support"));
    let built_files = result
        .relations()
        .iter()
        .filter(|relation| relation.kind() == SyntaxRelationKind::Builds)
        .filter_map(|relation| match relation.target() {
            SyntaxTarget::File(path) => std::str::from_utf8(path.as_bytes()).ok(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(built_files.contains(&"fixtures/rust-adapter/src/lib.rs"));
    assert!(built_files.contains(&"fixtures/rust-adapter/src/main.rs"));
    assert!(built_files.contains(&"fixtures/rust-adapter/tests/integration.rs"));
    Ok(())
}

#[test]
fn invalid_and_unbounded_cargo_manifests_fail_visibly() -> Result<(), Box<dyn Error>> {
    let adapter = RustLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let invalid = parse(
        &adapter,
        b"Cargo.toml",
        b"[package\nname = \"broken\"\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!invalid.coverage().is_complete());
    assert!(!invalid.diagnostics().is_empty());

    let invalid_encoding = parse(
        &adapter,
        b"Cargo.toml",
        b"[package]\nname = \"bad\xff\"\n",
        DiscoveredFileRoles::empty(),
    )?;
    assert!(!invalid_encoding.coverage().is_complete());
    assert!(invalid_encoding.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == a3_domain::ParseDiagnosticCode::InvalidEncoding
    }));

    let oversized = vec![b' '; 256 * 1024 + 1];
    let revision = revision(b"Cargo.toml", &oversized)?;
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
    adapter: &RustLanguageAdapter,
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
        .ok_or_else(|| format!("missing {kind:?} symbol {name}").into())
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
