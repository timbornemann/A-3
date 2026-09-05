use crate::source_range_for_node;
use a3_application::{LanguageParseControl, LanguageParseFailure, LanguageParseInput};
use a3_domain::{
    FlowArgument, FlowGap, FlowGapKind, FlowStep, FlowStepId, FlowStepKind, FlowValue, FlowValueId,
    FlowValueKind, FunctionFlow, IndexLanguage, ParsedSymbol, SourceRange, SymbolKind, SymbolName,
    SymbolReference,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use tree_sitter::Node;

type Bindings = Vec<BTreeMap<String, FlowValueId>>;

pub(super) fn analyze(
    mut node: Node<'_>,
    owner: &ParsedSymbol,
    input: LanguageParseInput<'_>,
    language: IndexLanguage,
    control: &dyn LanguageParseControl,
    context: (
        Instant,
        &[a3_domain::StaticImportBinding],
        &BTreeSet<String>,
    ),
) -> Result<FunctionFlow, LanguageParseFailure> {
    let mut ancestor = node.parent();
    let mut lexical_scope = owner.declaration_range();
    while let Some(parent) = ancestor {
        lexical_scope = source_range_for_node(parent)?;
        if matches!(
            parent.kind(),
            "block" | "statement_block" | "source_file" | "program" | "module"
        ) {
            break;
        }
        ancestor = parent.parent();
    }
    if node.kind() == "variable_declarator" {
        node = node.child_by_field_name("value").unwrap_or(node);
    }
    if node.kind() == "decorated_definition" {
        node = node.child_by_field_name("definition").unwrap_or(node);
    }
    let mut builder = Builder {
        source: input.source(),
        range: owner.declaration_range(),
        language,
        control,
        deadline: context.0,
        imports: context.1,
        blocked: context.2,
        visited: 0,
        elements: 0,
        steps: Vec::new(),
        values: Vec::new(),
        gaps: Vec::new(),
        bindings: vec![BTreeMap::new()],
        depth: 0,
    };
    let result = (|| {
        if let Some(parameters) = node
            .child_by_field_name("parameters")
            .or_else(|| node.child_by_field_name("parameter"))
        {
            for parameter in parameter_nodes(parameters) {
                let pattern = parameter
                    .child_by_field_name("pattern")
                    .or_else(|| parameter.child_by_field_name("name"))
                    .or_else(|| parameter.child_by_field_name("left"))
                    .unwrap_or(parameter);
                if !matches!(pattern.kind(), "identifier" | "self_parameter")
                    || matches!(
                        parameter.kind(),
                        "default_parameter"
                            | "typed_default_parameter"
                            | "rest_pattern"
                            | "list_splat_pattern"
                            | "dictionary_splat_pattern"
                    )
                {
                    builder.gap(FlowGapKind::Unsupported, parameter)?;
                }
                builder.define_pattern(
                    pattern,
                    FlowValueKind::Parameter,
                    Vec::new(),
                    None,
                    builder.range,
                    true,
                )?;
            }
        }
        let body = node.child_by_field_name("body").unwrap_or(node);
        if owner.kind() != SymbolKind::Module && node.child_by_field_name("body").is_none() {
            builder.gap(FlowGapKind::Unsupported, node)?;
            return Ok(());
        }
        let mut cursor = node.walk();
        let deferred = node.children(&mut cursor).any(|n| n.kind() == "async")
            || node.kind().contains("generator");
        let parent = if deferred {
            Some(builder.step(FlowStepKind::Deferred, node, None, None, Vec::new())?)
        } else {
            None
        };
        if owner.kind() == SymbolKind::Module {
            builder.sequence(body, None, builder.range)?;
        } else {
            let values = builder.walk(body, parent, builder.range)?;
            if !values.is_empty()
                && (language == IndexLanguage::Rust
                    || (!matches!(body.kind(), "block" | "statement_block")
                        && matches!(
                            node.kind(),
                            "arrow_function" | "closure_expression" | "lambda"
                        )))
            {
                builder.step(FlowStepKind::Return, body, parent, None, values)?;
            }
        }
        Ok::<(), BuildError>(())
    })();
    match result {
        Err(BuildError::Cancelled) => return Err(LanguageParseFailure::Cancelled),
        Err(BuildError::TimedOut) => return Err(LanguageParseFailure::TimedOut),
        Err(BuildError::Invalid) => return Err(LanguageParseFailure::InvalidResult),
        Err(BuildError::Limit) => {
            builder.steps.clear();
            builder.values.clear();
            builder.gaps.clear();
            builder.gaps.push(FlowGap {
                kind: FlowGapKind::Limit,
                range: builder.range,
            });
        }
        Ok(()) => {}
    }
    FunctionFlow::new(
        owner.id(),
        builder.range,
        builder.steps,
        builder.values,
        builder.gaps,
    )
    .and_then(|f| f.with_lexical_scope(lexical_scope))
    .map_err(|_| LanguageParseFailure::InvalidResult)
}

#[derive(Debug)]
enum BuildError {
    Invalid,
    Limit,
    Cancelled,
    TimedOut,
}

struct Builder<'a> {
    blocked: &'a BTreeSet<String>,
    imports: &'a [a3_domain::StaticImportBinding],
    source: &'a [u8],
    range: SourceRange,
    language: IndexLanguage,
    control: &'a dyn LanguageParseControl,
    deadline: Instant,
    visited: usize,
    elements: usize,
    steps: Vec<FlowStep>,
    values: Vec<FlowValue>,
    gaps: Vec<FlowGap>,
    bindings: Bindings,
    depth: usize,
}

