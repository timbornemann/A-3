//! Canonical content-free encoding for ADR-0121's one guarded Test recipe.

use a3_domain::{
    ContentHash, FileRevision, PythonIntegerTranslation, PythonIntegerTranslationWitness,
    RepositoryPath, SourcePosition, SourceRange, SymbolName,
};
use libsql::{Row, Transaction, Value};

pub(crate) const SCHEMA_VERSION: u32 = 55;

const VERSION: u8 = 1;
const HEADER: usize = 15;
const DIGEST: usize = 32;
const WITNESS: usize = 80;
const MAX_PATH: usize = 131_072;
const MAX_NAME: usize = 1_024;

pub(crate) const REQUIREMENT_SELECT: &str = ",CASE
    WHEN typeof(python_integer_translation)='blob' AND length(python_integer_translation) BETWEEN 51 AND 132143
    THEN python_integer_translation WHEN python_integer_translation IS NOT NULL THEN x'' END";

pub(crate) const WITNESS_SELECT: &str = ",CASE
    WHEN typeof(python_integer_translation_witness)='blob' AND length(python_integer_translation_witness) BETWEEN 131 AND 132223
    THEN python_integer_translation_witness WHEN python_integer_translation_witness IS NOT NULL THEN x'' END";

#[derive(Debug)]
pub(crate) struct InvalidGuard;

pub(crate) async fn supported(tx: &Transaction) -> libsql::Result<bool> {
    crate::migration::read_user_version(tx)
        .await
        .map(|version| version >= SCHEMA_VERSION)
}

pub(crate) fn write_requirement(requirement: &PythonIntegerTranslation) -> Vec<u8> {
    let mut payload = encode_target(requirement, VERSION);
    seal(&mut payload, b"requirement");
    payload
}

pub(crate) fn write_witness(witness: &PythonIntegerTranslationWitness) -> Vec<u8> {
    let mut payload = encode_target(
        witness.requirement(),
        PythonIntegerTranslationWitness::VERSION,
    );
    payload.extend_from_slice(witness.revision().content_hash().as_bytes());
    for range in [witness.declaration(), witness.expression()] {
        for field in [
            range.start_byte(),
            range.end_byte(),
            range.start_position().row(),
            range.start_position().column(),
            range.end_position().row(),
            range.end_position().column(),
        ] {
            payload.extend_from_slice(&field.to_le_bytes());
        }
    }
    seal(&mut payload, b"witness");
    payload
}

pub(crate) fn read_requirement(
    row: &Row,
    column: i32,
) -> Result<Option<PythonIntegerTranslation>, InvalidGuard> {
    read_blob(row, column)?
        .map(|bytes| {
            let payload = unseal(&bytes, b"requirement")?;
            let (target, rest) = decode_target(payload, VERSION)?;
            if !rest.is_empty() {
                return Err(InvalidGuard);
            }
            Ok(target)
        })
        .transpose()
}

pub(crate) fn read_witness(
    row: &Row,
    column: i32,
) -> Result<Option<PythonIntegerTranslationWitness>, InvalidGuard> {
    read_blob(row, column)?
        .map(|bytes| {
            let payload = unseal(&bytes, b"witness")?;
            let (target, rest) = decode_target(payload, PythonIntegerTranslationWitness::VERSION)?;
            if rest.len() != WITNESS {
                return Err(InvalidGuard);
            }
            let (hash, ranges) = take::<32>(rest)?;
            let (declaration, ranges) = decode_range(ranges)?;
            let (expression, _) = decode_range(ranges)?;
            let revision = FileRevision::new(target.path().clone(), ContentHash::from_bytes(hash));
            PythonIntegerTranslationWitness::new(target, revision, declaration, expression)
                .map_err(|_| InvalidGuard)
        })
        .transpose()
}

fn read_blob(row: &Row, column: i32) -> Result<Option<Vec<u8>>, InvalidGuard> {
    match row.get_value(column).map_err(|_| InvalidGuard)? {
        Value::Null => Ok(None),
        Value::Blob(bytes) => Ok(Some(bytes)),
        _ => Err(InvalidGuard),
    }
}

