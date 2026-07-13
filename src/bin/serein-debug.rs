use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

fn main() {
    if let Err(error) = run() {
        eprintln!("[serein-debug] {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    let mut app = "stress_lab".to_string();
    let mut debug_dir = env::var_os("SKY_SEREIN_DEBUG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("serein-debug"));
    let mut format = OutputFormat::Pretty;
    let mut command_index = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--app" => {
                index += 1;
                app = args
                    .get(index)
                    .ok_or_else(|| "--app requires a value".to_string())?
                    .clone();
            }
            "--debug-dir" => {
                index += 1;
                debug_dir = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--debug-dir requires a value".to_string())?,
                );
            }
            "--format" => {
                index += 1;
                format = match args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?
                    .as_str()
                {
                    "pretty" => OutputFormat::Pretty,
                    "json" => OutputFormat::Json,
                    other => return Err(format!("unknown format `{other}`")),
                };
            }
            value if value.starts_with("--") => return Err(format!("unknown flag `{value}`")),
            _ => {
                command_index = Some(index);
                break;
            }
        }
        index += 1;
    }

    let command_index = command_index.ok_or_else(|| "missing command".to_string())?;
    let command_args = &args[command_index..];
    let rpc = build_rpc(command_args)?;
    let endpoint = read_endpoint(&debug_dir, &app)?;
    let response = post_rpc(&endpoint, &rpc)?;
    wait_for_requested_screenshot(command_args, &response)?;
    crop_screenshot_element_if_supported(command_args, &response)?;

    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&response).unwrap()),
        OutputFormat::Pretty => {
            if response.get("ok").and_then(Value::as_bool) == Some(false) {
                return Err(response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_string());
            }
            if let Some(text) = response
                .get("result")
                .and_then(|result| result.get("text"))
                .and_then(Value::as_str)
            {
                print!("{text}");
            } else {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Pretty,
    Json,
}

#[derive(Debug)]
struct Endpoint {
    url: String,
    token: String,
}

fn build_rpc(args: &[String]) -> Result<Value, String> {
    let command = args
        .first()
        .ok_or_else(|| "missing command".to_string())?
        .as_str();
    let mut params = json!({});
    let method = match command {
        "snapshot" | "layers" | "elements" | "draw" | "state" => command,
        "diagnose-dropdown" | "diagnose-element" | "diagnose-overflow" | "diagnose-input"
        | "click" | "hover" => {
            let id = args
                .get(1)
                .ok_or_else(|| format!("{command} requires an element id"))?;
            params["id"] = json!(id);
            command
        }
        "screenshot" => {
            let path = flag_value(args, "--path")
                .ok_or_else(|| "screenshot requires --path PATH".to_string())?;
            params["path"] = json!(path);
            command
        }
        "screenshot-element" => {
            let id = args
                .get(1)
                .ok_or_else(|| "screenshot-element requires an element id".to_string())?;
            let path = flag_value(args, "--path")
                .ok_or_else(|| "screenshot-element requires --path PATH".to_string())?;
            params["id"] = json!(id);
            params["path"] = json!(path);
            if let Some(padding) = flag_value(args, "--padding") {
                if let Ok(padding) = padding.parse::<f64>() {
                    params["padding"] = json!(padding);
                }
            }
            command
        }
        "hit-test" => {
            let x = args
                .get(1)
                .ok_or_else(|| "hit-test requires x".to_string())?
                .parse::<f64>()
                .map_err(|error| format!("invalid x: {error}"))?;
            let y = args
                .get(2)
                .ok_or_else(|| "hit-test requires y".to_string())?
                .parse::<f64>()
                .map_err(|error| format!("invalid y: {error}"))?;
            params["x"] = json!(x);
            params["y"] = json!(y);
            command
        }
        "scroll" => {
            let id = args
                .get(1)
                .ok_or_else(|| "scroll requires an element id".to_string())?;
            params["id"] = json!(id);
            params["x"] = json!(flag_value(args, "--x")
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0));
            params["y"] = json!(flag_value(args, "--y")
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0));
            command
        }
        "type-text" => {
            let text = args
                .get(1)
                .ok_or_else(|| "type-text requires text".to_string())?;
            params["text"] = json!(text);
            command
        }
        other => return Err(format!("unknown command `{other}`")),
    };

    if let Some(filter) = flag_value(args, "--filter") {
        params["filter"] = json!(filter);
    }
    if let Some(max_rows) = flag_value(args, "--max-rows") {
        if let Ok(max_rows) = max_rows.parse::<u64>() {
            params["maxRows"] = json!(max_rows);
        }
    }

    Ok(json!({
        "jsonrpc": "2.0",
        "id": request_id(),
        "method": method,
        "params": params,
    }))
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn wait_for_requested_screenshot(args: &[String], response: &Value) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(());
    };
    if !matches!(command, "screenshot" | "screenshot-element") {
        return Ok(());
    }
    if response.get("ok").and_then(Value::as_bool) == Some(false) {
        return Ok(());
    }
    let Some(path) = response
        .get("result")
        .and_then(|result| result.get("path"))
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    wait_for_stable_file(PathBuf::from(path))
}

