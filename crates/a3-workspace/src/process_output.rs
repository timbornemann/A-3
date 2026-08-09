use a3_domain::{
    ProcessEventKind, ProcessOutputCapture, ProcessOutputChunk, ProcessOutputContent,
    ProcessOutputContentError, ProcessOutputDigest, ProcessOutputRedaction, ProcessStream,
};
use std::io::Read;
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};

const OUTPUT_READ_CHUNK_BYTES: usize = 8 * 1_024;
const OUTPUT_EVENT_CHUNK_BYTES: usize = 8 * 1_024;

pub(crate) enum ReaderMessage {
    Chunk(ProcessStream, Vec<u8>),
    Eof(ProcessStream),
    Failed,
}

pub(crate) fn spawn_reader<R>(
    stream: ProcessStream,
    mut reader: R,
    sender: SyncSender<ReaderMessage>,
) -> std::io::Result<JoinHandle<()>>
where
    R: Read + Send + 'static,
{
    thread::Builder::new()
        .name(match stream {
            ProcessStream::Stdout => "a3-process-stdout".to_owned(),
            ProcessStream::Stderr => "a3-process-stderr".to_owned(),
        })
        .spawn(move || {
            let mut buffer = [0u8; OUTPUT_READ_CHUNK_BYTES];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        let _ignored = sender.send(ReaderMessage::Eof(stream));
                        return;
                    }
                    Ok(read) => {
                        if sender
                            .send(ReaderMessage::Chunk(stream, buffer[..read].to_vec()))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ignored = sender.send(ReaderMessage::Failed);
                        return;
                    }
                }
            }
        })
}

pub(crate) fn join_reader(handle: JoinHandle<()>) -> Result<(), OutputCollectionError> {
    handle
        .join()
        .map_err(|_| OutputCollectionError::ReaderFailed)
}

pub(crate) struct OutputCollector {
    stream: ProcessStream,
    limit: usize,
    retained: Vec<u8>,
    pending_event: Vec<u8>,
    observed_bytes: u64,
    digest: blake3::Hasher,
    truncated: bool,
    truncation_emitted: bool,
    redaction: Option<ProcessOutputRedaction>,
}

impl OutputCollector {
    pub(crate) fn new(stream: ProcessStream, limit: u32) -> Self {
        Self {
            stream,
            limit: limit as usize,
            retained: Vec::new(),
            pending_event: Vec::new(),
            observed_bytes: 0,
            digest: blake3::Hasher::new(),
            truncated: false,
            truncation_emitted: false,
            redaction: None,
        }
    }

    pub(crate) fn ingest(
        &mut self,
        bytes: &[u8],
        emit: &mut impl FnMut(ProcessEventKind) -> Result<(), OutputCollectionError>,
    ) -> Result<(), OutputCollectionError> {
        self.observed_bytes = self
            .observed_bytes
            .checked_add(
                u64::try_from(bytes.len()).map_err(|_| OutputCollectionError::InvalidResult)?,
            )
            .ok_or(OutputCollectionError::InvalidResult)?;
        self.digest.update(bytes);

        if self.redaction.is_none() && self.retained.len() < self.limit {
            let retained = (self.limit - self.retained.len()).min(bytes.len());
            self.retained.extend_from_slice(&bytes[..retained]);
            self.pending_event.extend_from_slice(&bytes[..retained]);
            self.flush_complete_lines(emit)?;
        }
        if self.observed_bytes > self.limit as u64 {
            self.truncated = true;
            if !self.truncation_emitted {
                self.flush_pending(emit)?;
                emit(ProcessEventKind::OutputTruncated {
                    stream: self.stream,
                    observed_bytes: self.observed_bytes,
                })?;
                self.truncation_emitted = true;
            }
        }
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        emit: &mut impl FnMut(ProcessEventKind) -> Result<(), OutputCollectionError>,
    ) -> Result<ProcessOutputCapture, OutputCollectionError> {
        self.flush_pending(emit)?;
        let content = match self.redaction {
            Some(reason) => ProcessOutputContent::redacted(reason),
            None => {
                let text = String::from_utf8(self.retained)
                    .map_err(|_| OutputCollectionError::InvalidResult)?;
                ProcessOutputContent::text(text)
                    .map_err(|_| OutputCollectionError::InvalidResult)?
            }
        };
        ProcessOutputCapture::new(
            self.stream,
            content,
            self.observed_bytes,
            u32::try_from(self.limit).map_err(|_| OutputCollectionError::InvalidResult)?,
            self.truncated,
            ProcessOutputDigest::from_bytes(*self.digest.finalize().as_bytes()),
        )
        .map_err(|_| OutputCollectionError::InvalidResult)
    }

