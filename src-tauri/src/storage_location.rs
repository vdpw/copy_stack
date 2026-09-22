//! Bootstrap location and recoverable storage-directory changes.
//!
//! The recovery record is written before changing the location setting. Until
//! the original database is removed, it is authoritative on startup. Removing
//! that original file is the commit point; there is no fallible work afterward.

use crate::private_fs::{
    self, PrivateFileIdentity, PrivateFsError, PrivateFsErrorKind, PrivateTempFile,
};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const DB_FILE_NAME: &str = "clipecho.db";
const CONFIG_FILE_NAME: &str = "storage.json";
const RECOVERY_FILE_NAME: &str = "storage-move.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageMoveError {
    InvalidPath,
    PermissionDenied,
    AlreadyExists,
    MoveFailed,
}

impl std::fmt::Display for StorageMoveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPath => "the storage directory is invalid",
            Self::PermissionDenied => "the storage directory cannot be accessed",
            Self::AlreadyExists => "a database or SQLite sidecar already exists in the destination",
            Self::MoveFailed => "the storage move could not be completed",
        })
    }
}

impl std::error::Error for StorageMoveError {}

impl From<PrivateFsError> for StorageMoveError {
    fn from(error: PrivateFsError) -> Self {
        match error.kind() {
            PrivateFsErrorKind::AlreadyExists => Self::AlreadyExists,
            PrivateFsErrorKind::WrongOwner
            | PrivateFsErrorKind::InsecureDirectory
            | PrivateFsErrorKind::InsufficientPermissions
            | PrivateFsErrorKind::Io(std::io::ErrorKind::PermissionDenied) => {
                Self::PermissionDenied
            }
            PrivateFsErrorKind::InvalidPath
            | PrivateFsErrorKind::Symlink
            | PrivateFsErrorKind::NotDirectory
            | PrivateFsErrorKind::NotRegularFile
            | PrivateFsErrorKind::MultipleHardLinks
            | PrivateFsErrorKind::PathChanged => Self::InvalidPath,
            _ => Self::MoveFailed,
        }
    }
}

impl From<std::io::Error> for StorageMoveError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            _ => Self::MoveFailed,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StorageLocation {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocationConfig {
    directory: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveRecovery {
    source: PathBuf,
    destination: PathBuf,
    source_identity: PrivateFileIdentity,
    destination_identity: PrivateFileIdentity,
}

impl StorageLocation {
    pub(crate) fn for_app() -> Result<Self, StorageMoveError> {
        #[cfg(debug_assertions)]
        if let Some(root) = std::env::var_os("COPY_STACK_QA_DATA_DIR") {
            return Self::at_root(Path::new(&root));
        }
        let home = std::env::var_os("HOME").ok_or(StorageMoveError::InvalidPath)?;
        Self::at_root(&PathBuf::from(home).join(".clipecho"))
    }

    pub(crate) fn at_root(root: &Path) -> Result<Self, StorageMoveError> {
        if !root.is_absolute() {
            return Err(StorageMoveError::InvalidPath);
        }
        Ok(Self {
            root: private_fs::ensure_private_directory(root)?,
        })
    }

    pub(crate) fn database_path(&self) -> Result<PathBuf, StorageMoveError> {
        let recovery_path = self.root.join(RECOVERY_FILE_NAME);
        if let Some(recovery) = read_json::<MoveRecovery>(&recovery_path)? {
            validate_directory(
                recovery
                    .source
                    .parent()
                    .ok_or(StorageMoveError::InvalidPath)?,
            )?;
            validate_directory(
                recovery
                    .destination
                    .parent()
                    .ok_or(StorageMoveError::InvalidPath)?,
            )?;
            let source_exists = match std::fs::symlink_metadata(&recovery.source) {
                Ok(_) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            };
            let selected = if source_exists {
                if !recovery.source_identity.matches(&recovery.source)? {
                    return Err(StorageMoveError::InvalidPath);
                }
                recovery.source
            } else {
                if !recovery
                    .destination_identity
                    .matches(&recovery.destination)?
                {
                    return Err(StorageMoveError::MoveFailed);
                }
                recovery.destination
            };
            // The recovery record remains authoritative if configuration repair
            // is temporarily blocked. Never fall back to a fresh empty database.
            if self
                .save_directory(selected.parent().ok_or(StorageMoveError::InvalidPath)?)
                .is_ok()
            {
                let _ = remove_private_file(&recovery_path);
            }
            return Ok(selected);
        }
        match read_json::<LocationConfig>(&self.root.join(CONFIG_FILE_NAME))? {
            Some(config) => {
                let path = validate_directory(&config.directory)?.join(DB_FILE_NAME);
                // A disconnected volume must not silently create empty history.
                PrivateFileIdentity::read(&path)?;
                Ok(path)
            }
            None => Ok(self.root.join(DB_FILE_NAME)),
        }
    }

