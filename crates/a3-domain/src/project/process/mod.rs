mod result;
mod spec;

pub use result::{
    ProcessDuration, ProcessEvent, ProcessEventKind, ProcessEventSequence,
    ProcessEventSequenceError, ProcessExit, ProcessExitError, ProcessOutputCapture,
    ProcessOutputCaptureError, ProcessOutputChunk, ProcessOutputChunkError, ProcessOutputContent,
    ProcessOutputContentError, ProcessOutputDigest, ProcessOutputRedaction, ProcessRunResult,
    ProcessRunResultError, ProcessStream, ProcessTermination,
};
pub use spec::{
    ProcessArgument, ProcessArgumentError, ProcessEnvironmentVariable,
    ProcessEnvironmentVariableError, ProcessExecutable, ProcessExecutableError, ProcessOutputLimit,
    ProcessOutputLimitError, ProcessSpec, ProcessSpecError, ProcessSpecSchemaVersion,
    ProcessTimeout, ProcessTimeoutError,
};
