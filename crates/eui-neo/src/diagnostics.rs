use std::fmt;

use super::draw::UiDrawCommand;
use super::{Color, LayoutRect, Runtime, Screen, UiClip};

#[derive(Debug, Clone)]
pub struct UiDrawDebugTrace {
    pub screen: Screen,
    pub full_clip: UiClip,
    pub command_count: usize,
    pub primitive_count: usize,
    pub push_clip_count: usize,
    pub pop_clip_count: usize,
    pub max_clip_depth: usize,
    pub unbalanced_pops: usize,
    pub remaining_clip_depth: usize,
    pub commands: Vec<UiDrawDebugCommand>,
}

#[derive(Debug, Clone)]
pub struct UiDrawDebugCommand {
    pub index: usize,
    pub depth: usize,
    pub kind: &'static str,
    pub id: Option<String>,
    pub frame: Option<LayoutRect>,
    pub clipped_frame: Option<LayoutRect>,
    pub active_clip: UiClip,
    pub pushed_clip: Option<UiClip>,
    pub visible: bool,
    pub opacity: Option<f32>,
    pub color: Option<Color>,
    pub text: Option<String>,
}

impl UiDrawDebugTrace {
    pub fn from_runtime(runtime: &Runtime) -> Self {
        let screen = runtime.screen();
        let full_clip = UiClip::rect(LayoutRect::new(0.0, 0.0, screen.width, screen.height));
        Self::from_draw_commands(screen, full_clip, runtime.draw_list().commands())
    }

    pub fn from_draw_commands(
        screen: Screen,
        full_clip: UiClip,
        commands: &[UiDrawCommand],
    ) -> Self {
        let mut active_clip = full_clip;
        let mut stack = Vec::new();
        let mut traced = Vec::with_capacity(commands.len());
        let mut primitive_count = 0;
        let mut push_clip_count = 0;
        let mut pop_clip_count = 0;
        let mut max_clip_depth = 0;
        let mut unbalanced_pops = 0;

        for (index, command) in commands.iter().enumerate() {
            match command {
                UiDrawCommand::PushClip(clip) => {
                    push_clip_count += 1;
                    let depth = stack.len();
                    stack.push(active_clip);
                    active_clip = intersect_clip(active_clip, *clip)
                        .unwrap_or_else(|| UiClip::rect(LayoutRect::ZERO));
                    max_clip_depth = max_clip_depth.max(stack.len());
                    traced.push(UiDrawDebugCommand {
                        index,
                        depth,
                        kind: "PushClip",
                        id: None,
                        frame: None,
                        clipped_frame: Some(active_clip.rect),
                        active_clip,
                        pushed_clip: Some(*clip),
                        visible: active_clip.rect.width > 0.0 && active_clip.rect.height > 0.0,
                        opacity: None,
                        color: None,
                        text: None,
                    });
                }
                UiDrawCommand::PopClip => {
                    pop_clip_count += 1;
                    let depth = stack.len();
                    let before = active_clip;
                    match stack.pop() {
                        Some(previous) => active_clip = previous,
                        None => {
                            unbalanced_pops += 1;
                            active_clip = full_clip;
                        }
                    }
                    traced.push(UiDrawDebugCommand {
                        index,
                        depth,
                        kind: "PopClip",
                        id: None,
                        frame: None,
                        clipped_frame: Some(before.rect),
                        active_clip: before,
                        pushed_clip: None,
                        visible: true,
                        opacity: None,
                        color: None,
                        text: None,
                    });
                }
                UiDrawCommand::Rect(draw) => {
                    primitive_count += 1;
                    traced.push(trace_primitive(
                        index,
                        stack.len(),
                        "Rect",
                        draw.id.clone(),
                        draw.frame,
                        active_clip,
                        Some(draw.opacity),
                        Some(draw.color),
                        None,
                    ));
                }
                UiDrawCommand::Text(draw) => {
                    primitive_count += 1;
                    traced.push(trace_primitive(
                        index,
                        stack.len(),
                        "Text",
                        draw.id.clone(),
                        draw.frame,
                        active_clip,
                        Some(draw.opacity),
                        Some(draw.color),
                        Some(draw.text.clone()),
                    ));
                }
                UiDrawCommand::Image(draw) => {
                    primitive_count += 1;
                    traced.push(trace_primitive(
                        index,
                        stack.len(),
                        "Image",
                        draw.id.clone(),
                        draw.frame,
                        active_clip,
                        Some(draw.opacity),
                        Some(draw.tint),
                        None,
                    ));
                }
                UiDrawCommand::NineSlice(draw) => {
                    primitive_count += 1;
                    traced.push(trace_primitive(
                        index,
                        stack.len(),
                        "NineSlice",
                        draw.id.clone(),
                        draw.frame,
                        active_clip,
                        Some(draw.opacity),
                        Some(draw.tint),
                        None,
                    ));
                }
                UiDrawCommand::Polygon(draw) => {
                    primitive_count += 1;
                    traced.push(trace_primitive(
                        index,
                        stack.len(),
                        "Polygon",
                        draw.id.clone(),
                        draw.frame,
                        active_clip,
                        Some(draw.opacity),
                        Some(draw.color),
                        None,
                    ));
                }
            }
        }

        Self {
            screen,
            full_clip,
            command_count: commands.len(),
            primitive_count,
            push_clip_count,
            pop_clip_count,
            max_clip_depth,
            unbalanced_pops,
            remaining_clip_depth: stack.len(),
            commands: traced,
        }
    }