    fn flush_complete_lines(
        &mut self,
        emit: &mut impl FnMut(ProcessEventKind) -> Result<(), OutputCollectionError>,
    ) -> Result<(), OutputCollectionError> {
        while let Some(end) = self.pending_event.iter().position(|byte| *byte == b'\n') {
            let remainder = self.pending_event.split_off(end.saturating_add(1));
            let line = std::mem::replace(&mut self.pending_event, remainder);
            self.emit_candidate(line, false, emit)?;
            if self.redaction.is_some() {
                self.pending_event.clear();
                break;
            }
        }
        Ok(())
    }

    fn flush_pending(
        &mut self,
        emit: &mut impl FnMut(ProcessEventKind) -> Result<(), OutputCollectionError>,
    ) -> Result<(), OutputCollectionError> {
        if self.redaction.is_some() || self.pending_event.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending_event);
        self.emit_candidate(pending, true, emit)
    }

    fn emit_candidate(
        &mut self,
        mut bytes: Vec<u8>,
        final_fragment: bool,
        emit: &mut impl FnMut(ProcessEventKind) -> Result<(), OutputCollectionError>,
    ) -> Result<(), OutputCollectionError> {
        let text = match std::str::from_utf8(&bytes) {
            Ok(_) => String::from_utf8(bytes).map_err(|_| OutputCollectionError::InvalidResult)?,
            Err(error) if final_fragment && self.truncated && error.error_len().is_none() => {
                let invalid_suffix = bytes.len().saturating_sub(error.valid_up_to());
                bytes.truncate(error.valid_up_to());
                let retained_length = self.retained.len().saturating_sub(invalid_suffix);
                self.retained.truncate(retained_length);
                String::from_utf8(bytes).map_err(|_| OutputCollectionError::InvalidResult)?
            }
            Err(_) => {
                return self.redact(ProcessOutputRedaction::InvalidUtf8, emit);
            }
        };
        if text.is_empty() {
            return Ok(());
        }
        let safe = match ProcessOutputContent::text(text) {
            Ok(content) => content,
            Err(error) => {
                let reason = match error {
                    ProcessOutputContentError::SecretCandidate => {
                        ProcessOutputRedaction::SecretCandidate
                    }
                    ProcessOutputContentError::UnsafeControl => {
                        ProcessOutputRedaction::UnsafeControl
                    }
                };
                return self.redact(reason, emit);
            }
        };
        let text = safe.as_text().ok_or(OutputCollectionError::InvalidResult)?;
        let mut start = 0usize;
        while start < text.len() {
            let mut end = start
                .saturating_add(OUTPUT_EVENT_CHUNK_BYTES)
                .min(text.len());
            while end > start && !text.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            if end == start {
                return Err(OutputCollectionError::InvalidResult);
            }
            let chunk = ProcessOutputChunk::try_from_string(text[start..end].to_owned())
                .map_err(|_| OutputCollectionError::InvalidResult)?;
            emit(ProcessEventKind::Output {
                stream: self.stream,
                chunk,
            })?;
            start = end;
        }
        Ok(())
    }

    fn redact(
        &mut self,
        reason: ProcessOutputRedaction,
        emit: &mut impl FnMut(ProcessEventKind) -> Result<(), OutputCollectionError>,
    ) -> Result<(), OutputCollectionError> {
        self.redaction = Some(reason);
        self.retained.clear();
        self.pending_event.clear();
        emit(ProcessEventKind::OutputRedacted {
            stream: self.stream,
            reason,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputCollectionError {
    ReaderFailed,
    EventUnavailable,
    InvalidResult,
}
