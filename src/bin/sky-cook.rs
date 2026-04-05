use std::path::PathBuf;

use sky_engine::asset::{cook, AssetConfig, AssetError};

fn main() {
    if let Err(error) = run() {
        eprintln!("[sky-cook] {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AssetError> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let config = parse_config(&mut args)?;

    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Ok(());
    };

    match command {
        "import" => {
            let path = args.get(1).ok_or_else(|| AssetError::InvalidConfig {
                message: "missing path for `import`".to_string(),
            })?;
            let meta = cook::import_path(&config.asset_root, path)?;
            println!(
                "imported {} as {} ({})",
                meta.source_path, meta.asset_type, meta.asset_id
            );
        }
        "cook" => match args.get(1).map(String::as_str) {
            Some("--all") | None => {
                let manifest = cook::cook_all(&config)?;
                println!(
                    "cooked {} assets into {:?}",
                    manifest.assets.len(),
                    config.cooked_root()
                );
            }
            Some(query) => {
                let manifest = cook::cook_target(&config, query)?;
                println!(
                    "updated cooked manifest with {} assets in {:?}",
                    manifest.assets.len(),
                    config.cooked_root()
                );
            }
        },
        "verify" => match cook::verify(&config) {
            Ok(report) => {
                println!(
                    "verify clean: {} assets under {:?}",
                    report.issues.len(),
                    config.asset_root
                );
            }
            Err(AssetError::VerificationFailed { issues }) => {
                for issue in issues {
                    eprintln!("[verify] {issue}");
                }
                return Err(AssetError::VerificationFailed { issues: Vec::new() });
            }
            Err(error) => return Err(error),
        },
        _ => {
            print_usage();
            return Err(AssetError::InvalidConfig {
                message: format!("unknown command `{command}`"),
            });
        }
    }

    Ok(())
}

fn parse_config(args: &mut Vec<String>) -> Result<AssetConfig, AssetError> {
    let mut asset_root = PathBuf::from("assets");
    let mut target = AssetConfig::default_target();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--assets-root" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| AssetError::InvalidConfig {
                        message: "missing value for `--assets-root`".to_string(),
                    })?;
                asset_root = PathBuf::from(value);
                args.drain(index..=index + 1);
            }
            "--target" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| AssetError::InvalidConfig {
                        message: "missing value for `--target`".to_string(),
                    })?;
                target = value.clone();
                args.drain(index..=index + 1);
            }
            _ => index += 1,
        }
    }

    Ok(AssetConfig::new(asset_root, target))
}

fn print_usage() {
    eprintln!("sky-cook [--assets-root <dir>] [--target <name>] <command>");
    eprintln!("commands:");
    eprintln!("  import <path>");
    eprintln!("  cook --all");
    eprintln!("  cook <path|guid>");
    eprintln!("  verify");
}
