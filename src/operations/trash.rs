//! System Trash access. Keep the OS call separate from the background/UI logic.

use std::path::Path;

fn context() -> trash::TrashContext {
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = trash::TrashContext::default();
        // The crate defaults to Finder (AppleScript, sound, Automation permission).
        context.set_delete_method(DeleteMethod::NsFileManager);
        context
    }
    #[cfg(not(target_os = "macos"))]
    {
        trash::TrashContext::default()
    }
}

pub(crate) fn move_to_trash(path: &Path) -> Result<(), String> {
    let context = context();
    // Never call the real system Trash from unit tests, even through a UI handler.
    #[cfg(test)]
    {
        let _ = (context, path);
        Err("system Trash is disabled in unit tests".into())
    }
    #[cfg(not(test))]
    {
        // trash canonicalizes only the parent; NSFileManager/rename moves the link itself.
        context.delete(path).map_err(|error| error.to_string())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn trash_context_uses_ns_file_manager_without_calling_trash() {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        assert!(matches!(
            super::context().delete_method(),
            DeleteMethod::NsFileManager
        ));
    }
}
