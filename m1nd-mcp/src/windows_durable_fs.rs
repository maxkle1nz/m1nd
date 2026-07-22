//! Reviewed Windows filesystem primitives used by durable owner stores.
//!
//! Windows has no documented directory-fsync equivalent. Durable publication
//! therefore uses `MoveFileExW(..., MOVEFILE_WRITE_THROUGH)` instead of
//! pretending that opening a directory and returning `Ok(())` is a barrier.

use std::ffi::OsStr;
use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path};

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    NtCreateFile, FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
    FILE_OPEN_REPARSE_POINT as NT_FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
};
use windows_sys::Win32::Foundation::{
    RtlNtStatusToDosError, HANDLE, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, LockFileEx, MoveFileExW, UnlockFileEx, BY_HANDLE_FILE_INFORMATION,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA,
    LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::{IO_STATUS_BLOCK, OVERLAPPED};

const SHARE_WITHOUT_DELETE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;

pub(crate) fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn validate_opened_target(file: File, path: &Path) -> io::Result<File> {
    if is_reparse_point(&file.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Windows reparse point refused: {}", path.display()),
        ));
    }
    Ok(file)
}

pub(crate) fn open_create_new_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    validate_opened_target(options.open(path)?, path)
}

pub(crate) fn open_read_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    validate_opened_target(options.open(path)?, path)
}

pub(crate) fn open_write_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    validate_opened_target(options.open(path)?, path)
}

/// Durably truncates a torn journal tail on Windows.
///
/// A journal opened with [`open_read_append_create_no_follow`] carries only
/// `FILE_APPEND_DATA` (Rust drops `FILE_WRITE_DATA` for append handles), so
/// `File::set_len` on that handle is refused with `ERROR_ACCESS_DENIED`.
/// Recovery therefore truncates through a dedicated no-follow write handle,
/// which does hold `FILE_WRITE_DATA`, exactly as `evidence_spine` already does
/// for its own tail repair. The append handle keeps writing at end-of-file, so
/// the next record still lands immediately after the truncation point.
pub(crate) fn truncate_no_follow(path: &Path, len: u64) -> io::Result<()> {
    let file = open_write_no_follow(path)?;
    file.set_len(len)?;
    file.sync_all()
}

pub(crate) fn open_read_append_create_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .append(true)
        .create(true)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    validate_opened_target(options.open(path)?, path)
}

pub(crate) fn open_lock_file_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    validate_opened_target(options.open(path)?, path)
}

pub(crate) fn open_directory_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let file = validate_opened_target(options.open(path)?, path)?;
    if !file.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Windows directory handle target is not a directory: {}",
                path.display()
            ),
        ));
    }
    Ok(file)
}

/// Opens one child component relative to an already-open directory handle.
///
/// `NtCreateFile`'s `RootDirectory` binding is the Windows equivalent needed
/// here for `openat`: every component is resolved from the held parent handle,
/// and `FILE_OPEN_REPARSE_POINT` makes reparse refusal a property of the open
/// itself rather than a metadata-then-open check.
pub(crate) fn open_relative_directory_no_follow(
    parent: &File,
    component: &OsStr,
) -> io::Result<File> {
    let file = nt_open_relative(
        parent,
        component,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_OPEN,
        FILE_DIRECTORY_FILE | NT_FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_ATTRIBUTE_DIRECTORY,
        SHARE_WITHOUT_DELETE,
    )?;
    if !file.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "anchored Windows child is not a directory",
        ));
    }
    Ok(file)
}

pub(crate) fn create_relative_directory_no_follow(
    parent: &File,
    component: &OsStr,
) -> io::Result<File> {
    let file = nt_open_relative(
        parent,
        component,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_CREATE,
        FILE_DIRECTORY_FILE | NT_FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_ATTRIBUTE_DIRECTORY,
        SHARE_WITHOUT_DELETE,
    )?;
    if !file.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "new anchored Windows child is not a directory",
        ));
    }
    Ok(file)
}

pub(crate) fn open_relative_read_no_follow(parent: &File, component: &OsStr) -> io::Result<File> {
    nt_open_relative(
        parent,
        component,
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE | NT_FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_ATTRIBUTE_NORMAL,
        FILE_SHARE_READ,
    )
}

