//! Reproducible manual performance baseline for the S7 TypeScript adapter.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{ContentHash, DiscoveredFileRoles, FileRevision, Progress, RepositoryPath};
use a3_repo_index::{ParserPoolSize, TypeScriptJavaScriptLanguageAdapter};
use std::error::Error;
use std::fmt::Write;
use std::time::Instant;
use tree_sitter::{Language, Parser};

const STRUCTURAL_LINES: usize = 100_000;
const FUNCTION_COUNT: usize = STRUCTURAL_LINES / 5;

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
#[ignore = "manual 100,000-line TypeScript adapter baseline"]
fn parse_typescript_100k_line_fixture() -> Result<(), Box<dyn Error>> {
    let source = typescript_100k_line_fixture()?;
    let policy = LanguageParsePolicy::v1();
    assert!(source.len() <= policy.max_source_bytes());

    let language: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut direct_parser = Parser::new();
    direct_parser.set_language(&language)?;
    let direct_started = Instant::now();
    let direct_tree = direct_parser
        .parse(source.as_bytes(), None)
        .ok_or("direct Tree-sitter TypeScript parser produced no tree")?;
    let direct_elapsed = direct_started.elapsed();
    assert!(!direct_tree.root_node().has_error());

    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"src/index.ts".to_vec())?,
        ContentHash::from_bytes(*blake3::hash(source.as_bytes()).as_bytes()),
    );
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let adapter_started = Instant::now();
    let result = adapter.parse(
        LanguageParseInput::new(&revision, source.as_bytes(), DiscoveredFileRoles::empty()),
        policy,
        &SilentControl,
    )?;
    let adapter_elapsed = adapter_started.elapsed();

    assert!(result.coverage().is_complete());
    assert!(result.diagnostics().is_empty());
    assert_eq!(result.symbols().len(), FUNCTION_COUNT.saturating_add(1));
    assert_eq!(
        result.relations().len(),
        FUNCTION_COUNT.saturating_mul(3).saturating_add(1)
    );
    println!(
        "A^3 S7 TypeScript adapter baseline: {} structural lines, {} bytes, {} symbols, {} relations, direct={direct_elapsed:?}, full_adapter={adapter_elapsed:?}",
        STRUCTURAL_LINES,
        source.len(),
        result.symbols().len(),
        result.relations().len(),
    );
    Ok(())
}

fn typescript_100k_line_fixture() -> Result<String, std::fmt::Error> {
    let mut source = String::with_capacity(STRUCTURAL_LINES.saturating_mul(28));
    for function in 0..FUNCTION_COUNT {
        writeln!(source, "export function item_{function:05}(): void {{")?;
        writeln!(source, "  helper();")?;
        writeln!(source, "}}")?;
        writeln!(source, "// deterministic fixture separator")?;
        writeln!(source)?;
    }
    Ok(source)
}
