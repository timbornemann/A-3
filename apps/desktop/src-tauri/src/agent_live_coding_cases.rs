//! Closed public live cases. Additional oracles are outside the model's repository.
use a3_application::AgentApprovalFileOperation;
use std::{error::Error, path::Path};

macro_rules! fixture {
    ($case:literal, $file:literal) => {
        (
            $file,
            include_str!(concat!(
                "../../../../fixtures/agent-coding-eval-v1/",
                $case,
                "/",
                $file
            )),
        )
    };
}

const UNRELATED: (&str, &str) = (
    "unrelated.txt",
    "Preserve this independently owned fixture content.\n",
);
const BUGFIX: &[(&str, &str)] = &[
    fixture!("small-local-bugfix", "increment.py"),
    fixture!("small-local-bugfix", "tests/test_increment.py"),
    fixture!("small-local-bugfix", "pytest.py"),
    fixture!("small-local-bugfix", "pyproject.toml"),
    UNRELATED,
];
const TWO_MODULE: &[(&str, &str)] = &[
    fixture!("two-module-change", "pricing.py"),
    fixture!("two-module-change", "invoice.py"),
    fixture!("two-module-change", "tests/test_invoice.py"),
    fixture!("two-module-change", "pytest.py"),
    fixture!("two-module-change", "pyproject.toml"),
    UNRELATED,
];

pub(super) const ORACLE: &str = include_str!("../../../../fixtures/agent-live-coding-v2/oracle.py");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LiveCodingCase {
    Bugfix,
    TwoModule,
}

impl LiveCodingCase {
    pub(super) fn parse(value: Option<&str>) -> Result<Self, &'static str> {
        match value {
            None | Some("small-local-bugfix") => Ok(Self::Bugfix),
            Some("two-module-change") => Ok(Self::TwoModule),
            _ => Err("A3_LIVE_AGENT_CASE must be small-local-bugfix or two-module-change"),
        }
    }

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Bugfix => "small-local-bugfix",
            Self::TwoModule => "two-module-change",
        }
    }

    pub(super) fn files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Bugfix => BUGFIX,
            Self::TwoModule => TWO_MODULE,
        }
    }

    pub(super) fn sources(self) -> &'static [(&'static str, &'static str)] {
        &self.files()[..self.source_count()]
    }

    fn source_count(self) -> usize {
        match self {
            Self::Bugfix => 1,
            Self::TwoModule => 2,
        }
    }

    pub(super) fn protected(self) -> &'static [(&'static str, &'static str)] {
        &self.files()[self.source_count()..]
    }

    pub(super) fn objective(self) -> &'static str {
        match self {
            Self::Bugfix => {
                "Fix increment(value) in increment.py to increase its input by exactly one for any integer. Change only increment.py. Do not modify tests, pytest.py, pyproject.toml or unrelated.txt. Run the existing python -m pytest command and verify the actual result."
            }
            Self::TwoModule => {
                "Implement invoice discounts across pricing.py and invoice.py. discounted_total(cents, percent) must apply an integer percent discount to nonnegative integer cents, rounding down to whole cents; percent is 0 through 100 inclusive. invoice_total(line_cents, discount_percent) must call this helper on the sum of the line cents, then format the discounted total with a dollar sign and exactly two decimal digits. Empty line lists total zero. Change both pricing.py and invoice.py only. Do not modify tests, pytest.py, pyproject.toml or unrelated.txt. Run the existing python -m pytest command and verify the actual result."
            }
        }
    }

    pub(super) fn outcome(self) -> &'static str {
        match self {
            Self::Bugfix => "Fix increment.py and prove the unchanged tests pass",
            Self::TwoModule => {
                "Implement discounted_total in pricing.py and reuse it from invoice_total in invoice.py; prove the unchanged tests pass"
            }
        }
    }

    pub(super) fn evidence(self) -> &'static str {
        match self {
            Self::Bugfix => "Current increment.py and passing locked test result",
            Self::TwoModule => "Current pricing.py and invoice.py and passing locked test result",
        }
    }

    pub(super) fn patch_scope(
        self,
        operation: AgentApprovalFileOperation,
        source: Option<&str>,
        target: Option<&str>,
    ) -> bool {
        operation == AgentApprovalFileOperation::Update
            && source == target
            && self.sources().iter().any(|(path, _)| Some(*path) == source)
    }

    pub(super) fn originals_delivered(self, text: &str) -> usize {
        self.sources()
            .iter()
            .filter(|(path, body)| {
                text.split("[ORIGINAL_SOURCE ").skip(1).any(|block| {
                    block.starts_with(&format!("path={path} "))
                        && block
                            .split_once('\n')
                            .and_then(|(_, content)| content.split_once("\n[/ORIGINAL_SOURCE]"))
                            .is_some_and(|(content, _)| content.trim_end() == body.trim_end())
                })
            })
            .count()
    }

    pub(super) fn changed_sources(self, root: &Path) -> Result<usize, Box<dyn Error>> {
        let mut changed = 0;
        for (path, original) in self.sources() {
            if std::fs::read(root.join(path))? != original.as_bytes() {
                changed += 1;
            }
        }
        Ok(changed)
    }

    pub(super) fn protected_unchanged(self, root: &Path) -> bool {
        self.protected().iter().all(|(path, content)| {
            std::fs::read(root.join(path)).is_ok_and(|bytes| bytes == content.as_bytes())
        })
    }
}

