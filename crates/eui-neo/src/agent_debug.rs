//! Local agent-facing debug service for Neo UI runtimes.

use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::expert::UiDrawCommand;
use crate::{FrameInput, KeyboardEvent, LayoutRect, PointerEvent, Runtime, ScrollEvent};

const PROTOCOL_VERSION: u32 = 1;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

pub struct AgentDebugService {
    app_id: String,
    endpoint: AgentDebugEndpoint,
    commands: Receiver<AgentDebugRequest>,
    running: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDebugEffect {
    Screenshot { path: PathBuf },
}

impl std::fmt::Debug for AgentDebugService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDebugService")
            .field("app_id", &self.app_id)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct AgentDebugEndpoint {
    pub app_id: String,
    pub url: String,
    pub token: String,
    pub pid: u32,
    pub protocol_version: u32,
    pub endpoint_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct AgentDebugContext {
    state: Vec<(String, String)>,
    dropdowns: Vec<AgentDropdownState>,
}

#[derive(Debug, Clone)]
pub struct AgentDropdownState {
    pub id: String,
    pub open: Option<bool>,
    pub selected: Option<String>,
}

#[derive(Debug)]
struct AgentDebugRequest {
    id: Value,
    method: String,
    params: Value,
    respond: Sender<String>,
}

#[derive(Debug, Clone)]
struct AgentDebugCommand {
    id: Value,
    method: String,
    params: Value,
}

impl AgentDebugService {
    pub fn from_env(app_id: impl Into<String>) -> Option<Self> {
        if !env_flag("SKY_NEO_AGENT_DEBUG") {
            return None;
        }
        match Self::new(app_id) {
            Ok(service) => Some(service),
            Err(error) => {
                eprintln!("[neo-debug] failed to start agent debug service: {error}");
                None
            }
        }
    }

    pub fn new(app_id: impl Into<String>) -> std::io::Result<Self> {
        let app_id = app_id.into();
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let token = make_token(&app_id, addr);
        let (tx, rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let endpoint_path = endpoint_path(&app_id);
        let endpoint = AgentDebugEndpoint {
            app_id: app_id.clone(),
            url: format!("http://{addr}"),
            token: token.clone(),
            pid: std::process::id(),
            protocol_version: PROTOCOL_VERSION,
            endpoint_path,
        };
        write_endpoint(&endpoint)?;

        let thread_running = running.clone();
        let thread_endpoint = endpoint.clone();
        let thread = thread::spawn(move || {
            serve_debug_http(listener, thread_running, tx, thread_endpoint);
        });

        Ok(Self {
            app_id,
            endpoint,
            commands: rx,
            running,
            thread: Some(thread),
        })
    }

    pub fn endpoint(&self) -> &AgentDebugEndpoint {
        &self.endpoint
    }

    pub fn update(
        &mut self,
        runtime: &mut Runtime,
        context: AgentDebugContext,
    ) -> Vec<AgentDebugEffect> {
        let mut effects = Vec::new();
        for request in self.commands.try_iter().take(32) {
            let result = execute_command(
                runtime,
                &context,
                AgentDebugCommand {
                    id: request.id,
                    method: request.method,
                    params: request.params,
                },
            );
            effects.extend(result.effects);
            let _ = request.respond.send(result.response);
        }
        effects
    }
}

impl Drop for AgentDebugService {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(addr) = self.endpoint.url.strip_prefix("http://") {
            let _ = TcpStream::connect(addr);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.endpoint.endpoint_path);
    }
}

impl AgentDebugContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.state.push((key.into(), value.to_string()));
        self
    }

    pub fn dropdown(
        mut self,
        id: impl Into<String>,
        open: Option<bool>,
        selected: Option<impl ToString>,
    ) -> Self {
        self.dropdowns.push(AgentDropdownState {
            id: id.into(),
            open,
            selected: selected.map(|value| value.to_string()),
        });
        self
    }
}

fn serve_debug_http(
    listener: TcpListener,
    running: Arc<AtomicBool>,
    commands: Sender<AgentDebugRequest>,
    endpoint: AgentDebugEndpoint,
) {
    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => handle_connection(stream, &commands, &endpoint),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    commands: &Sender<AgentDebugRequest>,
    endpoint: &AgentDebugEndpoint,
) {
    let response = match read_http_request(&mut stream) {
        Ok(request) => route_request(request, commands, endpoint),
        Err(error) => http_response(400, "text/plain", &format!("bad request: {error}")),
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn route_request(
    request: HttpRequest,
    commands: &Sender<AgentDebugRequest>,
    endpoint: &AgentDebugEndpoint,
) -> String {
    if request.method == "GET" && request.path == "/json/version" {
        let body = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "appId": endpoint.app_id,
            "pid": endpoint.pid,
            "url": endpoint.url,
        })
        .to_string();
        return http_response(200, "application/json", &body);
    }

    if request.method != "POST" || request.path != "/rpc" {
        return http_response(
            404,
            "application/json",
            &json_error(Value::Null, "not_found"),
        );
    }

    if request.header("x-neo-debug-token") != Some(endpoint.token.as_str()) {
        return http_response(
            403,
            "application/json",
            &json_error(Value::Null, "forbidden"),
        );
    }

    let parsed: Value = match serde_json::from_str(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return http_response(
                400,
                "application/json",
                &json_error(Value::Null, &format!("invalid_json: {error}")),
            );
        }
    };
    let id = parsed.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = parsed.get("method").and_then(Value::as_str) else {
        return http_response(400, "application/json", &json_error(id, "missing_method"));
    };
    let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
    let (tx, rx) = mpsc::channel();
    if commands
        .send(AgentDebugRequest {
            id,
            method: method.to_string(),
            params,
            respond: tx,
        })
        .is_err()
    {
        return http_response(
            503,
            "application/json",
            &json_error(Value::Null, "service_closed"),
        );
    }
    match rx.recv_timeout(DEFAULT_TIMEOUT) {
        Ok(body) => http_response(200, "application/json", &body),
        Err(_) => http_response(504, "application/json", &json_error(Value::Null, "timeout")),
    }
}

