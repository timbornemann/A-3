use a3_domain::{
    FlowGap, FlowGapKind, FlowProcess, FlowProcessMode, FlowProcessTarget, FlowStep, FlowStepId,
    FlowStepKind, FunctionFlow, FunctionFlowError, LocalSymbolId, RepositoryPath, SourceRange,
    SymbolName, SymbolReference,
};

/// Recognizes literal argv and success-only chaining; never interprets a shell.
pub(crate) fn package_script(
    owner: LocalSymbolId,
    path: &RepositoryPath,
    range: SourceRange,
    command: &str,
) -> Result<FunctionFlow, FunctionFlowError> {
    let Some(commands) = commands(command) else {
        return FunctionFlow::new(
            owner,
            range,
            Vec::new(),
            Vec::new(),
            vec![FlowGap {
                kind: FlowGapKind::Unsupported,
                range,
            }],
        );
    };
    let mut steps = Vec::new();
    let mut gaps = Vec::new();
    let mut parent = None;
    for (i, argv) in commands.iter().enumerate() {
        if i > 0 {
            let id = FlowStepId::new((steps.len() + 1) as u32)?;
            steps.push(FlowStep {
                id,
                kind: FlowStepKind::Condition,
                parent,
                range,
                name: SymbolReference::try_from_string("previous-command-succeeded".to_owned())
                    .ok(),
                callee_range: None,
                process: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
                arguments: Vec::new(),
            });
            parent = Some(id);
        }
        let executable = argv.first().map(String::as_str).unwrap_or("");
        let mut mode = FlowProcessMode::Wait;
        let target = match executable {
            "node" | "nodejs" | "python" | "python3" => argv
                .get(1)
                .and_then(|s| super::process::relative_target(path, s))
                .map(FlowProcessTarget::File),
            "npm" | "pnpm" if argv.get(1).is_some_and(|s| s == "run") && argv.len() == 3 => argv
                .get(2)
                .and_then(|s| SymbolName::try_from_string(s.clone()).ok())
                .map(FlowProcessTarget::PackageScript),
            "tsc" | "rustc" => {
                mode = FlowProcessMode::CompileOnly;
                None
            }
            _ => None,
        };
        if target.is_none() && mode != FlowProcessMode::CompileOnly {
            gaps.push(FlowGap {
                kind: FlowGapKind::Unsupported,
                range,
            });
        }
        if matches!(target, Some(FlowProcessTarget::PackageScript(_))) {
            // Package-manager lifecycle hooks are outside this small static argv model.
            gaps.push(FlowGap {
                kind: FlowGapKind::Unsupported,
                range,
            });
        }
        steps.push(FlowStep {
            id: FlowStepId::new((steps.len() + 1) as u32)?,
            kind: FlowStepKind::Process,
            parent,
            range,
            name: SymbolReference::try_from_string(executable.to_owned()).ok(),
            callee_range: None,
            process: Some(FlowProcess {
                mode,
                target,
                arguments: Vec::new(),
            }),
            inputs: Vec::new(),
            outputs: Vec::new(),
            arguments: Vec::new(),
        });
    }
    FunctionFlow::new(owner, range, steps, Vec::new(), gaps)
}
fn commands(input: &str) -> Option<Vec<Vec<String>>> {
    if input.len() > 16_384 {
        return None;
    }
    let mut commands = Vec::new();
    let mut argv = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(
            c,
            '$' | '`'
                | '\\'
                | '\n'
                | '\r'
                | '\0'
                | '>'
                | '<'
                | '|'
                | ';'
                | '*'
                | '?'
                | '('
                | ')'
                | '='
                | '~'
                | '%'
                | '!'
        ) {
            return None;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                token.push(c);
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '&' => {
                if chars.next() != Some('&') {
                    return None;
                }
                if !token.is_empty() {
                    argv.push(std::mem::take(&mut token));
                }
                if argv.is_empty() {
                    return None;
                }
                commands.push(std::mem::take(&mut argv));
                if commands.len() > 64 {
                    return None;
                }
            }
            c if c.is_whitespace() => {
                if !token.is_empty() {
                    argv.push(std::mem::take(&mut token));
                }
            }
            c => token.push(c),
        }
        if argv.len() > 64 {
            return None;
        }
    }
    if quote.is_some() {
        return None;
    }
    if !token.is_empty() {
        argv.push(token);
    }
    if argv.is_empty() {
        return None;
    }
    commands.push(argv);
    Some(commands)
}
