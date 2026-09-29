//! Reviewed Windows filesystem primitives used by durable owner stores.
//!
//! Windows has no documented directory-fsync equivalent. Durable publication
//! therefore uses `MoveFileExW(..., MOVEFILE_WRITE_THROUGH)` instead of
//! pretending that opening a directory and returning `Ok(())` is a barrier.

use std::ffi::{c_void, OsStr};
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
    CloseHandle, LocalFree, RtlNtStatusToDosError, ERROR_SUCCESS, HANDLE, INVALID_HANDLE_VALUE,
    OBJ_CASE_INSENSITIVE, UNICODE_STRING,
};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    AclSizeInformation, CreateWellKnownSid, EqualSid, GetAce, GetAclInformation, GetLengthSid,
    GetTokenInformation, IsValidAcl, IsValidSecurityDescriptor, IsValidSid, LookupAccountNameW,
    MapGenericMask, TokenOwner, TokenUser, WinBuiltinAdministratorsSid, WinCreatorGroupSid,
    WinCreatorOwnerRightsSid, WinCreatorOwnerSid, WinLocalSystemSid, ACCESS_ALLOWED_ACE,
    ACE_HEADER, ACL, ACL_SIZE_INFORMATION, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION,
    GENERIC_MAPPING, INHERITED_ACE, INHERIT_ONLY_ACE, NO_PROPAGATE_INHERIT_ACE, OBJECT_INHERIT_ACE,
    OWNER_SECURITY_INFORMATION, PSID, TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, LockFileEx, MoveFileExW, UnlockFileEx, BY_HANDLE_FILE_INFORMATION,
    DELETE, FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
    WRITE_OWNER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::IO::{IO_STATUS_BLOCK, OVERLAPPED};

const SHARE_WITHOUT_DELETE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
const SECURITY_DIRECTORY_ACCESS: u32 =
    READ_CONTROL | FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;
const MIN_SID_BYTES: usize = 8;
const DANGEROUS_FOREIGN_ANCESTOR_ACCESS: u32 =
    DELETE | FILE_DELETE_CHILD | FILE_WRITE_DATA | WRITE_DAC | WRITE_OWNER | FILE_WRITE_ATTRIBUTES;

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

/// Proves that a launcher runtime is private using the Windows object handles
/// that name it. The original path is walked before canonicalization so a
/// reparse point cannot hide behind a safe canonical destination. Every opened
/// directory remains held until the canonical leaf has the same volume/file
/// identity as the original leaf.
pub(crate) fn canonical_private_runtime(runtime: &Path) -> io::Result<std::path::PathBuf> {
    if !runtime.is_absolute() {
        return Err(security_error(
            "Windows private runtime must be an absolute path",
        ));
    }

    let original_chain = open_original_directory_chain_no_follow(runtime)?;
    let principals = RuntimePrincipals::current()?;
    for (index, directory) in original_chain.iter().enumerate() {
        inspect_directory_security(directory, &principals, index + 1 == original_chain.len())?;
    }
    let original_leaf = original_chain
        .last()
        .ok_or_else(|| security_error("Windows private runtime had no opened leaf"))?;
    let original_identity = handle_identity(original_leaf)?;

    let canonical = std::fs::canonicalize(runtime)?;
    let canonical_leaf = open_security_directory_no_follow(&canonical)?;
    if handle_identity(&canonical_leaf)? != original_identity {
        return Err(security_error(
            "Windows private runtime changed identity between original and canonical handles",
        ));
    }
    Ok(canonical)
}

/// Opens a directory for the security proof. This deliberately asks only for
/// read-control and directory inspection rights: proving a DACL never needs
/// write, owner, or DACL modification access.
fn open_security_directory_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .access_mode(SECURITY_DIRECTORY_ACCESS)
        .share_mode(SHARE_WITHOUT_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let file = validate_opened_target(options.open(path)?, path)?;
    if !file.metadata()?.is_dir() {
        return Err(security_error(
            "Windows private-runtime security handle is not a directory",
        ));
    }
    Ok(file)
}