fn execute_command(
    runtime: &mut Runtime,
    context: &AgentDebugContext,
    command: AgentDebugCommand,
) -> AgentCommandOutput {
    let result = match command.method.as_str() {
        "snapshot" => snapshot_result(
            runtime,
            context,
            filter_param(&command.params),
            max_rows_param(&command.params),
        )
        .map(AgentCommandResult::value),
        "layers" => layers_result(runtime).map(AgentCommandResult::value),
        "elements" => elements_result(
            runtime,
            filter_param(&command.params),
            max_rows_param(&command.params),
        )
        .map(AgentCommandResult::value),
        "draw" => draw_result(
            runtime,
            filter_param(&command.params),
            max_rows_param(&command.params),
        )
        .map(AgentCommandResult::value),
        "state" => state_result(context).map(AgentCommandResult::value),
        "diagnose-dropdown" => {
            { diagnose_dropdown_result(runtime, context, id_param(&command.params)) }
                .map(AgentCommandResult::value)
        }
        "diagnose-element" => diagnose_element_result(runtime, id_param(&command.params))
            .map(AgentCommandResult::value),
        "diagnose-overflow" => diagnose_overflow_result(
            runtime,
            id_param(&command.params),
            max_rows_param(&command.params),
        )
        .map(AgentCommandResult::value),
        "diagnose-input" => {
            diagnose_input_result(runtime, id_param(&command.params)).map(AgentCommandResult::value)
        }
        "hit-test" => hit_test_result(
            runtime,
            number_param(&command.params, "x"),
            number_param(&command.params, "y"),
            max_rows_param(&command.params),
        )
        .map(AgentCommandResult::value),
        "click" => click_result(runtime, id_param(&command.params)).map(AgentCommandResult::value),
        "hover" => hover_result(runtime, id_param(&command.params)).map(AgentCommandResult::value),
        "scroll" => scroll_result(
            runtime,
            id_param(&command.params),
            number_param(&command.params, "x"),
            number_param(&command.params, "y"),
        )
        .map(AgentCommandResult::value),
        "screenshot" => screenshot_result(string_param(&command.params, "path")),
        "screenshot-element" => screenshot_element_result(
            runtime,
            id_param(&command.params),
            string_param(&command.params, "path"),
            number_param(&command.params, "padding").unwrap_or(16.0),
        ),
        "type-text" => type_text_result(runtime, string_param(&command.params, "text"))
            .map(AgentCommandResult::value),
        _ => Err(format!("unknown_method: {}", command.method)),
    };

    match result {
        Ok(result) => AgentCommandOutput {
            effects: result.effects,
            response: json!({
                "jsonrpc": "2.0",
                "id": command.id,
                "ok": true,
                "result": result.value,
            })
            .to_string(),
        },
        Err(error) => AgentCommandOutput {
            effects: Vec::new(),
            response: json!({
                "jsonrpc": "2.0",
                "id": command.id,
                "ok": false,
                "error": error,
            })
            .to_string(),
        },
    }
}

struct AgentCommandOutput {
    response: String,
    effects: Vec<AgentDebugEffect>,
}

struct AgentCommandResult {
    value: Value,
    effects: Vec<AgentDebugEffect>,
}

impl AgentCommandResult {
    fn value(value: Value) -> Self {
        Self {
            value,
            effects: Vec::new(),
        }
    }

    fn with_effect(value: Value, effect: AgentDebugEffect) -> Self {
        Self {
            value,
            effects: vec![effect],
        }
    }
}

fn snapshot_result(
    runtime: &Runtime,
    context: &AgentDebugContext,
    filter: Option<&str>,
    max_rows: usize,
) -> Result<Value, String> {
    let snapshot = runtime.diagnostics().current_snapshot();
    let mut text = String::new();
    write_summary(&mut text, runtime, &snapshot);
    write_state(&mut text, context);
    write_layers(&mut text, &snapshot);
    write_elements(&mut text, runtime, filter, max_rows);
    write_draw(&mut text, runtime, filter, max_rows);
    write_input(&mut text, &snapshot);
    write_events(&mut text, &snapshot, max_rows);
    Ok(json!({ "text": text }))
}

fn layers_result(runtime: &Runtime) -> Result<Value, String> {
    let snapshot = runtime.diagnostics().current_snapshot();
    let mut text = String::new();
    write_layers(&mut text, &snapshot);
    Ok(json!({ "text": text }))
}

fn elements_result(
    runtime: &Runtime,
    filter: Option<&str>,
    max_rows: usize,
) -> Result<Value, String> {
    let mut text = String::new();
    write_elements(&mut text, runtime, filter, max_rows);
    Ok(json!({ "text": text }))
}

fn draw_result(runtime: &Runtime, filter: Option<&str>, max_rows: usize) -> Result<Value, String> {
    let mut text = String::new();
    write_draw(&mut text, runtime, filter, max_rows);
    Ok(json!({ "text": text }))
}

fn state_result(context: &AgentDebugContext) -> Result<Value, String> {
    let mut text = String::new();
    write_state(&mut text, context);
    Ok(json!({ "text": text }))
}

