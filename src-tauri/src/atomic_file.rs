use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Owns a synced replacement beside its destination; dropping it cancels publication.
pub(crate) struct StagedFile {
    destination: PathBuf,
    temporary: PathBuf,
    published: bool,
}

impl StagedFile {
    pub(crate) fn prepare(destination: &Path, contents: &[u8]) -> io::Result<Self> {
        let parent = destination.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination has no parent")
        })?;
        fs::create_dir_all(parent)?;
        let name = destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let timestamp = crate::time::current_timestamp_millis();
        for suffix in 0.. {
            let temporary = parent.join(format!(".{name}.tmp-{timestamp}-{suffix}"));
            let mut file = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let staged = Self {
                destination: destination.to_owned(),
                temporary,
                published: false,
            };
            let result = file.write_all(contents).and_then(|_| file.sync_all());
            // Windows requires the file to be closed before replacement or cleanup.
            drop(file);
            result?;
            return Ok(staged);
        }
        unreachable!("suffix loop is unbounded")
    }

    pub(crate) fn publish(mut self) -> io::Result<()> {
        replace(&self.temporary, &self.destination)?;
        self.published = true;
        Ok(())
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.temporary);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub(crate) fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();

    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
