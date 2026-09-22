use resvera_core::pipeline::naming::strip_verbatim_prefix;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Manages the set of filesystem paths allowed to be served for previews
/// (both via the Tauri `asset://` protocol and via the `read_image_data` IPC fallback).
#[derive(Clone, Default)]
pub struct PreviewScope {
    allowed_dirs: Arc<Mutex<HashSet<PathBuf>>>,
    allowed_files: Arc<Mutex<HashSet<PathBuf>>>,
    tauri_scope: Arc<Mutex<Option<tauri::scope::fs::Scope>>>,
}

impl PreviewScope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_tauri_scope(&self, scope: tauri::scope::fs::Scope) {
        let mut guard = self.tauri_scope.lock().unwrap();
        *guard = Some(scope.clone());

        let dirs = self.allowed_dirs.lock().unwrap();
        for dir in dirs.iter() {
            let _ = scope.allow_directory(dir, true);
        }
        let files = self.allowed_files.lock().unwrap();
        for file in files.iter() {
            let _ = scope.allow_file(file);
        }
    }

    pub fn allow_file(&self, path: &Path) {
        let clean = strip_verbatim_prefix(path);
        let canonical = clean.canonicalize().unwrap_or_else(|_| clean.to_path_buf());
        {
            let mut files = self.allowed_files.lock().unwrap();
            files.insert(canonical.clone());
            files.insert(clean.to_path_buf());
        }

        if let Some(scope) = self.tauri_scope.lock().unwrap().as_ref() {
            let _ = scope.allow_file(&clean);
            let _ = scope.allow_file(&canonical);
        }
    }

    pub fn allow_directory(&self, path: &Path) {
        let clean = strip_verbatim_prefix(path);
        let canonical = clean.canonicalize().unwrap_or_else(|_| clean.to_path_buf());
        {
            let mut dirs = self.allowed_dirs.lock().unwrap();
            dirs.insert(canonical.clone());
            dirs.insert(clean.to_path_buf());
        }

        if let Some(scope) = self.tauri_scope.lock().unwrap().as_ref() {
            let _ = scope.allow_directory(&clean, true);
            let _ = scope.allow_directory(&canonical, true);
        }
    }

    pub fn is_allowed(&self, path: &Path, staging_dir: &Path, preview_dir: &Path) -> bool {
        let clean = strip_verbatim_prefix(path);
        let canonical = clean.canonicalize().ok();

        // 1. Check if within preview_cache_dir
        if is_same_or_child(&clean, preview_dir)
            || canonical
                .as_ref()
                .is_some_and(|c| is_same_or_child(c, preview_dir))
        {
            return true;
        }

        // 2. Check if within staging_dir
        if is_same_or_child(&clean, staging_dir)
            || canonical
                .as_ref()
                .is_some_and(|c| is_same_or_child(c, staging_dir))
        {
            return true;
        }

        // 3. Check allowed files
        let files = self.allowed_files.lock().unwrap();
        if files.iter().any(|f| {
            paths_match(f, &clean) || canonical.as_ref().is_some_and(|c| paths_match(f, c))
        }) {
            return true;
        }

        // 4. Check allowed directories
        let dirs = self.allowed_dirs.lock().unwrap();
        for dir in dirs.iter() {
            if is_same_or_child(&clean, dir)
                || canonical.as_ref().is_some_and(|c| is_same_or_child(c, dir))
            {
                return true;
            }
        }

        false
    }
}

fn normalize_path_str(p: &Path) -> String {
    let s = p.to_string_lossy();
    #[cfg(windows)]
    {
        s.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        s.to_string()
    }
}

fn paths_match(a: &Path, b: &Path) -> bool {
    normalize_path_str(a) == normalize_path_str(b)
}

fn is_same_or_child(target: &Path, base: &Path) -> bool {
    let t = normalize_path_str(target);
    let b = normalize_path_str(base);
    if t == b {
        return true;
    }
    #[cfg(windows)]
    let sep = "\\";
    #[cfg(not(windows))]
    let sep = "/";

    let b_prefix = if b.ends_with(sep) {
        b
    } else {
        format!("{b}{sep}")
    };
    t.starts_with(&b_prefix)
}