fn open_relative_security_directory_no_follow(
    parent: &File,
    component: &OsStr,
) -> io::Result<File> {
    let file = nt_open_relative(
        parent,
        component,
        SECURITY_DIRECTORY_ACCESS,
        FILE_OPEN,
        FILE_DIRECTORY_FILE | NT_FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_ATTRIBUTE_DIRECTORY,
        SHARE_WITHOUT_DELETE,
    )?;
    if !file.metadata()?.is_dir() {
        return Err(security_error(
            "Windows private-runtime anchored component is not a directory",
        ));
    }
    Ok(file)
}

fn open_original_directory_chain_no_follow(runtime: &Path) -> io::Result<Vec<File>> {
    if runtime
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(security_error(
            "Windows private-runtime path contains a non-normal component",
        ));
    }
    let root = runtime
        .ancestors()
        .last()
        .filter(|candidate| candidate.is_absolute())
        .ok_or_else(|| security_error("Windows private-runtime path has no absolute root"))?;
    let relative = runtime.strip_prefix(root).map_err(|_| {
        security_error("Windows private-runtime path could not be anchored at its original root")
    })?;

    let mut directories = vec![open_security_directory_no_follow(root)?];
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(security_error(
                "Windows private-runtime path has an unsafe anchored component",
            ));
        };
        let parent = directories
            .last()
            .ok_or_else(|| security_error("Windows private-runtime lost its parent handle"))?;
        directories.push(open_relative_security_directory_no_follow(
            parent, component,
        )?);
    }
    Ok(directories)
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct LocalSecurityDescriptor(*mut c_void);

impl Drop for LocalSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = LocalFree(self.0);
            }
        }
    }
}

/// Owns a token or SID buffer with pointer alignment suitable for the Windows
/// structs stored in it. A `Vec<u8>` does not promise that alignment.
struct AlignedBuffer {
    words: Vec<usize>,
    byte_len: usize,
}

impl AlignedBuffer {
    fn new(byte_len: usize) -> io::Result<Self> {
        let word_len = byte_len
            .checked_add(std::mem::size_of::<usize>() - 1)
            .map(|size| size / std::mem::size_of::<usize>())
            .filter(|words| *words > 0)
            .ok_or_else(|| security_error("Windows security buffer length overflow"))?;
        Ok(Self {
            words: vec![0; word_len],
            byte_len,
        })
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.words.as_mut_ptr().cast()
    }

    fn as_ptr(&self) -> *const c_void {
        self.words.as_ptr().cast()
    }
}

struct OwnedSid(AlignedBuffer);

impl OwnedSid {
    fn as_sid(&self) -> PSID {
        self.0.as_ptr() as PSID
    }
}

struct RuntimePrincipals {
    token_owner: AlignedBuffer,
    token_user: AlignedBuffer,
    local_system: OwnedSid,
    builtin_administrators: OwnedSid,
    creator_owner: OwnedSid,
    creator_group: OwnedSid,
    owner_rights: OwnedSid,
    trusted_installer: Option<OwnedSid>,
}