fn diagnose_dropdown_result(
    runtime: &Runtime,
    context: &AgentDebugContext,
    id: Option<&str>,
) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let popup_id = format!("{id}.popup");
    let surface_id = format!("{id}.popup.surface");
    let bg_suffix = format!("{id}.popup.bg");
    let snapshot = runtime.diagnostics().current_snapshot();
    let layer = snapshot
        .layers
        .iter()
        .find(|layer| layer.id.ends_with(&popup_id));
    let state = context.dropdowns.iter().find(|state| state.id == id);
    let surface = runtime.diagnostics().find(&surface_id);
    let popup_draw =
        runtime
            .draw_list()
            .commands()
            .iter()
            .enumerate()
            .find_map(|(index, command)| {
                draw_command_id(command)
                    .filter(|draw_id| draw_id.ends_with(&bg_suffix) || draw_id.contains(&popup_id))
                    .map(|draw_id| (index, draw_id.to_string()))
            });
    let first_issue = if state.and_then(|state| state.open) == Some(false) {
        "state"
    } else if layer.is_none() {
        "layer"
    } else if layer.is_some_and(|layer| !layer.open) {
        "layer_open"
    } else if layer.is_some_and(|layer| format!("{:?}", layer.anchor_source) == "Missing") {
        "anchor"
    } else if surface.is_none() {
        "element"
    } else if popup_draw.is_none() {
        "draw"
    } else if snapshot
        .layer_dismissals
        .iter()
        .any(|dismissal| dismissal.id.ends_with(&popup_id))
    {
        "pointer_dismissal"
    } else {
        "ok"
    };

    let mut text = String::new();
    let _ = writeln!(text, "DIAGNOSIS dropdown={id}");
    let _ = writeln!(
        text,
        "first_issue={first_issue} state_open={} state_selected={}",
        state
            .and_then(|state| state.open.map(|value| value.to_string()))
            .unwrap_or_else(|| "<not supplied>".to_string()),
        state
            .and_then(|state| state.selected.clone())
            .unwrap_or_else(|| "<not supplied>".to_string())
    );
    if let Some(layer) = layer {
        let _ = writeln!(
            text,
            "layer_exists=true layer_open={} z={} anchor={:?} anchor_source={:?} outside={:?} action={:?}",
            layer.open, layer.z_index, layer.anchor, layer.anchor_source, layer.outside_click, layer.action
        );
    } else {
        let _ = writeln!(text, "layer_exists=false");
    }
    if let Some(surface) = surface {
        let _ = writeln!(
            text,
            "popup_surface_exists=true frame={} z={} clip={} interactive={}",
            rect_text(surface.frame),
            surface.z_index,
            surface.clip,
            surface.interactive
        );
    } else {
        let _ = writeln!(text, "popup_surface_exists=false");
    }
    if let Some((index, id)) = popup_draw {
        let _ = writeln!(
            text,
            "popup_draw_exists=true draw_index={index} draw_id={id}"
        );
    } else {
        let _ = writeln!(text, "popup_draw_exists=false");
    }
    if let Some(pointer) = snapshot.layer_pointer.last() {
        let _ = writeln!(text, "last_layer_pointer={pointer:?}");
    }
    if let Some(dismissal) = snapshot.layer_dismissals.last() {
        let _ = writeln!(text, "last_dismissal={dismissal:?}");
    }
    Ok(json!({ "text": text, "first_issue": first_issue }))
}

fn diagnose_element_result(runtime: &Runtime, id: Option<&str>) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let Some(element) = runtime.diagnostics().find(id) else {
        let mut text = String::new();
        let _ = writeln!(text, "FAIL element={id}");
        let _ = writeln!(text, "first_issue=missing");
        let _ = writeln!(
            text,
            "No element with this id is present in the current tree."
        );
        return Ok(json!({ "text": text, "first_issue": "missing" }));
    };

    let snapshot = runtime.diagnostics().current_snapshot();
    let draw_match = find_draw_for_element(runtime, &element.id);
    let clipped_by =
        first_clip_excluding_element(runtime.diagnostics().roots(), &element.id, element.frame);
    let zero_size = element.frame.width <= 0.0 || element.frame.height <= 0.0;
    let invisible_opacity = element.opacity <= 0.0;
    let element_center = rect_center(element.frame);
    let blocked_by_layer = snapshot.layers.iter().any(|layer| {
        if !layer.open || layer.z_index <= element.z_index || element.id.contains(&layer.id) {
            return false;
        }
        runtime
            .diagnostics()
            .find(&layer.id)
            .or_else(|| {
                runtime
                    .diagnostics()
                    .find(layer.id.trim_start_matches("neo."))
            })
            .is_some_and(|layer_element| {
                layer_element.opacity > 0.0
                    && !layer_element.disabled
                    && layer_element.frame.contains(element_center)
            })
    });
    let first_issue = if zero_size {
        "layout"
    } else if invisible_opacity {
        "opacity"
    } else if clipped_by.is_some() {
        "clip"
    } else if draw_match.is_none() {
        "draw"
    } else if blocked_by_layer {
        "layer_block"
    } else {
        "ok"
    };

    let mut text = String::new();
    let status = if first_issue == "ok" { "PASS" } else { "FAIL" };
    let _ = writeln!(text, "{status} element={}", element.id);
    let _ = writeln!(
        text,
        "first_issue={first_issue} kind={:?} frame={} z={} clip={} opacity={:.2} interactive={} focusable={} disabled={}",
        element.kind,
        rect_text(element.frame),
        element.z_index,
        element.clip,
        element.opacity,
        element.interactive,
        element.focusable,
        element.disabled
    );
    if let Some((clip_id, clip_rect)) = clipped_by {
        let _ = writeln!(
            text,
            "clipped_by id={clip_id} clip_frame={}",
            rect_text(clip_rect)
        );
    }
    if let Some((index, draw_id, frame)) = draw_match {
        let _ = writeln!(
            text,
            "draw_exists=true draw_index={index} draw_id={draw_id} draw_frame={}",
            rect_text(frame)
        );
    } else {
        let _ = writeln!(text, "draw_exists=false");
    }
    let _ = writeln!(
        text,
        "input hover={:?} active={:?} capture={:?} keyboard={:?} text={:?}",
        snapshot.pointer_hover_id,
        snapshot.pointer_active_id,
        snapshot.pointer_capture_id,
        snapshot.keyboard_focus_id,
        snapshot.text_focus_id
    );
    if blocked_by_layer {
        let _ = writeln!(text, "higher_open_layers:");
        for layer in snapshot
            .layers
            .iter()
            .filter(|layer| layer.open && layer.z_index > element.z_index)
        {
            if let Some(layer_element) = runtime.diagnostics().find(&layer.id).or_else(|| {
                runtime
                    .diagnostics()
                    .find(layer.id.trim_start_matches("neo."))
            }) {
                let _ = writeln!(
                    text,
                    "  id={} z={} frame={} anchor={:?} source={:?}",
                    layer.id,
                    layer.z_index,
                    rect_text(layer_element.frame),
                    layer.anchor,
                    layer.anchor_source
                );
            }
        }
    }
    Ok(json!({ "text": text, "first_issue": first_issue }))
}