#[test]
fn live_case_selection_is_closed_and_scope_is_case_specific() {
    assert_eq!(LiveCodingCase::parse(None), Ok(LiveCodingCase::Bugfix));
    assert_eq!(
        LiveCodingCase::parse(Some("two-module-change")),
        Ok(LiveCodingCase::TwoModule)
    );
    for unknown in [
        "",
        "../two-module-change",
        "custom",
        "small-local-bugfix/other",
    ] {
        assert!(LiveCodingCase::parse(Some(unknown)).is_err());
    }
    for case in [LiveCodingCase::Bugfix, LiveCodingCase::TwoModule] {
        for (path, _) in case.sources() {
            assert!(case.patch_scope(AgentApprovalFileOperation::Update, Some(path), Some(path)));
            for operation in [
                AgentApprovalFileOperation::Add,
                AgentApprovalFileOperation::Move,
                AgentApprovalFileOperation::Delete,
            ] {
                assert!(!case.patch_scope(operation, Some(path), Some(path)));
            }
        }
        for (path, _) in case.protected() {
            assert!(!case.patch_scope(AgentApprovalFileOperation::Update, Some(path), Some(path)));
        }
        for path in ["../pricing.py", "pricing.py/other", "oracle.py"] {
            assert!(!case.patch_scope(AgentApprovalFileOperation::Update, Some(path), Some(path)));
        }
    }
    assert!(!LiveCodingCase::Bugfix.patch_scope(
        AgentApprovalFileOperation::Update,
        Some("pricing.py"),
        Some("pricing.py")
    ));
    assert!(!LiveCodingCase::TwoModule.patch_scope(
        AgentApprovalFileOperation::Update,
        Some("pricing.py"),
        Some("invoice.py")
    ));
    assert!(!LiveCodingCase::TwoModule.patch_scope(AgentApprovalFileOperation::Update, None, None));
}

#[test]
fn original_preflight_binds_each_body_to_its_own_source_block() {
    let case = LiveCodingCase::TwoModule;
    let blocks: Vec<_> = case
        .sources()
        .iter()
        .map(|(path, body)| {
            format!("[ORIGINAL_SOURCE path={path} hash=fixture]\n{body}\n[/ORIGINAL_SOURCE]")
        })
        .collect();
    assert_eq!(case.originals_delivered(&blocks.join("\n")), 2);
    assert_eq!(case.originals_delivered(&blocks[0]), 1);
    assert_eq!(
        case.originals_delivered("L3 file path=pricing.py hash=fixture"),
        0
    );
    let swapped = format!(
        "[ORIGINAL_SOURCE path=pricing.py hash=fixture]\n{}\n[/ORIGINAL_SOURCE]\n{}",
        case.sources()[1].1,
        case.sources()[0].1
    );
    assert_eq!(case.originals_delivered(&swapped), 0);
    assert_eq!(
        case.originals_delivered(&blocks[0].replace("[/ORIGINAL_SOURCE]", "")),
        0
    );
}

#[test]
fn independent_oracle_rejects_example_only_and_partial_implementations()
-> Result<(), Box<dyn Error>> {
    for case in [LiveCodingCase::Bugfix, LiveCodingCase::TwoModule] {
        let repository = super::support::TempDirectory::new()?;
        for (path, content) in case.files() {
            repository.write(path, content)?;
        }
        assert!(
            super::require_oracle_passed(super::run_oracle(case, repository.path(), || false)?)
                .is_err()
        );
        assert!(super::run_oracle(case, repository.path(), || true).is_err());
        match case {
            LiveCodingCase::Bugfix => {
                repository.write(
                    "increment.py",
                    "def increment(value: int) -> int:\n    return 42\n",
                )?;
                assert!(visible_tests_pass(repository.path())?);
                assert!(!super::run_oracle(case, repository.path(), || false)?);
                repository.write(
                    "increment.py",
                    "def increment(value: int) -> int:\n    return value + 1\n",
                )?;
            }
            LiveCodingCase::TwoModule => {
                repository.write(
                    "invoice.py",
                    "def invoice_total(line_cents, discount_percent):\n    return '$13.50'\n",
                )?;
                assert!(visible_tests_pass(repository.path())?);
                assert!(!super::run_oracle(case, repository.path(), || false)?);
                repository.write("pricing.py", "def discounted_total(cents, percent):\n    return cents * (100 - percent) // 100\n")?;
                assert!(!super::run_oracle(case, repository.path(), || false)?);
                // Correct arithmetic without the required helper delegation is insufficient.
                repository.write("invoice.py", "def invoice_total(line_cents, discount_percent):\n    total = sum(line_cents) * (100 - discount_percent) // 100\n    return f'${total // 100}.{total % 100:02d}'\n")?;
                assert!(visible_tests_pass(repository.path())?);
                assert!(!super::run_oracle(case, repository.path(), || false)?);
                repository.write("invoice.py", "from pricing import discounted_total\n\ndef invoice_total(line_cents, discount_percent):\n    total = discounted_total(sum(line_cents), discount_percent)\n    return f'${total // 100}.{total % 100:02d}'\n")?;
            }
        }
        assert!(visible_tests_pass(repository.path())?);
        super::require_oracle_passed(super::run_oracle(case, repository.path(), || false)?)?;
        assert!(case.protected_unchanged(repository.path()));
        assert_eq!(
            case.changed_sources(repository.path())?,
            case.sources().len()
        );
        assert!(!repository.path().join("oracle.py").exists());
    }
    Ok(())
}

fn visible_tests_pass(path: &Path) -> Result<bool, Box<dyn Error>> {
    super::run_check(
        std::process::Command::new("python")
            .args(["-B", "-m", "pytest"])
            .current_dir(path),
        || false,
    )
}
