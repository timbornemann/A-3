//! ADR-0093: bounded context materialization, not an agent/research action.
use a3_application::{
    AgentContextCompileInput, AgentSourcePage, AgentSourceReadControl, AgentSourceReadFailure,
    AgentSourceReader, ContextCompileControl, ContextCompileFailure,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, FileRevision, SourceRange,
    TaskLens, TaskLensTarget,
};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

const MAX_PAGES: usize = 2;
const MAX_LINES: u16 = 64;
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct PackedOriginals {
    pub text: String,
    pub tokens: u32,
    pub truncated: bool,
    pub replan: bool,
    pub delivered: Vec<a3_application::ContextOriginalSource>,
}

struct Candidate<'a> {
    revision: &'a FileRevision,
    range: Option<SourceRange>,
}

pub(super) async fn materialize(
    reader: &dyn AgentSourceReader,
    input: &AgentContextCompileInput,
    lens: &TaskLens,
    allowance: u32,
    control: &dyn ContextCompileControl,
) -> Result<PackedOriginals, ContextCompileFailure> {
    let replan = input.replan_localization().is_some() || input.replan_research().is_some();
    let mut packed = PackedOriginals {
        replan,
        ..PackedOriginals::default()
    };
    if replan {
        return Ok(packed);
    }
    let deadline = SourceDeadline {
        control,
        started: Instant::now(),
    };
    let recent = input
        .tool_results()
        .iter()
        .rev()
        .filter(|result| {
            result.snapshot_before() == lens.snapshot_id()
                && result.snapshot_after() == lens.snapshot_id()
        })
        .filter_map(|result| result.original_source())
        .map(|source| Candidate {
            revision: source.location().revision(),
            range: source.location().range(),
        });
    let ranked = lens
        .entries()
        .iter()
        .filter_map(|entry| match entry.target() {
            TaskLensTarget::File(revision) => Some(Candidate {
                revision,
                range: None,
            }),
            TaskLensTarget::SourceSpan { evidence, .. } => Some(Candidate {
                revision: evidence.revision(),
                range: Some(evidence.range()),
            }),
            TaskLensTarget::Symbol(symbol) => Some(Candidate {
                revision: symbol.revision(),
                range: Some(symbol.parsed().declaration_range()),
            }),
            TaskLensTarget::Repository(_) | TaskLensTarget::Module(_) => None,
        });
    let mut seen = BTreeSet::new();
    for candidate in recent.chain(ranked) {
        deadline.check()?;
        if !seen.insert(candidate.revision.path().clone()) {
            continue;
        }
        if seen.len() > MAX_PAGES || allowance == 0 {
            packed.truncated = true;
            break;
        }
        let request = request(&candidate)?;
        let result = reader
            .read_page(input.project(), candidate.revision, &request, &deadline)
            .await;
        deadline.check()?;
        let page = match result {
            Ok(page) => page,
            Err(AgentSourceReadFailure::Stale) => {
                return Err(ContextCompileFailure::StaleOrMismatchedInput);
            }
            Err(AgentSourceReadFailure::InvalidPage) => {
                return Err(ContextCompileFailure::InvalidPack);
            }
            Err(AgentSourceReadFailure::Cancelled) => return Err(ContextCompileFailure::Cancelled),
            Err(
                AgentSourceReadFailure::Unavailable
                | AgentSourceReadFailure::Denied
                | AgentSourceReadFailure::FileTooLarge
                | AgentSourceReadFailure::InvalidEncoding
                | AgentSourceReadFailure::BinaryContent
                | AgentSourceReadFailure::SecretCandidate
                | AgentSourceReadFailure::LineTooLong,
            ) => {
                packed.truncated = true;
                continue;
            }
        };
        validate_page(&page, candidate.revision, &request)?;
        let rendered = render(&page);
        super::reject_secret_candidate(&rendered)?;
        let cost = super::count(input.model_profile(), &rendered)?;
        let next = packed
            .tokens
            .checked_add(cost)
            .ok_or(ContextCompileFailure::InvalidPack)?;
        if next > allowance {
            packed.truncated = true;
            continue;
        }
        packed.text.push_str(&rendered);
        packed
            .delivered
            .push(a3_application::ContextOriginalSource::from_packed_page(
                lens.snapshot_id(),
                &page,
            ));
        packed.tokens = next;
        packed.truncated |= page.truncated();
    }
    Ok(packed)
}