fn diagnose_overflow_result(
    runtime: &Runtime,
    id: Option<&str>,
    max_rows: usize,
) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let element = runtime
        .diagnostics()
        .find(id)
        .ok_or_else(|| format!("missing element: {id}"))?;
    let mut overflows = Vec::new();
    collect_overflows(element, &mut overflows);

    let mut text = String::new();
    let status = if overflows.is_empty() { "PASS" } else { "FAIL" };
    let _ = writeln!(
        text,
        "{status} overflow parent={} frame={} children_checked={}",
        element.id,
        rect_text(element.frame),
        count_descendants(element)
    );
    if overflows.is_empty() {
        let _ = writeln!(text, "first_issue=ok");
    } else {
        let _ = writeln!(
            text,
            "first_issue=overflow overflow_count={}",
            overflows.len()
        );
        for record in overflows.iter().take(max_rows) {
            let _ = writeln!(
                text,
                "child={} parent={} frame={} parent_frame={} overflow left={:.1} right={:.1} top={:.1} bottom={:.1}",
                record.child_id,
                record.parent_id,
                rect_text(record.child_frame),
                rect_text(record.parent_frame),
                record.left,
                record.right,
                record.top,
                record.bottom
            );
        }
        if overflows.len() > max_rows {
            let _ = writeln!(text, "overflow_truncated={}", overflows.len() - max_rows);
        }
        let _ = writeln!(
            text,
            "suggestion=shrink child, increase parent, reposition child, or enable intentional clipping"
        );
    }
    Ok(json!({
        "text": text,
        "first_issue": if overflows.is_empty() { "ok" } else { "overflow" },
        "overflow_count": overflows.len(),
    }))
}

fn diagnose_input_result(runtime: &Runtime, id: Option<&str>) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let Some(element) = runtime.diagnostics().find(id) else {
        let text =
            format!("FAIL input={id}\nfirst_issue=missing\nNo element with this id is present.\n");
        return Ok(json!({ "text": text, "first_issue": "missing" }));
    };
    let center = rect_center(element.frame);
    let clipped_by =
        first_clip_excluding_element(runtime.diagnostics().roots(), &element.id, element.frame);
    let draw_match = find_draw_for_element(runtime, &element.id);
    let mut hits = Vec::new();
    let mut order = 0;
    for root in runtime.diagnostics().roots() {
        collect_hits(root, center, None, 0, &mut order, &mut hits);
    }
    hits.sort_by(|a, b| {
        b.z.cmp(&a.z)
            .then_with(|| b.depth.cmp(&a.depth))
            .then_with(|| b.order.cmp(&a.order))
    });
    let top_interactive = hits.iter().find(|hit| hit.can_receive_input());
    let top_matches_target = top_interactive
        .is_some_and(|hit| hit.id == element.id || hit.id.starts_with(&format!("{}.", element.id)));
    let first_issue = if !element.interactive {
        "non_interactive"
    } else if element.disabled {
        "disabled"
    } else if element.opacity <= 0.01 {
        "opacity"
    } else if clipped_by.is_some() {
        "clip"
    } else if draw_match.is_none() {
        "draw"
    } else if !top_matches_target {
        "covered"
    } else {
        "ok"
    };

    let mut text = String::new();
    let status = if first_issue == "ok" { "PASS" } else { "FAIL" };
    let _ = writeln!(text, "{status} input={}", element.id);
    let _ = writeln!(
        text,
        "first_issue={first_issue} center={:.1},{:.1} frame={} interactive={} disabled={} opacity={:.2} draw_exists={}",
        center[0],
        center[1],
        rect_text(element.frame),
        element.interactive,
        element.disabled,
        element.opacity,
        draw_match.is_some()
    );
    if let Some((clip_id, clip_rect)) = clipped_by {
        let _ = writeln!(
            text,
            "clipped_by id={clip_id} clip_frame={}",
            rect_text(clip_rect)
        );
    }
    if let Some(hit) = top_interactive {
        let _ = writeln!(
            text,
            "top_interactive id={} frame={} z={} kind={} disabled={} opacity={:.2}",
            hit.id,
            rect_text(hit.frame),
            hit.z,
            hit.kind,
            hit.disabled,
            hit.opacity
        );
    } else {
        let _ = writeln!(text, "top_interactive=<none>");
    }
    for hit in hits.iter().take(8) {
        let _ = writeln!(
            text,
            "hit id={} z={} depth={} interactive={} disabled={} opacity={:.2} clipped={}",
            hit.id, hit.z, hit.depth, hit.interactive, hit.disabled, hit.opacity, hit.clipped
        );
    }
    Ok(json!({ "text": text, "first_issue": first_issue }))
}