impl RuntimePrincipals {
    fn current() -> io::Result<Self> {
        let mut raw_token = std::ptr::null_mut();
        let opened = unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY,
                std::ptr::addr_of_mut!(raw_token),
            )
        };
        if opened == 0 || raw_token.is_null() || raw_token == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle(raw_token);
        let token_owner = token_information(token.0, TokenOwner)?;
        let token_user = token_information(token.0, TokenUser)?;
        let principals = Self {
            token_owner,
            token_user,
            local_system: well_known_sid(WinLocalSystemSid)?,
            builtin_administrators: well_known_sid(WinBuiltinAdministratorsSid)?,
            creator_owner: well_known_sid(WinCreatorOwnerSid)?,
            creator_group: well_known_sid(WinCreatorGroupSid)?,
            owner_rights: well_known_sid(WinCreatorOwnerRightsSid)?,
            // A lookup failure deliberately removes this exception. A chain
            // that needs TrustedInstaller will then fail its exact SID check;
            // a chain provable by the ordinary principals remains usable.
            trusted_installer: trusted_installer_sid().ok(),
        };
        validate_sid(principals.token_owner_sid())?;
        validate_sid(principals.token_user_sid())?;
        Ok(principals)
    }

    fn token_owner_sid(&self) -> PSID {
        if self.token_owner.byte_len < std::mem::size_of::<TOKEN_OWNER>() {
            return std::ptr::null_mut();
        }
        unsafe { (*self.token_owner.as_ptr().cast::<TOKEN_OWNER>()).Owner }
    }

    fn token_user_sid(&self) -> PSID {
        if self.token_user.byte_len < std::mem::size_of::<TOKEN_USER>() {
            return std::ptr::null_mut();
        }
        unsafe { (*self.token_user.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    fn is_leaf_trustee(&self, sid: PSID) -> bool {
        self.is_leaf_owner(sid)
            || sid_equal(sid, self.local_system.as_sid())
            || sid_equal(sid, self.builtin_administrators.as_sid())
    }

    fn is_leaf_owner(&self, sid: PSID) -> bool {
        sid_equal(sid, self.token_owner_sid()) || sid_equal(sid, self.token_user_sid())
    }

    fn is_ancestor_trustee(&self, sid: PSID) -> bool {
        self.is_leaf_trustee(sid)
            || self
                .trusted_installer
                .as_ref()
                .is_some_and(|trusted| sid_equal(sid, trusted.as_sid()))
    }
}

fn token_information(token: HANDLE, class: i32) -> io::Result<AlignedBuffer> {
    let mut required = 0u32;
    let first = unsafe {
        GetTokenInformation(
            token,
            class,
            std::ptr::null_mut(),
            0,
            std::ptr::addr_of_mut!(required),
        )
    };
    if first != 0 || required == 0 {
        return Err(security_error(
            "Windows token information did not report a required buffer length",
        ));
    }
    let mut buffer = AlignedBuffer::new(required as usize)?;
    let read = unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr(),
            required,
            std::ptr::addr_of_mut!(required),
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn well_known_sid(kind: i32) -> io::Result<OwnedSid> {
    let mut required = 0u32;
    let first = unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(required),
        )
    };
    if first != 0 || required == 0 {
        return Err(security_error(
            "Windows well-known SID did not report a required buffer length",
        ));
    }
    let mut sid = OwnedSid(AlignedBuffer::new(required as usize)?);
    let created = unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            sid.0.as_mut_ptr() as PSID,
            std::ptr::addr_of_mut!(required),
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    validate_sid(sid.as_sid())?;
    Ok(sid)
}

fn trusted_installer_sid() -> io::Result<OwnedSid> {
    let account = "NT SERVICE\\TrustedInstaller"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut sid_bytes = 0u32;
    let mut domain_chars = 0u32;
    let mut use_kind = 0i32;
    let first = unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            account.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(sid_bytes),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(domain_chars),
            std::ptr::addr_of_mut!(use_kind),
        )
    };
    if first != 0 || sid_bytes == 0 {
        return Err(security_error(
            "Windows TrustedInstaller SID lookup did not report a SID buffer length",
        ));
    }
    let mut sid = OwnedSid(AlignedBuffer::new(sid_bytes as usize)?);
    let mut domain = vec![0u16; usize::try_from(domain_chars.max(1)).unwrap_or(1)];
    let resolved = unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            account.as_ptr(),
            sid.0.as_mut_ptr() as PSID,
            std::ptr::addr_of_mut!(sid_bytes),
            domain.as_mut_ptr(),
            std::ptr::addr_of_mut!(domain_chars),
            std::ptr::addr_of_mut!(use_kind),
        )
    };
    if resolved == 0 {
        return Err(io::Error::last_os_error());
    }
    validate_sid(sid.as_sid())?;
    Ok(sid)
}

