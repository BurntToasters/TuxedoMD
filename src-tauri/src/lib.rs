use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

const MAX_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_WORKSPACE_FILES: usize = 20_000;

struct PendingOpenPaths(Mutex<Vec<String>>);

#[derive(Debug, Error)]
enum AppError {
    #[error("The selected file is larger than 16 MB")]
    FileTooLarge,
    #[error("The selected path is not a regular file")]
    NotAFile,
    #[error("The selected workspace path is not a folder")]
    NotADirectory,
    #[error("The workspace contains more than 20,000 Markdown files")]
    WorkspaceTooLarge,
    #[error("The file changed on disk before it could be saved")]
    Conflict,
    #[error("Invalid application state key")]
    InvalidStateKey,
    #[error("Application state path error: {0}")]
    StatePath(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Walk(#[from] walkdir::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileDocument {
    path: String,
    name: String,
    content: String,
    fingerprint: DocumentFingerprint,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DocumentFingerprint {
    modified_ms: u128,
    size: u64,
    hash: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceEntry {
    path: String,
    relative_path: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildInfo {
    edition: &'static str,
    version: &'static str,
}

#[tauri::command]
fn read_document(path: &Path) -> Result<FileDocument, AppError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::NotAFile);
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(AppError::FileTooLarge);
    }

    let content = fs::read_to_string(path)?;
    Ok(FileDocument {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled.md")
            .to_owned(),
        path: path.to_string_lossy().into_owned(),
        content,
        fingerprint: fingerprint(path, &metadata)?,
    })
}

#[tauri::command]
fn open_document(path: PathBuf) -> Result<FileDocument, AppError> {
    read_document(&path)
}

fn fingerprint(path: &Path, metadata: &fs::Metadata) -> Result<DocumentFingerprint, AppError> {
    let modified_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(DocumentFingerprint {
        modified_ms,
        size: metadata.len(),
        hash: blake3::hash(&fs::read(path)?).to_hex().to_string(),
    })
}

#[tauri::command]
fn save_document(
    path: PathBuf,
    content: String,
    expected_fingerprint: Option<DocumentFingerprint>,
    force: bool,
) -> Result<DocumentFingerprint, AppError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.md");
    let temporary = parent.join(format!(".{file_name}.tuxedo-tmp"));

    if !force {
        if let (Some(expected), Ok(metadata)) = (&expected_fingerprint, fs::metadata(&path)) {
            let actual = fingerprint(&path, &metadata)?;
            if actual != *expected {
                return Err(AppError::Conflict);
            }
        }
    }

    let result = (|| -> Result<(), std::io::Error> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        replace_file(&temporary, &path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(AppError::from)?;
    let metadata = fs::metadata(&path)?;
    fingerprint(&path, &metadata)
}

#[tauri::command]
fn probe_document(path: PathBuf) -> Result<FileDocument, AppError> {
    read_document(&path)
}

fn state_file(app: &AppHandle, key: &str) -> Result<PathBuf, AppError> {
    if !key
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(AppError::InvalidStateKey);
    }
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::StatePath(error.to_string()))?;
    Ok(directory.join("state").join(format!("{key}.json")))
}

#[tauri::command]
fn load_app_state(app: AppHandle, key: String) -> Result<Option<String>, AppError> {
    let path = state_file(&app, &key)?;
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[tauri::command]
fn save_app_state(app: AppHandle, key: String, content: String) -> Result<(), AppError> {
    let path = state_file(&app, &key)?;
    write_replacement(&path, content.as_bytes())
}

#[tauri::command]
fn delete_app_state(app: AppHandle, key: String) -> Result<(), AppError> {
    let path = state_file(&app, &key)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn write_replacement(path: &Path, content: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(content)?;
    file.sync_all()?;
    replace_file(&temporary, path)?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr::null};
    use windows_sys::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};

    if !destination.exists() {
        return fs::rename(temporary, destination);
    }
    let wide = |path: &Path| {
        OsStr::new(path.as_os_str())
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let destination = wide(destination);
    let temporary = wide(temporary);
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            temporary.as_ptr(),
            null(),
            REPLACEFILE_WRITE_THROUGH,
            0,
            0,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn scan_workspace(root: PathBuf) -> Result<Vec<WorkspaceEntry>, AppError> {
    if !root.is_dir() {
        return Err(AppError::NotADirectory);
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
    {
        let entry = entry?;
        if !entry.file_type().is_file() || !is_markdown(entry.path()) {
            continue;
        }
        if files.len() >= MAX_WORKSPACE_FILES {
            return Err(AppError::WorkspaceTooLarge);
        }
        let relative = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        files.push(WorkspaceEntry {
            path: entry.path().to_string_lossy().into_owned(),
            relative_path: relative.to_string_lossy().into_owned(),
            name: entry.file_name().to_string_lossy().into_owned(),
        });
    }
    files.sort_by(|a, b| {
        a.relative_path
            .to_lowercase()
            .cmp(&b.relative_path.to_lowercase())
    });
    Ok(files)
}

fn should_visit(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !(entry.file_type().is_dir()
        && (name.starts_with('.')
            || matches!(name.as_ref(), "node_modules" | "target" | "dist" | "build")))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "mdown" | "mkd"
            )
        })
}

#[tauri::command]
fn get_build_info() -> BuildInfo {
    BuildInfo {
        edition: option_env!("TUXEDO_EDITION").unwrap_or("community"),
        version: env!("CARGO_PKG_VERSION"),
    }
}

fn markdown_paths(arguments: impl IntoIterator<Item = String>) -> Vec<String> {
    arguments
        .into_iter()
        .filter_map(|argument| {
            let path = PathBuf::from(argument);
            path.is_file().then_some(path)
        })
        .filter(|path| is_markdown(path))
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
fn take_pending_open_paths(state: tauri::State<PendingOpenPaths>) -> Vec<String> {
    std::mem::take(&mut *state.0.lock().expect("pending open path lock poisoned"))
}

fn setup_native_menu(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    let handle = app.handle();
    let file = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &MenuItem::with_id(
                handle,
                "new-document",
                "New Document",
                true,
                Some("CmdOrCtrl+N"),
            )?,
            &MenuItem::with_id(handle, "open-document", "Open…", true, Some("CmdOrCtrl+O"))?,
            &MenuItem::with_id(handle, "save-document", "Save", true, Some("CmdOrCtrl+S"))?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::quit(handle, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        handle,
        "Edit",
        true,
        &[
            &MenuItem::with_id(handle, "find", "Find", true, Some("CmdOrCtrl+F"))?,
            &MenuItem::with_id(
                handle,
                "command-palette",
                "Command Palette",
                true,
                Some("CmdOrCtrl+Shift+P"),
            )?,
        ],
    )?;
    let view = Submenu::with_items(
        handle,
        "View",
        true,
        &[
            &MenuItem::with_id(
                handle,
                "toggle-sidebar",
                "Toggle Tools",
                true,
                Some("CmdOrCtrl+Shift+B"),
            )?,
            &MenuItem::with_id(
                handle,
                "editor-view",
                "Editor",
                true,
                Some("CmdOrCtrl+Shift+E"),
            )?,
            &MenuItem::with_id(
                handle,
                "preview-view",
                "Preview",
                true,
                Some("CmdOrCtrl+Shift+V"),
            )?,
            &MenuItem::with_id(handle, "settings", "Settings", true, Some("CmdOrCtrl+,"))?,
        ],
    )?;
    app.set_menu(Menu::with_items(handle, &[&file, &edit, &view])?)?;
    Ok(())
}

#[tauri::command]
fn get_licenses(app: tauri::AppHandle) -> Result<String, String> {
    fn load_license_file(
        app: &tauri::AppHandle,
        file_name: &str,
        attempted_paths: &mut Vec<String>,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, String> {
        attempted_paths.push(format!("asset:{file_name}"));
        if let Some(asset) = app.asset_resolver().get(file_name.to_string()) {
            let content = String::from_utf8(asset.bytes)
                .map_err(|e| format!("Failed to decode bundled {file_name}: {e}"))?;
            let parsed: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse bundled {file_name}: {e}"))?;
            let object = parsed
                .as_object()
                .ok_or_else(|| format!("Bundled {file_name} must be a JSON object"))?;
            return Ok(Some(object.clone()));
        }

        let candidate_suffixes = [
            file_name.to_string(),
            format!("public/{file_name}"),
            format!("dist/{file_name}"),
        ];

        if let Ok(resource_path) = app.path().resource_dir() {
            for suffix in &candidate_suffixes {
                let license_path = resource_path.join(suffix);
                attempted_paths.push(license_path.display().to_string());
                if let Ok(content) = fs::read_to_string(&license_path) {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&content).map_err(|e| {
                            format!("Failed to parse {}: {}", license_path.display(), e)
                        })?;
                    let object = parsed.as_object().ok_or_else(|| {
                        format!("{} must be a JSON object", license_path.display())
                    })?;
                    return Ok(Some(object.clone()));
                }
            }
        }

        for suffix in &candidate_suffixes {
            let license_path = std::path::Path::new(suffix);
            attempted_paths.push(license_path.display().to_string());
            if let Ok(content) = fs::read_to_string(license_path) {
                let parsed: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse {}: {}", license_path.display(), e))?;
                let object = parsed
                    .as_object()
                    .ok_or_else(|| format!("{} must be a JSON object", license_path.display()))?;
                return Ok(Some(object.clone()));
            }
        }

        Ok(None)
    }

    let mut attempted_paths: Vec<String> = Vec::new();
    let mut merged = serde_json::Map::new();
    let mut loaded_any = false;

    for file_name in ["licenses-npm.json", "licenses-cargo.json"] {
        if let Some(entries) = load_license_file(&app, file_name, &mut attempted_paths)? {
            loaded_any = true;
            for (key, value) in entries {
                merged.insert(key, value);
            }
        }
    }

    if !loaded_any {
        return Err(format!(
            "Failed to read licenses: no license files were found in bundled assets or known paths (tried: {})",
            attempted_paths.join(", ")
        ));
    }

    serde_json::to_string(&serde_json::Value::Object(merged))
        .map_err(|e| format!("Failed to encode merged licenses payload: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pending_paths = markdown_paths(std::env::args().skip(1));
    let mut builder = tauri::Builder::default().manage(PendingOpenPaths(Mutex::new(pending_paths)));
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, arguments, _| {
            let paths = markdown_paths(arguments);
            if !paths.is_empty() {
                let _ = app.emit("open-paths", paths);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }
    builder
        .setup(|app| Ok(setup_native_menu(app)?))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            open_document,
            save_document,
            scan_workspace,
            probe_document,
            get_build_info,
            load_app_state,
            save_app_state,
            delete_app_state,
            take_pending_open_paths,
            get_licenses
        ])
        .on_menu_event(|app, event| {
            let _ = app.emit("native-menu-command", event.id().as_ref().to_string());
        })
        .run(tauri::generate_context!())
        .expect("error while running Tuxedo MD");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_markdown_extensions_case_insensitively() {
        assert!(is_markdown(Path::new("notes/README.MD")));
        assert!(is_markdown(Path::new("notes/page.markdown")));
        assert!(!is_markdown(Path::new("notes/image.png")));
    }
}