    pub(crate) fn begin_move(
        &self,
        source: &Path,
        destination: &Path,
        source_identity: PrivateFileIdentity,
    ) -> Result<LocationMove, StorageMoveError> {
        if !source_identity.matches(source)? {
            return Err(StorageMoveError::InvalidPath);
        }
        let staged_config = stage_json(
            &self.root.join(CONFIG_FILE_NAME),
            &LocationConfig {
                directory: destination
                    .parent()
                    .ok_or(StorageMoveError::InvalidPath)?
                    .to_path_buf(),
            },
        )?;
        let recovery = MoveRecovery {
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            source_identity,
            destination_identity: PrivateFileIdentity::read(destination)?,
        };
        write_json(&self.root.join(RECOVERY_FILE_NAME), &recovery)?;
        Ok(LocationMove {
            location: self.clone(),
            source: source.to_path_buf(),
            source_identity: recovery.source_identity,
            staged_config: Some(staged_config),
            complete: false,
        })
    }

    fn save_directory(&self, directory: &Path) -> Result<(), StorageMoveError> {
        write_json(
            &self.root.join(CONFIG_FILE_NAME),
            &LocationConfig {
                directory: directory.to_path_buf(),
            },
        )
    }
}

pub(crate) struct LocationMove {
    location: StorageLocation,
    source: PathBuf,
    source_identity: PrivateFileIdentity,
    staged_config: Option<PrivateTempFile>,
    complete: bool,
}

impl LocationMove {
    pub(crate) fn save_new_location(&mut self) -> Result<(), StorageMoveError> {
        self.staged_config
            .take()
            .ok_or(StorageMoveError::MoveFailed)?
            .commit(&self.location.root.join(CONFIG_FILE_NAME))?;
        Ok(())
    }