pub(crate) fn create_relative_new_no_follow(parent: &File, component: &OsStr) -> io::Result<File> {
    nt_open_relative(
        parent,
        component,
        FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE | NT_FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_ATTRIBUTE_NORMAL,
        FILE_SHARE_READ,
    )
}

fn nt_open_relative(
    parent: &File,
    component: &OsStr,
    desired_access: u32,
    create_disposition: u32,
    create_options: u32,
    file_attributes: u32,
    share_access: u32,
) -> io::Result<File> {
    let mut encoded = validate_relative_component(component)?;
    let byte_len = encoded
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows relative component exceeds UNICODE_STRING limits",
            )
        })?;
    let name = UNICODE_STRING {
        Length: byte_len,
        MaximumLength: byte_len,
        Buffer: encoded.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(std::mem::size_of::<OBJECT_ATTRIBUTES>())
            .expect("OBJECT_ATTRIBUTES size fits u32"),
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: std::ptr::addr_of!(name),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle = INVALID_HANDLE_VALUE;
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            std::ptr::addr_of_mut!(handle),
            desired_access,
            std::ptr::addr_of!(attributes),
            std::ptr::addr_of_mut!(io_status),
            std::ptr::null(),
            file_attributes,
            share_access,
            create_disposition,
            create_options,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ));
    }
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(io::Error::other(
            "NtCreateFile succeeded without returning a valid handle",
        ));
    }
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    validate_opened_target(file, Path::new(component))
}

fn validate_relative_component(component: &OsStr) -> io::Result<Vec<u16>> {
    let path = Path::new(component);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(value)) if value == component)
        || components.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Windows anchored open requires exactly one relative component: {}",
                path.display()
            ),
        ));
    }
    let encoded = component.encode_wide().collect::<Vec<_>>();
    if encoded.is_empty()
        || encoded.iter().any(|&value| {
            value == 0
                || value == u16::from(b'/')
                || value == u16::from(b'\\')
                || value == u16::from(b':')
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unsafe Windows relative component refused: {}",
                path.display()
            ),
        ));
    }
    Ok(encoded)
}

/// Returns the same stable volume/file identity exposed by Windows' by-handle
/// metadata: `(dwVolumeSerialNumber, nFileIndexHigh:nFileIndexLow)`.
///
/// Rust's `std::os::windows::fs::MetadataExt::{volume_serial_number,file_index}`
/// remain unstable on stable Rust, so this calls the underlying stable Win32
/// primitive directly after a no-follow open.
pub(crate) fn handle_identity(file: &File) -> io::Result<(u64, u64)> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let result = unsafe {
        GetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            std::ptr::addr_of_mut!(information),
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows reparse-point handle identity refused",
        ));
    }
    let file_index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    Ok((u64::from(information.dwVolumeSerialNumber), file_index))
}

pub(crate) fn directory_identity(file: &File) -> io::Result<(u64, u64)> {
    handle_identity(file)
}

pub(crate) fn lock_file_exclusive(file: &File, fail_immediately: bool) -> io::Result<()> {
    let mut overlapped = OVERLAPPED::default();
    let mut flags = LOCKFILE_EXCLUSIVE_LOCK;
    if fail_immediately {
        flags |= LOCKFILE_FAIL_IMMEDIATELY;
    }
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            flags,
            0,
            u32::MAX,
            u32::MAX,
            std::ptr::addr_of_mut!(overlapped),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn unlock_file(file: &File) -> io::Result<()> {
    let mut overlapped = OVERLAPPED::default();
    let result = unsafe {
        UnlockFileEx(
            file.as_raw_handle() as HANDLE,
            0,
            u32::MAX,
            u32::MAX,
            std::ptr::addr_of_mut!(overlapped),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn move_new_write_through(source: &Path, destination: &Path) -> io::Result<()> {
    move_write_through(source, destination, false)
}

pub(crate) fn replace_write_through(source: &Path, destination: &Path) -> io::Result<()> {
    move_write_through(source, destination, true)
}

fn move_write_through(source: &Path, destination: &Path, replace: bool) -> io::Result<()> {
    let source = wide_path(source)?;
    let destination = wide_path(destination)?;
    let mut flags = MOVEFILE_WRITE_THROUGH;
    if replace {
        flags |= MOVEFILE_REPLACE_EXISTING;
    }
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Windows path contains an interior NUL: {}", path.display()),
        ));
    }
    encoded.push(0);
    Ok(encoded)
}