fn inspect_directory_security(
    directory: &File,
    principals: &RuntimePrincipals,
    is_leaf: bool,
) -> io::Result<()> {
    let mut owner = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            directory.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::addr_of_mut!(owner),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(dacl),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(descriptor),
        )
    };
    let _descriptor = LocalSecurityDescriptor(descriptor);
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    if descriptor.is_null() || owner.is_null() || dacl.is_null() {
        return Err(security_error(
            "Windows private-runtime security proof requires a concrete owner and DACL",
        ));
    }
    if unsafe { IsValidSecurityDescriptor(descriptor) } == 0 {
        return Err(security_error(
            "Windows private-runtime security descriptor is invalid",
        ));
    }
    validate_sid(owner)?;
    let owner_is_trusted = if is_leaf {
        principals.is_leaf_owner(owner)
    } else {
        principals.is_ancestor_trustee(owner)
    };
    if !owner_is_trusted {
        return Err(security_error(
            "Windows private-runtime directory owner is not an allowed concrete SID",
        ));
    }
    inspect_dacl(dacl, principals, is_leaf)
}

fn inspect_dacl(dacl: *mut ACL, principals: &RuntimePrincipals, is_leaf: bool) -> io::Result<()> {
    if dacl.is_null() || unsafe { IsValidAcl(dacl) } == 0 {
        return Err(security_error("Windows private-runtime DACL is invalid"));
    }
    let mut size = ACL_SIZE_INFORMATION::default();
    let read_size = unsafe {
        GetAclInformation(
            dacl,
            std::ptr::addr_of_mut!(size).cast(),
            u32::try_from(std::mem::size_of::<ACL_SIZE_INFORMATION>())
                .expect("ACL_SIZE_INFORMATION size fits u32"),
            AclSizeInformation,
        )
    };
    if read_size == 0 {
        return Err(io::Error::last_os_error());
    }
    let acl_size = usize::from(unsafe { (*dacl).AclSize });
    if acl_size < std::mem::size_of::<ACL>()
        || size.AclBytesInUse < std::mem::size_of::<ACL>() as u32
        || size.AclBytesInUse > acl_size as u32
        || size.AceCount != u32::from(unsafe { (*dacl).AceCount })
    {
        return Err(security_error(
            "Windows private-runtime DACL size or ACE count is inconsistent",
        ));
    }
    let acl_start = dacl as usize;
    let acl_end = acl_start
        .checked_add(acl_size)
        .ok_or_else(|| security_error("Windows private-runtime DACL address overflow"))?;
    for index in 0..size.AceCount {
        let mut ace = std::ptr::null_mut();
        let found = unsafe { GetAce(dacl, index, std::ptr::addr_of_mut!(ace)) };
        if found == 0 || ace.is_null() {
            return Err(security_error(
                "Windows private-runtime DACL returned an invalid ACE pointer",
            ));
        }
        let ace_start = ace as usize;
        let header_end = ace_start
            .checked_add(std::mem::size_of::<ACE_HEADER>())
            .ok_or_else(|| security_error("Windows private-runtime ACE address overflow"))?;
        if ace_start < acl_start || header_end > acl_end {
            return Err(security_error(
                "Windows private-runtime ACE header escapes its DACL",
            ));
        }
        let header = unsafe { &*(ace as *const ACE_HEADER) };
        let ace_size = usize::from(header.AceSize);
        let ace_end = ace_start
            .checked_add(ace_size)
            .ok_or_else(|| security_error("Windows private-runtime ACE size overflow"))?;
        let sid_offset = std::mem::size_of::<ACCESS_ALLOWED_ACE>() - std::mem::size_of::<u32>();
        if ace_size < sid_offset + MIN_SID_BYTES || ace_end > acl_end {
            return Err(security_error(
                "Windows private-runtime ACE length is invalid",
            ));
        }
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE && header.AceType != ACCESS_DENIED_ACE_TYPE {
            return Err(security_error(
                "Windows private-runtime DACL has an unsupported or conditional ACE",
            ));
        }
        if !has_only_known_ace_flags(header.AceFlags) {
            return Err(security_error(
                "Windows private-runtime DACL ACE has unsupported flags",
            ));
        }
        let access = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
        let sid = std::ptr::addr_of!(access.SidStart) as PSID;
        validate_sid(sid)?;
        let sid_len = unsafe { GetLengthSid(sid) } as usize;
        let sid_end = ace_start
            .checked_add(sid_offset)
            .and_then(|start| start.checked_add(sid_len))
            .ok_or_else(|| security_error("Windows private-runtime ACE SID size overflow"))?;
        if sid_len < MIN_SID_BYTES || sid_end > ace_end {
            return Err(security_error(
                "Windows private-runtime ACE SID length is invalid",
            ));
        }
        if header.AceType == ACCESS_ALLOWED_ACE_TYPE {
            inspect_allow_ace(sid, access.Mask, header.AceFlags, principals, is_leaf)?;
        } else if sid_equal(sid, principals.creator_owner.as_sid())
            || sid_equal(sid, principals.creator_group.as_sid())
            || sid_equal(sid, principals.owner_rights.as_sid())
        {
            return Err(security_error(
                "Windows private-runtime DACL has an unsupported owner template",
            ));
        }
    }
    Ok(())
}

