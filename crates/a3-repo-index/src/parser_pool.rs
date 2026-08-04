use a3_application::{LanguageParseControl, LanguageParseFailure, LanguageParsePolicy};
use a3_domain::{
    DiagnosticMessage, ParseCoverage, ParseDiagnostic, ParseDiagnosticCode,
    ParseDiagnosticSeverity, Progress, SourcePosition, SourceRange,
};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use std::cmp;
use std::error::Error;
use std::fmt;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};
use tree_sitter::{Language, Node, ParseOptions, ParseState, Parser, Tree};

const MAX_PARSER_POOL_SIZE: usize = 64;
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Positive bounded number of reusable parsers owned by one language adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserPoolSize(usize);

impl ParserPoolSize {
    /// Creates a pool size within the fixed process-local limit.
    pub fn new(value: usize) -> Result<Self, ParserPoolSizeError> {
        if value == 0 || value > MAX_PARSER_POOL_SIZE {
            return Err(ParserPoolSizeError(value));
        }
        Ok(Self(value))
    }

    /// Returns the number of parser instances.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Parser pool size was zero or exceeded the fixed maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserPoolSizeError(usize);

impl fmt::Display for ParserPoolSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "parser pool size {} is outside 1..=64", self.0)
    }
}

impl Error for ParserPoolSizeError {}

/// Failure while creating a reusable parser pool for one grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserPoolCreateError {
    /// The grammar ABI is incompatible with the Tree-sitter runtime.
    IncompatibleLanguage,
    /// An initialized parser could not enter its bounded owner queue.
    InternalQueue,
}

impl fmt::Display for ParserPoolCreateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleLanguage => {
                formatter.write_str("Tree-sitter language is incompatible")
            }
            Self::InternalQueue => formatter.write_str("parser pool queue initialization failed"),
        }
    }
}

impl Error for ParserPoolCreateError {}

/// Bounded reusable Tree-sitter parser pool with no detached work.
pub struct TreeSitterParserPool {
    available_sender: Sender<Parser>,
    available_receiver: Receiver<Parser>,
    size: ParserPoolSize,
}

impl fmt::Debug for TreeSitterParserPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TreeSitterParserPool")
            .field("size", &self.size)
            .field("available", &self.available_receiver.len())
            .finish()
    }
}

impl TreeSitterParserPool {
    /// Creates and assigns the same immutable grammar to every parser instance.
    pub fn new(language: &Language, size: ParserPoolSize) -> Result<Self, ParserPoolCreateError> {
        let (available_sender, available_receiver) = crossbeam_channel::bounded(size.get());
        for _ in 0..size.get() {
            let mut parser = Parser::new();
            parser
                .set_language(language)
                .map_err(|_| ParserPoolCreateError::IncompatibleLanguage)?;
            available_sender
                .try_send(parser)
                .map_err(|_| ParserPoolCreateError::InternalQueue)?;
        }
        Ok(Self {
            available_sender,
            available_receiver,
            size,
        })
    }

    /// Parses and bounds one source tree while retaining the parser in the owning pool.
    pub fn parse(
        &self,
        source: &[u8],
        policy: LanguageParsePolicy,
        control: &dyn LanguageParseControl,
    ) -> Result<TreeSitterParse, LanguageParseFailure> {
        self.parse_with_timeout(source, policy, control, policy.parse_timeout())
    }

    fn parse_with_timeout(
        &self,
        source: &[u8],
        policy: LanguageParsePolicy,
        control: &dyn LanguageParseControl,
        parse_timeout: Duration,
    ) -> Result<TreeSitterParse, LanguageParseFailure> {
        ensure_active(control)?;
        if source.len() > policy.max_source_bytes() {
            return Err(LanguageParseFailure::InputTooLarge);
        }
        let source_total =
            u64::try_from(source.len()).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
        let progress_total = source_total.max(1);
        report_progress(control, 0, progress_total)?;
        let mut lease = self.checkout(policy.pool_wait_timeout(), control)?;
        let parser = lease
            .parser
            .as_mut()
            .ok_or(LanguageParseFailure::ParseFailed)?;
        let started = Instant::now();
        if started.elapsed() >= parse_timeout {
            return Err(LanguageParseFailure::TimedOut);
        }
        let mut stop_reason = None;
        let mut progress =
            ParseProgress::new(progress_total, source_total, policy.max_progress_events())?;
        let tree = {
            let mut progress_callback = |state: &ParseState| {
                if control.is_cancelled() {
                    stop_reason = Some(ParseStopReason::Cancelled);
                    return ControlFlow::Break(());
                }
                if started.elapsed() >= parse_timeout {
                    stop_reason = Some(ParseStopReason::TimedOut);
                    return ControlFlow::Break(());
                }
                if let Err(error) = progress.observe(state.current_byte_offset(), control) {
                    stop_reason = Some(error);
                    return ControlFlow::Break(());
                }
                ControlFlow::Continue(())
            };
            let options = ParseOptions::new().progress_callback(&mut progress_callback);
            parser.parse_with_options(
                &mut |offset, _position| match source.get(offset..) {
                    Some(remaining) => remaining,
                    None => &[],
                },
                None,
                Some(options),
            )
        };
        parser.reset();
        if let Some(reason) = stop_reason {
            return Err(reason.into_failure());
        }
        let tree = tree.ok_or(LanguageParseFailure::ParseFailed)?;
        let (coverage, diagnostics) =
            inspect_tree(&tree, source.len(), policy, control, started, parse_timeout)?;
        report_progress(control, progress_total, progress_total)?;
        Ok(TreeSitterParse {
            tree,
            coverage,
            diagnostics,
        })
    }