fn encode_target(target: &PythonIntegerTranslation, version: u8) -> Vec<u8> {
    let path = target.path().as_bytes();
    let name = target.function().as_str().as_bytes();
    let mut output = Vec::with_capacity(HEADER + path.len() + name.len() + DIGEST);
    output.push(version);
    // RepositoryPath and SymbolName already enforce narrower construction bounds.
    output.extend_from_slice(&(path.len() as u32).to_le_bytes());
    output.extend_from_slice(&(name.len() as u16).to_le_bytes());
    output.extend_from_slice(&target.delta().to_le_bytes());
    output.extend_from_slice(path);
    output.extend_from_slice(name);
    output
}

fn decode_target(
    payload: &[u8],
    version: u8,
) -> Result<(PythonIntegerTranslation, &[u8]), InvalidGuard> {
    let ([stored_version], payload) = take::<1>(payload)?;
    if stored_version != version {
        return Err(InvalidGuard);
    }
    let (path_len, payload) = take::<4>(payload)?;
    let (name_len, payload) = take::<2>(payload)?;
    let (delta, payload) = take::<8>(payload)?;
    let path_len = u32::from_le_bytes(path_len) as usize;
    let name_len = usize::from(u16::from_le_bytes(name_len));
    if !(3..=MAX_PATH).contains(&path_len) || !(1..=MAX_NAME).contains(&name_len) {
        return Err(InvalidGuard);
    }
    let (path, payload) = payload.split_at_checked(path_len).ok_or(InvalidGuard)?;
    let (name, rest) = payload.split_at_checked(name_len).ok_or(InvalidGuard)?;
    let path = RepositoryPath::try_from_bytes(path.to_vec()).map_err(|_| InvalidGuard)?;
    let name = std::str::from_utf8(name).map_err(|_| InvalidGuard)?;
    let function = SymbolName::try_from_string(name.to_owned()).map_err(|_| InvalidGuard)?;
    if function.as_str() != name {
        return Err(InvalidGuard);
    }
    let target = PythonIntegerTranslation::new(path, function, i64::from_le_bytes(delta))
        .map_err(|_| InvalidGuard)?;
    Ok((target, rest))
}

fn decode_range(bytes: &[u8]) -> Result<(SourceRange, &[u8]), InvalidGuard> {
    let (start, bytes) = take::<4>(bytes)?;
    let (end, bytes) = take::<4>(bytes)?;
    let (start_row, bytes) = take::<4>(bytes)?;
    let (start_col, bytes) = take::<4>(bytes)?;
    let (end_row, bytes) = take::<4>(bytes)?;
    let (end_col, bytes) = take::<4>(bytes)?;
    let range = SourceRange::new(
        u32::from_le_bytes(start) as usize,
        u32::from_le_bytes(end) as usize,
        SourcePosition::new(u32::from_le_bytes(start_row), u32::from_le_bytes(start_col)),
        SourcePosition::new(u32::from_le_bytes(end_row), u32::from_le_bytes(end_col)),
    )
    .map_err(|_| InvalidGuard)?;
    Ok((range, bytes))
}

fn take<const N: usize>(input: &[u8]) -> Result<([u8; N], &[u8]), InvalidGuard> {
    let (head, tail) = input.split_at_checked(N).ok_or(InvalidGuard)?;
    Ok((head.try_into().map_err(|_| InvalidGuard)?, tail))
}

fn digest(payload: &[u8], kind: &[u8]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new_derive_key("a3 durable python integer translation v1");
    hasher.update(kind);
    hasher.update(payload);
    hasher.finalize()
}

fn seal(payload: &mut Vec<u8>, kind: &[u8]) {
    payload.extend_from_slice(digest(payload, kind).as_bytes());
}

fn unseal<'a>(bytes: &'a [u8], kind: &[u8]) -> Result<&'a [u8], InvalidGuard> {
    let split = bytes.len().checked_sub(DIGEST).ok_or(InvalidGuard)?;
    let (payload, stored) = bytes.split_at(split);
    if digest(payload, kind).as_bytes() != stored {
        return Err(InvalidGuard);
    }
    Ok(payload)
}
