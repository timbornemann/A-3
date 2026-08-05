use super::manifest::{ManifestBuilder, dependency_name};
use super::source::diagnostic;
use a3_application::{
    LanguageParseControl, LanguageParseFailure, LanguageParseInput, LanguageParsePolicy,
};
use a3_domain::{
    Confidence, LanguageAdapterRevision, LanguageParseResult, ParseDiagnosticCode, SourceRange,
    SymbolRole, SyntaxProvider, SyntaxRelationKind, SyntaxSource,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use toml::Spanned;

const MAX_PYPROJECT_BYTES: usize = 512 * 1024;

pub(super) fn parse(
    input: LanguageParseInput<'_>,
    policy: LanguageParsePolicy,
    control: &dyn LanguageParseControl,
    revision: &LanguageAdapterRevision,
) -> Result<LanguageParseResult, LanguageParseFailure> {
    if input.source().len() > MAX_PYPROJECT_BYTES {
        return Err(LanguageParseFailure::InputTooLarge);
    }
    let mut builder = ManifestBuilder::new(input, policy, control)?;
    let source = match std::str::from_utf8(builder.source()) {
        Ok(source) => source,
        Err(error) => {
            let start = error.valid_up_to();
            let end = start.saturating_add(error.error_len().unwrap_or(1));
            builder.push_diagnostic(diagnostic(
                ParseDiagnosticCode::InvalidEncoding,
                builder.range(start, end.min(builder.source().len()))?,
                "pyproject.toml is not valid UTF-8",
            )?)?;
            return builder.finish(revision);
        }
    };
    let metadata = match toml::from_str::<PyProject>(source) {
        Ok(metadata) => metadata,
        Err(error) => {
            let span = error.span().unwrap_or(0..builder.source().len());
            builder.push_diagnostic(diagnostic(
                ParseDiagnosticCode::SyntaxError,
                builder.range(span.start, span.end)?,
                "pyproject.toml syntax or supported metadata schema is invalid",
            )?)?;
            return builder.finish(revision);
        }
    };
    extract(&mut builder, metadata)?;
    builder.finish(revision)
}

fn extract(
    builder: &mut ManifestBuilder<'_>,
    metadata: PyProject,
) -> Result<(), LanguageParseFailure> {
    let project_name = metadata
        .project
        .as_ref()
        .and_then(|project| project.name.as_ref());
    let poetry_name = metadata
        .tool
        .as_ref()
        .and_then(|tool| tool.poetry.as_ref())
        .and_then(|poetry| poetry.name.as_ref());
    let name = project_name.or(poetry_name);
    let selection = name.map_or(Ok(builder.range(0, 0)?), |name| span_range(builder, name));
    let selection = selection?;
    let name = name.map_or("pyproject", |name| name.get_ref().as_str());
    let has_entrypoints = metadata
        .project
        .as_ref()
        .is_some_and(Project::has_entrypoints)
        || metadata
            .tool
            .as_ref()
            .and_then(|tool| tool.poetry.as_ref())
            .is_some_and(|poetry| !poetry.scripts.is_empty());
    let roles = if has_entrypoints {
        [SymbolRole::Entrypoint].as_slice()
    } else {
        [].as_slice()
    };
    let root = builder.add_root_module(name, selection, "Python pyproject package", roles)?;

    if let Some(build_system) = metadata.build_system {
        add_dependency_list(
            builder,
            root,
            build_system.requires,
            SyntaxRelationKind::Builds,
        )?;
        if let Some(backend) = build_system.build_backend {
            let range = span_range(builder, &backend)?;
            builder.push_reference(
                SyntaxSource::Symbol(root),
                backend.get_ref(),
                SyntaxRelationKind::Builds,
                range,
                SyntaxProvider::Manifest,
                Confidence::certain(),
            )?;
        }
    }
    if let Some(project) = metadata.project {
        add_dependency_list(
            builder,
            root,
            project.dependencies,
            SyntaxRelationKind::Imports,
        )?;
        for (group, dependencies) in project.optional_dependencies {
            let kind = dependency_group_kind(&group);
            add_dependency_list(builder, root, dependencies, kind)?;
        }
        add_scripts(builder, root, project.scripts)?;
        add_scripts(builder, root, project.gui_scripts)?;
        for (_group, entries) in project.entry_points {
            add_scripts(builder, root, entries)?;
        }
    }
    if let Some(tool) = metadata.tool {
        if let Some(poetry) = tool.poetry {
            add_poetry_dependencies(
                builder,
                root,
                poetry.dependencies,
                SyntaxRelationKind::Imports,
            )?;
            add_poetry_dependencies(
                builder,
                root,
                poetry.dev_dependencies,
                SyntaxRelationKind::Tests,
            )?;
            for (group, values) in poetry.group {
                add_poetry_dependencies(
                    builder,
                    root,
                    values.dependencies,
                    dependency_group_kind(&group),
                )?;
            }
            add_poetry_scripts(builder, root, poetry.scripts)?;
        }
        if let Some(pytest) = tool.pytest.and_then(|pytest| pytest.ini_options) {
            for path in pytest.testpaths {
                let range = span_range(builder, &path)?;
                builder.push_reference(
                    SyntaxSource::Symbol(root),
                    &format!("pytest:testpath:{}", path.get_ref()),
                    SyntaxRelationKind::Tests,
                    range,
                    SyntaxProvider::Manifest,
                    Confidence::certain(),
                )?;
            }
            for pattern in pytest.python_files {
                let range = span_range(builder, &pattern)?;
                builder.push_reference(
                    SyntaxSource::Symbol(root),
                    &format!("pytest:python-file:{}", pattern.get_ref()),
                    SyntaxRelationKind::Configures,
                    range,
                    SyntaxProvider::Manifest,
                    Confidence::certain(),
                )?;
            }
        }
    }
    Ok(())
}

fn add_dependency_list(
    builder: &mut ManifestBuilder<'_>,
    root: a3_domain::LocalSymbolId,
    dependencies: Vec<Spanned<String>>,
    kind: SyntaxRelationKind,
) -> Result<(), LanguageParseFailure> {
    for dependency in dependencies {
        builder.poll()?;
        let range = span_range(builder, &dependency)?;
        let Some(name) = dependency_name(dependency.get_ref()) else {
            builder.unsupported(
                range,
                "Python dependency specification has no static package name",
            )?;
            continue;
        };
        builder.push_reference(
            SyntaxSource::Symbol(root),
            name,
            kind,
            range,
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )?;
    }
    Ok(())
}

fn add_poetry_dependencies(
    builder: &mut ManifestBuilder<'_>,
    root: a3_domain::LocalSymbolId,
    dependencies: BTreeMap<String, Spanned<toml::Value>>,
    kind: SyntaxRelationKind,
) -> Result<(), LanguageParseFailure> {
    for (name, value) in dependencies {
        builder.poll()?;
        if name.eq_ignore_ascii_case("python") {
            continue;
        }
        let range = builder.range(value.span().start, value.span().end)?;
        if dependency_name(&name).is_none() {
            builder.unsupported(range, "Poetry dependency name is not statically supported")?;
            continue;
        }
        builder.push_reference(
            SyntaxSource::Symbol(root),
            &name,
            kind,
            range,
            SyntaxProvider::Manifest,
            Confidence::certain(),
        )?;
    }
    Ok(())
}

fn add_scripts(
    builder: &mut ManifestBuilder<'_>,
    root: a3_domain::LocalSymbolId,
    scripts: BTreeMap<String, Spanned<String>>,
) -> Result<(), LanguageParseFailure> {
    for (name, target) in scripts {
        builder.poll()?;
        let range = span_range(builder, &target)?;
        builder.add_entrypoint(root, &name, range, target.get_ref(), range)?;
    }
    Ok(())
}

fn add_poetry_scripts(
    builder: &mut ManifestBuilder<'_>,
    root: a3_domain::LocalSymbolId,
    scripts: BTreeMap<String, Spanned<toml::Value>>,
) -> Result<(), LanguageParseFailure> {
    for (name, value) in scripts {
        builder.poll()?;
        let range = builder.range(value.span().start, value.span().end)?;
        let Some(target) = value.get_ref().as_str() else {
            builder.unsupported(range, "Poetry script target must be a static string")?;
            continue;
        };
        builder.add_entrypoint(root, &name, range, target, range)?;
    }
    Ok(())
}

fn dependency_group_kind(group: &str) -> SyntaxRelationKind {
    let group = group.to_ascii_lowercase();
    if group.contains("test") || group.contains("dev") {
        SyntaxRelationKind::Tests
    } else {
        SyntaxRelationKind::Imports
    }
}

fn span_range<T>(
    builder: &ManifestBuilder<'_>,
    value: &Spanned<T>,
) -> Result<SourceRange, LanguageParseFailure> {
    builder.range(value.span().start, value.span().end)
}

#[derive(Debug, Deserialize)]
struct PyProject {
    project: Option<Project>,
    #[serde(rename = "build-system")]
    build_system: Option<BuildSystem>,
    tool: Option<Tools>,
}

#[derive(Debug, Deserialize)]
struct Project {
    name: Option<Spanned<String>>,
    #[serde(default)]
    dependencies: Vec<Spanned<String>>,
    #[serde(default, rename = "optional-dependencies")]
    optional_dependencies: BTreeMap<String, Vec<Spanned<String>>>,
    #[serde(default)]
    scripts: BTreeMap<String, Spanned<String>>,
    #[serde(default, rename = "gui-scripts")]
    gui_scripts: BTreeMap<String, Spanned<String>>,
    #[serde(default, rename = "entry-points")]
    entry_points: BTreeMap<String, BTreeMap<String, Spanned<String>>>,
}

impl Project {
    fn has_entrypoints(&self) -> bool {
        !self.scripts.is_empty() || !self.gui_scripts.is_empty() || !self.entry_points.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct BuildSystem {
    #[serde(default)]
    requires: Vec<Spanned<String>>,
    #[serde(rename = "build-backend")]
    build_backend: Option<Spanned<String>>,
}

#[derive(Debug, Deserialize)]
struct Tools {
    poetry: Option<Poetry>,
    pytest: Option<PytestTool>,
}

#[derive(Debug, Deserialize)]
struct Poetry {
    name: Option<Spanned<String>>,
    #[serde(default)]
    dependencies: BTreeMap<String, Spanned<toml::Value>>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, Spanned<toml::Value>>,
    #[serde(default)]
    group: BTreeMap<String, PoetryGroup>,
    #[serde(default)]
    scripts: BTreeMap<String, Spanned<toml::Value>>,
}

#[derive(Debug, Deserialize)]
struct PoetryGroup {
    #[serde(default)]
    dependencies: BTreeMap<String, Spanned<toml::Value>>,
}

#[derive(Debug, Deserialize)]
struct PytestTool {
    #[serde(rename = "ini_options")]
    ini_options: Option<PytestOptions>,
}

#[derive(Debug, Deserialize)]
struct PytestOptions {
    #[serde(default)]
    testpaths: Vec<Spanned<String>>,
    #[serde(default)]
    python_files: Vec<Spanned<String>>,
}
