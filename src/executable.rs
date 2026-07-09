use std::path::{Path, PathBuf};

pub(crate) fn current_exe_for_child_env() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok();
    let argv0 = std::env::args_os().next().map(PathBuf::from);
    let path = std::env::var_os("PATH");
    resolve_child_exe(current_exe.as_deref(), argv0.as_deref(), path.as_deref())
}

fn resolve_child_exe(
    current_exe: Option<&Path>,
    argv0: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(current_exe) = current_exe.filter(|path| path.exists()) {
        return Some(current_exe.to_path_buf());
    }

    if let Some(argv0) = argv0 {
        if argv0.components().count() > 1 && argv0.exists() {
            return Some(argv0.to_path_buf());
        }

        if let Some(found) = find_on_path(argv0, path) {
            return Some(found);
        }
    }

    find_on_path(Path::new("herdr"), path)
}

fn find_on_path(program: &Path, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let name = program.file_name()?;
    std::env::split_paths(path?).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.exists().then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    fn temp_exe(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("herdr-executable-test-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, b"#!/bin/sh\n").unwrap();
        path
    }

    #[test]
    fn child_exe_falls_back_to_path_when_current_exe_was_replaced() {
        let current_exe = Path::new("/tmp/herdr (deleted)");
        let herdr = temp_exe("herdr");
        let path = herdr.parent().unwrap().as_os_str();

        assert_eq!(
            resolve_child_exe(Some(current_exe), Some(Path::new("herdr")), Some(path)),
            Some(herdr)
        );
    }
}
