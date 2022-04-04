use crate::package_archive::{FileMode, PathType};
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

/// Called to link a file from the package cache into a prefix. This also replaces any prefix if,
/// it is present.
pub fn link_file(
    prefix: &Path,
    source_path: &Path,
    destination_path: &Path,
    prefix_placeholder: Option<&str>,
    path_type: PathType,
    file_mode: FileMode,
    always_copy: bool,
) -> anyhow::Result<Option<String>> {
    // Ensure all directories up to the path exist
    if let Some(parent) = destination_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create parent directory structure"))?;
        }
    }

    // If the path already exists, remove it
    // TODO: Properly handle clobbering here
    if destination_path.is_file() {
        // log::warn!(
        //     "Clobbering: $CONDA_PREFIX/{}",
        //     entry.relative_path.display()
        // );
        std::fs::remove_file(&destination_path)
            .with_context(|| format!("error removing existing file"))?;
    }

    if let Some(old_prefix) = &prefix_placeholder {
        // Determine the new prefix for the file
        let new_prefix = &prefix.to_string_lossy();
        let digest = match file_mode {
            FileMode::Text => {
                // TODO: Replace '\\' with '/' in prefix on windows
                copy_replace_prefix_text(&source_path, &destination_path, old_prefix, &new_prefix)?
            }
            FileMode::Binary => {
                let source_meta = std::fs::metadata(&source_path)
                    .context("unable to determine permissions of cached file")?;
                let digest = copy_replace_prefix_binary(
                    &source_path,
                    &destination_path,
                    old_prefix,
                    &new_prefix,
                )?;
                std::fs::set_permissions(destination_path, source_meta.permissions())
                    .context("unable to assign same permissions as source file")?;
                digest
            }
        };

        return Ok(Some(digest));
    } else if path_type == PathType::HardLink && always_copy {
        hard_link_entry(&source_path, &destination_path)?;
    } else if path_type == PathType::SoftLink && always_copy {
        soft_link_entry(&source_path, &destination_path)?;
    } else {
        copy_entry(&source_path, &destination_path)?;
    };

    Ok(None)
}

/// Copy the file from the source to the destination while replacing the `old_prefix` with the
/// `new_prefix` in binary occurrences.
fn copy_replace_prefix_binary(
    source_path: &Path,
    destination_path: &Path,
    old_prefix: &str,
    new_prefix: &str,
) -> anyhow::Result<String> {
    // Memory map the source file
    let source = {
        let file = std::fs::File::open(source_path).context("unable to open file from cache")?;
        unsafe { memmap2::Mmap::map(&file) }.context("unable to memory map file from cache")?
    };

    // Open the output file for writing
    let mut destination = std::fs::File::create(destination_path)
        .context("unable to open destination file for writing")?;

    // Get the prefixes as bytes
    let old_prefix = old_prefix.as_bytes();
    let new_prefix = new_prefix.as_bytes();

    let padding_len = if old_prefix.len() > new_prefix.len() {
        old_prefix.len() - new_prefix.len()
    } else {
        0
    };
    let padding = vec![0u8; padding_len];

    let mut digest = Sha256::new();
    let mut source_bytes = source.as_ref();
    loop {
        if let Some(index) = twoway::find_bytes(source_bytes, old_prefix) {
            // Find the end of the c-style string
            let mut end = index + old_prefix.len();
            while end < source.len() && source_bytes[end] != 0 {
                end += 1;
            }

            // Get the suffix part
            let suffix = &source_bytes[index + old_prefix.len()..end];

            // Write to disk
            destination
                .write_all(&source_bytes[..index])
                .and_then(|_| destination.write_all(new_prefix))
                .and_then(|_| destination.write_all(suffix))
                .and_then(|_| destination.write_all(&padding))
                .context("failed to write to destination")?;

            // Update digest
            digest.update(&source_bytes[..index]);
            digest.update(new_prefix);
            digest.update(suffix);
            digest.update(&padding);

            // Continue with the rest of the bytes
            source_bytes = &source_bytes[end..];
        } else {
            // Write to disk
            destination
                .write_all(&source_bytes)
                .context("failed to write to destination")?;

            // Update digest
            digest.update(&source_bytes);

            return Ok(format!("{:x}", digest.finalize()));
        }
    }
}

/// Copy the file from the source to the destination while replacing the `old_prefix` with the
/// `new_prefix` by searching for text occurrences.
fn copy_replace_prefix_text(
    source_path: &Path,
    destination_path: &Path,
    old_prefix: &str,
    new_prefix: &str,
) -> anyhow::Result<String> {
    // Memory map the source file
    let source = {
        let file = std::fs::File::open(source_path).context("unable to open file from cache")?;
        unsafe { memmap2::Mmap::map(&file) }.context("unable to memory map file from cache")?
    };

    // Open the output file for writing
    let mut destination = std::fs::File::create(destination_path)
        .context("unable to open destination file for writing")?;

    // Get the prefixes as bytes
    let old_prefix = old_prefix.as_bytes();
    let new_prefix = new_prefix.as_bytes();

    // TODO: Update shebang if present

    let mut digest = Sha256::new();
    let mut source_bytes = source.as_ref();
    loop {
        if let Some(index) = twoway::find_bytes(source_bytes, old_prefix) {
            // Write to disk
            destination
                .write_all(&source_bytes[..index])
                .and_then(|_| destination.write_all(new_prefix))
                .context("failed to write to destination")?;

            // Update digest
            digest.update(&source_bytes[..index]);
            digest.update(new_prefix);

            source_bytes = &source_bytes[index + old_prefix.len()..];
        } else {
            // Write to disk
            destination
                .write_all(&source_bytes)
                .context("failed to write to destination")?;

            // Update digest
            digest.update(&source_bytes);

            return Ok(format!("{:x}", digest.finalize()));
        }
    }
}

#[cfg(windows)]
fn symlink(source_path: &Path, destination_path: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(source_path, destination_path)
}

#[cfg(unix)]
fn symlink(source_path: &Path, destination_path: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source_path, destination_path)
}

/// Hard links an entry from the source archive to the destination. Falls back to soft-linking or
/// copying if hard-linking fails.
fn hard_link_entry(source_path: &Path, destination_path: &Path) -> anyhow::Result<()> {
    std::fs::hard_link(source_path, destination_path)
        .or_else(|e| {
            log::debug!("unable to hardlink `{}`: {}", destination_path.display(), e);
            symlink(&source_path, &destination_path)
        })
        .or_else(|e| {
            log::debug!("unable to softlink `{}`: {}", destination_path.display(), e);
            std::fs::copy(source_path, destination_path).map(|_| ())
        })
        .context("error hard linking entry")
}

/// Soft links an entry from the source archive to the destination. Falls back to copying if
/// soft-linking fails.
fn soft_link_entry(source_path: &Path, destination_path: &Path) -> anyhow::Result<()> {
    symlink(&source_path, &destination_path)
        .or_else(|e| {
            log::debug!("unable to softlink `{}`: {}", destination_path.display(), e);
            std::fs::copy(source_path, destination_path).map(|_| ())
        })
        .context("error soft linking entry")
}

/// Copies an entry from the source archive to the destination.
fn copy_entry(source_path: &Path, destination_path: &Path) -> anyhow::Result<()> {
    std::fs::copy(source_path, destination_path)
        .map(|_| ())
        .context("error copying entry")
}
