use super::ast::{
    VnCommandArg, VnSpan, YarnChoice, YarnCommand, YarnInstruction, YarnLine, YarnNode, YarnScript,
};
use super::diagnostics::{VnCompileError, VnDiagnostic};

pub fn parse_yarn_source(
    source_id: Option<String>,
    source: &str,
) -> Result<YarnScript, VnCompileError> {
    super::compiler::compile_yarn_source(source_id.as_deref().unwrap_or("<memory>"), source)?;

    let lines: Vec<&str> = source.lines().collect();
    let mut nodes = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = strip_comment(lines[index]).trim();
        if line.is_empty() {
            index += 1;
            continue;
        }

        let node_span = span(source_id.as_deref(), index + 1, 1);
        let Some(title) = line.strip_prefix("title:") else {
            return Err(error(
                "VN001",
                "expected Yarn node header 'title: ...'",
                node_span,
            ));
        };
        let title = title.trim();
        if title.is_empty() {
            return Err(error("VN002", "Yarn node title cannot be empty", node_span));
        }
        index += 1;

        let mut headers = std::collections::BTreeMap::new();
        let mut tags = Vec::new();
        headers.insert("title".to_owned(), title.to_owned());
        while index < lines.len() {
            let header_line = strip_comment(lines[index]).trim();
            if header_line.is_empty() {
                index += 1;
                continue;
            }
            if header_line == "---" {
                break;
            }
            if let Some((key, value)) = header_line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                if key == "tags" {
                    tags = value
                        .split_whitespace()
                        .filter(|tag| !tag.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                headers.insert(key.to_owned(), value.to_owned());
                index += 1;
                continue;
            }
            break;
        }

        if index >= lines.len() || strip_comment(lines[index]).trim() != "---" {
            return Err(error(
                "VN003",
                format!("node '{title}' is missing '---'"),
                span(source_id.as_deref(), index.saturating_add(1), 1),
            ));
        }
        index += 1;

        let body_start = index;
        while index < lines.len() && strip_comment(lines[index]).trim() != "===" {
            index += 1;
        }
        if index >= lines.len() {
            return Err(error(
                "VN004",
                format!("node '{title}' is missing '==='"),
                span(source_id.as_deref(), lines.len().max(1), 1),
            ));
        }

        let body = parse_instruction_block(
            &lines[body_start..index],
            body_start + 1,
            source_id.as_deref(),
            0,
        )?;
        nodes.push(YarnNode {
            title: title.to_owned(),
            tags,
            headers,
            body,
            span: node_span,
        });
        index += 1;
    }

    Ok(YarnScript { nodes })
}

fn parse_instruction_block(
    lines: &[&str],
    base_line: usize,
    source: Option<&str>,
    min_indent: usize,
) -> Result<Vec<YarnInstruction>, VnCompileError> {
    let mut instructions = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let raw = lines[index];
        let line_number = base_line + index;
        let trimmed = strip_comment(raw).trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        let indent = leading_spaces(raw);
        if indent < min_indent {
            break;
        }
        if trimmed.starts_with("->") {
            let (choices, consumed) = parse_choices(&lines[index..], line_number, source)?;
            instructions.extend(choices.into_iter().map(YarnInstruction::Choice));
            index += consumed;
            continue;
        }
        if trimmed.starts_with("<<") && trimmed.ends_with(">>") {
            instructions.push(YarnInstruction::Command(parse_command(
                trimmed,
                source,
                line_number,
            )?));
            index += 1;
            continue;
        }
        instructions.push(YarnInstruction::Line(parse_line(
            trimmed,
            source,
            line_number,
        )));
        index += 1;
    }

    Ok(instructions)
}