fn inspect_allow_ace(
    sid: PSID,
    mask: u32,
    flags: u8,
    principals: &RuntimePrincipals,
    is_leaf: bool,
) -> io::Result<()> {
    if sid_equal(sid, principals.creator_group.as_sid())
        || sid_equal(sid, principals.owner_rights.as_sid())
    {
        return Err(security_error(
            "Windows private-runtime DACL has an unsupported owner template",
        ));
    }
    if sid_equal(sid, principals.creator_owner.as_sid()) {
        if !restricted_creator_owner_template(flags) {
            return Err(security_error(
                "Windows private-runtime CREATOR_OWNER ACE is not a restricted inheritance template",
            ));
        }
        return Ok(());
    }

    let inherit_only = flags & (INHERIT_ONLY_ACE as u8) != 0;
    if is_leaf {
        // An inherit-only leaf ACE describes future files/directories. It must
        // therefore meet the same principal rule as a concrete leaf ACE.
        if !principals.is_leaf_trustee(sid) && map_file_generic_rights(mask) != 0 {
            return Err(security_error(
                "Windows private-runtime leaf or future-child DACL grants a foreign SID access",
            ));
        }
    } else if !inherit_only
        && !principals.is_ancestor_trustee(sid)
        && foreign_ancestor_allow_is_dangerous(mask)
    {
        return Err(security_error(
            "Windows private-runtime ancestor DACL grants a foreign SID dangerous replacement access",
        ));
    }
    Ok(())
}

fn restricted_creator_owner_template(flags: u8) -> bool {
    let required = (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE) as u8;
    has_only_known_ace_flags(flags) && flags & required == required
}

fn has_only_known_ace_flags(flags: u8) -> bool {
    let known = (OBJECT_INHERIT_ACE
        | CONTAINER_INHERIT_ACE
        | INHERIT_ONLY_ACE
        | NO_PROPAGATE_INHERIT_ACE
        | INHERITED_ACE) as u8;
    flags & !known == 0
}

fn foreign_ancestor_allow_is_dangerous(mask: u32) -> bool {
    map_file_generic_rights(mask) & DANGEROUS_FOREIGN_ANCESTOR_ACCESS != 0
}

fn map_file_generic_rights(mut mask: u32) -> u32 {
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    unsafe {
        MapGenericMask(std::ptr::addr_of_mut!(mask), std::ptr::addr_of!(mapping));
    }
    mask
}