    pub fn filtered<'a>(&'a self, needle: &'a str) -> impl Iterator<Item = &'a UiDrawDebugCommand> {
        self.commands.iter().filter(move |command| {
            command.id.as_deref().is_some_and(|id| id.contains(needle))
                || command
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains(needle))
        })
    }

    pub fn to_json_pretty(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"screen\": {},\n", json_screen(self.screen)));
        out.push_str(&format!(
            "  \"full_clip\": {},\n",
            json_clip(self.full_clip)
        ));
        out.push_str(&format!("  \"command_count\": {},\n", self.command_count));
        out.push_str(&format!(
            "  \"primitive_count\": {},\n",
            self.primitive_count
        ));
        out.push_str(&format!(
            "  \"push_clip_count\": {},\n",
            self.push_clip_count
        ));
        out.push_str(&format!("  \"pop_clip_count\": {},\n", self.pop_clip_count));
        out.push_str(&format!("  \"max_clip_depth\": {},\n", self.max_clip_depth));
        out.push_str(&format!(
            "  \"unbalanced_pops\": {},\n",
            self.unbalanced_pops
        ));
        out.push_str(&format!(
            "  \"remaining_clip_depth\": {},\n",
            self.remaining_clip_depth
        ));
        out.push_str("  \"commands\": [\n");
        for (i, command) in self.commands.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&command_json(command));
            if i + 1 != self.commands.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ]\n");
        out.push('}');
        out
    }
}

impl fmt::Display for UiDrawDebugTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "draw trace screen={:.1}x{:.1} commands={} primitives={} clips={}/{} max_depth={} unbalanced_pops={} remaining_depth={}",
            self.screen.width,
            self.screen.height,
            self.command_count,
            self.primitive_count,
            self.push_clip_count,
            self.pop_clip_count,
            self.max_clip_depth,
            self.unbalanced_pops,
            self.remaining_clip_depth
        )?;
        for command in &self.commands {
            writeln!(f, "{command}")?;
        }
        Ok(())
    }
}

impl fmt::Display for UiDrawDebugCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:04} d{} {}", self.index, self.depth, self.kind)?;
        if let Some(id) = &self.id {
            write!(f, " {id}")?;
        }
        if let Some(frame) = self.frame {
            write!(f, " frame={}", fmt_rect(frame))?;
        }
        if let Some(clipped) = self.clipped_frame {
            write!(f, " clipped={}", fmt_rect(clipped))?;
        }
        write!(
            f,
            " clip={} visible={}",
            fmt_rect(self.active_clip.rect),
            self.visible
        )?;
        if let Some(opacity) = self.opacity {
            write!(f, " opacity={opacity:.3}")?;
        }
        if let Some(text) = &self.text {
            write!(f, " text={text:?}")?;
        }
        Ok(())
    }
}