fn parse_choices(
    lines: &[&str],
    base_line: usize,
    source: Option<&str>,
) -> Result<(Vec<YarnChoice>, usize), VnCompileError> {
    let choice_indent = leading_spaces(lines[0]);
    let mut choices = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = strip_comment(lines[index]).trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        if leading_spaces(lines[index]) != choice_indent || !trimmed.starts_with("->") {
            break;
        }

        let line_number = base_line + index;
        let (text, condition) = parse_choice_header(trimmed, source, line_number)?;
        index += 1;

        let nested_start = index;
        while index < lines.len() {
            let next = strip_comment(lines[index]).trim();
            if next.is_empty() {
                index += 1;
                continue;
            }
            if leading_spaces(lines[index]) <= choice_indent {
                break;
            }
            index += 1;
        }

        let body = parse_instruction_block(
            &lines[nested_start..index],
            base_line + nested_start,
            source,
            choice_indent + 1,
        )?;
        choices.push(YarnChoice {
            text,
            condition,
            body,
            span: span(source, line_number, 1),
        });
    }

    Ok((choices, index))
}

fn parse_choice_header(
    line: &str,
    source: Option<&str>,
    line_number: usize,
) -> Result<(String, Option<String>), VnCompileError> {
    let mut body = line.trim_start_matches("->").trim().to_owned();
    let mut condition = None;
    if let Some(condition_start) = body.rfind("<<if ") {
        if !body.ends_with(">>") {
            return Err(error(
                "VN005",
                "choice condition must end with '>>'",
                span(source, line_number, condition_start + 1),
            ));
        }
        condition = Some(body[condition_start + 5..body.len() - 2].trim().to_owned());
        body.truncate(condition_start);
    }

    let text = body.trim().trim_matches('"').to_owned();
    if text.is_empty() {
        return Err(error(
            "VN006",
            "choice text cannot be empty",
            span(source, line_number, 1),
        ));
    }
    Ok((text, condition))
}

fn parse_line(line: &str, source: Option<&str>, line_number: usize) -> YarnLine {
    let (without_id, line_id) = if let Some(index) = line.find("#line:") {
        (
            line[..index].trim_end(),
            Some(line[index + "#line:".len()..].trim().to_owned()),
        )
    } else {
        (line, None)
    };

    let (speaker, text) = if let Some((speaker, text)) = without_id.split_once(':') {
        let speaker = speaker.trim();
        if !speaker.is_empty() && !speaker.contains(char::is_whitespace) {
            (Some(speaker.to_owned()), text.trim().to_owned())
        } else {
            (None, without_id.trim().to_owned())
        }
    } else {
        (None, without_id.trim().to_owned())
    };

    YarnLine {
        speaker,
        text,
        line_id: line_id.filter(|value| !value.is_empty()),
        span: span(source, line_number, 1),
    }
}

fn parse_command(
    line: &str,
    source: Option<&str>,
    line_number: usize,
) -> Result<YarnCommand, VnCompileError> {
    let raw = line.to_owned();
    let inner = line
        .strip_prefix("<<")
        .and_then(|value| value.strip_suffix(">>"))
        .unwrap_or(line)
        .trim();
    let tokens = tokenize_command(inner)
        .map_err(|message| error("VN007", message, span(source, line_number, 1)))?;
    if tokens.is_empty() {
        return Err(error(
            "VN008",
            "empty Yarn command",
            span(source, line_number, 1),
        ));
    }

    let name = tokens[0].clone();
    let args = tokens
        .into_iter()
        .skip(1)
        .map(|token| {
            if let Some((name, value)) = token.split_once('=') {
                if name.is_empty() {
                    return VnCommandArg::positional(token);
                }
                VnCommandArg::named(name, value)
            } else {
                VnCommandArg::positional(token)
            }
        })
        .collect();

    Ok(YarnCommand {
        name,
        args,
        raw,
        span: span(source, line_number, 1),
    })
}

fn tokenize_command(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quote = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quote = !in_quote,
            '\\' if in_quote => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ch if ch.is_whitespace() && !in_quote => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if in_quote {
        return Err("unterminated quoted string in Yarn command".to_owned());
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

fn error(code: &str, message: impl Into<String>, span: VnSpan) -> VnCompileError {
    VnCompileError::single(VnDiagnostic::error(code, message, span))
}

fn span(source: Option<&str>, line: usize, column: usize) -> VnSpan {
    let span = VnSpan::new(line as u32, column as u32);
    if let Some(source) = source {
        span.with_source(source)
    } else {
        span
    }
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

fn strip_comment(line: &str) -> &str {
    line.split_once("//")
        .map(|(before, _)| before)
        .unwrap_or(line)
}
