//! Locating game content (assets, data) and the per-user directory (settings, saves, logs).

use std::path::{Path, PathBuf};

/// Resolved directories used by the game.
#[derive(Clone, Debug)]
pub struct GamePaths {
    /// Directory containing `assets/` and `data/`.
    pub root: PathBuf,
    pub assets: PathBuf,
    pub data: PathBuf,
    /// Writable per-user directory (settings, save games, logs, screenshots).
    pub user: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error(
        "could not find the game content directory (a folder containing 'assets' and 'data'). \
         Searched: {0}. Set KINGDOMS_ROOT to point at it."
    )]
    ContentNotFound(String),
    #[error("could not create user directory {path}: {source}")]
    UserDir { path: String, source: std::io::Error },
}

impl GamePaths {
    /// Discovers content and user directories.
    ///
    /// Content root search order: `$KINGDOMS_ROOT`, the `Contents/Resources` folder of the macOS
    /// app bundle the executable runs from, the executable directory and its ancestors, the
    /// current working directory and its ancestors ([`content_candidates`]). The user
    /// directory can be overridden with `$KINGDOMS_USER_DIR` (used by tests).
    pub fn discover(app_name: &str) -> Result<Self, PathError> {
        let root = find_content_root()?;
        let user = match std::env::var_os("KINGDOMS_USER_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => default_user_dir(app_name),
        };
        std::fs::create_dir_all(&user)
            .map_err(|source| PathError::UserDir { path: user.display().to_string(), source })?;
        Ok(Self::from_dirs(root, user))
    }

    /// Builds paths from explicit directories (no filesystem checks).
    pub fn from_dirs(root: PathBuf, user: PathBuf) -> Self {
        Self { assets: root.join("assets"), data: root.join("data"), root, user }
    }

    pub fn saves_dir(&self) -> PathBuf {
        self.user.join("saves")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.user.join("logs")
    }

    pub fn screenshots_dir(&self) -> PathBuf {
        self.user.join("screenshots")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.user.join("settings.ron")
    }
}

fn is_content_root(dir: &Path) -> bool {
    dir.join("assets").is_dir() && dir.join("data").is_dir()
}

/// The `Contents/Resources` folder of the app bundle whose `Contents/MacOS` holds an executable
/// in `exe_dir` (`Kingdoms.app/Contents/MacOS/kingdoms`), if `exe_dir` is such a folder.
pub fn bundle_resources(exe_dir: &Path) -> Option<PathBuf> {
    let contents = exe_dir.parent()?;
    (exe_dir.file_name()? == "MacOS" && contents.file_name()? == "Contents").then(|| contents.join("Resources"))
}

/// Directories that may be the content root, in search order: `env_root` (`$KINGDOMS_ROOT`);
/// the resources of the app bundle the executable runs from (first, so a bundle built inside a
/// source checkout uses its own copy, not the checkout's); the executable's directory and its
/// ancestors (`cargo run`, a copied release folder); the working directory and its ancestors.
pub fn content_candidates(env_root: Option<PathBuf>, exe_dir: Option<&Path>, cwd: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = env_root.into_iter().collect();
    candidates.extend(exe_dir.and_then(bundle_resources));
    for start in [exe_dir, cwd].into_iter().flatten() {
        candidates.extend(start.ancestors().map(Path::to_path_buf));
    }
    candidates
}

fn find_content_root() -> Result<PathBuf, PathError> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    let candidates = content_candidates(
        std::env::var_os("KINGDOMS_ROOT").map(PathBuf::from),
        exe.as_deref().and_then(Path::parent),
        cwd.as_deref(),
    );
    match candidates.iter().find(|dir| is_content_root(dir)) {
        Some(dir) => Ok(dir.canonicalize().unwrap_or_else(|_| dir.clone())),
        None => Err(PathError::ContentNotFound(candidates.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", "))),
    }
}

/// Platform-specific default user directory.
pub fn default_user_dir(app_name: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        if let Some(home) = home {
            return home.join("Library/Application Support").join(app_name);
        }
    } else if cfg!(target_os = "windows") {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(app_name);
        }
    } else {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join(app_name.to_lowercase());
        }
        if let Some(home) = home {
            return home.join(".local/share").join(app_name.to_lowercase());
        }
    }
    PathBuf::from(".").join("userdata")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system's temporary folder.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kingdoms-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn content(dir: &Path) {
        for sub in ["assets", "data"] {
            if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
                panic!("cannot create {}: {e}", dir.display());
            }
        }
    }

    #[test]
    fn a_bundle_uses_its_own_resources_even_inside_a_checkout() {
        let repo = scratch("bundle");
        content(&repo);
        let app = repo.join("target/Kingdoms.app/Contents");
        content(&app.join("Resources"));
        let macos = app.join("MacOS");
        let release = repo.join("target/release");
        for dir in [&macos, &release] {
            if let Err(e) = std::fs::create_dir_all(dir) {
                panic!("{e}");
            }
        }
        let found = |exe: &Path, cwd: &Path| content_candidates(None, Some(exe), Some(cwd)).into_iter().find(|d| is_content_root(d));
        assert_eq!(found(&macos, Path::new("/")), Some(app.join("Resources")), "Finder starts apps in /");
        assert_eq!(found(&release, Path::new("/")), Some(repo.clone()), "cargo run: the checkout");
        // `$KINGDOMS_ROOT` comes first.
        let other = scratch("root");
        content(&other);
        let first = content_candidates(Some(other.clone()), Some(&macos), None).into_iter().find(|d| is_content_root(d));
        assert_eq!(first, Some(other.clone()));
        assert_eq!(bundle_resources(&release), None);
        assert_eq!(bundle_resources(Path::new("/")), None);
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn user_dirs_follow_the_platform() {
        let dir = default_user_dir("Kingdoms");
        if cfg!(target_os = "macos") {
            assert!(dir.ends_with("Library/Application Support/Kingdoms"), "{}", dir.display());
        } else if cfg!(target_os = "linux") && std::env::var_os("XDG_DATA_HOME").is_none() && std::env::var_os("HOME").is_some() {
            assert!(dir.ends_with(".local/share/kingdoms"), "{}", dir.display());
        }
    }
}
