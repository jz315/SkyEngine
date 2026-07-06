use super::{
    build_manifest_entry, build_manifest_provenance, collect_meta_files, collect_source_files,
    cooked_relative_path, dependency_cycles, is_asset_dirty, meta_cooker_drift, meta_path_for,
    read_manifest, read_meta, CookRegistry, VerifyReport,
};
use crate::asset::types::{AssetConfig, AssetError};

pub fn verify(config: &AssetConfig) -> Result<VerifyReport, AssetError> {
    let registry = CookRegistry::default();
    verify_with_registry(config, &registry)
}

pub fn verify_with_registry(
    config: &AssetConfig,
    registry: &CookRegistry,
) -> Result<VerifyReport, AssetError> {
    let mut report = VerifyReport::default();
    let source_files = collect_source_files(&config.asset_root, registry)?;
    let meta_files = collect_meta_files(&config.asset_root)?;

    let manifest = if config.manifest_path().exists() {
        Some(read_manifest(config)?)
    } else {
        report
            .issues
            .push(format!("manifest missing at {:?}", config.manifest_path()));
        None
    };

    for source in &source_files {
        let meta_path = meta_path_for(source);
        if !meta_path.exists() {
            report
                .issues
                .push(format!("missing meta for source {:?}", source));
        }
    }

    let mut known_ids = std::collections::HashSet::new();
    let mut seen_meta_ids = std::collections::HashMap::new();
    let mut metas = Vec::new();
    for meta_path in meta_files {
        match read_meta(&meta_path) {
            Ok(meta) => {
                known_ids.insert(meta.asset_id);
                if let Some(previous) = seen_meta_ids.insert(meta.asset_id, meta_path.clone()) {
                    report.issues.push(format!(
                        "duplicate asset id {} in {:?} and {:?}",
                        meta.asset_id, previous, meta_path
                    ));
                }
                metas.push((meta_path, meta));
            }
            Err(error) => report.issues.push(error.to_string()),
        }
    }

    for (meta_path, meta) in &metas {
        if let Some(issue) = meta_cooker_drift(registry, meta) {
            report.issues.push(format!("{:?}: {issue}", meta_path));
        }

        let source = config.asset_root.join(&meta.source_path);
        let source_exists = source.exists();
        if !source_exists {
            report.issues.push(format!(
                "meta {:?} points to missing source {:?}",
                meta_path, source
            ));
        }
        for dependency in &meta.dependencies {
            if !known_ids.contains(dependency) {
                report.issues.push(format!(
                    "meta {:?} has missing dependency {}",
                    meta_path, dependency
                ));
            }
        }

        let cooked_path = config
            .cooked_root()
            .join(cooked_relative_path(meta, registry));
        if !cooked_path.exists() {
            report.issues.push(format!(
                "cooked artifact missing for {} at {:?}",
                meta.asset_id, cooked_path
            ));
        } else if source_exists && is_asset_dirty(&source, meta_path, &cooked_path, meta)? {
            report.issues.push(format!(
                "cooked artifact out of date for {} at {:?}",
                meta.asset_id, cooked_path
            ));
        }

        if let Some(manifest) = &manifest {
            let Some(entry) = manifest
                .assets
                .iter()
                .find(|entry| entry.asset_id == meta.asset_id)
            else {
                report.issues.push(format!(
                    "manifest missing entry for asset {}",
                    meta.asset_id
                ));
                continue;
            };

            let expected = build_manifest_entry(meta, registry);
            if entry.asset_type != expected.asset_type
                || entry.importer != expected.importer
                || entry.cooker != expected.cooker
                || entry.version != expected.version
                || entry.source_path != expected.source_path
                || entry.cooked_path != expected.cooked_path
                || entry.dependencies != expected.dependencies
                || entry.import_settings != expected.import_settings
            {
                report.issues.push(format!(
                    "manifest entry mismatch for asset {}",
                    meta.asset_id
                ));
            }

            let expected_provenance = build_manifest_provenance(config, meta, registry)?;
            let Some(provenance) = manifest
                .provenance
                .iter()
                .find(|provenance| provenance.asset_id == meta.asset_id)
            else {
                report.issues.push(format!(
                    "manifest missing provenance for asset {}",
                    meta.asset_id
                ));
                continue;
            };
            if provenance != &expected_provenance {
                report.issues.push(format!(
                    "manifest provenance mismatch for asset {}",
                    meta.asset_id
                ));
            }
        }
    }

    for cycle in dependency_cycles(&metas) {
        report.issues.push(format!(
            "dependency cycle detected: {}",
            cycle
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" -> ")
        ));
    }

    if report.is_clean() {
        Ok(report)
    } else {
        Err(AssetError::VerificationFailed {
            issues: report.issues,
        })
    }
}
