use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::action::VnAction;
use super::asset::VnAssetState;
use super::audio::VnAudioState;
use super::dialogue::{VnDialogueChoice, VnDialogueState};
use super::progress::VnProgressState;
use super::scene::VnSceneState;
use super::script::{
    validate_script, VnCommandArg, VnCompileError, VnValidationOptions, VnValue, YarnChoice,
    YarnCommand, YarnInstruction, YarnLine, YarnProject, YarnScript,
};
use super::video::VnVideoState;

pub type VnRuntimeResult<T> = Result<T, VnRuntimeError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnRuntimeConfig {
    pub start_node: String,
}

impl Default for VnRuntimeConfig {
    fn default() -> Self {
        Self {
            start_node: "Start".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnActiveChoice {
    pub text: String,
    pub condition: Option<String>,
    pub source_index: usize,
    pub choice: YarnChoice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum VnRuntimeEvent {
    Line(YarnLine),
    Choices(Vec<VnActiveChoice>),
    Command(YarnCommand),
    Wait(f32),
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnStatus {
    Ready,
    Line,
    Choice,
    Waiting,
    Ended,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnStackFrame {
    pub node: String,
    pub instruction_index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct VnConditionalFrame {
    branch_taken: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnConditionalSnapshot {
    pub branch_taken: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnRuntimeSnapshot {
    pub current_node: String,
    pub instruction_index: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inline: Vec<YarnInstruction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_stack: Vec<VnStackFrame>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub condition_stack: Vec<VnConditionalSnapshot>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, VnValue>,
    pub scene: VnSceneState,
    pub dialogue: VnDialogueState,
    pub audio: VnAudioState,
    pub video: VnVideoState,
    pub assets: VnAssetState,
    pub progress: VnProgressState,
    pub status: VnStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_choices: Vec<VnActiveChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_line: Option<YarnLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_remaining: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnRuntime {
    script: YarnScript,
    current_node: String,
    instruction_index: usize,
    inline: VecDeque<YarnInstruction>,
    call_stack: Vec<VnStackFrame>,
    condition_stack: Vec<VnConditionalFrame>,
    variables: BTreeMap<String, VnValue>,
    scene: VnSceneState,
    dialogue: VnDialogueState,
    audio: VnAudioState,
    video: VnVideoState,
    assets: VnAssetState,
    progress: VnProgressState,
    status: VnStatus,
    active_choices: Vec<VnActiveChoice>,
    last_line: Option<YarnLine>,
    wait_remaining: Option<f32>,
}

impl VnRuntime {
    pub fn new(script: YarnScript, config: VnRuntimeConfig) -> VnRuntimeResult<Self> {
        validate_script(
            &script,
            VnValidationOptions::default().with_start_node(config.start_node.clone()),
        )
        .map_err(VnRuntimeError::Compile)?;

        if !script.has_node(&config.start_node) {
            return Err(VnRuntimeError::MissingNode(config.start_node));
        }

        Ok(Self {
            script,
            current_node: config.start_node,
            instruction_index: 0,
            inline: VecDeque::new(),
            call_stack: Vec::new(),
            condition_stack: Vec::new(),
            variables: BTreeMap::new(),
            scene: VnSceneState::default(),
            dialogue: VnDialogueState::default(),
            audio: VnAudioState::default(),
            video: VnVideoState::default(),
            assets: VnAssetState::default(),
            progress: VnProgressState::default(),
            status: VnStatus::Ready,
            active_choices: Vec::new(),
            last_line: None,
            wait_remaining: None,
        })
    }

    pub fn from_script(script: YarnScript, start_node: impl Into<String>) -> VnRuntimeResult<Self> {
        Self::new(
            script,
            VnRuntimeConfig {
                start_node: start_node.into(),
            },
        )
    }

    pub fn from_project(project: YarnProject) -> VnRuntimeResult<Self> {
        let start_node = project.manifest.start_node.clone();
        Self::from_script(project.script, start_node)
    }

    pub fn script(&self) -> &YarnScript {
        &self.script
    }

    pub fn current_node(&self) -> &str {
        &self.current_node
    }

    pub fn instruction_index(&self) -> usize {
        self.instruction_index
    }

    pub fn status(&self) -> &VnStatus {
        &self.status
    }

    pub fn variables(&self) -> &BTreeMap<String, VnValue> {
        &self.variables
    }

    pub fn scene(&self) -> &VnSceneState {
        &self.scene
    }

    pub fn scene_mut(&mut self) -> &mut VnSceneState {
        &mut self.scene
    }

    pub fn dialogue(&self) -> &VnDialogueState {
        &self.dialogue
    }

    pub fn dialogue_mut(&mut self) -> &mut VnDialogueState {
        &mut self.dialogue
    }

    pub fn audio(&self) -> &VnAudioState {
        &self.audio
    }

    pub fn audio_mut(&mut self) -> &mut VnAudioState {
        &mut self.audio
    }

    pub fn video(&self) -> &VnVideoState {
        &self.video
    }

    pub fn video_mut(&mut self) -> &mut VnVideoState {
        &mut self.video
    }

    pub fn assets(&self) -> &VnAssetState {
        &self.assets
    }

    pub fn assets_mut(&mut self) -> &mut VnAssetState {
        &mut self.assets
    }

    pub fn progress(&self) -> &VnProgressState {
        &self.progress
    }

    pub fn progress_mut(&mut self) -> &mut VnProgressState {
        &mut self.progress
    }

    pub fn wait_remaining(&self) -> Option<f32> {
        self.wait_remaining
    }

    pub fn variable(&self, name: &str) -> Option<&VnValue> {
        self.variables.get(normalize_variable_name(name))
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: VnValue) {
        let name = name.into();
        self.variables
            .insert(normalize_variable_name(&name).to_owned(), value);
    }

    pub fn active_choices(&self) -> &[VnActiveChoice] {
        &self.active_choices
    }

    pub fn last_line(&self) -> Option<&YarnLine> {
        self.last_line.as_ref()
    }

    pub fn can_advance(&self) -> bool {
        matches!(self.status, VnStatus::Ready | VnStatus::Line)
    }

    pub fn complete_wait(&mut self) {
        if self.status == VnStatus::Waiting {
            self.status = VnStatus::Ready;
            self.wait_remaining = None;
        }
    }

    pub fn tick(&mut self, delta_seconds: f32) {
        if self.status == VnStatus::Waiting {
            if let Some(remaining) = &mut self.wait_remaining {
                *remaining = (*remaining - delta_seconds.max(0.0)).max(0.0);
                if *remaining <= 0.0 {
                    self.complete_wait();
                }
            }
        }
    }

    pub fn apply_action(&mut self, action: VnAction) -> VnRuntimeResult<Option<VnRuntimeEvent>> {
        match action {
            VnAction::Advance | VnAction::Confirm => match self.status {
                VnStatus::Line if !self.dialogue.line_complete => {
                    self.dialogue.complete_line();
                    Ok(None)
                }
                VnStatus::Choice => {
                    let selected = self.dialogue.selected_choice;
                    self.choose(selected)?;
                    Ok(Some(self.advance()?))
                }
                VnStatus::Waiting => {
                    self.complete_wait();
                    Ok(None)
                }
                VnStatus::Ready | VnStatus::Line => Ok(Some(self.advance()?)),
                VnStatus::Ended => Ok(Some(VnRuntimeEvent::End)),
            },
            VnAction::Choice(index) => {
                if self.status == VnStatus::Choice {
                    self.dialogue.selected_choice = index;
                    self.choose(index)?;
                    Ok(Some(self.advance()?))
                } else {
                    Ok(None)
                }
            }
            VnAction::Up => {
                self.dialogue.select_previous_choice();
                Ok(None)
            }
            VnAction::Down => {
                self.dialogue.select_next_choice();
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub fn snapshot(&self) -> VnRuntimeSnapshot {
        VnRuntimeSnapshot {
            current_node: self.current_node.clone(),
            instruction_index: self.instruction_index,
            inline: self.inline.iter().cloned().collect(),
            call_stack: self.call_stack.clone(),
            condition_stack: self
                .condition_stack
                .iter()
                .map(|frame| VnConditionalSnapshot {
                    branch_taken: frame.branch_taken,
                })
                .collect(),
            variables: self.variables.clone(),
            scene: self.scene.clone(),
            dialogue: self.dialogue.clone(),
            audio: self.audio.clone(),
            video: self.video.clone(),
            assets: self.assets.clone(),
            progress: self.progress.clone(),
            status: self.status.clone(),
            active_choices: self.active_choices.clone(),
            last_line: self.last_line.clone(),
            wait_remaining: self.wait_remaining,
        }
    }

    pub fn restore_snapshot(&mut self, snapshot: VnRuntimeSnapshot) -> VnRuntimeResult<()> {
        if !self.script.has_node(&snapshot.current_node) {
            return Err(VnRuntimeError::MissingNode(snapshot.current_node));
        }
        self.current_node = snapshot.current_node;
        self.instruction_index = snapshot.instruction_index;
        self.inline = snapshot.inline.into_iter().collect();
        self.call_stack = snapshot.call_stack;
        self.condition_stack = snapshot
            .condition_stack
            .into_iter()
            .map(|frame| VnConditionalFrame {
                branch_taken: frame.branch_taken,
            })
            .collect();
        self.variables = snapshot.variables;
        self.scene = snapshot.scene;
        self.dialogue = snapshot.dialogue;
        self.audio = snapshot.audio;
        self.video = snapshot.video;
        self.assets = snapshot.assets;
        self.progress = snapshot.progress;
        self.status = snapshot.status;
        self.active_choices = snapshot.active_choices;
        self.last_line = snapshot.last_line;
        self.wait_remaining = snapshot.wait_remaining;
        Ok(())
    }

    pub fn jump_to_node(&mut self, node: impl Into<String>) -> VnRuntimeResult<()> {
        let node = node.into();
        if !self.script.has_node(&node) {
            return Err(VnRuntimeError::MissingNode(node));
        }
        self.current_node = node;
        self.instruction_index = 0;
        self.inline.clear();
        self.condition_stack.clear();
        self.active_choices.clear();
        self.dialogue.clear_choices();
        self.status = VnStatus::Ready;
        Ok(())
    }

    pub fn choose(&mut self, index: usize) -> VnRuntimeResult<()> {
        if self.status != VnStatus::Choice {
            return Err(VnRuntimeError::ChoiceNotActive);
        }
        let choice =
            self.active_choices
                .get(index)
                .cloned()
                .ok_or(VnRuntimeError::InvalidChoiceIndex {
                    index,
                    len: self.active_choices.len(),
                })?;

        self.active_choices.clear();
        self.dialogue.clear_choices();
        self.status = VnStatus::Ready;
        for instruction in choice.choice.body.iter().rev() {
            self.inline.push_front(instruction.clone());
        }
        Ok(())
    }

    pub fn advance(&mut self) -> VnRuntimeResult<VnRuntimeEvent> {
        match self.status {
            VnStatus::Choice => {
                return Ok(VnRuntimeEvent::Choices(self.active_choices.clone()));
            }
            VnStatus::Waiting => {
                return Err(VnRuntimeError::Waiting);
            }
            VnStatus::Ended => {
                return Ok(VnRuntimeEvent::End);
            }
            VnStatus::Ready | VnStatus::Line => {}
        }

        self.status = VnStatus::Ready;

        loop {
            let Some(popped) = self.pop_next_instruction()? else {
                if let Some(frame) = self.call_stack.pop() {
                    self.current_node = frame.node;
                    self.instruction_index = frame.instruction_index;
                    continue;
                }
                self.status = VnStatus::Ended;
                return Ok(VnRuntimeEvent::End);
            };

            match popped.instruction {
                YarnInstruction::Line(line) => {
                    self.last_line = Some(line.clone());
                    self.dialogue.present_line(line.clone());
                    self.status = VnStatus::Line;
                    return Ok(VnRuntimeEvent::Line(line));
                }
                YarnInstruction::Choice(choice) => {
                    let mut choices = vec![choice];
                    while matches!(
                        self.peek_next_instruction()?,
                        Some(YarnInstruction::Choice(_))
                    ) {
                        let Some(popped) = self.pop_next_instruction()? else {
                            break;
                        };
                        if let YarnInstruction::Choice(choice) = popped.instruction {
                            choices.push(choice);
                        }
                    }

                    let mut active_choices = Vec::new();
                    for (source_index, choice) in choices.into_iter().enumerate() {
                        let visible = match choice.condition.as_deref() {
                            Some(condition) => self.eval_condition(condition)?,
                            None => true,
                        };
                        if visible {
                            active_choices.push(VnActiveChoice {
                                text: choice.text.clone(),
                                condition: choice.condition.clone(),
                                source_index,
                                choice,
                            });
                        }
                    }

                    if active_choices.is_empty() {
                        return Err(VnRuntimeError::NoVisibleChoices);
                    }

                    self.active_choices = active_choices;
                    self.dialogue.set_choices(
                        self.active_choices
                            .iter()
                            .map(|choice| VnDialogueChoice {
                                text: choice.text.clone(),
                                source_index: choice.source_index,
                                condition: choice.condition.clone(),
                            })
                            .collect(),
                    );
                    self.status = VnStatus::Choice;
                    return Ok(VnRuntimeEvent::Choices(self.active_choices.clone()));
                }
                YarnInstruction::Command(command) => {
                    if let Some(event) = self.handle_command(command)? {
                        return Ok(event);
                    }
                }
            }
        }
    }

    fn handle_command(&mut self, command: YarnCommand) -> VnRuntimeResult<Option<VnRuntimeEvent>> {
        match command.name.as_str() {
            "set" => {
                self.execute_set(&command)?;
                Ok(None)
            }
            "jump" => {
                let target = command
                    .first_positional_raw()
                    .ok_or_else(|| VnRuntimeError::MalformedCommand(command.raw.clone()))?
                    .to_owned();
                self.jump_to_node(target)?;
                Ok(None)
            }
            "call" => {
                let target = command
                    .first_positional_raw()
                    .ok_or_else(|| VnRuntimeError::MalformedCommand(command.raw.clone()))?
                    .to_owned();
                self.call_stack.push(VnStackFrame {
                    node: self.current_node.clone(),
                    instruction_index: self.instruction_index,
                });
                self.jump_to_node(target)?;
                Ok(None)
            }
            "return" => {
                if let Some(frame) = self.call_stack.pop() {
                    self.current_node = frame.node;
                    self.instruction_index = frame.instruction_index;
                    self.inline.clear();
                    self.condition_stack.clear();
                    Ok(None)
                } else {
                    self.status = VnStatus::Ended;
                    Ok(Some(VnRuntimeEvent::End))
                }
            }
            "if" => {
                let condition = command_condition(&command);
                let branch_taken = self.eval_condition(&condition)?;
                self.condition_stack
                    .push(VnConditionalFrame { branch_taken });
                if !branch_taken {
                    self.skip_to_next_conditional_branch()?;
                }
                Ok(None)
            }
            "elseif" => {
                let Some(frame_index) = self.condition_stack.len().checked_sub(1) else {
                    return Err(VnRuntimeError::UnmatchedConditional(command.raw));
                };
                if self.condition_stack[frame_index].branch_taken {
                    self.skip_to_matching_endif()?;
                } else {
                    let condition = command_condition(&command);
                    let branch_taken = self.eval_condition(&condition)?;
                    self.condition_stack[frame_index].branch_taken = branch_taken;
                    if !branch_taken {
                        self.skip_to_next_conditional_branch()?;
                    }
                }
                Ok(None)
            }
            "else" => {
                let Some(frame) = self.condition_stack.last_mut() else {
                    return Err(VnRuntimeError::UnmatchedConditional(command.raw));
                };
                if frame.branch_taken {
                    self.skip_to_matching_endif()?;
                } else {
                    frame.branch_taken = true;
                }
                Ok(None)
            }
            "endif" => {
                if self.condition_stack.pop().is_none() {
                    return Err(VnRuntimeError::UnmatchedConditional(command.raw));
                }
                Ok(None)
            }
            "wait" => {
                let seconds = command
                    .positional_values()
                    .next()
                    .and_then(|value| match value {
                        VnValue::Number(value) => Some(*value as f32),
                        _ => None,
                    })
                    .or_else(|| {
                        command
                            .named_arg("duration")
                            .and_then(|arg| match arg.value {
                                VnValue::Number(value) => Some(value as f32),
                                _ => None,
                            })
                    })
                    .unwrap_or(0.0);
                self.status = VnStatus::Waiting;
                self.wait_remaining = Some(seconds);
                Ok(Some(VnRuntimeEvent::Wait(seconds)))
            }
            _ => {
                self.scene.apply_command(&command);
                self.audio.apply_command(&command);
                self.video.apply_command(&command);
                self.assets.apply_command(&command);
                self.progress.apply_command(&command);
                self.status = VnStatus::Ready;
                Ok(Some(VnRuntimeEvent::Command(command)))
            }
        }
    }

    fn execute_set(&mut self, command: &YarnCommand) -> VnRuntimeResult<()> {
        let args: Vec<_> = command.positional_args().collect();
        let variable = args
            .first()
            .ok_or_else(|| VnRuntimeError::MalformedCommand(command.raw.clone()))?;
        let value_arg = if args.get(1).is_some_and(|arg| arg.raw == "=") {
            args.get(2)
        } else {
            args.get(1)
        }
        .ok_or_else(|| VnRuntimeError::MalformedCommand(command.raw.clone()))?;

        let value = self.resolve_value_arg(value_arg);
        self.variables
            .insert(normalize_variable_name(&variable.raw).to_owned(), value);
        Ok(())
    }

    fn resolve_value_arg(&self, arg: &VnCommandArg) -> VnValue {
        if arg.raw.starts_with('$') {
            return self
                .variables
                .get(normalize_variable_name(&arg.raw))
                .cloned()
                .unwrap_or(VnValue::Bool(false));
        }
        arg.value.clone()
    }

    fn eval_condition(&self, condition: &str) -> VnRuntimeResult<bool> {
        let condition = condition.trim();
        if condition.is_empty() {
            return Err(VnRuntimeError::InvalidCondition(condition.to_owned()));
        }

        if let Some(rest) = condition.strip_prefix('!') {
            return Ok(!self.eval_condition(rest)?);
        }

        for operator in ["==", "!=", ">=", "<=", ">", "<"] {
            if let Some((left, right)) = condition.split_once(operator) {
                let left = self.resolve_expression_value(left.trim());
                let right = self.resolve_expression_value(right.trim());
                return compare_values(&left, &right, operator)
                    .ok_or_else(|| VnRuntimeError::InvalidCondition(condition.to_owned()));
            }
        }

        Ok(self.resolve_expression_value(condition).truthy())
    }

    fn resolve_expression_value(&self, raw: &str) -> VnValue {
        let raw = raw.trim();
        if raw.starts_with('$') {
            return self
                .variables
                .get(normalize_variable_name(raw))
                .cloned()
                .unwrap_or(VnValue::Bool(false));
        }
        let unquoted = strip_quotes(raw);
        super::script::parse_value_literal(unquoted)
    }

    fn peek_next_instruction(&self) -> VnRuntimeResult<Option<&YarnInstruction>> {
        if let Some(instruction) = self.inline.front() {
            return Ok(Some(instruction));
        }
        let node = self
            .script
            .node(&self.current_node)
            .ok_or_else(|| VnRuntimeError::MissingNode(self.current_node.clone()))?;
        Ok(node.body.get(self.instruction_index))
    }

    fn pop_next_instruction(&mut self) -> VnRuntimeResult<Option<PoppedInstruction>> {
        if let Some(instruction) = self.inline.pop_front() {
            return Ok(Some(PoppedInstruction {
                instruction,
                source: InstructionSource::Inline,
            }));
        }

        let node = self
            .script
            .node(&self.current_node)
            .ok_or_else(|| VnRuntimeError::MissingNode(self.current_node.clone()))?;
        let Some(instruction) = node.body.get(self.instruction_index).cloned() else {
            return Ok(None);
        };
        self.instruction_index += 1;
        Ok(Some(PoppedInstruction {
            instruction,
            source: InstructionSource::Node,
        }))
    }

    fn unpop_instruction(&mut self, popped: PoppedInstruction) {
        match popped.source {
            InstructionSource::Inline => self.inline.push_front(popped.instruction),
            InstructionSource::Node => {
                debug_assert!(self.instruction_index > 0);
                self.instruction_index = self.instruction_index.saturating_sub(1);
            }
        }
    }

    fn skip_to_next_conditional_branch(&mut self) -> VnRuntimeResult<()> {
        let mut depth = 0usize;
        while let Some(popped) = self.pop_next_instruction()? {
            if let YarnInstruction::Command(command) = &popped.instruction {
                match command.name.as_str() {
                    "if" => depth += 1,
                    "endif" if depth == 0 => {
                        self.condition_stack.pop();
                        return Ok(());
                    }
                    "endif" => depth -= 1,
                    "elseif" | "else" if depth == 0 => {
                        self.unpop_instruction(popped);
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        Err(VnRuntimeError::UnclosedConditional)
    }

    fn skip_to_matching_endif(&mut self) -> VnRuntimeResult<()> {
        let mut depth = 0usize;
        while let Some(popped) = self.pop_next_instruction()? {
            if let YarnInstruction::Command(command) = &popped.instruction {
                match command.name.as_str() {
                    "if" => depth += 1,
                    "endif" if depth == 0 => {
                        self.condition_stack.pop();
                        return Ok(());
                    }
                    "endif" => depth -= 1,
                    _ => {}
                }
            }
        }

        Err(VnRuntimeError::UnclosedConditional)
    }
}

#[derive(Clone, Debug)]
struct PoppedInstruction {
    instruction: YarnInstruction,
    source: InstructionSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstructionSource {
    Inline,
    Node,
}

#[derive(Debug)]
pub enum VnRuntimeError {
    Compile(VnCompileError),
    RuntimeUnavailable,
    MissingSaveSlot(String),
    MissingNode(String),
    ChoiceNotActive,
    InvalidChoiceIndex { index: usize, len: usize },
    NoVisibleChoices,
    Waiting,
    MalformedCommand(String),
    InvalidCondition(String),
    UnmatchedConditional(String),
    UnclosedConditional,
}

impl fmt::Display for VnRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(error) => write!(f, "{error}"),
            Self::RuntimeUnavailable => f.write_str("VN runtime is not loaded"),
            Self::MissingSaveSlot(slot) => write!(f, "VN save slot `{slot}` does not exist"),
            Self::MissingNode(node) => write!(f, "VN node `{node}` does not exist"),
            Self::ChoiceNotActive => f.write_str("no VN choice is currently active"),
            Self::InvalidChoiceIndex { index, len } => {
                write!(
                    f,
                    "VN choice index {index} is out of range for {len} choices"
                )
            }
            Self::NoVisibleChoices => f.write_str("choice group has no visible choices"),
            Self::Waiting => f.write_str("VN runtime is waiting"),
            Self::MalformedCommand(command) => write!(f, "malformed VN command `{command}`"),
            Self::InvalidCondition(condition) => {
                write!(f, "invalid VN condition `{condition}`")
            }
            Self::UnmatchedConditional(command) => {
                write!(f, "conditional command `{command}` is not matched")
            }
            Self::UnclosedConditional => f.write_str("conditional block is missing `endif`"),
        }
    }
}

impl Error for VnRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Compile(error) => Some(error),
            _ => None,
        }
    }
}

fn command_condition(command: &YarnCommand) -> String {
    command
        .positional_args()
        .map(|arg| arg.raw.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_variable_name(name: &str) -> &str {
    name.trim().strip_prefix('$').unwrap_or(name.trim())
}

fn strip_quotes(raw: &str) -> &str {
    let raw = raw.trim();
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

fn compare_values(left: &VnValue, right: &VnValue, operator: &str) -> Option<bool> {
    match operator {
        "==" => Some(values_equal(left, right)),
        "!=" => Some(!values_equal(left, right)),
        ">" | "<" | ">=" | "<=" => {
            let (VnValue::Number(left), VnValue::Number(right)) = (left, right) else {
                return None;
            };
            Some(match operator {
                ">" => left > right,
                "<" => left < right,
                ">=" => left >= right,
                "<=" => left <= right,
                _ => unreachable!(),
            })
        }
        _ => None,
    }
}

fn values_equal(left: &VnValue, right: &VnValue) -> bool {
    match (left, right) {
        (VnValue::Bool(left), VnValue::Bool(right)) => left == right,
        (VnValue::Number(left), VnValue::Number(right)) => (left - right).abs() <= f64::EPSILON,
        (VnValue::String(left), VnValue::String(right)) => left == right,
        _ => left.to_string() == right.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRANCHING_SCRIPT: &str = r#"
title: Start
---
Alice: Ready? #line:start.alice.0001
-> Go with Alice
    <<set $route = "alice">>
    <<jump Ending>>
-> Go alone <<if $can_go_alone>>
    <<set $route = "alone">>
    <<jump Ending>>
===

title: Ending
---
再见。 #line:ending.narrator.0001
===
"#;

    #[test]
    fn runtime_branches_through_visible_choice() {
        let script = YarnScript::parse_source("branching.yarn", BRANCHING_SCRIPT).unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();

        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Line(_)
        ));
        let VnRuntimeEvent::Choices(choices) = runtime.advance().unwrap() else {
            panic!("expected choices");
        };
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].text, "Go with Alice");

        runtime.choose(0).unwrap();
        let VnRuntimeEvent::Line(line) = runtime.advance().unwrap() else {
            panic!("expected ending line");
        };
        assert_eq!(runtime.current_node(), "Ending");
        assert_eq!(line.line_id.as_deref(), Some("ending.narrator.0001"));
        assert_eq!(
            runtime.variable("route"),
            Some(&VnValue::String("alice".to_owned()))
        );
    }

    #[test]
    fn runtime_filters_choice_conditions_from_variables() {
        let script = YarnScript::parse_source("branching.yarn", BRANCHING_SCRIPT).unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        runtime.set_variable("can_go_alone", VnValue::Bool(true));

        runtime.advance().unwrap();
        let VnRuntimeEvent::Choices(choices) = runtime.advance().unwrap() else {
            panic!("expected choices");
        };
        assert_eq!(choices.len(), 2);

        runtime.choose(1).unwrap();
        runtime.advance().unwrap();
        assert_eq!(
            runtime.variable("$route"),
            Some(&VnValue::String("alone".to_owned()))
        );
    }

    #[test]
    fn runtime_executes_if_else_blocks() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<set $route = "alice">>
<<if $route == "alice">>
Alice route. #line:start.1
<<else>>
Other route. #line:start.2
<<endif>>
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();

        let VnRuntimeEvent::Line(line) = runtime.advance().unwrap() else {
            panic!("expected line");
        };
        assert_eq!(line.text, "Alice route.");
    }

    #[test]
    fn runtime_tracks_scene_and_dialogue_state() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<scene "bg/classroom.png" transition="fade" duration=0.4>>
<<show alice "chars/alice/smile.png" expression="smile" at="right" z=10>>
Alice: Hello. #line:start.alice.0001
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();

        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Command(_)
        ));
        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Command(_)
        ));
        let VnRuntimeEvent::Line(line) = runtime.advance().unwrap() else {
            panic!("expected line");
        };

        assert_eq!(line.text, "Hello.");
        assert_eq!(
            runtime
                .scene()
                .background
                .as_ref()
                .map(|layer| layer.asset.as_str()),
            Some("bg/classroom.png")
        );
        let alice = runtime.scene().actors.get("alice").unwrap();
        assert_eq!(alice.expression.as_deref(), Some("smile"));
        assert_eq!(alice.position.as_deref(), Some("right"));
        assert_eq!(runtime.dialogue().backlog.len(), 1);
        assert_eq!(
            runtime
                .dialogue()
                .current_line
                .as_ref()
                .unwrap()
                .line_id
                .as_deref(),
            Some("start.alice.0001")
        );
    }

    #[test]
    fn runtime_applies_actions_to_complete_line_and_choose() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
-> A
    <<set $route = "a">>
    <<jump Ending>>
-> B
    <<set $route = "b">>
    <<jump Ending>>
===

title: Ending
---
Done. #line:end.1
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();

        let event = runtime.apply_action(VnAction::Advance).unwrap().unwrap();
        assert!(matches!(event, VnRuntimeEvent::Line(_)));
        assert!(!runtime.dialogue().line_complete);

        assert!(runtime.apply_action(VnAction::Advance).unwrap().is_none());
        assert!(runtime.dialogue().line_complete);

        let event = runtime.apply_action(VnAction::Advance).unwrap().unwrap();
        assert!(matches!(event, VnRuntimeEvent::Choices(_)));
        runtime.apply_action(VnAction::Down).unwrap();
        runtime.apply_action(VnAction::Confirm).unwrap();

        assert_eq!(
            runtime.variable("route"),
            Some(&VnValue::String("b".to_owned()))
        );
    }

    #[test]
    fn runtime_snapshots_audio_and_video_intents() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<play_bgm "audio/theme.ogg" loop=true fade=1.0>>
<<voice alice "voice/alice_0001.ogg">>
<<play_video "movie/op.webm" layer=40>>
<<preload "images/cg/op.png" "audio/se/chime.ogg">>
<<checkpoint "opening">>
<<unlock_cg "op_cg">>
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();

        for _ in 0..6 {
            runtime.advance().unwrap();
        }
        let snapshot = runtime.snapshot();

        assert_eq!(
            snapshot.audio.bgm.as_ref().map(|bgm| bgm.asset.as_str()),
            Some("audio/theme.ogg")
        );
        assert_eq!(
            snapshot
                .audio
                .voice
                .as_ref()
                .map(|voice| voice.asset.as_str()),
            Some("voice/alice_0001.ogg")
        );
        assert_eq!(
            snapshot
                .video
                .active
                .as_ref()
                .map(|video| video.asset.as_str()),
            Some("movie/op.webm")
        );
        assert_eq!(snapshot.assets.intents.len(), 2);
        assert!(snapshot.progress.checkpoints.contains("opening"));
        assert!(snapshot.progress.unlocked_cg.contains("op_cg"));

        let mut restored = VnRuntime::from_script(runtime.script().clone(), "Start").unwrap();
        restored.restore_snapshot(snapshot).unwrap();
        assert_eq!(
            restored.audio().bgm.as_ref().map(|bgm| bgm.asset.as_str()),
            Some("audio/theme.ogg")
        );
        assert_eq!(
            restored
                .video()
                .active
                .as_ref()
                .map(|video| video.asset.as_str()),
            Some("movie/op.webm")
        );
    }
}
