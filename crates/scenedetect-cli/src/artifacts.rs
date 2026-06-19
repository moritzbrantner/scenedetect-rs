use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use scenedetect_core::{DetectionOptions, DetectorConfig, FrameRate, SceneList};
use serde::{Deserialize, Serialize};

const ARTIFACT_SCHEMA_VERSION: u32 = 1;
const ARTIFACT_DIR: &str = ".scenedetect-rs";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneListRequest {
    artifact_schema_version: u32,
    input: InputFingerprint,
    effective_frame_rate: f64,
    frame_rate_override: Option<f64>,
    detector: DetectorConfig,
    options: DetectionOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct InputFingerprint {
    path: String,
    byte_len: u64,
    modified_unix_nanos: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct SceneListArtifact {
    schema_version: u32,
    kind: String,
    request: SceneListRequest,
    scene_list: SceneList,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FileFingerprint {
    byte_len: u64,
    modified_unix_nanos: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct RenderManifest {
    schema_version: u32,
    kind: String,
    render_kind: String,
    output_path: String,
    output_fingerprint: FileFingerprint,
    scene_list_request_key: String,
    request: SceneListRequest,
}

pub fn scene_list_request(
    input: &Path,
    effective_frame_rate: FrameRate,
    frame_rate_override: Option<FrameRate>,
    detector: &DetectorConfig,
    options: &DetectionOptions,
) -> Result<SceneListRequest> {
    let metadata = fs::metadata(input)
        .with_context(|| format!("failed to read input video metadata {}", input.display()))?;
    let modified_unix_nanos = modified_unix_nanos(&metadata)?;
    let path = input
        .canonicalize()
        .with_context(|| format!("failed to canonicalize input video {}", input.display()))?;

    Ok(SceneListRequest {
        artifact_schema_version: ARTIFACT_SCHEMA_VERSION,
        input: InputFingerprint {
            path: path.display().to_string(),
            byte_len: metadata.len(),
            modified_unix_nanos,
        },
        effective_frame_rate: effective_frame_rate.0,
        frame_rate_override: frame_rate_override.map(|frame_rate| frame_rate.0),
        detector: detector.clone(),
        options: options.clone(),
    })
}

pub fn request_key(request: &SceneListRequest) -> Result<String> {
    let bytes = serde_json::to_vec(request)?;
    Ok(stable_key(&bytes))
}

pub fn default_scene_list_artifact_path(output_dir: &Path, request_key: &str) -> PathBuf {
    output_dir
        .join(ARTIFACT_DIR)
        .join("scene-list")
        .join(format!("{request_key}.json"))
}

pub fn read_scene_list_artifact(
    path: &Path,
    request: &SceneListRequest,
) -> Result<Option<SceneList>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read Scene List Artifact {}", path.display()))
        }
    };
    let Ok(artifact) = serde_json::from_reader::<_, SceneListArtifact>(file) else {
        return Ok(None);
    };
    if artifact.schema_version == ARTIFACT_SCHEMA_VERSION
        && artifact.kind == "scene_list_artifact"
        && artifact.request == *request
    {
        Ok(Some(artifact.scene_list))
    } else {
        Ok(None)
    }
}

pub fn write_scene_list_artifact(
    path: &Path,
    request: &SceneListRequest,
    scene_list: &SceneList,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create artifact directory {}", parent.display()))?;
    }
    let file = File::create(path)
        .with_context(|| format!("failed to create Scene List Artifact {}", path.display()))?;
    let artifact = SceneListArtifact {
        schema_version: ARTIFACT_SCHEMA_VERSION,
        kind: "scene_list_artifact".to_owned(),
        request: request.clone(),
        scene_list: scene_list.clone(),
    };
    serde_json::to_writer_pretty(file, &artifact)?;
    Ok(())
}

pub fn render_manifest_path(
    output_dir: &Path,
    output_path: &Path,
    render_kind: &str,
) -> Result<PathBuf> {
    let key = stable_key(format!("{}:{render_kind}", normalized_path(output_path)?).as_bytes());
    Ok(output_dir
        .join(ARTIFACT_DIR)
        .join("renders")
        .join(format!("{key}.json")))
}

pub fn reusable_output_exists(
    manifest_path: &Path,
    output_path: &Path,
    render_kind: &str,
    request_key: &str,
    request: &SceneListRequest,
) -> Result<bool> {
    let output_fingerprint = match file_fingerprint(output_path) {
        Ok(fingerprint) => fingerprint,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read output file {}", output_path.display()))
        }
    };

    let file = match File::open(manifest_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read render manifest {}", manifest_path.display())
            })
        }
    };
    let Ok(manifest) = serde_json::from_reader::<_, RenderManifest>(file) else {
        return Ok(false);
    };

    Ok(manifest.schema_version == ARTIFACT_SCHEMA_VERSION
        && manifest.kind == "render_manifest"
        && manifest.render_kind == render_kind
        && manifest.output_path == normalized_path(output_path)?
        && manifest.output_fingerprint == output_fingerprint
        && manifest.scene_list_request_key == request_key
        && manifest.request == *request)
}

pub fn write_render_manifest(
    manifest_path: &Path,
    output_path: &Path,
    render_kind: &str,
    request_key: &str,
    request: &SceneListRequest,
) -> Result<()> {
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create render manifest directory {}",
                parent.display()
            )
        })?;
    }
    let manifest = RenderManifest {
        schema_version: ARTIFACT_SCHEMA_VERSION,
        kind: "render_manifest".to_owned(),
        render_kind: render_kind.to_owned(),
        output_path: normalized_path(output_path)?,
        output_fingerprint: file_fingerprint(output_path)
            .with_context(|| format!("failed to fingerprint output {}", output_path.display()))?,
        scene_list_request_key: request_key.to_owned(),
        request: request.clone(),
    };
    let file = File::create(manifest_path).with_context(|| {
        format!(
            "failed to create render manifest {}",
            manifest_path.display()
        )
    })?;
    serde_json::to_writer_pretty(file, &manifest)?;
    Ok(())
}

fn normalized_path(path: &Path) -> Result<String> {
    if path.exists() {
        return Ok(path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize path {}", path.display()))?
            .display()
            .to_string());
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .with_context(|| format!("failed to canonicalize directory {}", parent.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("path has no filename: {}", path.display()))?;
    Ok(parent.join(file_name).display().to_string())
}

fn file_fingerprint(path: &Path) -> std::io::Result<FileFingerprint> {
    let metadata = fs::metadata(path)?;
    Ok(FileFingerprint {
        byte_len: metadata.len(),
        modified_unix_nanos: modified_unix_nanos(&metadata)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?,
    })
}

fn modified_unix_nanos(metadata: &fs::Metadata) -> Result<u64> {
    let nanos = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| anyhow!("modified time predates unix epoch: {error}"))?
        .as_nanos();
    u64::try_from(nanos).map_err(|_| anyhow!("modified time does not fit in u64 nanoseconds"))
}

fn stable_key(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
