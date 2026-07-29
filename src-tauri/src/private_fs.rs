//! Unix filesystem primitives for Copy Stack's private data.
//!
//! The public helpers in this module deliberately do not include caller paths
//! in their errors. A clipboard export path can contain user-controlled text,
//! and filesystem errors are routinely forwarded to logs or UI surfaces.
//!
//! Existing objects are only tightened: permission bits are removed before
//! use, but missing owner permissions are never granted. Newly-created private
//! objects start with their final permissions (`0700` for directories and
//! `0600` for files).

use std::fmt;
use std::fs::{File, Metadata};
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::fs::{DirBuilder, OpenOptions, Permissions};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::path::Component;

pub const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
pub const PRIVATE_FILE_MODE: u32 = 0o600;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateFsErrorKind {
    InvalidPath,
    UnsupportedPlatform,
    NotFound,
    Symlink,
    NotDirectory,
    NotRegularFile,
    WrongOwner,
    MultipleHardLinks,
    InsecureDirectory,
    InsufficientPermissions,
    PathChanged,
    AlreadyExists,
    Io(io::ErrorKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateFsError {
    operation: &'static str,
    subject: &'static str,
    kind: PrivateFsErrorKind,
}

impl PrivateFsError {
    pub fn kind(&self) -> PrivateFsErrorKind {
        self.kind
    }

    pub fn operation(&self) -> &'static str {
        self.operation
    }

    fn new(operation: &'static str, subject: &'static str, kind: PrivateFsErrorKind) -> Self {
        Self {
            operation,
            subject,
            kind,
        }
    }

    fn io(operation: &'static str, subject: &'static str, error: &io::Error) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => PrivateFsErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => PrivateFsErrorKind::AlreadyExists,
            kind => PrivateFsErrorKind::Io(kind),
        };
        Self::new(operation, subject, kind)
    }
}

impl fmt::Display for PrivateFsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.kind {
            PrivateFsErrorKind::InvalidPath => "the path is invalid",
            PrivateFsErrorKind::UnsupportedPlatform => {
                "private filesystem enforcement is unsupported on this platform"
            }
            PrivateFsErrorKind::NotFound => "the object was not found",
            PrivateFsErrorKind::Symlink => "symbolic links are not allowed",
            PrivateFsErrorKind::NotDirectory => "the object is not a directory",
            PrivateFsErrorKind::NotRegularFile => "the object is not a regular file",
            PrivateFsErrorKind::WrongOwner => "the object is owned by another user",
            PrivateFsErrorKind::MultipleHardLinks => {
                "private files must have exactly one hard link"
            }
            PrivateFsErrorKind::InsecureDirectory => "the directory is writable by another user",
            PrivateFsErrorKind::InsufficientPermissions => "required owner permissions are missing",
            PrivateFsErrorKind::PathChanged => "the object changed while it was being validated",
            PrivateFsErrorKind::AlreadyExists => "the object already exists",
            PrivateFsErrorKind::Io(_) => "the operating system rejected the operation",
        };

        write!(
            formatter,
            "failed to {} private {}: {}",
            self.operation, self.subject, reason
        )
    }
}

impl std::error::Error for PrivateFsError {}

/// An exclusively-created `0600` file that removes its own directory entry if
/// it is dropped before a successful atomic commit.
pub struct PrivateTempFile {
    file: File,
    path: PathBuf,
    #[cfg(unix)]
    identity: FileIdentity,
    cleanup_armed: bool,
}

impl fmt::Debug for PrivateTempFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrivateTempFile")
            .finish_non_exhaustive()
    }
}

impl PrivateTempFile {
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub fn sync_all(&self) -> Result<(), PrivateFsError> {
        self.file
            .sync_all()
            .map_err(|error| PrivateFsError::io("sync", "temporary file", &error))
    }

