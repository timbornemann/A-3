//! Reproducible manual performance baseline for the S5 Tree-sitter parser pool.

use a3_application::{LanguageParseControl, LanguageParseControlError, LanguageParsePolicy};
use a3_domain::Progress;
use a3_repo_index::{ParserPoolSize, TreeSitterParserPool};
use std::error::Error;
use std::time::Instant;
use tree_sitter::{Language, Parser};

const STRUCTURAL_LINES: usize = 100_000;

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
#[ignore = "manual 100,000-line Tree-sitter parser baseline"]
fn parse_json_100k_line_fixture() -> Result<(), Box<dyn Error>> {
    let source = json_100k_line_fixture();
    let policy = LanguageParsePolicy::v1();
    assert!(source.len() <= policy.max_source_bytes());
    let language: Language = tree_sitter_json::LANGUAGE.into();
    let mut direct_parser = Parser::new();
    direct_parser.set_language(&language)?;
    let direct_started = Instant::now();
    let direct_tree = direct_parser
        .parse(source.as_bytes(), None)
        .ok_or("direct Tree-sitter parser produced no tree")?;
    let direct_elapsed = direct_started.elapsed();
    assert!(!direct_tree.root_node().has_error());

    let pool = TreeSitterParserPool::new(&language, ParserPoolSize::new(1)?)?;
    let bounded_started = Instant::now();
    let parsed = pool.parse(source.as_bytes(), policy, &SilentControl)?;
    let bounded_elapsed = bounded_started.elapsed();

    assert!(parsed.coverage().is_complete());
    println!(
        "A^3 S5 Tree-sitter baseline: {} structural lines, {} bytes, direct={direct_elapsed:?}, bounded={bounded_elapsed:?}",
        STRUCTURAL_LINES,
        source.len(),
    );
    Ok(())
}

fn json_100k_line_fixture() -> String {
    let mut source = String::with_capacity(STRUCTURAL_LINES.saturating_mul(16));
    source.push_str("{\n");
    for line in 0..STRUCTURAL_LINES {
        source.push_str(&format!("\"key_{line:06}\":0"));
        if line.saturating_add(1) < STRUCTURAL_LINES {
            source.push(',');
        }
        source.push('\n');
    }
    source.push('}');
    source
}
