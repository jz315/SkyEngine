use std::path::{Path, PathBuf};

use super::benchmark::BenchmarkConfig;

const DEFAULT_MODEL_PATH: &str =
    "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.model3.json";

pub struct DemoOptions {
    pub model_paths: Vec<PathBuf>,
    pub ui_visible: bool,
    pub benchmark: Option<BenchmarkConfig>,
}

pub fn parse_args() -> DemoOptions {
    let mut inputs = Vec::new();
    let mut ui_visible = true;
    let mut benchmark = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--no-ui" => ui_visible = false,
            "--ui" => ui_visible = true,
            "--bench-frames" => {
                let sample_frames = args
                    .get(i + 1)
                    .expect("--bench-frames requires a positive integer")
                    .parse::<u32>()
                    .expect("--bench-frames requires a positive integer");
                let warmup_frames = benchmark.map(|config: BenchmarkConfig| config.warmup_frames);
                benchmark = Some(BenchmarkConfig {
                    warmup_frames: warmup_frames.unwrap_or(120),
                    sample_frames,
                });
                i += 2;
                continue;
            }
            "--bench-warmup" => {
                let warmup_frames = args
                    .get(i + 1)
                    .expect("--bench-warmup requires a non-negative integer")
                    .parse::<u32>()
                    .expect("--bench-warmup requires a non-negative integer");
                let sample_frames = benchmark.map(|config: BenchmarkConfig| config.sample_frames);
                benchmark = Some(BenchmarkConfig {
                    warmup_frames,
                    sample_frames: sample_frames.unwrap_or(1000),
                });
                i += 2;
                continue;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ if args[i].starts_with('-') => {
                eprintln!("Unknown option: {}", args[i]);
                print_usage();
                std::process::exit(1);
            }
            _ => inputs.push(args[i].clone()),
        }
        i += 1;
    }

    let model_paths = if inputs.is_empty() {
        let default_path = PathBuf::from(DEFAULT_MODEL_PATH);
        eprintln!(
            "[Live2D] No model path provided; using default: {}",
            default_path.display()
        );
        vec![default_path]
    } else {
        let model_paths = expand_model_inputs(inputs);
        if model_paths.is_empty() {
            print_usage();
            std::process::exit(1);
        }
        model_paths
    };

    DemoOptions {
        model_paths,
        ui_visible,
        benchmark,
    }
}

fn print_usage() {
    eprintln!("Usage: live2d_demo [--no-ui] [model-or-folder] [more models/folders ...]");
    eprintln!("  --no-ui   start in pure render mode for FPS A/B");
    eprintln!("  --ui      force the control panel on at startup");
    eprintln!("  --bench-frames <N>   run N measured frames, print average FPS, then exit");
    eprintln!("  --bench-warmup <N>   warm up N frames before benchmark sampling");
    eprintln!("  U         toggle the control panel while running");
    eprintln!("  You can pass one or more .model3.json files and/or folders.");
    eprintln!("  If no path is provided, the demo uses the hardcoded default model:");
    eprintln!("    {DEFAULT_MODEL_PATH}");
    eprintln!("  Folders are scanned recursively for every *.model3.json.");
    eprintln!("  All matching models are loaded; only the selected model is rendered.");
    eprintln!(
        "  e.g. live2d_demo --no-ui CubismSdkForNative\\CubismSdkForNative-5-r.5\\Samples\\Resources"
    );
}

fn expand_model_inputs(inputs: Vec<String>) -> Vec<PathBuf> {
    let mut model_paths = Vec::new();

    for input in inputs {
        let path = PathBuf::from(&input);
        if path.is_dir() {
            if let Err(err) = collect_models_under_dir(&path, &mut model_paths) {
                eprintln!(
                    "[Live2D] Failed to scan directory {}: {err}",
                    path.display()
                );
            }
        } else if path.is_file() {
            if is_model_json(&path) {
                model_paths.push(path);
            } else {
                eprintln!("[Live2D] Ignoring non-model file: {}", path.display());
            }
        } else {
            eprintln!("[Live2D] Path not found: {}", path.display());
        }
    }

    model_paths.sort();
    model_paths.dedup();
    model_paths
}

fn collect_models_under_dir(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_models_under_dir(&path, out)?;
        } else if path.is_file() && is_model_json(&path) {
            out.push(path);
        }
    }

    Ok(())
}

fn is_model_json(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".model3.json"))
}
