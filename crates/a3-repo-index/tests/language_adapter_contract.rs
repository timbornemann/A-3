//! Common V1 contract exercised through a minimal Tree-sitter JSON probe.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseFailure, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    Confidence, IndexLanguage, LanguageAdapterContractVersion, LanguageAdapterRevision,
    LanguageAdapterVersion, LanguageParseArtifacts, LanguageParseResult, LocalSymbolId,
    ParsedSymbol, RepositoryPath, SymbolKind, SymbolName, SyntaxProvider, SyntaxRelation,
    SyntaxRelationKind, SyntaxSource, SyntaxTarget,
};
use a3_language_adapter_contract_tests::{
    ContractResult, LanguageAdapterContractFixture, verify_language_adapter_contract,
};
use a3_repo_index::{
    ParserPoolSize, PythonLanguageAdapter, RustLanguageAdapter, TreeSitterParserPool,
    TypeScriptJavaScriptLanguageAdapter, source_range_for_node, verify_language_parse_input,
};
use std::str;
use tree_sitter::{Language, Node};

const VALID_JSON: &[u8] = b"{\"alpha\":1,\"beta\":2}\n";
const INVALID_JSON: &[u8] = b"{\"alpha\": }\n";
const VALID_RUST: &[u8] = b"pub fn alpha() {\n    beta();\n}\n\nfn beta() {}\n";
const INVALID_RUST: &[u8] = b"pub fn broken( {\n";
const VALID_TYPESCRIPT: &[u8] = b"export function alpha(): number {\n  return beta();\n}\n\nfunction beta(): number { return 1; }\n";
const INVALID_TYPESCRIPT: &[u8] = b"export function broken( {\n";
const VALID_PYTHON: &[u8] = b"def alpha():\n    return beta()\n\ndef beta():\n    return 1\n";
const INVALID_PYTHON: &[u8] = b"def broken(\n    return value\n";

#[derive(Debug)]
struct JsonContractAdapter {
    revision: LanguageAdapterRevision,
    pool: TreeSitterParserPool,
}

impl JsonContractAdapter {
    fn new() -> ContractResult<Self> {
        let language: Language = tree_sitter_json::LANGUAGE.into();
        Ok(Self {
            revision: LanguageAdapterRevision::new(
                IndexLanguage::Generic,
                LanguageAdapterVersion::try_from_string(
                    "contract-json-tree-sitter-0.24.8-v1".to_owned(),
                )?,
            ),
            pool: TreeSitterParserPool::new(&language, ParserPoolSize::new(2)?)?,
        })
    }

    fn extract_pair(
        &self,
        pair: Node<'_>,
        source: &[u8],
        id: LocalSymbolId,
    ) -> Result<(ParsedSymbol, SyntaxRelation), LanguageParseFailure> {
        let key = pair
            .child_by_field_name("key")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let key_bytes = source
            .get(key.byte_range())
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let key_text =
            str::from_utf8(key_bytes).map_err(|_| LanguageParseFailure::InvalidResult)?;
        let name = key_text
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let declaration_range = source_range_for_node(pair)?;
        let selection_range = source_range_for_node(key)?;
        let symbol = ParsedSymbol::new(
            id,
            SymbolKind::Field,
            SymbolName::try_from_string(name.to_owned())
                .map_err(|_| LanguageParseFailure::InvalidResult)?,
            declaration_range,
            selection_range,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)?;
        let relation = SyntaxRelation::new(
            SyntaxSource::File,
            SyntaxTarget::Symbol(id),
            SyntaxRelationKind::Defines,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            declaration_range,
        );
        Ok((symbol, relation))
    }
}

impl LanguageAdapter for JsonContractAdapter {
    fn revision(&self) -> &LanguageAdapterRevision {
        &self.revision
    }

    fn contract_version(&self) -> LanguageAdapterContractVersion {
        LanguageAdapterContractVersion::v1()
    }

    fn supports_path(&self, path: &RepositoryPath) -> bool {
        path.as_bytes().ends_with(b".json")
    }