    fn checkout(
        &self,
        timeout: Duration,
        control: &dyn LanguageParseControl,
    ) -> Result<ParserLease<'_>, LanguageParseFailure> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(LanguageParseFailure::ParserUnavailable)?;
        loop {
            ensure_active(control)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(LanguageParseFailure::ParserUnavailable);
            }
            match self
                .available_receiver
                .recv_timeout(cmp::min(remaining, CANCELLATION_POLL_INTERVAL))
            {
                Ok(parser) => {
                    return Ok(ParserLease {
                        parser: Some(parser),
                        sender: &self.available_sender,
                    });
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(LanguageParseFailure::ParserUnavailable);
                }
            }
        }
    }
}

struct ParserLease<'a> {
    parser: Option<Parser>,
    sender: &'a Sender<Parser>,
}

impl Drop for ParserLease<'_> {
    fn drop(&mut self) {
        let Some(parser) = self.parser.take() else {
            return;
        };
        match self.sender.try_send(parser) {
            Ok(()) => {}
            Err(error) => drop(error.into_inner()),
        }
    }
}

/// Successful bounded Tree-sitter parse plus normalized syntax diagnostics.
#[derive(Debug)]
pub struct TreeSitterParse {
    tree: Tree,
    coverage: ParseCoverage,
    diagnostics: Vec<ParseDiagnostic>,
}

impl TreeSitterParse {
    /// Returns the concrete syntax tree for language-specific extraction.
    #[must_use]
    pub const fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Returns byte and incomplete-region coverage.
    #[must_use]
    pub const fn coverage(&self) -> ParseCoverage {
        self.coverage
    }

    /// Returns canonical bounded syntax diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[ParseDiagnostic] {
        &self.diagnostics
    }

    /// Separates the tree from normalized domain observations.
    #[must_use]
    pub fn into_parts(self) -> (Tree, ParseCoverage, Vec<ParseDiagnostic>) {
        (self.tree, self.coverage, self.diagnostics)
    }
}

#[derive(Debug, Clone, Copy)]
enum ParseStopReason {
    Cancelled,
    TimedOut,
    ProgressUnavailable,
}

impl ParseStopReason {
    const fn into_failure(self) -> LanguageParseFailure {
        match self {
            Self::Cancelled => LanguageParseFailure::Cancelled,
            Self::TimedOut => LanguageParseFailure::TimedOut,
            Self::ProgressUnavailable => LanguageParseFailure::ProgressUnavailable,
        }
    }
}

struct ParseProgress {
    total: u64,
    source_total: u64,
    highest: u64,
    next_report: u64,
    stride: u64,
    reports: usize,
    max_reports: usize,
}

impl ParseProgress {
    fn new(
        total: u64,
        source_total: u64,
        max_reports: usize,
    ) -> Result<Self, LanguageParseFailure> {
        let intermediate_slots = max_reports.saturating_sub(2).max(1);
        let intermediate_slots = u64::try_from(intermediate_slots)
            .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
        let stride = total.div_ceil(intermediate_slots).max(1);
        Ok(Self {
            total,
            source_total,
            highest: 0,
            next_report: stride,
            stride,
            reports: 1,
            max_reports,
        })
    }