fn sid_equal(left: PSID, right: PSID) -> bool {
    !left.is_null()
        && !right.is_null()
        && unsafe { IsValidSid(left) } != 0
        && unsafe { IsValidSid(right) } != 0
        && unsafe { EqualSid(left, right) } != 0
}

fn validate_sid(sid: PSID) -> io::Result<()> {
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return Err(security_error("Windows private-runtime SID is invalid"));
    }
    Ok(())
}

fn security_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
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

#[cfg(test)]
mod tests {
    use super::{
        canonical_private_runtime, foreign_ancestor_allow_is_dangerous, inspect_directory_security,
        open_original_directory_chain_no_follow, open_security_directory_no_follow,
        restricted_creator_owner_template, well_known_sid, AlignedBuffer, RuntimePrincipals,
    };
    use std::fs::OpenOptions;
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{
        GetLastError, SetLastError, ERROR_SUCCESS, GENERIC_WRITE, LUID,
    };
    use windows_sys::Win32::Security::Authorization::{SetSecurityInfo, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AddAccessAllowedAce, AdjustTokenPrivileges, GetLengthSid, InitializeAcl,
        LookupPrivilegeValueW, WinWorldSid, ACL, ACL_REVISION, CONTAINER_INHERIT_ACE,
        DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE, LUID_AND_ATTRIBUTES, OBJECT_INHERIT_ACE,
        OWNER_SECURITY_INFORMATION, PSID, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
        TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_ADD_SUBDIRECTORY, FILE_ALL_ACCESS, FILE_DELETE_CHILD,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, READ_CONTROL,
        SYNCHRONIZE, WRITE_DAC, WRITE_OWNER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // The caller runs in a dedicated test subprocess: a panic or failed native
    // call cannot change privileges in the concurrent parent test process.
    // Restore the previous privilege state before evaluating the real guard.
    fn assign_foreign_test_owner(path: &Path, owner: PSID) -> io::Result<()> {
        let mut raw_token = std::ptr::null_mut();
        if unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                std::ptr::addr_of_mut!(raw_token),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let token = super::OwnedHandle(raw_token);
        let name: Vec<u16> = "SeRestorePrivilege\0".encode_utf16().collect();
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let enabled = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let mut previous: TOKEN_PRIVILEGES = unsafe { std::mem::zeroed() };
        let mut returned = 0;
        unsafe { SetLastError(ERROR_SUCCESS) };
        let changed = unsafe {
            AdjustTokenPrivileges(
                token.0,
                0,
                &enabled,
                std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
                &mut previous,
                &mut returned,
            )
        };
        let status = unsafe { GetLastError() };
        if changed == 0 || status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let assigned = (|| {
            let directory = OpenOptions::new()
                .access_mode(READ_CONTROL | WRITE_OWNER)
                .share_mode(super::SHARE_WITHOUT_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?;
            let status = unsafe {
                SetSecurityInfo(
                    directory.as_raw_handle() as _,
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION,
                    owner,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if status == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(status as i32))
            }
        })();
        unsafe { SetLastError(ERROR_SUCCESS) };
        let restored = unsafe {
            AdjustTokenPrivileges(
                token.0,
                0,
                &previous,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let status = unsafe { GetLastError() };
        if restored == 0 || status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        assigned
    }

    fn align_4(length: usize) -> usize {
        (length + 3) & !3
    }

    fn replace_test_dacl(path: &Path, entries: &[(PSID, u32)]) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options
            .access_mode(
                READ_CONTROL | WRITE_DAC | FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            )
            .share_mode(super::SHARE_WITHOUT_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
        let directory = options.open(path)?;

        let acl_bytes = entries
            .iter()
            .try_fold(std::mem::size_of::<ACL>(), |total, (sid, _)| {
                let sid_bytes = unsafe { GetLengthSid(*sid) } as usize;
                total.checked_add(align_4(8usize.checked_add(sid_bytes)?))
            })
            .ok_or_else(|| io::Error::other("test ACL length overflow"))?;
        let mut storage = AlignedBuffer::new(acl_bytes)?;
        let acl = storage.as_mut_ptr() as *mut ACL;
        let initialized = unsafe {
            InitializeAcl(
                acl,
                u32::try_from(acl_bytes).map_err(|_| io::Error::other("test ACL too large"))?,
                ACL_REVISION,
            )
        };
        if initialized == 0 {
            return Err(io::Error::last_os_error());
        }
        for (sid, mask) in entries {
            if unsafe { AddAccessAllowedAce(acl, ACL_REVISION, *mask, *sid) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let status = unsafe {
            SetSecurityInfo(
                directory.as_raw_handle() as _,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null_mut(),
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }

    #[test]
    fn foreign_ancestor_mask_policy_blocks_reparse_capable_rights_only() {
        for (mask, expected_dangerous) in [
            (DELETE, true),
            (FILE_DELETE_CHILD, true),
            (FILE_WRITE_DATA, true),
            (WRITE_DAC, true),
            (WRITE_OWNER, true),
            (FILE_WRITE_ATTRIBUTES, true),
            (GENERIC_WRITE, true),
            (FILE_ADD_SUBDIRECTORY, false),
            (FILE_READ_DATA, false),
        ] {
            assert_eq!(
                foreign_ancestor_allow_is_dangerous(mask),
                expected_dangerous,
                "ancestor mask {mask:#x}"
            );
        }
    }

    #[test]
    fn creator_owner_must_remain_an_inherit_only_child_template() {
        assert!(restricted_creator_owner_template(
            (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE) as u8
        ));
        assert!(!restricted_creator_owner_template(
            (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
        ));
        assert!(!restricted_creator_owner_template(INHERIT_ONLY_ACE as u8));
        assert!(!restricted_creator_owner_template(
            (OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE) as u8
        ));
        assert!(!restricted_creator_owner_template(
            (OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE | 0x40) as u8
        ));
    }

    #[test]
    fn native_leaf_dacl_refuses_a_foreign_read_allow() {
        let temp = tempfile::tempdir().expect("native tempdir");
        let runtime = temp.path().join("private-runtime");
        std::fs::create_dir(&runtime).expect("runtime directory");
        let principals = RuntimePrincipals::current().expect("native token principals");
        let world = well_known_sid(WinWorldSid).expect("world SID");
        replace_test_dacl(
            &runtime,
            &[
                (principals.token_user_sid(), FILE_ALL_ACCESS),
                (world.as_sid(), FILE_READ_DATA),
            ],
        )
        .expect("test DACL");
        let directory = open_security_directory_no_follow(&runtime).expect("security handle");
        assert!(
            inspect_directory_security(&directory, &principals, true).is_err(),
            "a foreign leaf read allow must fail before launcher state exists"
        );
    }

    #[test]
    fn native_ancestor_dacl_refuses_foreign_write_data() {
        let temp = tempfile::tempdir().expect("native tempdir");
        let ancestor = temp.path().join("ancestor");
        std::fs::create_dir(&ancestor).expect("ancestor directory");
        let principals = RuntimePrincipals::current().expect("native token principals");
        let world = well_known_sid(WinWorldSid).expect("world SID");
        replace_test_dacl(
            &ancestor,
            &[
                (principals.token_user_sid(), FILE_ALL_ACCESS),
                (world.as_sid(), FILE_WRITE_DATA),
            ],
        )
        .expect("test DACL");
        let directory = open_security_directory_no_follow(&ancestor).expect("security handle");
        assert!(
            inspect_directory_security(&directory, &principals, false).is_err(),
            "a foreign ancestor FILE_WRITE_DATA allow can retag the namespace"
        );
    }

    #[test]
    fn native_leaf_owner_refuses_foreign_owner_from_directory_descriptor() {
        const WORKER: &str = "M1ND_NATIVE_FOREIGN_OWNER_WORKER";
        const MARKER: &str = "m1nd_foreign_owner_real_descriptor_verified";
        if std::env::var(WORKER).as_deref() != Ok("1") {
            let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
                .args(["--exact", "windows_durable_fs::tests::native_leaf_owner_refuses_foreign_owner_from_directory_descriptor", "--nocapture"])
                .env(WORKER, "1")
                .output()
                .expect("isolated native owner fixture");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "native owner fixture failed: {stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                stdout.lines().any(|line| line == MARKER),
                "test subprocess did not execute its real owner fixture: {stdout}"
            );
            return;
        }
        let temp = tempfile::tempdir().expect("native tempdir");
        let runtime = temp.path().join("private-runtime");
        std::fs::create_dir(&runtime).expect("runtime directory");
        let principals = RuntimePrincipals::current().expect("native token principals");
        let world = well_known_sid(WinWorldSid).expect("world SID");
        assert!(principals.is_leaf_owner(principals.token_owner_sid()));
        assert!(principals.is_leaf_owner(principals.token_user_sid()));
        assert!(!principals.is_leaf_owner(world.as_sid()));
        replace_test_dacl(&runtime, &[(principals.token_user_sid(), FILE_ALL_ACCESS)])
            .expect("owner-private leaf DACL");
        canonical_private_runtime(&runtime).expect("real private runtime before owner mutation");
        assign_foreign_test_owner(&runtime, world.as_sid())
            .expect("assign actual foreign owner and restore test privilege");
        let error = canonical_private_runtime(&runtime)
            .expect_err("foreign directory owner must be refused by GetSecurityInfo");
        assert!(
            error
                .to_string()
                .contains("directory owner is not an allowed concrete SID"),
            "wrong owner refusal: {error}"
        );
        assert!(!runtime.join("graph_snapshot.json").exists());
        assert!(!runtime.join("registry").exists());
        println!("\n{MARKER}");
    }

    #[test]
    fn native_null_dacl_refuses_a_concrete_security_descriptor() {
        let temp = tempfile::tempdir().expect("native tempdir");
        let runtime = temp.path().join("private-runtime");
        std::fs::create_dir(&runtime).expect("runtime directory");
        let principals = RuntimePrincipals::current().expect("native token principals");
        replace_test_dacl(&runtime, &[(principals.token_user_sid(), FILE_ALL_ACCESS)])
            .expect("owner-private leaf DACL");
        canonical_private_runtime(&runtime).expect("private runtime before null DACL");
        let directory = OpenOptions::new()
            .access_mode(READ_CONTROL | WRITE_DAC)
            .share_mode(super::SHARE_WITHOUT_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&runtime)
            .expect("DACL fixture handle");
        let status = unsafe {
            SetSecurityInfo(
                directory.as_raw_handle() as _,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, ERROR_SUCCESS, "set real null DACL");
        let error = inspect_directory_security(&directory, &principals, true)
            .expect_err("null DACL must fail despite a concrete allocated descriptor");
        assert!(
            error
                .to_string()
                .contains("requires a concrete owner and DACL"),
            "wrong null-DACL refusal: {error}"
        );
    }

    #[test]
    fn native_original_reparse_and_missing_proof_fail_closed() {
        let temp = tempfile::tempdir().expect("native tempdir");
        let target = temp.path().join("target");
        let reparse = temp.path().join("reparse");
        std::fs::create_dir(&target).expect("target directory");
        std::os::windows::fs::symlink_dir(&target, &reparse)
            .expect("native Windows test permission to create a directory reparse point");
        assert!(
            open_original_directory_chain_no_follow(&reparse).is_err(),
            "original reparse component must fail before canonicalization"
        );
        assert!(
            canonical_private_runtime(&temp.path().join("missing-private-runtime")).is_err(),
            "missing original leaf has no privacy-proof fallback"
        );
    }
}