    fn parse(
        &self,
        input: LanguageParseInput<'_>,
        policy: LanguageParsePolicy,
        control: &dyn LanguageParseControl,
    ) -> Result<LanguageParseResult, LanguageParseFailure> {
        if !self.supports_path(input.revision().path()) {
            return Err(LanguageParseFailure::UnsupportedPath);
        }
        if policy.contract_version() != self.contract_version() {
            return Err(LanguageParseFailure::InvalidResult);
        }
        verify_language_parse_input(input, policy, control)?;
        let parsed = self.pool.parse(input.source(), policy, control)?;
        let (tree, coverage, diagnostics) = parsed.into_parts();
        let root = tree.root_node();
        let object = root
            .named_child(0)
            .filter(|node| node.kind() == "object")
            .ok_or(LanguageParseFailure::InvalidResult)?;
        let mut artifacts = LanguageParseArtifacts {
            diagnostics,
            ..LanguageParseArtifacts::default()
        };
        for index in 0..object.named_child_count() {
            if control.is_cancelled() {
                return Err(LanguageParseFailure::Cancelled);
            }
            if artifacts.symbols.len() >= policy.max_symbols()
                || artifacts.relations.len() >= policy.max_relations()
            {
                return Err(LanguageParseFailure::ResourceLimitExceeded);
            }
            let index =
                u32::try_from(index).map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?;
            let pair = object
                .named_child(index)
                .ok_or(LanguageParseFailure::InvalidResult)?;
            if pair.kind() != "pair" {
                continue;
            }
            let next_id = artifacts
                .symbols
                .len()
                .checked_add(1)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or(LanguageParseFailure::ResourceLimitExceeded)?;
            let (symbol, relation) = self.extract_pair(
                pair,
                input.source(),
                LocalSymbolId::new(next_id)
                    .map_err(|_| LanguageParseFailure::ResourceLimitExceeded)?,
            )?;
            artifacts.symbols.push(symbol);
            artifacts.relations.push(relation);
        }
        LanguageParseResult::new(
            input.revision().clone(),
            self.revision.clone(),
            self.contract_version(),
            coverage,
            artifacts,
        )
        .map_err(|_| LanguageParseFailure::InvalidResult)
    }
}

#[test]
fn tree_sitter_adapter_satisfies_the_shared_v1_contract() -> ContractResult<()> {
    let adapter = JsonContractAdapter::new()?;
    verify_language_adapter_contract(
        &adapter,
        LanguageAdapterContractFixture {
            supported_path: b"fixture.json",
            unsupported_path: b"fixture.txt",
            valid_source: VALID_JSON,
            invalid_source: INVALID_JSON,
            expected_golden: concat!(
                "path=666978747572652e6a736f6e hash=8a2432cb9438d280cfd54f6b712dad05531ef8509aa555f9470bd43a7e3a6298 language=generic adapter=contract-json-tree-sitter-0.24.8-v1 contract=1 coverage=21/21/0\n",
                "symbol id=1 kind=Field name=\"alpha\" signature=None declaration=1..10@0:1..0:10 selection=1..8@0:1..0:8 documentation=- visibility=Unknown test=false entrypoint=false\n",
                "symbol id=2 kind=Field name=\"beta\" signature=None declaration=11..19@0:11..0:19 selection=11..17@0:11..0:17 documentation=- visibility=Unknown test=false entrypoint=false\n",
                "relation source=File target=Symbol(LocalSymbolId(1)) kind=Defines provider=TreeSitter confidence=10000 evidence=1..10@0:1..0:10\n",
                "relation source=File target=Symbol(LocalSymbolId(2)) kind=Defines provider=TreeSitter confidence=10000 evidence=11..19@0:11..0:19\n",
            ),
        },
    )
}

#[test]
fn rust_adapter_satisfies_the_shared_v1_contract() -> ContractResult<()> {
    let adapter = RustLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    verify_language_adapter_contract(
        &adapter,
        LanguageAdapterContractFixture {
            supported_path: b"src/lib.rs",
            unsupported_path: b"src/lib.txt",
            valid_source: VALID_RUST,
            invalid_source: INVALID_RUST,
            expected_golden: concat!(
                "path=7372632f6c69622e7273 hash=c478091adfc0f68932a250d62387a2a921fd7d47b3faaec65e8e101e6a10550f language=rust adapter=rust-tree-sitter-0.24.2-cargo-v1-flow-v1-contract-v1 contract=1 coverage=45/45/0\n",
                "symbol id=1 kind=Module name=\"lib\" signature=None declaration=0..45@0:0..5:0 selection=0..0@0:0..0:0 documentation=- visibility=Internal test=false entrypoint=true\n",
                "symbol id=2 kind=Function name=\"alpha\" signature=Some(\"pub fn alpha()\") declaration=0..30@0:0..2:1 selection=7..12@0:7..0:12 documentation=- visibility=Public test=false entrypoint=false\n",
                "symbol id=3 kind=Function name=\"beta\" signature=Some(\"fn beta()\") declaration=32..44@4:0..4:12 selection=35..39@4:3..4:7 documentation=- visibility=Private test=false entrypoint=false\n",
                "relation source=File target=Symbol(LocalSymbolId(1)) kind=Defines provider=TreeSitter confidence=10000 evidence=0..45@0:0..5:0\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(2)) kind=Contains provider=TreeSitter confidence=10000 evidence=0..30@0:0..2:1\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(2)) kind=Exports provider=TreeSitter confidence=10000 evidence=7..12@0:7..0:12\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(3)) kind=Contains provider=TreeSitter confidence=10000 evidence=32..44@4:0..4:12\n",
                "relation source=Symbol(LocalSymbolId(2)) target=Unresolved(SymbolReference(\"beta\")) kind=Calls provider=TreeSitter confidence=7500 evidence=21..25@1:4..1:8\n",
            ),
        },
    )
}