fn request(candidate: &Candidate<'_>) -> Result<AgentFileInspection, ContextCompileFailure> {
    let start = candidate
        .range
        .map_or(0, |range| range.start_position().row())
        .checked_add(1)
        .ok_or(ContextCompileFailure::InvalidPack)?;
    let lines = candidate.range.map_or(u32::from(MAX_LINES), |range| {
        let end = range.end_position();
        end.row()
            .saturating_sub(range.start_position().row())
            .saturating_add(u32::from(end.column() != 0))
            .clamp(1, u32::from(MAX_LINES))
    });
    Ok(AgentFileInspection::new(
        candidate.revision.path().clone(),
        AgentFileStartLine::new(start).map_err(|_| ContextCompileFailure::InvalidPack)?,
        AgentFileLineCount::new(
            u16::try_from(lines).map_err(|_| ContextCompileFailure::InvalidPack)?,
        )
        .map_err(|_| ContextCompileFailure::InvalidPack)?,
    ))
}

fn validate_page(
    page: &AgentSourcePage,
    revision: &FileRevision,
    request: &AgentFileInspection,
) -> Result<(), ContextCompileFailure> {
    if page.revision() != revision || page.start_line() != request.start_line() {
        return Err(ContextCompileFailure::StaleOrMismatchedInput);
    }
    let range = page.range();
    if usize::try_from(range.len()).ok() != Some(page.text().len())
        || page.text().lines().count() > usize::from(request.line_count().get())
    {
        return Err(ContextCompileFailure::InvalidPack);
    }
    // EOF pages may start beyond the last line; nonempty pages must begin on the requested line.
    if !page.text().is_empty() {
        let start = range.start_position();
        let mut row = start.row();
        let mut column = 0u32;
        for byte in page.text().bytes() {
            if byte == b'\n' {
                row = row
                    .checked_add(1)
                    .ok_or(ContextCompileFailure::InvalidPack)?;
                column = 0;
            } else {
                column = column
                    .checked_add(1)
                    .ok_or(ContextCompileFailure::InvalidPack)?;
            }
        }
        if start.row().checked_add(1) != Some(request.start_line().get())
            || start.column() != 0
            || range.end_position() != a3_domain::SourcePosition::new(row, column)
            || page
                .next_start_line()
                .is_some_and(|next| column != 0 || row.checked_add(1) != Some(next.get()))
        {
            return Err(ContextCompileFailure::InvalidPack);
        }
    }
    Ok(())
}

fn render(page: &AgentSourcePage) -> String {
    let range = page.range();
    let next = page
        .next_start_line()
        .map_or_else(|| "none".to_owned(), |line| line.get().to_string());
    format!(
        "[ORIGINAL_SOURCE path={} hash={} bytes={}..{} start_line={} next_line={} partial={}]\n{}\n[/ORIGINAL_SOURCE]\n",
        super::path_text(page.revision().path()),
        super::hex(page.revision().content_hash().as_bytes()),
        range.start_byte(),
        range.end_byte(),
        page.start_line().get(),
        next,
        page.start_line().get() != 1 || page.truncated(),
        page.text(),
    )
}

#[derive(Debug)]
struct SourceDeadline<'a> {
    control: &'a dyn ContextCompileControl,
    started: Instant,
}

impl SourceDeadline<'_> {
    fn check(&self) -> Result<(), ContextCompileFailure> {
        if self.control.is_cancelled() {
            Err(ContextCompileFailure::Cancelled)
        } else if self.started.elapsed() >= TIMEOUT {
            Err(ContextCompileFailure::TimedOut)
        } else {
            Ok(())
        }
    }
}

impl AgentSourceReadControl for SourceDeadline<'_> {
    fn is_cancelled(&self) -> bool {
        self.check().is_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_application::{ContextCompilePhase, TaskLensControlError};

    #[derive(Debug)]
    struct Control(bool);
    impl ContextCompileControl for Control {
        fn is_cancelled(&self) -> bool {
            self.0
        }
        fn report_phase(&self, _: ContextCompilePhase) -> Result<(), TaskLensControlError> {
            Ok(())
        }
    }

    #[test]
    fn deadline_is_shared_cooperative_and_distinguishes_timeout_from_user_cancellation() {
        let expired = SourceDeadline {
            control: &Control(false),
            started: Instant::now() - TIMEOUT,
        };
        assert_eq!(expired.check(), Err(ContextCompileFailure::TimedOut));
        assert!(expired.is_cancelled());
        let cancelled = SourceDeadline {
            control: &Control(true),
            started: Instant::now(),
        };
        assert_eq!(cancelled.check(), Err(ContextCompileFailure::Cancelled));
        assert!(cancelled.is_cancelled());
    }
}
