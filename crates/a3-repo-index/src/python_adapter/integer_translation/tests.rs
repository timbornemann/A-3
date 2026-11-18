use super::*;
use crate::ParserPoolSize;
use a3_application::LanguageParseControlError;
use a3_domain::{ContentHash, Progress, RepositoryPath, SymbolName};
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct Active;

impl LanguageParseControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct Cancelled;

impl LanguageParseControl for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

#[test]
fn integer_translation_proves_both_signs_and_binds_exact_ranges() -> TestResult {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    for (delta, expression) in [(1, "(+value - (-0x_1))"), (-1, "value - (0b10 - 0o1)")] {
        let function = format!("def adjust(value: int) -> int:\r\n    return {expression}");
        let source = format!("# harmless UTF-8: \u{3bb}\r\n\r\n{function}\r\n");
        let required = requirement(delta)?;
        let revision = revision(required.path().clone(), source.as_bytes());
        let witness = PythonIntegerTranslationVerifier::prove(
            &adapter,
            &required,
            &revision,
            source.as_bytes(),
            &Active,
        )?
        .ok_or("expected integer translation proof")?;
        assert_eq!(witness.requirement(), &required);
        assert_eq!(witness.revision(), &revision);
        let declaration = witness.declaration();
        assert_eq!(
            source[declaration.start_byte() as usize..declaration.end_byte() as usize]
                .trim_end_matches(['\r', '\n']),
            function
        );
        let expression_range = witness.expression();
        assert_eq!(
            &source[expression_range.start_byte() as usize..expression_range.end_byte() as usize],
            expression
        );
        assert_eq!(declaration.start_position().row(), 2);
        assert_eq!(expression_range.start_position().row(), 3);
    }
    let required = requirement(i64::MIN)?;
    let source = b"def adjust(value): return value - 9223372036854775808\n";
    let revision = revision(required.path().clone(), source);
    assert!(
        adapter
            .prove_python_integer_translation(&required, &revision, source, &Active)?
            .is_some()
    );
    Ok(())
}

#[test]
fn integer_translation_leaves_wrong_or_unsupported_python_unproved() -> TestResult {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let required = requirement(1)?;
    for source in [
        "def adjust(value): return 42",
        "def adjust(value): return value - value + 42",
        "def adjust(value): return value * 0 + 42",
        "def adjust(value): return value + 2",
        "def adjust(value): return value + value + 1",
        "def adjust(value): return 1 - value",
        "def adjust(value): return value + f()",
        "def adjust(value): return value + True",
        "def adjust(value): return value + 1,",
        "def adjust(value): return (value + 1 if value == 41 else 42)",
        "def adjust(value): return value + external",
        "def adjust(value): return value + 01",
        "def adjust(value): return value + 1L",
        "def adjust(value): return value + 1j",
        "def adjust(value): return value + 1_",
        "def adjust(value): return value + 1 + 170141183460469231731687303715884105727",
        "def adjust(value, other): return value + 1",
        "def adjust(value=0): return value + 1",
        "def adjust(*, value): return value + 1",
        "def adjust(value, /): return value + 1",
        "def adjust(*value: int): return value + 1",
        "def adjust(await): return await + 1",
        "def adjust(__debug__): return __debug__ + 1",
        "def adjust(value: typing.Any): return value + 1",
        "def adjust(value: int) -> bool: return value + 1",
        "async def adjust(value): return value + 1",
        "@wrap\ndef adjust(value): return value + 1",
        "def adjust[T](value): return value + 1",
        "def adjust(value):\n    value = value + 1\n    return value",
        "def adjust(value): return value + 1\ndef adjust(value): return 42",
        "def adjust(value): return value + 1\nadjust = lambda value: 42",
        "import side_effect\ndef adjust(value): return value + 1",
        "  def adjust(value): return value + 1",
        "def other(value): return value + 1",
        "def adjust(value",
        "def adjust(value): return value +",
    ] {
        let revision = revision(required.path().clone(), source.as_bytes());
        assert!(
            adapter
                .prove_python_integer_translation(&required, &revision, source.as_bytes(), &Active)?
                .is_none(),
            "unexpected proof for {source}"
        );
    }
    Ok(())
}

#[test]
fn integer_translation_rejects_encoding_changes_and_excessive_nesting() -> TestResult {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let required = requirement(1)?;
    let deep = format!(
        "def adjust(value): return {}value + 1{}\n",
        "(".repeat(MAX_EXPRESSION_DEPTH + 1),
        ")".repeat(MAX_EXPRESSION_DEPTH + 1)
    );
    for source in [
        b"# coding: unicode_escape\ndef adjust(value): return value + 1\n".as_slice(),
        b"#!python\n# encoding=utf_7\ndef adjust(value): return value + 1\n",
        b"def adjust(value):\r    return value + 1\r",
        b"def adjust(value):\n\treturn value + 1\n",
        b"def adjust(value): return value + 1\n# invalid UTF-8 \xff",
        b"def adjust(value): return value + 1\n# NUL \0",
        "def adjust(value): return\u{200b}value + 1\n".as_bytes(),
        deep.as_bytes(),
    ] {
        let revision = revision(required.path().clone(), source);
        assert!(
            adapter
                .prove_python_integer_translation(&required, &revision, source, &Active)?
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn integer_translation_requires_complete_current_bytes_path_and_live_control() -> TestResult {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let required = requirement(1)?;
    let source = b"def adjust(value): return value + 1\n";
    let current = revision(required.path().clone(), source);
    assert_eq!(
        adapter.prove_python_integer_translation(
            &required,
            &current,
            b"def adjust(value): return value + 2\n",
            &Active,
        ),
        Err(LanguageParseFailure::RevisionMismatch)
    );
    assert_eq!(
        adapter.prove_python_integer_translation(
            &required,
            &current,
            &source[..source.len() - 1],
            &Active
        ),
        Err(LanguageParseFailure::RevisionMismatch)
    );
    let foreign = revision(
        RepositoryPath::try_from_bytes(b"foreign.py".to_vec())?,
        source,
    );
    assert_eq!(
        adapter.prove_python_integer_translation(&required, &foreign, source, &Active),
        Err(LanguageParseFailure::RevisionMismatch)
    );
    let oversized = vec![b' '; MAX_PYTHON_TRANSLATION_SOURCE_BYTES + 1];
    assert_eq!(
        adapter.prove_python_integer_translation(
            &required,
            &revision(required.path().clone(), &oversized),
            &oversized,
            &Active,
        ),
        Err(LanguageParseFailure::InputTooLarge)
    );
    assert_eq!(
        adapter.prove_python_integer_translation(&required, &current, source, &Cancelled),
        Err(LanguageParseFailure::Cancelled)
    );
    Ok(())
}

fn requirement(delta: i64) -> TestResult<PythonIntegerTranslation> {
    Ok(PythonIntegerTranslation::new(
        RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
        SymbolName::try_from_string("adjust".to_owned())?,
        delta,
    )?)
}

fn revision(path: RepositoryPath, source: &[u8]) -> FileRevision {
    FileRevision::new(
        path,
        ContentHash::from_bytes(*blake3::hash(source).as_bytes()),
    )
}
