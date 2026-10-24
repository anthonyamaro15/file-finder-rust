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
        context.delete(path).map_err(error_message)
    }
}

/// The crate's messages are Debug dumps; macOS's reason follows "`trashItemAtURL` failed: ".
fn error_message(error: trash::Error) -> String {
    match error {
        trash::Error::Unknown { description } => {
            if let Some((_, reason)) = description.split_once("failed: ") {
                return reason.to_string();
            }
            description
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
#[test]
fn error_message_keeps_the_system_reason() {
    let error = trash::Error::Unknown {
        description: "While deleting '\"/v/a.txt\"', `trashItemAtURL` failed: “a.txt” couldn’t be moved to the trash because the volume is read only.".into(),
    };
    assert_eq!(
        error_message(error),
        "“a.txt” couldn’t be moved to the trash because the volume is read only."
    );
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
