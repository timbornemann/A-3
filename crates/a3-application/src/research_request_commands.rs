//! Necessary name coverage for explicit request invocations, never shell parsing or proof.

/// Bounded projection from the unchanged user request. No source or model text is authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResearchRequestCommands {
    names: Vec<String>,
}

impl ResearchRequestCommands {
    /// Recognizes only runner + relative script + command + complete argument placeholder.
    /// Unsupported syntax and over-complex requests retain the original request alone.
    #[must_use]
    pub fn from_objective(objective: &str) -> Self {
        if objective.len() > 32 * 1024 {
            return Self::default();
        }
        let mut names = Vec::new();
        let mut words = objective.split_whitespace();
        while let Some(word) = words.next() {
            let runner = unframe(word);
            if !matches!(runner, "python" | "python3" | "node") {
                continue;
            }
            let mut tail = words.clone();
            let (Some(script), Some(command), Some(argument)) =
                (tail.next(), tail.next(), tail.next())
            else {
                continue;
            };
            let script = script.trim_matches(['`', '\'', '"']);
            let command = command.trim_matches(['`', '\'', '"']);
            let argument = unframe(argument);
            let placeholder = argument.strip_prefix('<').and_then(|s| s.strip_suffix('>'));
            if !relative_script(runner, script)
                || !identifier(command)
                || !placeholder.is_some_and(identifier)
            {
                continue;
            }
            if !names.iter().any(|name| name == command) {
                if names.len() == 4 {
                    // Do not pretend a truncated subset protects a larger invocation set.
                    return Self::default();
                }
                names.push(command.to_owned());
            }
        }
        Self { names }
    }

    /// Exact case-sensitive command names, in their first occurrence order.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Each active result must name the command itself, not borrow a prerequisite's text.
    /// This is a necessary lexical condition, not evidence of correct use or behavior.
    #[must_use]
    pub fn missing_from(&self, result: &str) -> bool {
        self.names.iter().any(|name| {
            !result
                .split(|ch: char| {
                    !ch.is_alphanumeric() && !matches!(ch, '_' | '-' | '.' | '/' | '\\')
                })
                .any(|token| token.trim_end_matches('.') == name)
        })
    }
}

fn unframe(word: &str) -> &str {
    word.trim_matches([
        '`', '\'', '"', '(', ')', '[', ']', '{', '}', ',', ';', ':', '.', '!', '?',
    ])
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn relative_script(runner: &str, path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 256
        && !path.starts_with(['/', '\\'])
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b'\\'))
        && path
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "..")
        && match runner {
            "python" | "python3" => path.ends_with(".py"),
            "node" => [".js", ".mjs", ".cjs"]
                .iter()
                .any(|suffix| path.ends_with(suffix)),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_commands_are_literal_bounded_and_not_a_prose_or_shell_parser() {
        let request = "Größe 🦀: Plane `python main.py import-csv <filepath>`, then node src/cli.mjs export_JSON <output>. Again python3 main.py import-csv <filepath>.";
        let commands = ResearchRequestCommands::from_objective(request);
        assert_eq!(commands.names(), ["import-csv", "export_JSON"]);
        assert!(!commands.missing_from(
            "Implement `import-csv` and test 'export_JSON' with valid/invalid input."
        ));
        assert!(!commands.missing_from("Implement import-csv. Test export_JSON."));
        for text in [
            "import export_JSON",
            "IMPORT-CSV export_JSON",
            "import-csv2 export_JSON",
            "import-csv.py export_JSON",
            "äimport-csv export_JSON",
            "commands/import-csv export_JSON",
            "import-csv export-json",
        ] {
            assert!(commands.missing_from(text), "{text}");
        }
        for unsupported in [
            "CLI-Befehl import-csv in main.py",
            "python main.py import-csv",
            "python main.py import-csv <filepath",
            "python main.py import-csv filepath",
            "python -m main import-csv <filepath>",
            "python main.js import-csv <filepath>",
            "node main.py import-csv <filepath>",
            "python ../main.py import-csv <filepath>",
            "python /main.py import-csv <filepath>",
            "python main.py import-csv;delete <filepath>",
            "python main.py import-csv; <filepath>",
            "python main.py, import-csv <filepath>",
            "python main.py $(delete) <filepath>",
        ] {
            assert!(
                ResearchRequestCommands::from_objective(unsupported)
                    .names()
                    .is_empty(),
                "{unsupported}"
            );
        }
        let four = (0..4)
            .map(|n| format!("python tool.py cmd{n} <arg>"))
            .collect::<Vec<_>>()
            .join("; ");
        assert_eq!(
            ResearchRequestCommands::from_objective(&four).names().len(),
            4
        );
        assert!(
            ResearchRequestCommands::from_objective(&format!("{four}; python tool.py fifth <arg>"))
                .names()
                .is_empty()
        );
        assert!(
            ResearchRequestCommands::from_objective(&format!(
                "{} {request}",
                "ö".repeat(16 * 1024)
            ))
            .names()
            .is_empty()
        );
        let name = "x".repeat(64);
        assert_eq!(
            ResearchRequestCommands::from_objective(&format!("python main.py {name} <arg>"))
                .names(),
            std::slice::from_ref(&name)
        );
        assert!(
            ResearchRequestCommands::from_objective(&format!("python main.py {name}x <arg>"))
                .names()
                .is_empty()
        );
    }
}
