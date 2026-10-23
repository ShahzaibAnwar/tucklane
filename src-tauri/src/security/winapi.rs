//! Thin, safe wrappers around the Win32 handle APIs Tucklane uses to defend
//! against link-following and time-of-check/time-of-use races.
//!
//! The pattern used throughout is: open a handle *without following reparse
//! points*, ask Windows what the handle really refers to, validate that, and
//! then act on the same handle.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{
    FileAttributeTagInfo, FileRenameInfo, GetDriveTypeW, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, SetFileInformationByHandle,
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, VOLUME_NAME_DOS,
};

/// `DELETE` standard access right (required to rename a file by handle).
pub const ACCESS_DELETE: u32 = 0x0001_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const DRIVE_REMOTE: u32 = 4;

/// Facts about an open handle, as reported by Windows (not by the path we used).
#[derive(Debug, Clone)]
pub struct HandleInfo {
    pub final_path: PathBuf,
    pub is_dir: bool,
    pub attributes: u32,
    pub reparse_tag: Option<u32>,
    pub size: u64,
    pub modified_ms: i64,
    /// Volume serial number + file index. Survives same-volume renames.
    pub file_id: String,
}

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Opens `path` without following a final reparse point. Works for files and
/// directories. `share` controls which concurrent openers we tolerate.
pub fn open_no_follow(path: &Path, access: u32, share: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access | SYNCHRONIZE)
        .share_mode(share)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// Opens for metadata only, tolerating any other opener.
pub fn open_for_query(path: &Path) -> io::Result<File> {
    open_no_follow(
        path,
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    )
}

/// Opens a file for a handle-based move. Fails with a sharing violation if any
/// other process has the file open for writing or deleting, which is how we
/// detect files that are still being written (e.g. an active download).
pub fn open_for_move(path: &Path) -> io::Result<File> {
    open_no_follow(path, ACCESS_DELETE | FILE_READ_ATTRIBUTES, FILE_SHARE_READ)
}

/// Opens a directory and pins it: while this handle is alive the directory
/// (and therefore its ancestors) cannot be renamed or replaced by a link.
pub fn open_dir_pinned(path: &Path) -> io::Result<File> {
    // FILE_LIST_DIRECTORY makes this open participate in share-mode checks;
    // an attributes-only open would not block a concurrent rename.
    const FILE_LIST_DIRECTORY: u32 = 0x0001;
    open_no_follow(
        path,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )
}

pub fn final_path(file: &File) -> io::Result<PathBuf> {
    let h = file.as_raw_handle() as HANDLE;
    let mut buf: Vec<u16> = vec![0; 512];
    loop {
        // SAFETY: buffer pointer/length are valid for the duration of the call.
        let n = unsafe {
            GetFinalPathNameByHandleW(
                h,
                buf.as_mut_ptr(),
                buf.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        } as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n < buf.len() {
            buf.truncate(n);
            return Ok(PathBuf::from(std::ffi::OsString::from_wide(&buf)));
        }
        buf.resize(n + 1, 0);
    }
}

pub fn handle_info(file: &File) -> io::Result<HandleInfo> {
    let h = file.as_raw_handle() as HANDLE;
    // SAFETY: zeroed POD structs, valid handle, correct sizes.
    let mut bhfi: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(h, &mut bhfi) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut tag: FILE_ATTRIBUTE_TAG_INFO = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            h,
            FileAttributeTagInfo,
            &mut tag as *mut _ as *mut _,
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    let attributes = bhfi.dwFileAttributes;
    let reparse_tag = if ok != 0 && attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        Some(tag.ReparseTag)
    } else {
        None
    };
    let size = ((bhfi.nFileSizeHigh as u64) << 32) | bhfi.nFileSizeLow as u64;
    let ft = ((bhfi.ftLastWriteTime.dwHighDateTime as u64) << 32)
        | bhfi.ftLastWriteTime.dwLowDateTime as u64;
    Ok(HandleInfo {
        final_path: final_path(file)?,
        is_dir: attributes & FILE_ATTRIBUTE_DIRECTORY != 0,
        attributes,
        reparse_tag,
        size,
        modified_ms: filetime_to_unix_ms(ft),
        file_id: format!(
            "{:08x}-{:08x}{:08x}",
            bhfi.dwVolumeSerialNumber, bhfi.nFileIndexHigh, bhfi.nFileIndexLow
        ),
    })
}

/// FILETIME (100ns ticks since 1601) to Unix milliseconds.
pub fn filetime_to_unix_ms(ft: u64) -> i64 {
    (ft as i64 - 116_444_736_000_000_000) / 10_000
}

/// Renames the file behind `file` to `dest` (a full path) without ever
/// replacing an existing file. The handle must have been opened with DELETE.
pub fn rename_by_handle_no_replace(file: &File, dest: &Path) -> io::Result<()> {
    let name: Vec<u16> = dest.as_os_str().encode_wide().collect();
    // FILE_RENAME_INFO layout on x64: { u32 ReplaceIfExists/Flags; [pad]; HANDLE RootDirectory; u32 FileNameLength; WCHAR FileName[1] }
    #[repr(C)]
    struct RenameHeader {
        replace: u32,
        root: HANDLE,
        name_len: u32,
    }
    let header_size = std::mem::offset_of!(RenameHeader, name_len) + 4;
    let total = header_size + (name.len() + 1) * 2;
    let mut buf: Vec<u64> = vec![0; total.div_ceil(8)];
    let base = buf.as_mut_ptr() as *mut u8;
    // SAFETY: buffer is large enough and 8-byte aligned; fields written at their C offsets.
    unsafe {
        let hdr = base as *mut RenameHeader;
        (*hdr).replace = 0;
        (*hdr).root = std::ptr::null_mut();
        (*hdr).name_len = (name.len() * 2) as u32;
        let name_ptr = base.add(header_size) as *mut u16;
        std::ptr::copy_nonoverlapping(name.as_ptr(), name_ptr, name.len());
        if SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileRenameInfo,
            base as *const _,
            total as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// True when `path` is on a fixed local disk (the only kind of volume where
/// Windows reliably provides a Recycle Bin).
pub fn is_fixed_drive(path: &Path) -> bool {
    const DRIVE_FIXED: u32 = 3;
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        let root = format!("{}\\", &s[..2]);
        // SAFETY: null-terminated wide string.
        return unsafe { GetDriveTypeW(wide(Path::new(&root)).as_ptr()) } == DRIVE_FIXED;
    }
    false
}

/// True when the volume holding `path` is a network drive.
pub fn is_remote_drive(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    if s.starts_with(r"\\") || s.starts_with("UNC\\") {
        return true;
    }
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        let root = format!("{}\\", &s[..2]);
        // SAFETY: null-terminated wide string.
        let t = unsafe { GetDriveTypeW(wide(Path::new(&root)).as_ptr()) };
        return t == DRIVE_REMOTE;
    }
    false
}
