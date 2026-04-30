use std::collections::{BTreeMap, BTreeSet};

use yarnspinner::compiler::{
    Declaration as YarnDeclaration, DiagnosticSeverity as YarnDiagnosticSeverity,
};
use yarnspinner::core::Type as YarnType;
use yarnspinner::prelude::{YarnCompiler, YarnFile};

use super::diagnostics::{VnCompileError, VnDiagnostic, VnDiagnosticSeverity};
use super::VnSpan;

pub fn compile_yarn_source(
    source_id: impl Into<String>,
    source: &str,
) -> Result<YarnCompileSummary, VnCompileError> {
    let source_id = source_id.into();
    let compiler_source = source.trim_start_matches(|ch| matches!(ch, '\u{feff}' | '\r' | '\n'));
    let file = YarnFile {
        file_name: source_id,
        source: compiler_source.to_owned(),
    };

    let mut compiler = YarnCompiler::new();
    compiler.add_file(file);
    for (variable, inferred_type) in infer_implicit_variable_declarations(compiler_source) {
        let declaration = YarnDeclaration::new(variable, inferred_type.clone());
        let declaration = match inferred_type {
            YarnType::Boolean => declaration.with_default_value(false),
            YarnType::Number => declaration.with_default_value(0.0),
            YarnType::String => declaration.with_default_value(""),
            _ => declaration,
        };
        compiler.declare_variable(declaration);
    }

    let compilation = compiler.compile().map_err(|error| {
        VnCompileError::new(
            error
                .0
                .into_iter()
                .map(|diagnostic| {
                    let severity = match diagnostic.severity {
                        YarnDiagnosticSeverity::Error => VnDiagnosticSeverity::Error,
                        YarnDiagnosticSeverity::Warning => VnDiagnosticSeverity::Warning,
                    };
                    let span = diagnostic
                        .range
                        .as_ref()
                        .map(|range| {
                            let mut span = VnSpan::new(
                                range.start.line.saturating_add(1) as u32,
                                range.start.character.saturating_add(1) as u32,
                            );
                            span.source = diagnostic.file_name.clone();
                            span
                        })
                        .unwrap_or_else(|| {
                            let mut span = VnSpan::new(1, 1);
                            span.source = diagnostic.file_name.clone();
                            span
                        });
                    let code = if diagnostic.message.contains("endif")
                        || diagnostic.message.contains("COMMAND_ENDIF")
                    {
                        "vn.yarn.condition.missing_endif"
                    } else {
                        "vn.yarnspinner.compiler"
                    };
                    VnDiagnostic::new(severity, code, diagnostic.message, span)
                })
                .collect(),
        )
    })?;

    Ok(YarnCompileSummary {
        contains_implicit_string_tags: compilation.contains_implicit_string_tags,
        warning_count: compilation.warnings.len(),
        string_count: compilation.string_table.len(),
        node_count: compilation
            .program
            .as_ref()
            .map(|program| program.nodes.len())
            .unwrap_or_default(),
    })
}

fn infer_implicit_variable_declarations(source: &str) -> BTreeMap<String, YarnType> {
    let mut seen = BTreeMap::new();
    let mut assigned = BTreeSet::new();

    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("<<set ") {
            if let Some(variable) = first_variable(rest) {
                assigned.insert(variable);
            }
        }
        if let Some(rest) = trimmed.strip_prefix("<<declare ") {
            if let Some(variable) = first_variable(rest) {
                assigned.insert(variable);
            }
        }

        for variable in variables_in(line) {
            let inferred_type = infer_variable_type_from_line(line, &variable);
            seen.entry(variable)
                .and_modify(|existing| {
                    if *existing == YarnType::Boolean {
                        *existing = inferred_type.clone();
                    }
                })
                .or_insert(inferred_type);
        }
    }

    for variable in assigned {
        seen.remove(&variable);
    }
    seen
}

fn infer_variable_type_from_line(line: &str, variable: &str) -> YarnType {
    let Some(start) = line.find(variable) else {
        return YarnType::Boolean;
    };
    let tail = &line[start + variable.len()..];
    for operator in ["==", "!=", ">=", "<=", ">", "<"] {
        let Some((_, right)) = tail.split_once(operator) else {
            continue;
        };
        let right = right.trim_start();
        if right.starts_with('"') {
            return YarnType::String;
        }
        if right
            .chars()
            .next()
            .is_some_and(|ch| ch == '-' || ch.is_ascii_digit())
        {
            return YarnType::Number;
        }
    }
    YarnType::Boolean
}

fn first_variable(input: &str) -> Option<String> {
    variables_in(input).into_iter().next()
}

fn variables_in(input: &str) -> Vec<String> {
    let mut variables = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch != '$' {
            continue;
        }
        let start = index;
        let mut end = index + ch.len_utf8();
        while let Some((next_index, next)) = chars.peek().copied() {
            if next == '_' || next.is_ascii_alphanumeric() {
                end = next_index + next.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        if end > start + 1 {
            variables.push(input[start..end].to_owned());
        }
    }
    variables
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YarnCompileSummary {
    pub node_count: usize,
    pub string_count: usize,
    pub warning_count: usize,
    pub contains_implicit_string_tags: bool,
}
