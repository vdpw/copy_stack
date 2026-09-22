use crate::command_error::{CommandError, CommandResult, ErrorCode, Operation};

/// Called only on the AppKit main thread. The system panel supplies its own
/// localized labels and returns cancellation without changing any setting.
#[cfg(target_os = "macos")]
pub fn choose_directory(current_directory: &str) -> CommandResult<Option<String>> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSString, NSURL};

    let mtm = MainThreadMarker::new().ok_or_else(picker_error)?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(true);
    let directory_url =
        NSURL::fileURLWithPath_isDirectory(&NSString::from_str(current_directory), true);
    panel.setDirectoryURL(Some(&directory_url));
    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    let path = panel
        .URL()
        .and_then(|url| url.path())
        .ok_or_else(picker_error)?;
    Ok(Some(path.to_string()))
}

#[cfg(not(target_os = "macos"))]
pub fn choose_directory(_current_directory: &str) -> CommandResult<Option<String>> {
    Err(picker_error())
}

fn picker_error() -> CommandError {
    CommandError::new(ErrorCode::StorageMoveFailed, Operation::MoveStorage, false)
}