    fn observe(
        &mut self,
        byte_offset: usize,
        control: &dyn LanguageParseControl,
    ) -> Result<(), ParseStopReason> {
        let observed = u64::try_from(byte_offset)
            .map_or(u64::MAX, |value| value)
            .min(self.source_total)
            .min(self.total.saturating_sub(1));
        self.highest = self.highest.max(observed);
        if self.highest >= self.next_report && self.reports.saturating_add(1) < self.max_reports {
            report_progress(control, self.highest, self.total)
                .map_err(|_| ParseStopReason::ProgressUnavailable)?;
            self.reports = self.reports.saturating_add(1);
            self.next_report = self
                .highest
                .checked_div(self.stride)
                .and_then(|value| value.checked_add(1))
                .and_then(|value| value.checked_mul(self.stride))
                .map_or(self.total, |next| next);
        }
        Ok(())
    }
}

fn inspect_tree(
    tree: &Tree,
    source_bytes: usize,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    started: Instant,
    parse_timeout: Duration,
) -> Result<(ParseCoverage, Vec<ParseDiagnostic>), LanguageParseFailure> {
    let mut stack = vec![(tree.root_node(), 1usize)];
    let mut visited = 0usize;
    let mut diagnostics = Vec::new();
    while let Some((node, depth)) = stack.pop() {
        visited = visited
            .checked_add(1)
            .filter(|count| *count <= policy.max_tree_nodes())
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        if depth > policy.max_tree_depth() {
            return Err(LanguageParseFailure::ResourceLimitExceeded);
        }
        if visited.is_multiple_of(256) {
            ensure_active(control)?;
            if started.elapsed() >= parse_timeout {
                return Err(LanguageParseFailure::TimedOut);
            }
        }
        if node.is_error() || node.is_missing() {
            if diagnostics.len() >= policy.max_diagnostics() {
                return Err(LanguageParseFailure::ResourceLimitExceeded);
            }
            diagnostics.push(syntax_diagnostic(node)?);
        }
        let child_depth = depth
            .checked_add(1)
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        for index in (0..node.child_count()).rev() {
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let child = node.child(index).ok_or(LanguageParseFailure::ParseFailed)?;
            stack.push((child, child_depth));
        }
    }
    ensure_active(control)?;
    diagnostics.sort();
    diagnostics.dedup();
    let (incomplete_bytes, incomplete_regions) = incomplete_summary(&diagnostics)?;
    let covered_bytes = source_bytes.saturating_sub(incomplete_bytes);
    let coverage = ParseCoverage::new(source_bytes, covered_bytes, incomplete_regions)
        .map_err(|_| LanguageParseFailure::InvalidResult)?;
    Ok((coverage, diagnostics))
}

fn syntax_diagnostic(node: Node<'_>) -> Result<ParseDiagnostic, LanguageParseFailure> {
    let (code, message) = if node.is_missing() {
        (
            ParseDiagnosticCode::MissingSyntax,
            "parser inserted missing syntax",
        )
    } else {
        (ParseDiagnosticCode::SyntaxError, "syntax error")
    };
    Ok(ParseDiagnostic::new(
        code,
        ParseDiagnosticSeverity::Error,
        source_range_for_node(node)?,
        DiagnosticMessage::try_from_string(message.to_owned())
            .map_err(|_| LanguageParseFailure::InvalidResult)?,
    ))
}

/// Converts a Tree-sitter node range into the shared bounded source range.
pub fn source_range_for_node(node: Node<'_>) -> Result<SourceRange, LanguageParseFailure> {
    let start = node.start_position();
    let end = node.end_position();
    SourceRange::new(
        node.start_byte(),
        node.end_byte(),
        SourcePosition::new(
            u32::try_from(start.row).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
            u32::try_from(start.column).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
        ),
        SourcePosition::new(
            u32::try_from(end.row).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
            u32::try_from(end.column).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
        ),
    )
    .map_err(|_| LanguageParseFailure::InvalidResult)
}

fn incomplete_summary(
    diagnostics: &[ParseDiagnostic],
) -> Result<(usize, usize), LanguageParseFailure> {
    let mut ranges = diagnostics
        .iter()
        .map(ParseDiagnostic::range)
        .map(|range| (range.start_byte(), range.end_byte()))
        .collect::<Vec<_>>();
    ranges.sort();
    ranges.dedup();
    let mut total = 0u64;
    let mut region_count = 0usize;
    let mut current: Option<(u32, u32)> = None;
    for (start, end) in ranges {
        match current {
            Some((current_start, current_end)) if start <= current_end => {
                current = Some((current_start, current_end.max(end)));
            }
            Some((current_start, current_end)) => {
                total = total
                    .checked_add(u64::from(current_end - current_start))
                    .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
                region_count = region_count
                    .checked_add(1)
                    .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
                current = Some((start, end));
            }
            None => current = Some((start, end)),
        }
    }
    if let Some((start, end)) = current {
        total = total
            .checked_add(u64::from(end - start))
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
        region_count = region_count
            .checked_add(1)
            .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
    }
    let total = usize::try_from(total).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
    Ok((total, region_count))
}