    pub(crate) fn finish(mut self) {
        self.complete = true;
        // A leftover record resolves the committed destination on startup.
        let _ = remove_private_file(&self.location.root.join(RECOVERY_FILE_NAME));
    }
}

impl Drop for LocationMove {
    fn drop(&mut self) {
        if !self.complete && self.source_identity.matches(&self.source) == Ok(true) {
            if let Some(directory) = self.source.parent() {
                if self.location.save_directory(directory).is_ok() {
                    let _ = remove_private_file(&self.location.root.join(RECOVERY_FILE_NAME));
                }
            }
        }
    }
}

pub(crate) fn validate_directory(directory: &Path) -> Result<PathBuf, StorageMoveError> {
    if !directory.is_absolute() || directory.as_os_str().is_empty() || directory.to_str().is_none()
    {
        return Err(StorageMoveError::InvalidPath);
    }
    Ok(private_fs::resolve_private_path(directory)?)
}

pub(crate) fn reject_existing_sqlite_files(path: &Path) -> Result<(), StorageMoveError> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut file = path.as_os_str().to_os_string();
        file.push(suffix);
        match std::fs::symlink_metadata(PathBuf::from(file)) {
            Ok(_) => return Err(StorageMoveError::AlreadyExists),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<Option<T>, StorageMoveError> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let mut bytes = Vec::new();
    private_fs::read_private_file(path)?
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 {
        return Err(StorageMoveError::InvalidPath);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| StorageMoveError::InvalidPath)
}

fn stage_json(path: &Path, value: &impl Serialize) -> Result<PrivateTempFile, StorageMoveError> {
    let bytes = serde_json::to_vec(value).map_err(|_| StorageMoveError::InvalidPath)?;
    let mut staged = private_fs::create_private_temp_file(path, 0)?;
    staged.file_mut().write_all(&bytes)?;
    staged.sync_all()?;
    Ok(staged)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), StorageMoveError> {
    stage_json(path, value)?.commit(path)?;
    Ok(())
}

fn remove_private_file(path: &Path) -> Result<(), StorageMoveError> {
    PrivateFileIdentity::read(path)?.remove(path)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture(label: &str) -> (StorageLocation, PathBuf, PathBuf, PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "clipecho-location-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let location = StorageLocation::at_root(&root.join("bootstrap")).unwrap();
        let source = root.join("source").join(DB_FILE_NAME);
        let destination = root.join("destination").join(DB_FILE_NAME);
        private_fs::create_private_new_file(&source)
            .unwrap()
            .retain();
        private_fs::create_private_new_file(&destination)
            .unwrap()
            .retain();
        (location, source, destination, root)
    }

    #[test]
    fn interrupted_move_recovers_the_side_of_the_source_removal_commit_point() {
        for committed in [false, true] {
            let (location, source, destination, root) = fixture("interrupted");
            let mut movement = location
                .begin_move(
                    &source,
                    &destination,
                    PrivateFileIdentity::read(&source).unwrap(),
                )
                .unwrap();
            movement.save_new_location().unwrap();
            // Simulate process termination without executing rollback Drop.
            std::mem::forget(movement);
            if committed {
                std::fs::remove_file(&source).unwrap();
            }
            let expected = if committed { &destination } else { &source };
            assert_eq!(&location.database_path().unwrap(), expected);
            assert_eq!(&location.database_path().unwrap(), expected);
            assert!(!location.root.join(RECOVERY_FILE_NAME).exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn failed_configuration_rollback_keeps_original_authoritative_on_restart() {
        let (location, source, destination, root) = fixture("rollback");
        let mut movement = location
            .begin_move(
                &source,
                &destination,
                PrivateFileIdentity::read(&source).unwrap(),
            )
            .unwrap();
        movement.save_new_location().unwrap();
        let config = location.root.join(CONFIG_FILE_NAME);
        std::fs::remove_file(&config).unwrap();
        std::fs::create_dir(&config).unwrap();
        drop(movement);
        assert!(location.root.join(RECOVERY_FILE_NAME).exists());
        // Destination cleanup can already have removed the uncommitted copy.
        std::fs::remove_file(&destination).unwrap();
        assert_eq!(location.database_path().unwrap(), source);
        std::fs::remove_dir(&config).unwrap();
        assert_eq!(location.database_path().unwrap(), source);
        assert!(!location.root.join(RECOVERY_FILE_NAME).exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inaccessible_original_during_recovery_never_selects_the_copy() {
        let (location, source, destination, root) = fixture("inaccessible");
        let mut movement = location
            .begin_move(
                &source,
                &destination,
                PrivateFileIdentity::read(&source).unwrap(),
            )
            .unwrap();
        movement.save_new_location().unwrap();
        std::mem::forget(movement);
        let parent = source.parent().unwrap();
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(
            location.database_path(),
            Err(StorageMoveError::PermissionDenied)
        );
        assert!(location.root.join(RECOVERY_FILE_NAME).exists());
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(location.database_path().unwrap(), source);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_keeps_recovery_evidence_when_original_is_replaced() {
        let (location, source, destination, root) = fixture("replaced");
        let mut movement = location
            .begin_move(
                &source,
                &destination,
                PrivateFileIdentity::read(&source).unwrap(),
            )
            .unwrap();
        movement.save_new_location().unwrap();
        std::fs::rename(&source, source.with_extension("original")).unwrap();
        private_fs::create_private_new_file(&source)
            .unwrap()
            .retain();
        drop(movement);
        assert!(location.root.join(RECOVERY_FILE_NAME).exists());
        assert_eq!(location.database_path(), Err(StorageMoveError::InvalidPath));
        std::fs::remove_dir_all(root).unwrap();
    }
}
