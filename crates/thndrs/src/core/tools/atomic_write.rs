//! Same-directory temporary-file writes used by workspace write tools.
//!
//! The target is never opened for writing. The complete new content is written,
//! flushed, synchronized, and closed in a temporary file before the final
//! install step. Replacement installs use rename; creation uses a hard link so
//! a target that appears after validation cannot be overwritten.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use tempfile::Builder;

const TEMP_FILE_PREFIX: &str = ".thndrs-write-";

/// Whether the target must be absent or may be replaced at install time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    /// Install only when the target is still absent.
    Create,
    /// Atomically replace the target when the platform supports replacement
    /// renames for the filesystem.
    Replace,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailurePoint {
    /// Fail after a prefix has reached the temporary file.
    AfterPartialWrite,
    /// Fail after the complete temporary file has been synchronized.
    BeforeInstall,
}

#[cfg(test)]
thread_local! {
    static NEXT_FAILURE: std::cell::Cell<Option<FailurePoint>> = const { std::cell::Cell::new(None) };
}

/// Write `content` without exposing a partially written target.
pub fn write(target: &Path, content: &[u8], mode: WriteMode) -> io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target has no parent directory"))?;

    let target_permissions = match mode {
        WriteMode::Create => None,
        WriteMode::Replace => match fs::metadata(target) {
            Ok(metadata) => Some(metadata.permissions()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        },
    };

    let mut temporary = Builder::new().prefix(TEMP_FILE_PREFIX).tempfile_in(parent)?;
    if let Some(permissions) = target_permissions {
        temporary.as_file().set_permissions(permissions)?;
    }

    write_content(temporary.as_file_mut(), content)?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;

    #[cfg(test)]
    if take_failure(FailurePoint::BeforeInstall) {
        return Err(io::Error::other("injected atomic-write install failure"));
    }

    // Converting to TempPath closes the file descriptor before the install.
    // TempPath removes the temporary path if the install fails or a later
    // operation returns an error.
    let temporary_path = temporary.into_temp_path();
    match mode {
        WriteMode::Create => fs::hard_link(&temporary_path, target),
        WriteMode::Replace => fs::rename(&temporary_path, target),
    }
}

#[cfg(test)]
pub fn fail_next_for_test(point: FailurePoint) {
    NEXT_FAILURE.with(|failure| failure.set(Some(point)));
}

fn write_content(file: &mut fs::File, content: &[u8]) -> io::Result<()> {
    #[cfg(test)]
    if take_failure(FailurePoint::AfterPartialWrite) {
        let partial_len = content.len().min(1);
        if partial_len > 0 {
            file.write_all(&content[..partial_len])?;
        }
        return Err(io::Error::other("injected atomic-write content failure"));
    }

    file.write_all(content)
}

#[cfg(test)]
fn take_failure(point: FailurePoint) -> bool {
    NEXT_FAILURE.with(|failure| {
        if failure.get() == Some(point) {
            failure.set(None);
            true
        } else {
            false
        }
    })
}
