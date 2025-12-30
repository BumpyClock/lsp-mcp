// ABOUTME: Global and thread-local mount directory management.
// ABOUTME: Provides workspace root path accessors for path resolution across the codebase.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};

static GLOBAL_MOUNT_DIR: LazyLock<Arc<RwLock<PathBuf>>> =
    LazyLock::new(|| Arc::new(RwLock::new(PathBuf::from("/mnt/workspace"))));

thread_local! {
    static THREAD_LOCAL_MOUNT_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn get_mount_dir() -> PathBuf {
    THREAD_LOCAL_MOUNT_DIR.with(|local| {
        local
            .borrow()
            .clone()
            .unwrap_or_else(|| GLOBAL_MOUNT_DIR.read().unwrap().clone())
    })
}

pub fn set_thread_local_mount_dir(path: impl AsRef<Path>) {
    THREAD_LOCAL_MOUNT_DIR.with(|local| {
        *local.borrow_mut() = Some(path.as_ref().to_path_buf());
    });
}

pub fn unset_thread_local_mount_dir() {
    THREAD_LOCAL_MOUNT_DIR.with(|local| {
        *local.borrow_mut() = None;
    });
}

pub fn set_global_mount_dir(path: impl AsRef<Path>) {
    let mut global_dir = GLOBAL_MOUNT_DIR.write().unwrap();
    *global_dir = path.as_ref().to_path_buf();
}
