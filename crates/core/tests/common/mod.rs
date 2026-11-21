use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static SCRATCH_FOLDERS: AtomicU32 = AtomicU32::new(0);

pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        let count = SCRATCH_FOLDERS.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("wb-core-{name}-{}-{count}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch folder");
        Scratch { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