fn hit_test_result(
    runtime: &Runtime,
    x: Option<f64>,
    y: Option<f64>,
    max_rows: usize,
) -> Result<Value, String> {
    let x = x.ok_or_else(|| "missing x".to_string())? as f32;
    let y = y.ok_or_else(|| "missing y".to_string())? as f32;
    let mut hits = Vec::new();
    let mut order = 0;
    for root in runtime.diagnostics().roots() {
        collect_hits(root, [x, y], None, 0, &mut order, &mut hits);
    }
    hits.sort_by(|a, b| {
        b.z.cmp(&a.z)
            .then_with(|| b.depth.cmp(&a.depth))
            .then_with(|| b.order.cmp(&a.order))
    });

    let mut text = String::new();
    let _ = writeln!(text, "HIT_TEST point={x:.1},{y:.1} hits={}", hits.len());
    if let Some(hit) = hits.iter().find(|hit| hit.can_receive_input()) {
        let _ = writeln!(
            text,
            "top_interactive id={} z={} frame={} kind={}",
            hit.id,
            hit.z,
            rect_text(hit.frame),
            hit.kind
        );
    } else {
        let _ = writeln!(text, "top_interactive=<none>");
    }
    for hit in hits.iter().take(max_rows) {
        let _ = writeln!(
            text,
            "id={} kind={} frame={} z={} depth={} interactive={} focusable={} disabled={} opacity={:.2} clipped={}",
            hit.id,
            hit.kind,
            rect_text(hit.frame),
            hit.z,
            hit.depth,
            hit.interactive,
            hit.focusable,
            hit.disabled,
            hit.opacity,
            hit.clipped
        );
    }
    Ok(json!({ "text": text, "hits": hits.len() }))
}

fn screenshot_result(path: Option<&str>) -> Result<AgentCommandResult, String> {
    let path = path.ok_or_else(|| "missing path".to_string())?;
    let mut text = String::new();
    let _ = writeln!(text, "SCREENSHOT requested path={path}");
    Ok(AgentCommandResult::with_effect(
        json!({ "text": text, "path": path }),
        AgentDebugEffect::Screenshot {
            path: PathBuf::from(path),
        },
    ))
}

fn screenshot_element_result(
    runtime: &Runtime,
    id: Option<&str>,
    path: Option<&str>,
    padding: f64,
) -> Result<AgentCommandResult, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let path = path.ok_or_else(|| "missing path".to_string())?;
    let element = runtime
        .diagnostics()
        .find(id)
        .ok_or_else(|| format!("missing element: {id}"))?;
    let padding = padding.max(0.0) as f32;
    let rect = padded_rect(
        element.frame,
        padding,
        runtime.screen().width,
        runtime.screen().height,
    );
    let mut text = String::new();
    let _ = writeln!(text, "SCREENSHOT_ELEMENT requested id={}", element.id);
    let _ = writeln!(
        text,
        "path={path} crop={} padding={padding:.1}",
        rect_text(rect)
    );
    Ok(AgentCommandResult::with_effect(
        json!({
            "text": text,
            "path": path,
            "element_id": element.id,
            "screen": {
                "width": runtime.screen().width,
                "height": runtime.screen().height,
            },
            "crop": {
                "x": rect.x,
                "y": rect.y,
                "width": rect.width,
                "height": rect.height,
            }
        }),
        AgentDebugEffect::Screenshot {
            path: PathBuf::from(path),
        },
    ))
}

fn click_result(runtime: &mut Runtime, id: Option<&str>) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let frame = runtime
        .diagnostics()
        .find(id)
        .map(|element| element.frame)
        .ok_or_else(|| format!("missing element: {id}"))?;
    let point = [frame.x + frame.width * 0.5, frame.y + frame.height * 0.5];
    let changed =
        runtime.dispatch_frame_input(FrameInput::new(runtime.screen(), 0.0).pointer_events([
            PointerEvent::pressed_at(point[0], point[1]),
            PointerEvent::released_at(point[0], point[1]),
        ]));
    Ok(json!({
        "text": format!(
            "CLICK id={id} frame={} point={:.1},{:.1} input_changed={} needs_compose={}\n",
            rect_text(frame),
            point[0],
            point[1],
            changed,
            runtime.needs_compose()
        ),
        "input_changed": changed,
        "needs_compose": runtime.needs_compose(),
    }))
}

fn hover_result(runtime: &mut Runtime, id: Option<&str>) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let frame = runtime
        .diagnostics()
        .find(id)
        .map(|element| element.frame)
        .ok_or_else(|| format!("missing element: {id}"))?;
    let point = [frame.x + frame.width * 0.5, frame.y + frame.height * 0.5];
    let changed = runtime.dispatch_frame_input(
        FrameInput::new(runtime.screen(), 0.0).pointer(PointerEvent::at(point[0], point[1])),
    );
    Ok(json!({
        "text": format!("HOVER id={id} point={:.1},{:.1} input_changed={changed}\n", point[0], point[1]),
        "input_changed": changed,
    }))
}