impl Builder<'_> {
    fn poll(&mut self) -> Result<(), BuildError> {
        self.visited += 1;
        if self.visited.is_multiple_of(64) {
            if self.control.is_cancelled() {
                return Err(BuildError::Cancelled);
            }
            if Instant::now() >= self.deadline {
                return Err(BuildError::TimedOut);
            }
        }
        if self.visited > 32_768 {
            return Err(BuildError::Limit);
        }
        Ok(())
    }
    fn charge(&mut self, elements: usize) -> Result<(), BuildError> {
        self.elements = self.elements.saturating_add(elements);
        if self.elements >= a3_domain::MAX_FUNCTION_FLOW_ELEMENTS {
            return Err(BuildError::Limit);
        }
        Ok(())
    }
    fn gap(&mut self, kind: FlowGapKind, node: Node<'_>) -> Result<(), BuildError> {
        let gap = FlowGap {
            kind,
            range: range(node)?,
        };
        if !self.gaps.contains(&gap) {
            self.charge(1)?;
            self.gaps.push(gap);
        }
        Ok(())
    }
    fn step(
        &mut self,
        kind: FlowStepKind,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        name: Option<String>,
        inputs: Vec<FlowValueId>,
    ) -> Result<FlowStepId, BuildError> {
        self.poll()?;
        let id = FlowStepId::new((self.steps.len() + 1) as u32).map_err(|_| BuildError::Limit)?;
        let inputs = unique(inputs);
        self.charge(1 + inputs.len())?;
        self.steps.push(FlowStep {
            id,
            kind,
            parent,
            range: range(node)?,
            name: name.and_then(|n| SymbolReference::try_from_string(n).ok()),
            callee_range: None,
            process: None,
            inputs,
            outputs: Vec::new(),
            arguments: Vec::new(),
        });
        Ok(id)
    }
    fn value(
        &mut self,
        name: String,
        kind: FlowValueKind,
        node: Node<'_>,
        scope: SourceRange,
        dependencies: Vec<FlowValueId>,
        producer: Option<FlowStepId>,
    ) -> Result<FlowValueId, BuildError> {
        self.poll()?;
        let id = FlowValueId::new((self.values.len() + 1) as u32).map_err(|_| BuildError::Limit)?;
        let dependencies = unique(dependencies);
        self.charge(1 + dependencies.len())?;
        self.values.push(FlowValue {
            id,
            name: SymbolName::try_from_string(name).map_err(|_| BuildError::Limit)?,
            kind,
            range: range(node)?,
            scope,
            dependencies,
            producer,
            script_argument: None,
        });
        Ok(id)
    }
    fn lookup(&self, name: &str) -> Option<FlowValueId> {
        self.bindings
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
    fn bind(&mut self, name: String, id: FlowValueId, declaration: bool) {
        if !declaration {
            for scope in self.bindings.iter_mut().rev() {
                if let Some(binding) = scope.get_mut(&name) {
                    *binding = id;
                    return;
                }
            }
        }
        if let Some(scope) = self.bindings.last_mut() {
            scope.insert(name, id);
        }
    }
    fn define_pattern(
        &mut self,
        node: Node<'_>,
        kind: FlowValueKind,
        dependencies: Vec<FlowValueId>,
        producer: Option<FlowStepId>,
        scope: SourceRange,
        declaration: bool,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        if matches!(
            node.kind(),
            "identifier" | "shorthand_property_identifier_pattern" | "self" | "self_parameter"
        ) {
            let name = text(self.source, node)
                .ok_or(BuildError::Invalid)?
                .to_owned();
            let id = self.value(name.clone(), kind, node, scope, dependencies, producer)?;
            self.bind(name, id, declaration);
            return Ok(vec![id]);
        }
        if matches!(
            node.kind(),
            "type_annotation" | "type_identifier" | "scoped_type_identifier" | "primitive_type"
        ) {
            return Ok(Vec::new());
        }
        if matches!(
            node.kind(),
            "member_expression"
                | "attribute"
                | "field_expression"
                | "subscript"
                | "subscript_expression"
        ) {
            self.gap(FlowGapKind::Dynamic, node)?;
            return Ok(vec![self.value(
                identifier_path(self.source, node).unwrap_or_else(|| "state".to_owned()),
                FlowValueKind::External,
                node,
                scope,
                dependencies,
                producer,
            )?]);
        }
        let mut values = Vec::new();
        if let Some(pattern) = node
            .child_by_field_name("pattern")
            .or_else(|| node.child_by_field_name("name"))
        {
            return self.define_pattern(pattern, kind, dependencies, producer, scope, declaration);
        }
        for child in children(node) {
            values.extend(self.define_pattern(
                child,
                kind,
                dependencies.clone(),
                producer,
                scope,
                declaration,
            )?);
        }
        Ok(values)
    }
    fn walk(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        self.poll()?;
        if self.depth >= 128 {
            self.gap(FlowGapKind::Limit, node)?;
            return Ok(Vec::new());
        }
        self.depth += 1;
        let result = self.walk_inner(node, parent, scope);
        self.depth -= 1;
        result
    }
    fn walk_inner(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        match node.kind() {
            "identifier" | "self" | "shorthand_property_identifier" => {
                let name = text(self.source, node).ok_or(BuildError::Invalid)?;
                if let Some(id) = self.lookup(name) {
                    return Ok(vec![id]);
                }
                let id = self.value(
                    name.to_owned(),
                    FlowValueKind::External,
                    node,
                    scope,
                    Vec::new(),
                    None,
                )?;
                return Ok(vec![id]);
            }
            "string" | "string_literal" | "raw_string_literal" | "integer" | "float" | "number"
            | "true" | "false" | "none" | "null" | "undefined" | "comment" | "line_comment"
            | "block_comment" | "type_annotation" | "type_identifier" | "primitive_type" => {
                return Ok(Vec::new());
            }
            "function_item"
            | "function_definition"
            | "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "closure_expression"
            | "lambda"
            | "class_definition"
            | "class_declaration"
            | "method_definition"
            | "decorated_definition"
            | "impl_item"
            | "struct_item"
            | "trait_item"
            | "enum_item"
            | "interface_declaration"
            | "type_alias_declaration" => return Ok(Vec::new()),
            "import_statement" | "import_from_statement" | "use_declaration" => {
                return Ok(Vec::new());
            }
            "statement_block" | "block" => {
                let lexical = self.language != IndexLanguage::Python;
                if lexical {
                    self.bindings.push(BTreeMap::new());
                }
                let result = self.block(node, parent, range(node)?);
                if lexical {
                    self.bindings.pop();
                }
                return result;
            }
            "call_expression" | "call" | "new_expression" => return self.call(node, parent, scope),
            "variable_declarator"
            | "let_declaration"
            | "assignment"
            | "assignment_expression"
            | "augmented_assignment"
            | "augmented_assignment_expression" => {
                let rhs = node
                    .child_by_field_name("value")
                    .or_else(|| node.child_by_field_name("right"));
                let lhs = node
                    .child_by_field_name("pattern")
                    .or_else(|| node.child_by_field_name("name"))
                    .or_else(|| node.child_by_field_name("left"));
                let mut inputs = if let Some(rhs) = rhs {
                    self.walk(rhs, parent, scope)?
                } else {
                    Vec::new()
                };
                if node.kind().starts_with("augmented")
                    && let Some(lhs) = lhs
                {
                    inputs.extend(self.walk(lhs, parent, scope)?);
                }
                let id = self.step(FlowStepKind::Assign, node, parent, None, inputs.clone())?;
                if let Some(lhs) = lhs {
                    let declaration =
                        matches!(node.kind(), "variable_declarator" | "let_declaration");
                    let output = self.define_pattern(
                        lhs,
                        FlowValueKind::Local,
                        inputs,
                        Some(id),
                        scope,
                        declaration,
                    )?;
                    self.charge(output.len())?;
                    self.steps[(id.get() - 1) as usize].outputs = output.clone();
                    return Ok(output);
                }
                self.gap(FlowGapKind::Unsupported, node)?;
                return Ok(Vec::new());
            }
            "return_statement"
            | "return_expression"
            | "throw_statement"
            | "raise_statement"
            | "break_statement"
            | "break_expression"
            | "continue_statement"
            | "continue_expression" => {
                let inputs = self.sequence(node, parent, scope)?;
                let kind = match node.kind() {
                    "return_statement" | "return_expression" => FlowStepKind::Return,
                    "throw_statement" | "raise_statement" => FlowStepKind::Throw,
                    "break_statement" | "break_expression" => FlowStepKind::Break,
                    _ => FlowStepKind::Continue,
                };
                self.step(kind, node, parent, None, inputs.clone())?;
                return Ok(inputs);
            }
            "if_statement" | "if_expression" | "conditional_expression" | "ternary_expression" => {
                return self.condition(node, parent, scope);
            }
            "for_statement" | "for_expression" | "for_in_statement" | "while_statement"
            | "while_expression" | "loop_expression" | "do_statement" => {
                return self.loop_body(node, parent, scope);
            }
            "await_expression" | "await" => {
                let inputs = self.sequence(node, parent, scope)?;
                self.step(FlowStepKind::Await, node, parent, None, inputs.clone())?;
                return Ok(inputs);
            }
            "try_statement" | "try_expression" | "match_expression" | "switch_statement"
            | "with_statement" => {
                return self.dispatch(node, parent, scope);
            }
            "binary_expression" | "boolean_operator" => {
                let operator = node
                    .child_by_field_name("operator")
                    .and_then(|n| text(self.source, n));
                if matches!(operator, Some("&&" | "||" | "and" | "or" | "??")) {
                    let left = node.child_by_field_name("left");
                    let right = node.child_by_field_name("right");
                    let mut inputs = if let Some(left) = left {
                        self.walk(left, parent, scope)?
                    } else {
                        Vec::new()
                    };
                    let id =
                        self.step(FlowStepKind::Condition, node, parent, None, inputs.clone())?;
                    let before = self.bindings.clone();
                    if let Some(right) = right {
                        inputs.extend(self.walk(right, Some(id), scope)?);
                    }
                    let after = self.bindings.clone();
                    self.bindings = before.clone();
                    self.merge(node, scope, &[before, after])?;
                    return Ok(unique(inputs));
                }
            }
            "subscript" | "subscript_expression" => {
                if let Some((name, slot)) =
                    super::process::script_argument(node, self.source, self.imports)
                {
                    let object = node
                        .child_by_field_name("object")
                        .or_else(|| node.child_by_field_name("value"));
                    let root = object
                        .and_then(|n| text(self.source, n))
                        .and_then(|s| s.split('.').next())
                        .unwrap_or("");
                    if self.blocked.contains(root)
                        || self.bindings.iter().any(|scope| scope.contains_key(root))
                    {
                        self.gap(FlowGapKind::Dynamic, node)?;
                        return self.sequence(node, parent, scope);
                    }
                    let id = self.value(
                        name,
                        FlowValueKind::ScriptArgument,
                        node,
                        scope,
                        Vec::new(),
                        None,
                    )?;
                    self.values[(id.get() - 1) as usize].script_argument = Some(slot);
                    return Ok(vec![id]);
                }
                self.gap(FlowGapKind::Dynamic, node)?;
            }
            "attribute" | "member_expression" | "field_expression" => {
                let object = node
                    .child_by_field_name("object")
                    .or_else(|| node.child_by_field_name("value"));
                let inputs = if let Some(object) = object {
                    self.walk(object, parent, scope)?
                } else {
                    Vec::new()
                };
                self.gap(FlowGapKind::Dynamic, node)?;
                return Ok(vec![self.value(
                    identifier_path(self.source, node).unwrap_or_else(|| "state".to_owned()),
                    FlowValueKind::External,
                    node,
                    scope,
                    inputs,
                    None,
                )?]);
            }
            "macro_invocation" | "yield" | "yield_expression" | "ERROR" => {
                self.gap(
                    if node.kind() == "ERROR" {
                        FlowGapKind::ParseError
                    } else {
                        FlowGapKind::Unsupported
                    },
                    node,
                )?;
                self.step(FlowStepKind::Unknown, node, parent, None, Vec::new())?;
                return Ok(Vec::new());
            }
            _ => {}
        }
        self.sequence(node, parent, scope)
    }
    fn dispatch(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        // Dispatch, exception types and cleanup overrides are only partially known.
        // Each alternative gets its own region; a return must never hide finally.
        self.gap(FlowGapKind::Unsupported, node)?;
        let id = self.step(FlowStepKind::Handler, node, parent, None, Vec::new())?;
        let before = self.bindings.clone();
        let mut alternatives = vec![before.clone()];
        let mut branches = children(node);
        if matches!(node.kind(), "match_expression" | "switch_statement") {
            branches.clear();
            for child in children(node) {
                if matches!(child.kind(), "match_block" | "switch_body") {
                    branches.extend(children(child));
                } else {
                    self.walk(child, Some(id), scope)?;
                }
            }
        }
        let mut cleanup = None;
        for branch in branches {
            if branch.kind() == "finally_clause" {
                cleanup = Some(branch);
                continue;
            }
            self.bindings = before.clone();
            let branch_id = self.step(
                FlowStepKind::Branch,
                branch,
                Some(id),
                Some(branch.kind().to_owned()),
                Vec::new(),
            )?;
            self.walk(branch, Some(branch_id), scope)?;
            alternatives.push(self.bindings.clone());
        }
        self.bindings = before;
        self.merge(node, scope, &alternatives)?;
        if let Some(cleanup) = cleanup {
            let cleanup_id = self.step(
                FlowStepKind::Handler,
                cleanup,
                Some(id),
                Some("finally".to_owned()),
                Vec::new(),
            )?;
            self.walk(cleanup, Some(cleanup_id), scope)?;
        }
        Ok(Vec::new())
    }
    fn sequence(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        let mut values = Vec::new();
        for child in children(node) {
            values.extend(self.walk(child, parent, scope)?);
            if exits(child) {
                break;
            }
        }
        Ok(unique(values))
    }
    fn block(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        let mut tail = Vec::new();
        for child in children(node)
            .into_iter()
            .filter(|n| !n.kind().contains("comment"))
        {
            let values = self.walk(child, parent, scope)?;
            if exits(child) {
                return Ok(Vec::new());
            }
            tail = if self.language == IndexLanguage::Rust
                && !matches!(child.kind(), "let_declaration" | "expression_statement")
            {
                values
            } else {
                Vec::new()
            };
        }
        Ok(tail)
    }
    fn loop_body(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        if let Some(initial) = node.child_by_field_name("initializer") {
            self.walk(initial, parent, scope)?;
        }
        let iterable = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("right"));
        let inputs = if let Some(iterable) = iterable {
            self.walk(iterable, parent, scope)?
        } else {
            Vec::new()
        };
        let id = self.step(FlowStepKind::Loop, node, parent, None, inputs.clone())?;
        let before = self.bindings.clone();
        if let Some(pattern) = node
            .child_by_field_name("pattern")
            .or_else(|| node.child_by_field_name("left"))
        {
            self.define_pattern(
                pattern,
                FlowValueKind::Local,
                inputs,
                None,
                range(node)?,
                true,
            )?;
        }
        let condition = node.child_by_field_name("condition");
        if node.kind() != "do_statement"
            && let Some(condition) = condition
        {
            self.walk(condition, Some(id), scope)?;
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.walk(body, Some(id), scope)?;
        }
        if let Some(update) = node.child_by_field_name("increment") {
            self.walk(update, Some(id), scope)?;
        }
        if node.kind() == "do_statement"
            && let Some(condition) = condition
        {
            self.walk(condition, Some(id), scope)?;
        }
        let after = self.bindings.clone();
        self.bindings = before.clone();
        self.merge(node, scope, &[before, after])?;
        // Iteration counts, exhaustion and loop-carried aliases are not runtime facts.
        self.gap(FlowGapKind::Unsupported, node)?;
        Ok(Vec::new())
    }
    fn call(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        let function = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("constructor"));
        let name = function.and_then(|n| identifier_path(self.source, n));
        if name.is_none() {
            if let Some(function) = function {
                self.walk(function, parent, scope)?;
            }
            self.gap(FlowGapKind::Dynamic, node)?;
        }
        let mut arguments = Vec::new();
        if let Some(args) = node.child_by_field_name("arguments") {
            for argument in children(args) {
                let keyword = if argument.kind() == "keyword_argument" {
                    argument
                        .child_by_field_name("name")
                        .and_then(|n| text(self.source, n))
                        .and_then(|n| SymbolName::try_from_string(n.to_owned()).ok())
                } else {
                    None
                };
                let value = if keyword.is_some() {
                    argument.child_by_field_name("value").unwrap_or(argument)
                } else {
                    argument
                };
                if matches!(
                    argument.kind(),
                    "spread_element" | "list_splat" | "dictionary_splat"
                ) {
                    self.gap(FlowGapKind::Dynamic, argument)?;
                }
                let values = self.walk(value, parent, scope)?;
                self.charge(1 + values.len())?;
                arguments.push(FlowArgument {
                    keyword,
                    values,
                    range: range(argument)?,
                });
            }
        }
        let inputs = arguments
            .iter()
            .flat_map(|a| a.values.iter().copied())
            .collect();
        let id = self.step(FlowStepKind::Call, node, parent, name, inputs)?;
        let result = self.value(
            "result".to_owned(),
            FlowValueKind::CallResult,
            node,
            scope,
            Vec::new(),
            Some(id),
        )?;
        self.steps[(id.get() - 1) as usize].arguments = arguments;
        self.steps[(id.get() - 1) as usize].callee_range = function.map(range).transpose()?;
        self.charge(1)?;
        self.steps[(id.get() - 1) as usize].outputs = vec![result];
        Ok(vec![result])
    }
    fn condition(
        &mut self,
        node: Node<'_>,
        parent: Option<FlowStepId>,
        scope: SourceRange,
    ) -> Result<Vec<FlowValueId>, BuildError> {
        let test = node
            .child_by_field_name("condition")
            .or_else(|| node.child_by_field_name("test"))
            .or_else(|| {
                (node.kind() == "conditional_expression" && self.language == IndexLanguage::Python)
                    .then(|| node.named_child(1))
                    .flatten()
            });
        let input = if let Some(test) = test {
            self.walk(test, parent, scope)?
        } else {
            Vec::new()
        };
        let id = self.step(FlowStepKind::Condition, node, parent, None, input)?;
        let before = self.bindings.clone();
        let python_ternary =
            node.kind() == "conditional_expression" && self.language == IndexLanguage::Python;
        let has_alternative = node.child_by_field_name("alternative").is_some() || python_ternary;
        let mut alternatives = if has_alternative {
            Vec::new()
        } else {
            vec![before.clone()]
        };
        let mut values = Vec::new();
        for field in ["consequence", "alternative"] {
            if let Some(branch) = node.child_by_field_name(field).or_else(|| {
                if python_ternary {
                    node.named_child(if field == "consequence" { 0 } else { 2 })
                } else {
                    None
                }
            }) {
                self.bindings = before.clone();
                let branch_id = self.step(
                    FlowStepKind::Branch,
                    branch,
                    Some(id),
                    Some(field.to_owned()),
                    Vec::new(),
                )?;
                values.extend(self.walk(branch, Some(branch_id), scope)?);
                if !exits(branch) {
                    alternatives.push(self.bindings.clone());
                }
            }
        }
        self.bindings = before;
        self.merge(node, scope, &alternatives)?;
        Ok(unique(values))
    }
    fn merge(
        &mut self,
        node: Node<'_>,
        scope: SourceRange,
        alternatives: &[Bindings],
    ) -> Result<(), BuildError> {
        for level in 0..self.bindings.len() {
            let names = alternatives
                .iter()
                .filter_map(|a| a.get(level))
                .flat_map(|m| m.keys().cloned())
                .collect::<BTreeSet<_>>();
            for name in names {
                let inputs = unique(
                    alternatives
                        .iter()
                        .filter_map(|a| a.get(level).and_then(|m| m.get(&name)).copied())
                        .collect(),
                );
                if inputs.len() > 1 {
                    let id = self.value(
                        name.clone(),
                        FlowValueKind::Merge,
                        node,
                        scope,
                        inputs,
                        None,
                    )?;
                    self.bindings[level].insert(name, id);
                } else if let Some(id) = inputs.first() {
                    self.bindings[level].insert(name, *id);
                }
            }
        }
        Ok(())
    }
}