#[test]
fn typescript_javascript_adapter_satisfies_the_shared_v1_contract() -> ContractResult<()> {
    let adapter = TypeScriptJavaScriptLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    verify_language_adapter_contract(
        &adapter,
        LanguageAdapterContractFixture {
            supported_path: b"src/index.ts",
            unsupported_path: b"src/index.txt",
            valid_source: VALID_TYPESCRIPT,
            invalid_source: INVALID_TYPESCRIPT,
            expected_golden: concat!(
                "path=7372632f696e6465782e7473 hash=e594b82bec6b1823c4e94d3e239ff3d7f5a0230c97798c146434655c96de8e79 language=typescript-javascript adapter=typescript-javascript-ts-0.23.2-js-0.25.0-json-0.24.8-package-v1-flow-v1-contract-v1 contract=1 coverage=92/92/0\n",
                "symbol id=1 kind=Module name=\"index\" signature=None declaration=0..92@0:0..5:0 selection=0..0@0:0..0:0 documentation=- visibility=Internal test=false entrypoint=true\n",
                "symbol id=2 kind=Function name=\"alpha\" signature=Some(\"function alpha(): number\") declaration=7..52@0:7..2:1 selection=16..21@0:16..0:21 documentation=- visibility=Public test=false entrypoint=false\n",
                "symbol id=3 kind=Function name=\"beta\" signature=Some(\"function beta(): number\") declaration=54..91@4:0..4:37 selection=63..67@4:9..4:13 documentation=- visibility=Internal test=false entrypoint=false\n",
                "relation source=File target=Symbol(LocalSymbolId(1)) kind=Defines provider=TreeSitter confidence=10000 evidence=0..92@0:0..5:0\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(2)) kind=Contains provider=TreeSitter confidence=10000 evidence=7..52@0:7..2:1\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(2)) kind=Exports provider=TreeSitter confidence=10000 evidence=16..21@0:16..0:21\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(3)) kind=Contains provider=TreeSitter confidence=10000 evidence=54..91@4:0..4:37\n",
                "relation source=Symbol(LocalSymbolId(2)) target=Unresolved(SymbolReference(\"beta\")) kind=Calls provider=TreeSitter confidence=7500 evidence=43..47@1:9..1:13\n",
            ),
        },
    )
}

#[test]
fn python_adapter_satisfies_the_shared_v1_contract() -> ContractResult<()> {
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(2)?)?;
    verify_language_adapter_contract(
        &adapter,
        LanguageAdapterContractFixture {
            supported_path: b"src/core.py",
            unsupported_path: b"src/core.txt",
            valid_source: VALID_PYTHON,
            invalid_source: INVALID_PYTHON,
            expected_golden: concat!(
                "path=7372632f636f72652e7079 hash=d44fbff365fa31cd6a340a24582b052ce1fd773a69b87626569e03cbe6ef6701 language=python adapter=python-tree-sitter-0.25.0-pyproject-setup-requirements-v1-flow-v1-contract-v1 contract=1 coverage=57/57/0\n",
                "symbol id=1 kind=Module name=\"core\" signature=None declaration=0..57@0:0..5:0 selection=0..0@0:0..0:0 documentation=- visibility=Internal test=false entrypoint=false\n",
                "symbol id=2 kind=Function name=\"alpha\" signature=Some(\"def alpha()\") declaration=0..30@0:0..1:17 selection=4..9@0:4..0:9 documentation=- visibility=Public test=false entrypoint=false\n",
                "symbol id=3 kind=Function name=\"beta\" signature=Some(\"def beta()\") declaration=32..56@3:0..4:12 selection=36..40@3:4..3:8 documentation=- visibility=Public test=false entrypoint=false\n",
                "relation source=File target=Symbol(LocalSymbolId(1)) kind=Defines provider=TreeSitter confidence=10000 evidence=0..57@0:0..5:0\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(2)) kind=Contains provider=TreeSitter confidence=10000 evidence=0..30@0:0..1:17\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(2)) kind=Exports provider=LanguageHeuristic confidence=8500 evidence=4..9@0:4..0:9\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(3)) kind=Contains provider=TreeSitter confidence=10000 evidence=32..56@3:0..4:12\n",
                "relation source=Symbol(LocalSymbolId(1)) target=Symbol(LocalSymbolId(3)) kind=Exports provider=LanguageHeuristic confidence=8500 evidence=36..40@3:4..3:8\n",
                "relation source=Symbol(LocalSymbolId(2)) target=Unresolved(SymbolReference(\"beta\")) kind=Calls provider=TreeSitter confidence=7000 evidence=24..28@1:11..1:15\n",
            ),
        },
    )
}
