use std::fs;

use super::*;

const SAMPLE_SCRIPT: &str = r#"
title: Start
tags: opening common
---
<<scene "images/bg/classroom.png" transition="fade" duration=0.4>>
Alice: 早上好。 #line:start.alice.0001
今天的天空很亮。 #line:start.narrator.0001
-> 和 Alice 一起走
    <<set $route = "alice">>
    <<jump Ending>>
-> 一个人去学校 <<if $can_go_alone>>
    <<set $route = "alone">>
    <<jump Ending>>
===

title: Ending
---
再见。 #line:ending.narrator.0001
===
"#;

#[test]
fn parser_reads_nodes_lines_commands_and_choices() {
    let script = YarnScript::parse_source("sample.yarn", SAMPLE_SCRIPT).unwrap();
    assert_eq!(script.nodes.len(), 2);

    let start = script.node("Start").unwrap();
    assert_eq!(start.tags, ["opening", "common"]);

    let YarnInstruction::Command(scene) = &start.body[0] else {
        panic!("expected scene command");
    };
    assert_eq!(scene.name, "scene");
    assert_eq!(
        scene.first_positional_raw(),
        Some("images/bg/classroom.png")
    );
    assert_eq!(
        scene.named_arg("transition").map(|arg| arg.raw.as_str()),
        Some("fade")
    );

    let YarnInstruction::Line(line) = &start.body[1] else {
        panic!("expected dialogue line");
    };
    assert_eq!(line.speaker.as_deref(), Some("Alice"));
    assert_eq!(line.text, "早上好。");
    assert_eq!(line.line_id.as_deref(), Some("start.alice.0001"));

    let choices: Vec<_> = start
        .body
        .iter()
        .filter_map(|instruction| match instruction {
            YarnInstruction::Choice(choice) => Some(choice),
            _ => None,
        })
        .collect();
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[1].condition.as_deref(), Some("$can_go_alone"));
    assert_eq!(choices[0].body.len(), 2);
}

#[test]
fn validator_reports_missing_jump_target() {
    let error = YarnScript::parse_str(
        r#"
title: Start
---
<<jump Missing>>
===
"#,
    )
    .unwrap_err();

    assert!(error
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "vn.yarn.jump.missing_target"));
}

#[test]
fn validator_reports_unbalanced_conditionals() {
    let error = YarnScript::parse_str(
        r#"
title: Start
---
<<if $flag>>
Still inside.
===
"#,
    )
    .unwrap_err();

    assert!(error
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "vn.yarn.condition.missing_endif"));
}

#[test]
fn project_loads_manifest_and_scripts_from_root() {
    let temp = tempfile::tempdir().unwrap();
    let story_dir = temp.path().join("story");
    fs::create_dir(&story_dir).unwrap();
    fs::write(story_dir.join("main.yarn"), SAMPLE_SCRIPT).unwrap();

    let project = YarnProject::from_manifest_str(
        r##"
title = "Sky VN"
start_node = "Start"
resolution = [1280, 720]
default_language = "zh-CN"
scripts = ["story/main.yarn"]

[characters.Alice]
display_name = "Alice"
color = "#8fd3ff"
"##,
        temp.path(),
    )
    .unwrap();

    assert_eq!(project.manifest.title, "Sky VN");
    assert_eq!(project.script.nodes.len(), 2);
    assert!(project.script.has_node("Ending"));
}