fn range(node: Node<'_>) -> Result<SourceRange, BuildError> {
    source_range_for_node(node).map_err(|_| BuildError::Invalid)
}
fn text<'a>(source: &'a [u8], node: Node<'_>) -> Option<&'a str> {
    std::str::from_utf8(source.get(node.start_byte()..node.end_byte())?).ok()
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn unique(values: Vec<FlowValueId>) -> Vec<FlowValueId> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn parameter_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    if matches!(node.kind(), "identifier" | "self_parameter") {
        vec![node]
    } else {
        children(node)
    }
}
fn exits(node: Node<'_>) -> bool {
    exit_at(node, 0)
}
fn exit_at(node: Node<'_>, depth: usize) -> bool {
    if depth >= 128 {
        return false;
    }
    if matches!(node.kind(), "block" | "statement_block" | "else_clause") {
        return children(node).into_iter().any(|n| exit_at(n, depth + 1));
    }
    if matches!(node.kind(), "if_statement" | "if_expression") {
        return node
            .child_by_field_name("consequence")
            .is_some_and(|n| exit_at(n, depth + 1))
            && node
                .child_by_field_name("alternative")
                .is_some_and(|n| exit_at(n, depth + 1));
    }
    matches!(
        node.kind(),
        "return_statement"
            | "return_expression"
            | "throw_statement"
            | "raise_statement"
            | "break_statement"
            | "break_expression"
            | "continue_statement"
            | "continue_expression"
    ) || (node.kind() == "expression_statement"
        && node.named_child(0).is_some_and(|n| exit_at(n, depth + 1)))
}
fn identifier_path(source: &[u8], node: Node<'_>) -> Option<String> {
    let name = text(source, node)?;
    if name.len() <= 1_024
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$'))
    {
        Some(name.to_owned())
    } else {
        None
    }
}