fn trace_primitive(
    index: usize,
    depth: usize,
    kind: &'static str,
    id: String,
    frame: LayoutRect,
    active_clip: UiClip,
    opacity: Option<f32>,
    color: Option<Color>,
    text: Option<String>,
) -> UiDrawDebugCommand {
    let clipped_frame = intersect_rect(frame, active_clip.rect);
    let visible = clipped_frame
        .is_some_and(|rect| rect.width > 0.0 && rect.height > 0.0 && opacity.unwrap_or(1.0) > 0.0);
    UiDrawDebugCommand {
        index,
        depth,
        kind,
        id: Some(id),
        frame: Some(frame),
        clipped_frame,
        active_clip,
        pushed_clip: None,
        visible,
        opacity,
        color,
        text,
    }
}

fn intersect_clip(left: UiClip, right: UiClip) -> Option<UiClip> {
    intersect_rect(left.rect, right.rect).map(|rect| UiClip {
        rect,
        radius: left.radius.min(right.radius),
    })
}

fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

fn fmt_rect(rect: LayoutRect) -> String {
    format!(
        "({:.1},{:.1},{:.1},{:.1})",
        rect.x, rect.y, rect.width, rect.height
    )
}

fn json_screen(screen: Screen) -> String {
    format!(
        "{{\"width\":{},\"height\":{}}}",
        json_f32(screen.width),
        json_f32(screen.height)
    )
}

fn json_clip(clip: UiClip) -> String {
    format!(
        "{{\"rect\":{},\"radius\":{}}}",
        json_rect(clip.rect),
        json_f32(clip.radius)
    )
}

fn json_rect(rect: LayoutRect) -> String {
    format!(
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"right\":{},\"bottom\":{}}}",
        json_f32(rect.x),
        json_f32(rect.y),
        json_f32(rect.width),
        json_f32(rect.height),
        json_f32(rect.right()),
        json_f32(rect.bottom())
    )
}

fn json_color(color: Color) -> String {
    format!(
        "{{\"r\":{},\"g\":{},\"b\":{},\"a\":{}}}",
        json_f32(color.r),
        json_f32(color.g),
        json_f32(color.b),
        json_f32(color.a)
    )
}

fn command_json(command: &UiDrawDebugCommand) -> String {
    let id = command
        .id
        .as_ref()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .unwrap_or_else(|| "null".to_string());
    let frame = command
        .frame
        .map(json_rect)
        .unwrap_or_else(|| "null".to_string());
    let clipped_frame = command
        .clipped_frame
        .map(json_rect)
        .unwrap_or_else(|| "null".to_string());
    let pushed_clip = command
        .pushed_clip
        .map(json_clip)
        .unwrap_or_else(|| "null".to_string());
    let opacity = command
        .opacity
        .map(json_f32)
        .unwrap_or_else(|| "null".to_string());
    let color = command
        .color
        .map(json_color)
        .unwrap_or_else(|| "null".to_string());
    let text = command
        .text
        .as_ref()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .unwrap_or_else(|| "null".to_string());

    format!(
        "{{\"index\":{},\"depth\":{},\"kind\":\"{}\",\"id\":{},\"frame\":{},\"clipped_frame\":{},\"active_clip\":{},\"pushed_clip\":{},\"visible\":{},\"opacity\":{},\"color\":{},\"text\":{}}}",
        command.index,
        command.depth,
        command.kind,
        id,
        frame,
        clipped_frame,
        json_clip(command.active_clip),
        pushed_clip,
        command.visible,
        opacity,
        color,
        text
    )
}

fn json_f32(value: f32) -> String {
    if value.is_finite() {
        format!("{value:.3}")
    } else {
        "null".to_string()
    }
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}
