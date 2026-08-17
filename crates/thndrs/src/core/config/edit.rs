//! Shared atomic, comment-preserving TOML configuration edits.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use toml_edit::DocumentMut;

/// Edit a TOML file after validating both its current and rendered contents.
///
/// The edit is skipped when the closure returns `None`. Otherwise the rendered
/// TOML replaces the destination atomically.
pub(crate) fn edit_toml_file<T>(
    path: &Path, description: &str, validate: impl Fn(&str) -> io::Result<()>,
    edit: impl FnOnce(&mut DocumentMut) -> io::Result<Option<T>>,
) -> io::Result<Option<T>> {
    let source = read_optional_toml(path, description)?;
    validate(&source)?;
    let mut document = source.parse::<DocumentMut>().map_err(|source| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {description} {}: {source}", path.display()),
        )
    })?;

    let Some(result) = edit(&mut document)? else {
        return Ok(None);
    };

    let rendered = document.to_string();
    validate(&rendered)?;
    replace_atomically(path, rendered.as_bytes())?;
    Ok(Some(result))
}

/// Atomically write already-rendered TOML configuration content.
pub(crate) fn write_toml_file(path: &Path, description: &str, contents: &str) -> io::Result<()> {
    contents.parse::<DocumentMut>().map_err(|source| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {description} {}: {source}", path.display()),
        )
    })?;
    replace_atomically(path, contents.as_bytes())
}

fn read_optional_toml(path: &Path, description: &str) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(source) => Ok(source),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(io::Error::new(
            source.kind(),
            format!("failed to read {description} {}: {source}", path.display()),
        )),
    }
}

fn replace_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("configuration path {} has no parent directory", path.display()),
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
