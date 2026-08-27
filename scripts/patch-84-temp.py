#!/usr/bin/env python3
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


main_path = Path("crates/scenedetect-cli/src/main.rs")
main = main_path.read_text()
main = replace_once(
    main,
    "mod artifacts;\nmod native_stats;\nmod scene_list_command;",
    "mod artifacts;\nmod inspect_command;\nmod native_stats;\nmod scene_list_command;",
    "module declaration",
)
main = replace_once(
    main,
    "enum Command {\n    Detect(NativeDetectArgs),\n    Render(NativeRenderArgs),",
    "enum Command {\n    Detect(NativeDetectArgs),\n    Render(NativeRenderArgs),\n    Inspect(inspect_command::InspectArgs),",
    "inspect command enum",
)
main = replace_once(
    main,
    "        Command::Detect(args) => return handle_native_detect(&cli, args),\n        Command::Render(args) => return handle_native_render(args),",
    "        Command::Detect(args) => return handle_native_detect(&cli, args),\n        Command::Render(args) => return handle_native_render(args),\n        Command::Inspect(args) => return inspect_command::run(args),",
    "inspect command dispatch",
)
main = replace_once(
    main,
    "        Command::Detect(_) | Command::Render(_) => None,",
    "        Command::Detect(_) | Command::Render(_) | Command::Inspect(_) => None,",
    "legacy command exclusion",
)
main_path.write_text(main)

stats_path = Path("crates/scenedetect-cli/src/native_stats.rs")
stats = stats_path.read_text()
old_reader = '''pub fn read_detection_stats_document_for_input(input: &Path) -> Result<DetectionStatsDocument> {
    let path = detection_stats_path_for_input(input)?;
    let file = File::open(&path).with_context(|| {
        format!(
            "Detection Stats are missing for {}; run `scenedetect-rs detect content -i {}` first",
            input.display(),
            input.display()
        )
    })?;
    let document: DetectionStatsDocument = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse Detection Stats {}", path.display()))?;
    document.validate()?;
    Ok(document)
}
'''
new_reader = '''pub fn read_detection_stats_document_for_input(input: &Path) -> Result<DetectionStatsDocument> {
    Ok(read_detection_stats_document(input)?.1)
}

pub fn read_detection_stats_document(
    input_or_stats: &Path,
) -> Result<(PathBuf, DetectionStatsDocument)> {
    let path = detection_stats_path_for_input_or_stats(input_or_stats)?;
    let default_recovery = default_recovery_command(input_or_stats);
    let file = File::open(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow!(
                "Detection Stats artifact {} is missing. Recovery: `{}`",
                path.display(),
                default_recovery
            )
        } else {
            anyhow!(
                "failed to read Detection Stats artifact {}: {}. Recovery: `{}`",
                path.display(),
                error,
                default_recovery
            )
        }
    })?;
    let document: DetectionStatsDocument = serde_json::from_reader(file).map_err(|error| {
        anyhow!(
            "Detection Stats artifact {} is malformed: {}. Recovery: `{}`",
            path.display(),
            error,
            default_recovery
        )
    })?;
    document.validate().map_err(|error| {
        anyhow!(
            "Detection Stats artifact {} is invalid: {}. Recovery: `{}`",
            path.display(),
            error,
            recovery_command_for_document(&document)
        )
    })?;

    let source_path = if is_detection_stats_path(input_or_stats) {
        PathBuf::from(&document.input.path)
    } else {
        input_or_stats.to_path_buf()
    };
    validate_input_fingerprint(&source_path, &path, &document)?;
    Ok((path, document))
}

pub fn detection_stats_path_for_input_or_stats(input_or_stats: &Path) -> Result<PathBuf> {
    if is_detection_stats_path(input_or_stats) {
        Ok(input_or_stats.to_path_buf())
    } else {
        detection_stats_path_for_input(input_or_stats)
    }
}

fn validate_input_fingerprint(
    input: &Path,
    stats_path: &Path,
    document: &DetectionStatsDocument,
) -> Result<()> {
    let metadata = match fs::metadata(input) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read input metadata {}", input.display()))
        }
    };
    let canonical = input
        .canonicalize()
        .with_context(|| format!("failed to canonicalize input {}", input.display()))?;
    let modified = modified_unix_nanos(&metadata)?;
    let changed = canonical.display().to_string() != document.input.path
        || metadata.len() != document.input.byte_len
        || modified != document.input.modified_unix_nanos;
    if changed {
        return Err(anyhow!(
            "Detection Stats artifact {} is stale for changed input {}. Recovery: `{}`",
            stats_path.display(),
            input.display(),
            recovery_command_for_document(document)
        ));
    }
    Ok(())
}

fn is_detection_stats_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".scenedetect.json"))
}

fn default_recovery_command(input_or_stats: &Path) -> String {
    if is_detection_stats_path(input_or_stats) {
        "scenedetect-rs detect content -i <video>".to_owned()
    } else {
        format!(
            "scenedetect-rs detect content -i {}",
            shell_quote(input_or_stats)
        )
    }
}

fn recovery_command_for_document(document: &DetectionStatsDocument) -> String {
    let mut command = format!(
        "scenedetect-rs detect {} -i {} --min-scene-len {}",
        document.detector.name(),
        shell_quote(Path::new(&document.input.path)),
        document.options.min_scene_len
    );
    match &document.detector {
        DetectionStatsDetector::Content(config) => {
            command.push_str(&format!(" --threshold {}", config.threshold));
            command.push_str(&format!(
                " --weights {} {} {} {}",
                config.weights.hue,
                config.weights.saturation,
                config.weights.luminance,
                config.weights.edges
            ));
            if config.luma_only {
                command.push_str(" --luma-only");
            }
        }
        DetectionStatsDetector::Adaptive(config) => {
            command.push_str(&format!(
                " --threshold {} --min-content-val {} --frame-window {}",
                config.threshold, config.min_content_val, config.frame_window
            ));
            command.push_str(&format!(
                " --weights {} {} {} {}",
                config.weights.hue,
                config.weights.saturation,
                config.weights.luminance,
                config.weights.edges
            ));
            if config.luma_only {
                command.push_str(" --luma-only");
            }
        }
        DetectionStatsDetector::Threshold(config) => {
            command.push_str(&format!(
                " --threshold {} --fade-bias {}",
                config.threshold, config.fade_bias
            ));
        }
        DetectionStatsDetector::Histogram(config) => {
            command.push_str(&format!(
                " --threshold {} --bins {}",
                config.threshold, config.bins
            ));
        }
        DetectionStatsDetector::Hash(config) => {
            command.push_str(&format!(
                " --threshold {} --size {} --lowpass {}",
                config.threshold, config.size, config.lowpass
            ));
        }
    }
    command.push_str(" --force");
    command
}

fn shell_quote(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\\'', "'\\''"))
}
'''
stats = replace_once(stats, old_reader, new_reader, "Detection Stats reader")
stats_path.write_text(stats)