fn wait_for_stable_file(path: PathBuf) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut last_len = 0;
    let mut stable_ticks = 0;
    while std::time::Instant::now() < deadline {
        if let Ok(metadata) = fs::metadata(&path) {
            let len = metadata.len();
            if len > 0 && len == last_len {
                stable_ticks += 1;
            } else {
                stable_ticks = 0;
            }
            last_len = len;
            if stable_ticks >= 3 {
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    Err(format!("screenshot was not completed: {}", path.display()))
}

#[cfg(feature = "asset")]
fn crop_screenshot_element_if_supported(args: &[String], response: &Value) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("screenshot-element") {
        return Ok(());
    }
    if response.get("ok").and_then(Value::as_bool) == Some(false) {
        return Ok(());
    }
    let result = response
        .get("result")
        .ok_or_else(|| "screenshot-element response missing result".to_string())?;
    let path = result
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "screenshot-element response missing path".to_string())?;
    let crop = result
        .get("crop")
        .ok_or_else(|| "screenshot-element response missing crop".to_string())?;
    let x = crop.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = crop.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    let width = crop.get("width").and_then(Value::as_f64).unwrap_or(0.0);
    let height = crop.get("height").and_then(Value::as_f64).unwrap_or(0.0);
    let screen = result.get("screen");
    let logical_width = screen
        .and_then(|screen| screen.get("width"))
        .and_then(Value::as_f64);
    let logical_height = screen
        .and_then(|screen| screen.get("height"))
        .and_then(Value::as_f64);
    crop_image_in_place(
        PathBuf::from(path),
        x,
        y,
        width,
        height,
        logical_width,
        logical_height,
    )
}

#[cfg(not(feature = "asset"))]
fn crop_screenshot_element_if_supported(_args: &[String], _response: &Value) -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "asset")]
fn crop_image_in_place(
    path: PathBuf,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    logical_width: Option<f64>,
    logical_height: Option<f64>,
) -> Result<(), String> {
    let image = image::open(&path)
        .map_err(|error| format!("failed to open screenshot {}: {error}", path.display()))?;
    let scale_x = logical_width
        .filter(|value| *value > 0.0)
        .map(|value| image.width() as f64 / value)
        .unwrap_or(1.0);
    let scale_y = logical_height
        .filter(|value| *value > 0.0)
        .map(|value| image.height() as f64 / value)
        .unwrap_or(1.0);
    let x = (x * scale_x).floor().max(0.0) as u32;
    let y = (y * scale_y).floor().max(0.0) as u32;
    let max_width = image.width().saturating_sub(x);
    let max_height = image.height().saturating_sub(y);
    let width = ((width * scale_x).ceil().max(1.0) as u32).min(max_width);
    let height = ((height * scale_y).ceil().max(1.0) as u32).min(max_height);
    let cropped = image.crop_imm(x, y, width, height);
    let tmp = path.with_extension("crop.tmp.png");
    cropped
        .save(&tmp)
        .map_err(|error| format!("failed to save crop {}: {error}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .map_err(|error| format!("failed to replace crop {}: {error}", path.display()))
}

fn read_endpoint(debug_dir: &Path, app: &str) -> Result<Endpoint, String> {
    let path = debug_dir.join(app).join("endpoint.json");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid endpoint {}: {error}", path.display()))?;
    Ok(Endpoint {
        url: json
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "endpoint missing url".to_string())?
            .to_string(),
        token: json
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| "endpoint missing token".to_string())?
            .to_string(),
    })
}

fn post_rpc(endpoint: &Endpoint, body: &Value) -> Result<Value, String> {
    let body = body.to_string();
    let addr = endpoint
        .url
        .strip_prefix("http://")
        .ok_or_else(|| format!("unsupported endpoint url {}", endpoint.url))?;
    let mut stream =
        TcpStream::connect(addr).map_err(|error| format!("failed to connect {addr}: {error}"))?;
    let request = format!(
        "POST /rpc HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nX-Serein-Debug-Token: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint.token,
        body.len(),
        body
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("failed to write request: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("failed to read response: {error}"))?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    serde_json::from_str(body).map_err(|error| format!("invalid JSON response: {error}\n{body}"))
}

fn request_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn print_help() {
    eprintln!(
        "serein-debug [--app stress_lab] [--debug-dir target/serein-debug] [--format pretty|json] <command>\n\
         commands:\n\
           snapshot [--filter TEXT] [--max-rows N]\n\
           layers\n\
           elements [--filter TEXT] [--max-rows N]\n\
           draw [--filter TEXT] [--max-rows N]\n\
           state\n\
           diagnose-dropdown ID\n\
           diagnose-element ID\n\
           diagnose-overflow ID [--max-rows N]\n\
           diagnose-input ID\n\
           hit-test X Y [--max-rows N]\n\
           screenshot --path PATH\n\
           screenshot-element ID --path PATH [--padding N]  (crop requires feature asset)\n\
           click ID\n\
           hover ID\n\
           scroll ID --x DX --y DY\n\
           type-text TEXT"
    );
}