fn ensure_active(control: &dyn LanguageParseControl) -> Result<(), LanguageParseFailure> {
    if control.is_cancelled() {
        return Err(LanguageParseFailure::Cancelled);
    }
    Ok(())
}

fn report_progress(
    control: &dyn LanguageParseControl,
    completed: u64,
    total: u64,
) -> Result<(), LanguageParseFailure> {
    let progress =
        Progress::determinate(completed, total).map_err(|_| LanguageParseFailure::InvalidResult)?;
    control
        .report_progress(progress)
        .map_err(|_| LanguageParseFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{ParserPoolSize, TreeSitterParserPool};
    use a3_application::{
        LanguageParseControl, LanguageParseControlError, LanguageParseFailure, LanguageParsePolicy,
    };
    use a3_domain::Progress;
    use std::error::Error;
    use std::time::Duration;
    use tree_sitter::Language;

    #[derive(Debug, Default)]
    struct ActiveControl;

    impl LanguageParseControl for ActiveControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct CancelledControl;

    impl LanguageParseControl for CancelledControl {
        fn is_cancelled(&self) -> bool {
            true
        }

        fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct RejectCompletionControl;

    impl LanguageParseControl for RejectCompletionControl {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn report_progress(&self, progress: Progress) -> Result<(), LanguageParseControlError> {
            if progress.is_complete() {
                return Err(LanguageParseControlError::Unavailable);
            }
            Ok(())
        }
    }

    #[test]
    fn pool_size_is_positive_and_bounded() {
        assert!(ParserPoolSize::new(0).is_err());
        assert!(ParserPoolSize::new(1).is_ok());
        assert!(ParserPoolSize::new(64).is_ok());
        assert!(ParserPoolSize::new(65).is_err());
    }

    #[test]
    fn syntax_errors_are_partial_and_do_not_poison_reused_parser() -> Result<(), Box<dyn Error>> {
        let pool = json_pool(1)?;
        let invalid = pool.parse(b"{\"value\": }", LanguageParsePolicy::v1(), &ActiveControl)?;
        assert!(!invalid.coverage().is_complete());
        assert!(!invalid.diagnostics().is_empty());

        let valid = pool.parse(b"{\"value\": 1}", LanguageParsePolicy::v1(), &ActiveControl)?;
        assert!(valid.coverage().is_complete());
        assert!(valid.diagnostics().is_empty());
        Ok(())
    }

    #[test]
    fn checkout_is_cancellable_and_has_a_bounded_wait() -> Result<(), Box<dyn Error>> {
        let pool = json_pool(1)?;
        let _lease = pool.checkout(Duration::from_millis(10), &ActiveControl)?;
        assert!(matches!(
            pool.checkout(Duration::from_millis(10), &CancelledControl),
            Err(LanguageParseFailure::Cancelled)
        ));
        assert!(matches!(
            pool.checkout(Duration::from_millis(1), &ActiveControl),
            Err(LanguageParseFailure::ParserUnavailable)
        ));
        Ok(())
    }

    #[test]
    fn timeout_and_progress_failures_return_the_parser_lease() -> Result<(), Box<dyn Error>> {
        let pool = json_pool(1)?;
        assert!(matches!(
            pool.parse_with_timeout(
                b"{}",
                LanguageParsePolicy::v1(),
                &ActiveControl,
                Duration::ZERO,
            ),
            Err(LanguageParseFailure::TimedOut)
        ));
        assert!(matches!(
            pool.parse(b"{}", LanguageParsePolicy::v1(), &RejectCompletionControl,),
            Err(LanguageParseFailure::ProgressUnavailable)
        ));
        assert!(
            pool.parse(b"{}", LanguageParsePolicy::v1(), &ActiveControl)
                .is_ok()
        );
        Ok(())
    }

    fn json_pool(size: usize) -> Result<TreeSitterParserPool, Box<dyn Error>> {
        let language: Language = tree_sitter_json::LANGUAGE.into();
        Ok(TreeSitterParserPool::new(
            &language,
            ParserPoolSize::new(size)?,
        )?)
    }
}
