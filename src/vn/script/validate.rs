use std::collections::BTreeSet;

use super::ast::{VnSpan, VnValue, YarnCommand, YarnInstruction, YarnScript};
use super::diagnostics::{VnCompileError, VnDiagnostic};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VnValidationOptions {
    pub require_line_ids: bool,
    pub start_node: Option<String>,
}

impl VnValidationOptions {
    pub fn with_start_node(mut self, start_node: impl Into<String>) -> Self {
        self.start_node = Some(start_node.into());
        self
    }

    pub fn require_line_ids(mut self, require_line_ids: bool) -> Self {
        self.require_line_ids = require_line_ids;
        self
    }
}

pub fn validate_script(
    script: &YarnScript,
    options: VnValidationOptions,
) -> Result<(), VnCompileError> {
    let mut diagnostics = Vec::new();
    let mut nodes = BTreeSet::new();

    for node in &script.nodes {
        if !nodes.insert(node.title.clone()) {
            diagnostics.push(VnDiagnostic::error(
                "VN100",
                format!("duplicate Yarn node '{}'", node.title),
                node.span.clone(),
            ));
        }
    }

    if let Some(start_node) = &options.start_node {
        if !script.has_node(start_node) {
            diagnostics.push(VnDiagnostic::error(
                "vn.yarn.start.missing_node",
                format!("start node '{start_node}' does not exist"),
                root_span(),
            ));
        }
    }

    for node in &script.nodes {
        validate_instructions(&node.body, script, &options, &mut diagnostics);
    }

    if diagnostics.iter().any(VnDiagnostic::is_error) {
        Err(VnCompileError::new(diagnostics))
    } else {
        Ok(())
    }
}

fn validate_instructions(
    instructions: &[YarnInstruction],
    script: &YarnScript,
    options: &VnValidationOptions,
    diagnostics: &mut Vec<VnDiagnostic>,
) {
    let mut condition_stack = Vec::<VnSpan>::new();

    for instruction in instructions {
        match instruction {
            YarnInstruction::Line(line) => {
                if options.require_line_ids && line.line_id.is_none() {
                    diagnostics.push(VnDiagnostic::warning(
                        "vn.yarn.line.missing_id",
                        "dialogue line is missing a line id",
                        line.span.clone(),
                    ));
                }
            }
            YarnInstruction::Command(command) => match command.name.as_str() {
                "jump" | "call" => validate_node_target(command, script, diagnostics),
                "if" => condition_stack.push(command.span.clone()),
                "elseif" | "else" => {
                    if condition_stack.is_empty() {
                        diagnostics.push(VnDiagnostic::error(
                            "vn.yarn.condition.unmatched_branch",
                            format!("<<{}>> has no matching <<if>>", command.name),
                            command.span.clone(),
                        ));
                    }
                }
                "endif" => {
                    if condition_stack.pop().is_none() {
                        diagnostics.push(VnDiagnostic::error(
                            "vn.yarn.condition.unmatched_endif",
                            "<<endif>> has no matching <<if>>",
                            command.span.clone(),
                        ));
                    }
                }
                _ => {}
            },
            YarnInstruction::Choice(choice) => {
                if choice.text.trim().is_empty() {
                    diagnostics.push(VnDiagnostic::error(
                        "vn.yarn.choice.empty_text",
                        "choice text cannot be empty",
                        choice.span.clone(),
                    ));
                }
                validate_instructions(&choice.body, script, options, diagnostics);
            }
        }
    }

    for span in condition_stack {
        diagnostics.push(VnDiagnostic::error(
            "vn.yarn.condition.missing_endif",
            "<<if>> is missing a matching <<endif>>",
            span,
        ));
    }
}

fn validate_node_target(
    command: &YarnCommand,
    script: &YarnScript,
    diagnostics: &mut Vec<VnDiagnostic>,
) {
    let target = command.positional_values().next().and_then(VnValue::as_str);
    match target {
        Some(target) if script.has_node(target) => {}
        Some(target) => diagnostics.push(VnDiagnostic::error(
            "vn.yarn.jump.missing_target",
            format!("{} targets missing node '{target}'", command.raw),
            command.span.clone(),
        )),
        None => diagnostics.push(VnDiagnostic::error(
            "vn.yarn.jump.missing_target",
            format!("{} requires a target node", command.raw),
            command.span.clone(),
        )),
    }
}

pub(crate) fn root_span() -> VnSpan {
    VnSpan::new(1, 1)
}