    /// Atomically installs this temporary file at `final_path`.
    ///
    /// Both paths must resolve to the same secure directory. An existing final
    /// entry is validated and tightened before the rename. On Unix, `rename`
    /// replaces the directory entry rather than following it.
    pub fn commit(mut self, final_path: &Path) -> Result<(), PrivateFsError> {
        #[cfg(unix)]
        {
            let final_path = prepare_private_output_path(final_path)?;
            let final_parent = final_path.parent().ok_or_else(|| {
                PrivateFsError::new("commit", "output file", PrivateFsErrorKind::InvalidPath)
            })?;
            let temp_parent = self.path.parent().ok_or_else(|| {
                PrivateFsError::new("commit", "temporary file", PrivateFsErrorKind::InvalidPath)
            })?;

            if final_parent != temp_parent {
                return Err(PrivateFsError::new(
                    "commit",
                    "temporary file",
                    PrivateFsErrorKind::InvalidPath,
                ));
            }

            validate_open_file_path(
                &self.file,
                &self.path,
                self.identity,
                "commit",
                "temporary file",
            )?;
            validate_private_file_metadata(
                &self
                    .file
                    .metadata()
                    .map_err(|error| PrivateFsError::io("inspect", "temporary file", &error))?,
                current_euid(),
                true,
                "commit",
                "temporary file",
            )?;

            std::fs::rename(&self.path, &final_path)
                .map_err(|error| PrivateFsError::io("atomically replace", "output file", &error))?;
            self.cleanup_armed = false;

            let installed = std::fs::symlink_metadata(&final_path)
                .map_err(|error| PrivateFsError::io("verify", "output file", &error))?;
            validate_private_file_metadata(
                &installed,
                current_euid(),
                true,
                "verify",
                "output file",
            )?;
            if FileIdentity::from_metadata(&installed) != self.identity {
                return Err(PrivateFsError::new(
                    "verify",
                    "output file",
                    PrivateFsErrorKind::PathChanged,
                ));
            }

            sync_directory(final_parent)?;
            Ok(())
        }

        #[cfg(not(unix))]
        {
            let _ = final_path;
            Err(PrivateFsError::new(
                "commit",
                "output file",
                PrivateFsErrorKind::UnsupportedPlatform,
            ))
        }
    }
}

impl Drop for PrivateTempFile {
    fn drop(&mut self) {
        if !self.cleanup_armed {
            return;
        }

        #[cfg(unix)]
        {
            let _ = remove_file_if_identity_matches(&self.path, self.identity);
        }
    }
}