fn scroll_result(
    runtime: &mut Runtime,
    id: Option<&str>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<Value, String> {
    let id = id.ok_or_else(|| "missing id".to_string())?;
    let frame = runtime
        .diagnostics()
        .find(id)
        .map(|element| element.frame)
        .ok_or_else(|| format!("missing element: {id}"))?;
    let point = [frame.x + frame.width * 0.5, frame.y + frame.height * 0.5];
    let x = x.unwrap_or(0.0) as f32;
    let y = y.unwrap_or(0.0) as f32;
    let changed = runtime.dispatch_frame_input(
        FrameInput::new(runtime.screen(), 0.0)
            .pointer(PointerEvent::at(point[0], point[1]))
            .scroll(ScrollEvent { x, y }),
    );
    Ok(json!({
        "text": format!("SCROLL id={id} delta={x:.1},{y:.1} input_changed={changed}\n"),
        "input_changed": changed,
    }))
}

fn type_text_result(runtime: &mut Runtime, text: Option<&str>) -> Result<Value, String> {
    let text = text.ok_or_else(|| "missing text".to_string())?;
    let changed = runtime.dispatch_frame_input(FrameInput::new(runtime.screen(), 0.0).keyboard(
        KeyboardEvent {
            text: text.to_string(),
            ..KeyboardEvent::default()
        },
    ));
    Ok(json!({
        "text": format!("TYPE text={text:?} input_changed={changed}\n"),
        "input_changed": changed,
    }))
}

fn write_summary(out: &mut String, runtime: &Runtime, snapshot: &crate::expert::UiDebugSnapshot) {
    let _ = writeln!(out, "SUMMARY");
    let _ = writeln!(
        out,
        "frame={} screen={:.0}x{:.0} roots={} layout={:?} needs_render={} needs_compose={} full_redraw={} pass={:?}",
        snapshot.frame_index,
        snapshot.screen.width,
        snapshot.screen.height,
        runtime.diagnostics().roots().len(),
        snapshot.layout_mode,
        snapshot.needs_render,
        snapshot.needs_compose,
        snapshot.full_redraw,
        snapshot.pass_flags
    );
}

fn write_state(out: &mut String, context: &AgentDebugContext) {
    let _ = writeln!(out, "\nSTATE");
    if context.state.is_empty() && context.dropdowns.is_empty() {
        let _ = writeln!(out, "<none supplied>");
        return;
    }
    for (key, value) in &context.state {
        let _ = writeln!(out, "{key}={value}");
    }
    for dropdown in &context.dropdowns {
        let _ = writeln!(
            out,
            "dropdown id={} open={} selected={}",
            dropdown.id,
            dropdown
                .open
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<not supplied>".to_string()),
            dropdown
                .selected
                .clone()
                .unwrap_or_else(|| "<not supplied>".to_string())
        );
    }
}

fn write_layers(out: &mut String, snapshot: &crate::expert::UiDebugSnapshot) {
    let _ = writeln!(out, "\nLAYERS count={}", snapshot.layers.len());
    for layer in &snapshot.layers {
        let _ = writeln!(
            out,
            "id={} open={} z={} kind={:?} action={:?} anchor={:?} source={:?} outside={:?}",
            layer.id,
            layer.open,
            layer.z_index,
            layer.kind,
            layer.action,
            layer.anchor,
            layer.anchor_source,
            layer.outside_click
        );
    }
}

fn write_elements(out: &mut String, runtime: &Runtime, filter: Option<&str>, max_rows: usize) {
    let _ = writeln!(out, "\nELEMENTS filter={:?}", filter);
    let mut written = 0;
    let mut total = 0;
    for root in runtime.diagnostics().roots() {
        write_element(out, root, None, filter, max_rows, &mut written, &mut total);
    }
    let _ = writeln!(out, "elements_shown={written} elements_total={total}");
}

fn write_element(
    out: &mut String,
    element: &crate::Element,
    parent: Option<&str>,
    filter: Option<&str>,
    max_rows: usize,
    written: &mut usize,
    total: &mut usize,
) {
    *total += 1;
    let matches = filter.is_none_or(|filter| element.id.contains(filter));
    if matches && *written < max_rows {
        *written += 1;
        let _ = writeln!(
            out,
            "id={} parent={} kind={:?} frame={} z={} clip={} interactive={} focusable={} text={:?}",
            element.id,
            parent.unwrap_or("-"),
            element.kind,
            rect_text(element.frame),
            element.z_index,
            element.clip,
            element.interactive,
            element.focusable,
            element.text
        );
    }
    for child in &element.children {
        write_element(
            out,
            child,
            Some(&element.id),
            filter,
            max_rows,
            written,
            total,
        );
    }
}

fn write_draw(out: &mut String, runtime: &Runtime, filter: Option<&str>, max_rows: usize) {
    let draw_list = runtime.draw_list();
    let _ = writeln!(
        out,
        "\nDRAW filter={filter:?} commands={}",
        draw_list.commands().len()
    );
    let mut written = 0;
    for (index, command) in draw_list.commands().iter().enumerate() {
        if written >= max_rows {
            break;
        }
        if let Some(id) = draw_command_id(command) {
            if filter.is_some_and(|filter| !id.contains(filter)) {
                continue;
            }
        } else if filter.is_some() {
            continue;
        }
        written += 1;
        let _ = writeln!(out, "#{index} {}", draw_command_text(command));
    }
    let _ = writeln!(out, "draw_shown={written}");
}

fn write_input(out: &mut String, snapshot: &crate::expert::UiDebugSnapshot) {
    let _ = writeln!(out, "\nINPUT");
    let _ = writeln!(
        out,
        "hover={:?} active={:?} capture={:?} keyboard={:?} text={:?} scroll={:?} drag={:?}",
        snapshot.pointer_hover_id,
        snapshot.pointer_active_id,
        snapshot.pointer_capture_id,
        snapshot.keyboard_focus_id,
        snapshot.text_focus_id,
        snapshot.scroll_owner_id,
        snapshot.drag_owner_id
    );
    for pointer in &snapshot.layer_pointer {
        let _ = writeln!(out, "layer_pointer={pointer:?}");
    }
    for dismissal in &snapshot.layer_dismissals {
        let _ = writeln!(out, "layer_dismissal={dismissal:?}");
    }
}

fn write_events(out: &mut String, snapshot: &crate::expert::UiDebugSnapshot, max_rows: usize) {
    let _ = writeln!(out, "\nEVENTS count={}", snapshot.events.len());
    for event in snapshot.events.iter().take(max_rows) {
        let _ = writeln!(out, "{event:?}");
    }
    let _ = writeln!(
        out,
        "\nINVALIDATIONS count={}",
        snapshot.invalidations.len()
    );
    for invalidation in snapshot.invalidations.iter().take(max_rows) {
        let _ = writeln!(out, "{invalidation:?}");
    }
}

#[derive(Debug, Clone)]
struct HitRecord {
    id: String,
    kind: String,
    frame: LayoutRect,
    z: i32,
    depth: usize,
    order: usize,
    interactive: bool,
    focusable: bool,
    disabled: bool,
    opacity: f32,
    clipped: bool,
}

#[derive(Debug, Clone)]
struct OverflowRecord {
    parent_id: String,
    child_id: String,
    parent_frame: LayoutRect,
    child_frame: LayoutRect,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl HitRecord {
    fn can_receive_input(&self) -> bool {
        self.interactive && !self.disabled && !self.clipped && self.opacity > 0.01
    }
}

fn collect_overflows(element: &crate::Element, records: &mut Vec<OverflowRecord>) {
    for child in &element.children {
        let left = (element.frame.x - child.frame.x).max(0.0);
        let right = (child.frame.right() - element.frame.right()).max(0.0);
        let top = (element.frame.y - child.frame.y).max(0.0);
        let bottom = (child.frame.bottom() - element.frame.bottom()).max(0.0);
        if left > 0.5 || right > 0.5 || top > 0.5 || bottom > 0.5 {
            records.push(OverflowRecord {
                parent_id: element.id.clone(),
                child_id: child.id.clone(),
                parent_frame: element.frame,
                child_frame: child.frame,
                left,
                right,
                top,
                bottom,
            });
        }
        collect_overflows(child, records);
    }
}

fn count_descendants(element: &crate::Element) -> usize {
    element
        .children
        .iter()
        .map(|child| 1 + count_descendants(child))
        .sum()
}

fn collect_hits(
    element: &crate::Element,
    point: [f32; 2],
    clip: Option<LayoutRect>,
    depth: usize,
    order: &mut usize,
    hits: &mut Vec<HitRecord>,
) {
    let current_order = *order;
    *order += 1;
    let clipped = clip.is_some_and(|clip| !clip.contains(point));
    if element.frame.contains(point) {
        hits.push(HitRecord {
            id: element.id.clone(),
            kind: format!("{:?}", element.kind),
            frame: element.frame,
            z: element.z_index,
            depth,
            order: current_order,
            interactive: element.interactive,
            focusable: element.focusable,
            disabled: element.disabled,
            opacity: element.opacity,
            clipped,
        });
    }

    let child_clip = if element.clip {
        Some(match clip {
            Some(clip) => intersect_rect(clip, element.frame),
            None => element.frame,
        })
    } else {
        clip
    };
    for child in &element.children {
        collect_hits(child, point, child_clip, depth + 1, order, hits);
    }
}

fn find_draw_for_element(runtime: &Runtime, id: &str) -> Option<(usize, String, LayoutRect)> {
    runtime
        .draw_list()
        .commands()
        .iter()
        .enumerate()
        .find_map(|(index, command)| {
            let draw_id = draw_command_id(command)?;
            let matches = draw_id == id
                || draw_id.ends_with(id)
                || draw_id.contains(&format!("{id}."))
                || id.contains(draw_id);
            matches.then(|| (index, draw_id.to_string(), draw_command_frame(command)))
        })
}

fn first_clip_excluding_element(
    roots: &[crate::Element],
    target_id: &str,
    target_frame: LayoutRect,
) -> Option<(String, LayoutRect)> {
    roots
        .iter()
        .find_map(|root| first_clip_in_element(root, target_id, target_frame, None))
        .flatten()
}

fn first_clip_in_element(
    element: &crate::Element,
    target_id: &str,
    target_frame: LayoutRect,
    active_clip: Option<(&str, LayoutRect)>,
) -> Option<Option<(String, LayoutRect)>> {
    if element.id == target_id {
        let center = [
            target_frame.x + target_frame.width * 0.5,
            target_frame.y + target_frame.height * 0.5,
        ];
        return Some(
            active_clip
                .filter(|(_, clip)| !clip.contains(center))
                .map(|(id, rect)| (id.to_string(), rect)),
        );
    }

    let next_clip = if element.clip {
        let clip = match active_clip {
            Some((_, rect)) => intersect_rect(rect, element.frame),
            None => element.frame,
        };
        Some((element.id.as_str(), clip))
    } else {
        active_clip
    };
    element
        .children
        .iter()
        .find_map(|child| first_clip_in_element(child, target_id, target_frame, next_clip))
}

fn intersect_rect(a: LayoutRect, b: LayoutRect) -> LayoutRect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    LayoutRect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

fn draw_command_id(command: &UiDrawCommand) -> Option<&str> {
    match command {
        UiDrawCommand::Rect(draw) => Some(&draw.id),
        UiDrawCommand::Text(draw) => Some(&draw.id),
        UiDrawCommand::Image(draw) => Some(&draw.id),
        UiDrawCommand::NineSlice(draw) => Some(&draw.id),
        UiDrawCommand::Polygon(draw) => Some(&draw.id),
        UiDrawCommand::PushClip(_) | UiDrawCommand::PopClip => None,
    }
}

fn draw_command_frame(command: &UiDrawCommand) -> LayoutRect {
    match command {
        UiDrawCommand::Rect(draw) => draw.frame,
        UiDrawCommand::Text(draw) => draw.frame,
        UiDrawCommand::Image(draw) => draw.frame,
        UiDrawCommand::NineSlice(draw) => draw.frame,
        UiDrawCommand::Polygon(draw) => draw.frame,
        UiDrawCommand::PushClip(clip) => clip.rect,
        UiDrawCommand::PopClip => LayoutRect::ZERO,
    }
}

fn draw_command_text(command: &UiDrawCommand) -> String {
    match command {
        UiDrawCommand::Rect(draw) => format!(
            "rect id={} frame={} opacity={:.2} radius={:.1}",
            draw.id,
            rect_text(draw.frame),
            draw.opacity,
            draw.radius
        ),
        UiDrawCommand::Text(draw) => format!(
            "text id={} frame={} opacity={:.2} text={:?}",
            draw.id,
            rect_text(draw.frame),
            draw.opacity,
            draw.text
        ),
        UiDrawCommand::Image(draw) => format!(
            "image id={} frame={} opacity={:.2}",
            draw.id,
            rect_text(draw.frame),
            draw.opacity
        ),
        UiDrawCommand::NineSlice(draw) => format!(
            "nine id={} frame={} opacity={:.2}",
            draw.id,
            rect_text(draw.frame),
            draw.opacity
        ),
        UiDrawCommand::Polygon(draw) => format!(
            "poly id={} frame={} opacity={:.2} points={}",
            draw.id,
            rect_text(draw.frame),
            draw.opacity,
            draw.points.len()
        ),
        UiDrawCommand::PushClip(clip) => {
            format!(
                "push_clip frame={} radius={:.1}",
                rect_text(clip.rect),
                clip.radius
            )
        }
        UiDrawCommand::PopClip => "pop_clip".to_string(),
    }
}

fn filter_param(params: &Value) -> Option<&str> {
    string_param(params, "filter")
}

fn id_param(params: &Value) -> Option<&str> {
    string_param(params, "id")
}

fn string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn number_param(params: &Value, key: &str) -> Option<f64> {
    params.get(key).and_then(Value::as_f64)
}

fn max_rows_param(params: &Value) -> usize {
    params
        .get("maxRows")
        .and_then(Value::as_u64)
        .unwrap_or(120)
        .clamp(1, 1000) as usize
}

fn rect_text(rect: LayoutRect) -> String {
    format!(
        "{:.1},{:.1} {:.1}x{:.1}",
        rect.x, rect.y, rect.width, rect.height
    )
}

fn rect_center(rect: LayoutRect) -> [f32; 2] {
    [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
}

fn padded_rect(rect: LayoutRect, padding: f32, max_width: f32, max_height: f32) -> LayoutRect {
    let x = (rect.x - padding).max(0.0);
    let y = (rect.y - padding).max(0.0);
    let right = (rect.right() + padding).min(max_width);
    let bottom = (rect.bottom() + padding).min(max_height);
    LayoutRect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

fn json_error(id: Value, error: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "ok": false,
        "error": error,
    })
    .to_string()
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<HttpRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 1024];
    loop {
        let read = stream.read(&mut temp)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > 64 * 1024 {
            break;
        }
    }
    let header_end = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "missing headers"))?;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "missing request line")
    })?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    let mut content_length = 0_usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((key, value));
        }
    }
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut temp)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&temp[..read]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn http_response(status: u16, content_type: &str, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn endpoint_path(app_id: &str) -> PathBuf {
    let base = std::env::var_os("SKY_NEO_DEBUG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("neo-debug"));
    base.join(app_id).join("endpoint.json")
}

fn write_endpoint(endpoint: &AgentDebugEndpoint) -> std::io::Result<()> {
    if let Some(parent) = endpoint.endpoint_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = json!({
        "protocolVersion": endpoint.protocol_version,
        "appId": endpoint.app_id,
        "url": endpoint.url,
        "token": endpoint.token,
        "pid": endpoint.pid,
    })
    .to_string();
    atomic_write(&endpoint.endpoint_path, &body)
}

fn atomic_write(path: &Path, body: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body)?;
    fs::rename(tmp, path)
}

fn make_token(app_id: &str, addr: SocketAddr) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{app_id}-{}-{}-{nanos:x}", std::process::id(), addr.port())
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_path_uses_app_id() {
        let path = endpoint_path("stress_lab");
        assert!(path.ends_with(PathBuf::from("stress_lab").join("endpoint.json")));
    }

    #[test]
    fn json_error_contains_message() {
        let body = json_error(json!(7), "bad");
        assert!(body.contains("\"id\":7"));
        assert!(body.contains("bad"));
    }
}
