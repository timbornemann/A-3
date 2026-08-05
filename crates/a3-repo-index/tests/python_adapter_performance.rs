//! Reproducible manual S8 baseline; excluded from the default test run.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{ContentHash, DiscoveredFileRoles, FileRevision, Progress, RepositoryPath};
use a3_repo_index::{ParserPoolSize, PythonLanguageAdapter};
use std::error::Error;
use std::io;
use std::time::Instant;
use tree_sitter::{Language, Parser};

const STRUCTURAL_LINES: usize = 100_000;
const FUNCTION_COUNT: usize = STRUCTURAL_LINES / 2;

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
#[ignore = "manual 100,000-line Python adapter baseline"]
fn parse_python_100k_line_fixture() -> Result<(), Box<dyn Error>> {
    let mut source = String::with_capacity(FUNCTION_COUNT.saturating_mul(40));
    for index in 0..FUNCTION_COUNT {
        source.push_str(&format!("def function_{index:05}():\n    helper()\n"));
    }
    assert_eq!(source.lines().count(), STRUCTURAL_LINES);

    let language: Language = tree_sitter_python::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language)?;
    let direct_started = Instant::now();
    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| io::Error::other("Python benchmark parser returned no tree"))?;
    let direct_elapsed = direct_started.elapsed();
    assert!(!tree.root_node().has_error());

    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"benchmark/module.py".to_vec())?,
        ContentHash::from_bytes(*blake3::hash(source.as_bytes()).as_bytes()),
    );
    let adapter_started = Instant::now();
    let result = adapter.parse(
        LanguageParseInput::new(&revision, source.as_bytes(), DiscoveredFileRoles::empty()),
        LanguageParsePolicy::v1(),
        &SilentControl,
    )?;
    let adapter_elapsed = adapter_started.elapsed();
    assert!(
        result.coverage().is_complete(),
        "{:?}",
        result.diagnostics()
    );
    assert_eq!(result.symbols().len(), FUNCTION_COUNT.saturating_add(1));
    assert_eq!(
        result.relations().len(),
        FUNCTION_COUNT.saturating_mul(3).saturating_add(1)
    );

    println!(
        "A^3 S8 Python adapter baseline: {STRUCTURAL_LINES} structural lines, {} bytes, {} symbols, {} relations, direct={direct_elapsed:?}, full_adapter={adapter_elapsed:?}",
        source.len(),
        result.symbols().len(),
        result.relations().len(),
    );
    Ok(())
}