/// Resolves a path lexically to an absolute path without following symlinks.
pub fn resolve_private_path(path: &Path) -> Result<PathBuf, PrivateFsError> {
    #[cfg(unix)]
    {
        normalize_absolute_path(path)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Err(PrivateFsError::new(
            "resolve",
            "path",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

/// Creates or tightens an application-private directory.
///
/// Existing permission bits outside `0700` are removed. If any owner bit is
/// missing, the function returns an error instead of granting it.
pub fn ensure_private_directory(path: &Path) -> Result<PathBuf, PrivateFsError> {
    #[cfg(unix)]
    {
        let path = normalize_absolute_path(path)?;
        if path.parent().is_none() || path == Path::new("/") {
            return Err(PrivateFsError::new(
                "prepare",
                "directory",
                PrivateFsErrorKind::InvalidPath,
            ));
        }

        let parent = path.parent().ok_or_else(|| {
            PrivateFsError::new("prepare", "directory", PrivateFsErrorKind::InvalidPath)
        })?;
        ensure_secure_directory_tree(parent)?;

        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                validate_directory_metadata(
                    &metadata,
                    current_euid(),
                    false,
                    "validate",
                    "directory",
                )?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                create_directory_private(&path)?;
            }
            Err(error) => {
                return Err(PrivateFsError::io("inspect", "directory", &error));
            }
        }

        harden_directory_exact(&path)?;
        Ok(path)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Err(PrivateFsError::new(
            "prepare",
            "directory",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

/// Validates a final private output path and creates missing parent
/// directories as `0700`.
///
/// Existing parent directories are not otherwise modified, but the immediate
/// parent must be owned by the current user, have owner `rwx`, and not be
/// writable by group or other users. An existing output file is tightened to
/// `0600`.
pub fn prepare_private_output_path(path: &Path) -> Result<PathBuf, PrivateFsError> {
    #[cfg(unix)]
    {
        let path = normalize_absolute_path(path)?;
        if path.file_name().is_none() {
            return Err(PrivateFsError::new(
                "prepare",
                "output path",
                PrivateFsErrorKind::InvalidPath,
            ));
        }
        let parent = path.parent().ok_or_else(|| {
            PrivateFsError::new("prepare", "output path", PrivateFsErrorKind::InvalidPath)
        })?;

        ensure_secure_directory_tree(parent)?;
        harden_private_file_if_exists(&path)?;
        Ok(path)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Err(PrivateFsError::new(
            "prepare",
            "output path",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

/// Tightens an existing regular file to `0600`.
///
/// Missing files are accepted. Existing files must already have owner read and
/// write permission; the function never adds either bit.
pub fn harden_private_file_if_exists(path: &Path) -> Result<(), PrivateFsError> {
    #[cfg(unix)]
    {
        let path = normalize_absolute_path(path)?;
        let before = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(PrivateFsError::io("inspect", "file", &error)),
        };

        validate_private_file_metadata(&before, current_euid(), false, "validate", "file")?;
        require_mode(&before, PRIVATE_FILE_MODE, "tighten", "file")?;

        let file = open_existing_no_follow(&path, false, "file")?;
        let opened = file
            .metadata()
            .map_err(|error| PrivateFsError::io("inspect", "file", &error))?;
        validate_private_file_metadata(&opened, current_euid(), false, "validate", "file")?;

        let identity = FileIdentity::from_metadata(&opened);
        validate_path_identity(&path, identity, "validate", "file")?;

        if permission_mode(&opened) != PRIVATE_FILE_MODE {
            file.set_permissions(Permissions::from_mode(PRIVATE_FILE_MODE))
                .map_err(|error| PrivateFsError::io("tighten permissions on", "file", &error))?;
        }

        let hardened = file
            .metadata()
            .map_err(|error| PrivateFsError::io("verify", "file", &error))?;
        validate_private_file_metadata(&hardened, current_euid(), true, "verify", "file")?;
        validate_path_identity(&path, identity, "verify", "file")
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Err(PrivateFsError::new(
            "tighten",
            "file",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

/// Pre-creates a SQLite database privately and tightens any existing database,
/// rollback journal, WAL, and shared-memory files.
pub fn prepare_sqlite_database(path: &Path) -> Result<PathBuf, PrivateFsError> {
    #[cfg(unix)]
    {
        let path = normalize_absolute_path(path)?;
        let parent = path.parent().ok_or_else(|| {
            PrivateFsError::new("prepare", "database path", PrivateFsErrorKind::InvalidPath)
        })?;
        ensure_private_directory(parent)?;

        if !path_exists_no_follow(&path)? {
            let mut file = create_private_file_at(&path, "database file")?;
            // This is the final database inode, not a disposable mirror temp.
            file.cleanup_armed = false;
            drop(file);
        }

        harden_sqlite_files(&path)?;
        Ok(path)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Err(PrivateFsError::new(
            "prepare",
            "database path",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

/// Tightens SQLite's main file and any sidecars that currently exist.
///
/// Call this after opening the connection and after journal-mode transitions
/// that may create `-wal`, `-shm`, or `-journal`.
pub fn harden_sqlite_files(database_path: &Path) -> Result<(), PrivateFsError> {
    #[cfg(unix)]
    {
        let database_path = normalize_absolute_path(database_path)?;
        harden_private_file_if_exists(&database_path)?;
        for suffix in ["-wal", "-shm", "-journal"] {
            harden_private_file_if_exists(&sqlite_sidecar_path(&database_path, suffix))?;
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = database_path;
        Err(PrivateFsError::new(
            "tighten",
            "SQLite files",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

/// Creates a same-directory, exclusively-created `0600` temporary file.
///
/// The name is deterministic enough to diagnose but collision-safe through
/// `create_new`; no existing entry is ever opened or truncated.
pub fn create_private_temp_file(
    final_path: &Path,
    generation: u64,
) -> Result<PrivateTempFile, PrivateFsError> {
    #[cfg(unix)]
    {
        let final_path = prepare_private_output_path(final_path)?;
        let parent = final_path.parent().ok_or_else(|| {
            PrivateFsError::new("create", "temporary file", PrivateFsErrorKind::InvalidPath)
        })?;
        let file_name = final_path.file_name().ok_or_else(|| {
            PrivateFsError::new("create", "temporary file", PrivateFsErrorKind::InvalidPath)
        })?;

        for attempt in 0..32_u32 {
            let mut temp_name = OsString::from(".");
            temp_name.push(file_name);
            temp_name.push(format!(
                ".copy-stack-jsonl.{}.{}.{}.tmp",
                std::process::id(),
                generation,
                attempt
            ));
            let temp_path = parent.join(temp_name);

            match create_private_file_at(&temp_path, "temporary file") {
                Ok(file) => return Ok(file),
                Err(error) if error.kind() == PrivateFsErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(PrivateFsError::new(
            "create",
            "temporary file",
            PrivateFsErrorKind::AlreadyExists,
        ))
    }

    #[cfg(not(unix))]
    {
        let _ = (final_path, generation);
        Err(PrivateFsError::new(
            "create",
            "temporary file",
            PrivateFsErrorKind::UnsupportedPlatform,
        ))
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl FileIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnixObjectKind {
    Directory,
    RegularFile,
    Symlink,
    Other,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnixObjectFacts {
    kind: UnixObjectKind,
    uid: u32,
    mode: u32,
    hard_links: u64,
}

#[cfg(unix)]
impl UnixObjectFacts {
    fn from_metadata(metadata: &Metadata) -> Self {
        let file_type = metadata.file_type();
        let kind = if file_type.is_symlink() {
            UnixObjectKind::Symlink
        } else if file_type.is_dir() {
            UnixObjectKind::Directory
        } else if file_type.is_file() {
            UnixObjectKind::RegularFile
        } else {
            UnixObjectKind::Other
        };

        Self {
            kind,
            uid: metadata.uid(),
            mode: metadata.mode() & 0o7777,
            hard_links: metadata.nlink(),
        }
    }
}

#[cfg(unix)]
fn validate_directory_facts(
    facts: UnixObjectFacts,
    expected_uid: u32,
    secure_parent: bool,
) -> Result<(), PrivateFsErrorKind> {
    match facts.kind {
        UnixObjectKind::Symlink => return Err(PrivateFsErrorKind::Symlink),
        UnixObjectKind::Directory => {}
        _ => return Err(PrivateFsErrorKind::NotDirectory),
    }
    if facts.uid != expected_uid {
        return Err(PrivateFsErrorKind::WrongOwner);
    }
    if secure_parent && facts.mode & 0o022 != 0 {
        return Err(PrivateFsErrorKind::InsecureDirectory);
    }
    if facts.mode & PRIVATE_DIRECTORY_MODE != PRIVATE_DIRECTORY_MODE {
        return Err(PrivateFsErrorKind::InsufficientPermissions);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_private_file_facts(
    facts: UnixObjectFacts,
    expected_uid: u32,
    exact_mode: bool,
) -> Result<(), PrivateFsErrorKind> {
    match facts.kind {
        UnixObjectKind::Symlink => return Err(PrivateFsErrorKind::Symlink),
        UnixObjectKind::RegularFile => {}
        _ => return Err(PrivateFsErrorKind::NotRegularFile),
    }
    if facts.uid != expected_uid {
        return Err(PrivateFsErrorKind::WrongOwner);
    }
    if facts.hard_links != 1 {
        return Err(PrivateFsErrorKind::MultipleHardLinks);
    }
    if facts.mode & PRIVATE_FILE_MODE != PRIVATE_FILE_MODE {
        return Err(PrivateFsErrorKind::InsufficientPermissions);
    }
    if exact_mode && facts.mode != PRIVATE_FILE_MODE {
        return Err(PrivateFsErrorKind::InsufficientPermissions);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_directory_metadata(
    metadata: &Metadata,
    expected_uid: u32,
    secure_parent: bool,
    operation: &'static str,
    subject: &'static str,
) -> Result<(), PrivateFsError> {
    validate_directory_facts(
        UnixObjectFacts::from_metadata(metadata),
        expected_uid,
        secure_parent,
    )
    .map_err(|kind| PrivateFsError::new(operation, subject, kind))
}

#[cfg(unix)]
fn validate_private_file_metadata(
    metadata: &Metadata,
    expected_uid: u32,
    exact_mode: bool,
    operation: &'static str,
    subject: &'static str,
) -> Result<(), PrivateFsError> {
    validate_private_file_facts(
        UnixObjectFacts::from_metadata(metadata),
        expected_uid,
        exact_mode,
    )
    .map_err(|kind| PrivateFsError::new(operation, subject, kind))
}

#[cfg(unix)]
fn normalize_absolute_path(path: &Path) -> Result<PathBuf, PrivateFsError> {
    if path.as_os_str().is_empty() {
        return Err(PrivateFsError::new(
            "resolve",
            "path",
            PrivateFsErrorKind::InvalidPath,
        ));
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| PrivateFsError::io("resolve", "path", &error))?
            .join(path)
    };

    let mut normalized = PathBuf::from("/");
    for component in absolute.components() {
        match component {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized != Path::new("/") {
                    normalized.pop();
                }
            }
            Component::Normal(component) => normalized.push(component),
            Component::Prefix(_) => {
                return Err(PrivateFsError::new(
                    "resolve",
                    "path",
                    PrivateFsErrorKind::InvalidPath,
                ));
            }
        }
    }
    Ok(normalized)
}

#[cfg(unix)]
fn ensure_secure_directory_tree(path: &Path) -> Result<(), PrivateFsError> {
    let path = normalize_absolute_path(path)?;
    let expected_uid = current_euid();
    let mut current = PathBuf::from("/");
    let mut created_any = false;

    for component in path.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        current.push(component);

        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    if trusted_system_directory_symlink(&current, &metadata) {
                        continue;
                    }
                    return Err(PrivateFsError::new(
                        "validate",
                        "directory path",
                        PrivateFsErrorKind::Symlink,
                    ));
                }
                if !metadata.is_dir() {
                    return Err(PrivateFsError::new(
                        "validate",
                        "directory path",
                        PrivateFsErrorKind::NotDirectory,
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if !created_any {
                    let parent = current.parent().ok_or_else(|| {
                        PrivateFsError::new("create", "directory", PrivateFsErrorKind::InvalidPath)
                    })?;
                    validate_secure_parent(parent, expected_uid)?;
                }
                create_directory_private(&current)?;
                created_any = true;
            }
            Err(error) => {
                return Err(PrivateFsError::io("inspect", "directory path", &error));
            }
        }
    }

    validate_secure_parent(&path, expected_uid)
}

#[cfg(unix)]
fn trusted_system_directory_symlink(path: &Path, metadata: &Metadata) -> bool {
    if metadata.uid() != 0 {
        return false;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(parent_metadata) = std::fs::symlink_metadata(parent) else {
        return false;
    };
    if parent_metadata.file_type().is_symlink()
        || !parent_metadata.is_dir()
        || parent_metadata.uid() != 0
        || permission_mode(&parent_metadata) & 0o022 != 0
    {
        return false;
    }

    std::fs::metadata(path)
        .map(|target| target.is_dir() && target.uid() == 0)
        .unwrap_or(false)
}

#[cfg(unix)]
fn validate_secure_parent(path: &Path, expected_uid: u32) -> Result<(), PrivateFsError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| PrivateFsError::io("inspect", "parent directory", &error))?;
    validate_directory_metadata(
        &metadata,
        expected_uid,
        true,
        "validate",
        "parent directory",
    )
}

#[cfg(unix)]
fn create_directory_private(path: &Path) -> Result<(), PrivateFsError> {
    let mut builder = DirBuilder::new();
    builder.mode(PRIVATE_DIRECTORY_MODE);
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(PrivateFsError::io("create", "directory", &error)),
    }
    harden_directory_exact(path)
}

#[cfg(unix)]
fn harden_directory_exact(path: &Path) -> Result<(), PrivateFsError> {
    let before = std::fs::symlink_metadata(path)
        .map_err(|error| PrivateFsError::io("inspect", "directory", &error))?;
    validate_directory_metadata(&before, current_euid(), false, "validate", "directory")?;
    require_mode(&before, PRIVATE_DIRECTORY_MODE, "tighten", "directory")?;

    let directory = open_existing_no_follow(path, true, "directory")?;
    let opened = directory
        .metadata()
        .map_err(|error| PrivateFsError::io("inspect", "directory", &error))?;
    validate_directory_metadata(&opened, current_euid(), false, "validate", "directory")?;
    let identity = FileIdentity::from_metadata(&opened);
    validate_path_identity(path, identity, "validate", "directory")?;

    if permission_mode(&opened) != PRIVATE_DIRECTORY_MODE {
        directory
            .set_permissions(Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|error| PrivateFsError::io("tighten permissions on", "directory", &error))?;
    }

    let hardened = directory
        .metadata()
        .map_err(|error| PrivateFsError::io("verify", "directory", &error))?;
    validate_directory_metadata(&hardened, current_euid(), true, "verify", "directory")?;
    if permission_mode(&hardened) != PRIVATE_DIRECTORY_MODE {
        return Err(PrivateFsError::new(
            "verify",
            "directory",
            PrivateFsErrorKind::InsufficientPermissions,
        ));
    }
    validate_path_identity(path, identity, "verify", "directory")
}

#[cfg(unix)]
fn require_mode(
    metadata: &Metadata,
    required: u32,
    operation: &'static str,
    subject: &'static str,
) -> Result<(), PrivateFsError> {
    if permission_mode(metadata) & required == required {
        Ok(())
    } else {
        Err(PrivateFsError::new(
            operation,
            subject,
            PrivateFsErrorKind::InsufficientPermissions,
        ))
    }
}

#[cfg(unix)]
fn permission_mode(metadata: &Metadata) -> u32 {
    metadata.mode() & 0o7777
}

#[cfg(unix)]
fn path_exists_no_follow(path: &Path) -> Result<bool, PrivateFsError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(PrivateFsError::io("inspect", "path", &error)),
    }
}

#[cfg(unix)]
fn create_private_file_at(
    path: &Path,
    subject: &'static str,
) -> Result<PrivateTempFile, PrivateFsError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE);
    add_no_follow_flag(&mut options);

    let file = options
        .open(path)
        .map_err(|error| PrivateFsError::io("create", subject, &error))?;
    file.set_permissions(Permissions::from_mode(PRIVATE_FILE_MODE))
        .map_err(|error| PrivateFsError::io("set permissions on", subject, &error))?;

    let metadata = file
        .metadata()
        .map_err(|error| PrivateFsError::io("inspect", subject, &error))?;
    validate_private_file_metadata(&metadata, current_euid(), true, "validate", subject)?;
    let identity = FileIdentity::from_metadata(&metadata);
    validate_path_identity(path, identity, "validate", subject)?;

    Ok(PrivateTempFile {
        file,
        path: path.to_path_buf(),
        identity,
        cleanup_armed: true,
    })
}

#[cfg(unix)]
fn open_existing_no_follow(
    path: &Path,
    directory: bool,
    subject: &'static str,
) -> Result<File, PrivateFsError> {
    let mut options = OpenOptions::new();
    options.read(true);
    if !directory {
        options.write(true);
    }
    add_no_follow_flag(&mut options);
    options
        .open(path)
        .map_err(|error| PrivateFsError::io("open", subject, &error))
}

#[cfg(unix)]
fn validate_open_file_path(
    file: &File,
    path: &Path,
    expected: FileIdentity,
    operation: &'static str,
    subject: &'static str,
) -> Result<(), PrivateFsError> {
    let opened = file
        .metadata()
        .map_err(|error| PrivateFsError::io(operation, subject, &error))?;
    if FileIdentity::from_metadata(&opened) != expected {
        return Err(PrivateFsError::new(
            operation,
            subject,
            PrivateFsErrorKind::PathChanged,
        ));
    }
    validate_path_identity(path, expected, operation, subject)
}

#[cfg(unix)]
fn validate_path_identity(
    path: &Path,
    expected: FileIdentity,
    operation: &'static str,
    subject: &'static str,
) -> Result<(), PrivateFsError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| PrivateFsError::io(operation, subject, &error))?;
    if metadata.file_type().is_symlink() {
        return Err(PrivateFsError::new(
            operation,
            subject,
            PrivateFsErrorKind::Symlink,
        ));
    }
    if FileIdentity::from_metadata(&metadata) != expected {
        return Err(PrivateFsError::new(
            operation,
            subject,
            PrivateFsErrorKind::PathChanged,
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn remove_file_if_identity_matches(
    path: &Path,
    expected: FileIdentity,
) -> Result<(), PrivateFsError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(PrivateFsError::io("clean up", "temporary file", &error));
        }
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || FileIdentity::from_metadata(&metadata) != expected
    {
        return Err(PrivateFsError::new(
            "clean up",
            "temporary file",
            PrivateFsErrorKind::PathChanged,
        ));
    }
    std::fs::remove_file(path)
        .map_err(|error| PrivateFsError::io("clean up", "temporary file", &error))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), PrivateFsError> {
    let directory = open_existing_no_follow(path, true, "parent directory")?;
    directory
        .sync_all()
        .map_err(|error| PrivateFsError::io("sync", "parent directory", &error))
}

#[cfg(unix)]
fn sqlite_sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut value = database_path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(unix)]
fn current_euid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    // SAFETY: `geteuid` has no preconditions and does not dereference memory.
    unsafe { geteuid() }
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
const O_NOFOLLOW_FLAG: i32 = 0x0002_0000;
#[cfg(all(
    unix,
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )
))]
const O_NOFOLLOW_FLAG: i32 = 0x0000_0100;
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))
))]
const O_NOFOLLOW_FLAG: i32 = 0;

#[cfg(unix)]
fn add_no_follow_flag(options: &mut OpenOptions) {
    if O_NOFOLLOW_FLAG != 0 {
        options.custom_flags(O_NOFOLLOW_FLAG);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::io::{Read, Seek, Write};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "copy-stack-private-fs-test-{}-{}-{}",
                std::process::id(),
                sequence,
                label
            ));
            std::fs::create_dir(&path).expect("test directory should be created");
            std::fs::set_permissions(
                &path,
                std::fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE),
            )
            .expect("test directory permissions should be private");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn mode(path: &Path) -> u32 {
        std::fs::symlink_metadata(path)
            .expect("metadata should exist")
            .mode()
            & 0o7777
    }

    #[test]
    fn creates_and_tightens_private_directories() {
        let root = TestDirectory::new("directories");
        let nested = root.path.join("nested").join("data");

        ensure_private_directory(&nested).expect("private directory should be prepared");
        assert_eq!(mode(&root.path.join("nested")), PRIVATE_DIRECTORY_MODE);
        assert_eq!(mode(&nested), PRIVATE_DIRECTORY_MODE);

        std::fs::set_permissions(&nested, Permissions::from_mode(0o755))
            .expect("test permissions should change");
        ensure_private_directory(&nested).expect("loose permissions should be tightened");
        assert_eq!(mode(&nested), PRIVATE_DIRECTORY_MODE);
    }

    #[test]
    fn missing_owner_permissions_are_not_granted() {
        let root = TestDirectory::new("no-expand");
        let directory = root.path.join("data");
        std::fs::create_dir(&directory).expect("directory should be created");
        std::fs::set_permissions(&directory, Permissions::from_mode(0o500))
            .expect("test permissions should change");

        let error =
            ensure_private_directory(&directory).expect_err("missing write must be rejected");
        assert_eq!(error.kind(), PrivateFsErrorKind::InsufficientPermissions);
        assert_eq!(mode(&directory), 0o500);

        let file = root.path.join("history.jsonl");
        std::fs::write(&file, b"old").expect("file should be created");
        std::fs::set_permissions(&file, Permissions::from_mode(0o400))
            .expect("test permissions should change");
        let error =
            harden_private_file_if_exists(&file).expect_err("missing owner write must be rejected");
        assert_eq!(error.kind(), PrivateFsErrorKind::InsufficientPermissions);
        assert_eq!(mode(&file), 0o400);
    }

    #[test]
    fn tightens_existing_private_files_and_rejects_hard_links() {
        let root = TestDirectory::new("files");
        let file = root.path.join("copy_stack.db");
        std::fs::write(&file, b"database").expect("file should be created");
        std::fs::set_permissions(&file, Permissions::from_mode(0o666))
            .expect("test permissions should change");

        harden_private_file_if_exists(&file).expect("file should be tightened");
        assert_eq!(mode(&file), PRIVATE_FILE_MODE);

        let linked = root.path.join("linked.db");
        std::fs::hard_link(&file, &linked).expect("hard link should be created");
        let error = harden_private_file_if_exists(&file).expect_err("hard links must be rejected");
        assert_eq!(error.kind(), PrivateFsErrorKind::MultipleHardLinks);
    }

    #[test]
    fn rejects_symlinked_components_and_does_not_touch_the_target() {
        let root = TestDirectory::new("symlinks");
        let real = root.path.join("real");
        std::fs::create_dir(&real).expect("real directory should be created");
        std::fs::set_permissions(&real, Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .expect("real directory should be private");
        let linked = root.path.join("linked");
        symlink(&real, &linked).expect("symlink should be created");

        let error = prepare_private_output_path(&linked.join("history.jsonl"))
            .expect_err("symlinked parents must be rejected");
        assert_eq!(error.kind(), PrivateFsErrorKind::Symlink);

        let victim = root.path.join("victim");
        std::fs::write(&victim, b"unchanged").expect("victim should be created");
        std::fs::set_permissions(&victim, Permissions::from_mode(PRIVATE_FILE_MODE))
            .expect("victim should be private");
        let output = root.path.join("history.jsonl");
        symlink(&victim, &output).expect("output symlink should be created");
        let error =
            prepare_private_output_path(&output).expect_err("output symlink must be rejected");
        assert_eq!(error.kind(), PrivateFsErrorKind::Symlink);
        assert_eq!(
            std::fs::read(&victim).expect("victim should remain readable"),
            b"unchanged"
        );
    }

    #[test]
    fn private_temp_commit_is_atomic_and_cleans_up_on_rejection() {
        let root = TestDirectory::new("commit");
        let output = root.path.join("history.jsonl");
        std::fs::write(&output, b"old complete\n").expect("old output should be created");
        std::fs::set_permissions(&output, Permissions::from_mode(PRIVATE_FILE_MODE))
            .expect("old output should be private");

        let mut temp =
            create_private_temp_file(&output, 1).expect("temporary file should be private");
        assert_eq!(
            temp.file_mut()
                .metadata()
                .expect("temporary metadata should exist")
                .mode()
                & 0o7777,
            PRIVATE_FILE_MODE
        );
        temp.file_mut()
            .write_all(b"new complete\n")
            .expect("temporary content should write");
        temp.sync_all().expect("temporary file should sync");
        temp.commit(&output)
            .expect("temporary file should commit atomically");
        assert_eq!(
            std::fs::read(&output).expect("new output should be readable"),
            b"new complete\n"
        );
        assert_eq!(mode(&output), PRIVATE_FILE_MODE);

        let victim = root.path.join("victim");
        std::fs::write(&victim, b"victim").expect("victim should be created");
        std::fs::set_permissions(&victim, Permissions::from_mode(PRIVATE_FILE_MODE))
            .expect("victim should be private");
        std::fs::remove_file(&output).expect("output should be removed");
        symlink(&victim, &output).expect("unsafe final symlink should be created");

        let before_entries = std::fs::read_dir(&root.path)
            .expect("directory should list")
            .count();
        let error = create_private_temp_file(&output, 2)
            .expect_err("unsafe final path must fail before temp creation");
        assert_eq!(error.kind(), PrivateFsErrorKind::Symlink);
        let after_entries = std::fs::read_dir(&root.path)
            .expect("directory should list")
            .count();
        assert_eq!(before_entries, after_entries);
        assert_eq!(
            std::fs::read(&victim).expect("victim should remain readable"),
            b"victim"
        );
    }

    #[test]
    fn sqlite_preparation_covers_sqlite_created_journal_wal_and_shm() {
        let root = TestDirectory::new("sqlite");
        let data = root.path.join("data");
        std::fs::create_dir(&data).expect("data directory should be created");
        std::fs::set_permissions(&data, Permissions::from_mode(0o755))
            .expect("data directory permissions should be loose");
        let database = data.join("copy_stack.db");

        prepare_sqlite_database(&database).expect("database should be prepared");
        assert_eq!(mode(&data), PRIVATE_DIRECTORY_MODE);
        assert_eq!(mode(&database), PRIVATE_FILE_MODE);

        let connection = Connection::open(&database).expect("SQLite database should open");
        connection
            .execute_batch("CREATE TABLE private_sidecar_test (value INTEGER NOT NULL);")
            .expect("SQLite schema should initialize");
        {
            let transaction = connection
                .unchecked_transaction()
                .expect("rollback-journal transaction should start");
            transaction
                .execute("INSERT INTO private_sidecar_test (value) VALUES (1)", [])
                .expect("rollback-journal row should write");
            let journal = sqlite_sidecar_path(&database, "-journal");
            assert!(journal.exists(), "SQLite should create a rollback journal");
            assert_eq!(mode(&journal), PRIVATE_FILE_MODE);

            std::fs::set_permissions(&journal, Permissions::from_mode(0o644))
                .expect("journal permissions should be made loose for repair");
            harden_sqlite_files(&database).expect("live rollback journal should be tightened");
            assert_eq!(mode(&journal), PRIVATE_FILE_MODE);
            transaction
                .rollback()
                .expect("rollback-journal transaction should roll back");
        }

        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .expect("SQLite should enable WAL mode");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        connection
            .execute_batch(
                "PRAGMA wal_autocheckpoint = 0;
                 INSERT INTO private_sidecar_test (value) VALUES (2);",
            )
            .expect("WAL row should write");

        for suffix in ["-wal", "-shm"] {
            let sidecar = sqlite_sidecar_path(&database, suffix);
            assert!(sidecar.exists(), "SQLite should create {suffix}");
            assert_eq!(
                mode(&sidecar),
                PRIVATE_FILE_MODE,
                "SQLite-created {suffix} should inherit private permissions"
            );
            std::fs::set_permissions(&sidecar, Permissions::from_mode(0o644))
                .expect("sidecar permissions should be made loose for repair");
        }
        harden_sqlite_files(&database).expect("live WAL sidecars should be tightened");
        for suffix in ["-wal", "-shm"] {
            assert_eq!(
                mode(&sqlite_sidecar_path(&database, suffix)),
                PRIVATE_FILE_MODE,
                "{suffix} should be private after repair"
            );
        }
    }

    #[test]
    fn synthetic_unix_facts_cover_owner_type_link_and_parent_checks() {
        let uid = 501;
        let regular = UnixObjectFacts {
            kind: UnixObjectKind::RegularFile,
            uid,
            mode: PRIVATE_FILE_MODE,
            hard_links: 1,
        };
        assert_eq!(validate_private_file_facts(regular, uid, true), Ok(()));
        assert_eq!(
            validate_private_file_facts(
                UnixObjectFacts {
                    uid: uid + 1,
                    ..regular
                },
                uid,
                true
            ),
            Err(PrivateFsErrorKind::WrongOwner)
        );
        assert_eq!(
            validate_private_file_facts(
                UnixObjectFacts {
                    hard_links: 2,
                    ..regular
                },
                uid,
                true
            ),
            Err(PrivateFsErrorKind::MultipleHardLinks)
        );
        assert_eq!(
            validate_private_file_facts(
                UnixObjectFacts {
                    kind: UnixObjectKind::Other,
                    ..regular
                },
                uid,
                true
            ),
            Err(PrivateFsErrorKind::NotRegularFile)
        );

        let public_parent = UnixObjectFacts {
            kind: UnixObjectKind::Directory,
            uid,
            mode: 0o733,
            hard_links: 2,
        };
        assert_eq!(
            validate_directory_facts(public_parent, uid, true),
            Err(PrivateFsErrorKind::InsecureDirectory)
        );
    }

    #[test]
    fn errors_are_path_redacted() {
        let sensitive_name = "clipboard-secret-history.jsonl";
        let error = PrivateFsError::new("validate", "output file", PrivateFsErrorKind::Symlink);
        let rendered = error.to_string();
        assert!(!rendered.contains(sensitive_name));
        assert!(!rendered.contains('/'));
        assert!(rendered.contains("symbolic links"));
    }

    #[test]
    fn committed_file_remains_open_and_readable_until_drop() {
        let root = TestDirectory::new("open-after-rename");
        let output = root.path.join("history.jsonl");
        let mut temp =
            create_private_temp_file(&output, 7).expect("temporary file should be created");
        temp.file_mut()
            .write_all(b"snapshot")
            .expect("snapshot should write");
        temp.file_mut()
            .flush()
            .expect("snapshot should flush through file handle");
        temp.file_mut()
            .rewind()
            .expect("temporary handle should rewind");
        let mut value = String::new();
        temp.file_mut()
            .read_to_string(&mut value)
            .expect("temporary handle should read");
        assert_eq!(value, "snapshot");
        temp.commit(&output).expect("snapshot should commit");
    }
}
