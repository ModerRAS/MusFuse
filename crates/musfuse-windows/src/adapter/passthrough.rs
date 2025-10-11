use std::ffi::c_void;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::RwLock;
use tracing::{debug, error, trace, warn};
use winfsp::constants::FspCleanupFlags;
use winfsp::filesystem::{
    DirBuffer, DirInfo, DirMarker, FileInfo, FileSecurity, OpenFileInfo, VolumeInfo, WideNameInfo,
};
use winfsp::{FspError, Result, U16CStr};
use windows::Win32::Foundation::{STATUS_DIRECTORY_NOT_EMPTY, STATUS_OBJECT_NAME_COLLISION};

/// File context that holds the open file handle and metadata
#[derive(Debug)]
pub struct FileContext {
    /// Path to the file relative to source directory
    pub path: PathBuf,
    /// Whether this is marked for deletion
    pub delete_on_close: bool,
    /// Optional file handle for read/write operations
    pub file: RwLock<Option<fs::File>>,
}

impl FileContext {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            delete_on_close: false,
            file: RwLock::new(None),
        }
    }
}

/// Passthrough filesystem implementation that transparently maps to a source directory
pub struct PassthroughFS {
    /// Source directory to pass through
    source: PathBuf,
}

impl PassthroughFS {
    /// Create a new passthrough filesystem for the given source directory
    pub fn new(source: PathBuf) -> Result<Self> {
        if !source.exists() {
            return Err(FspError::IO(std::io::ErrorKind::NotFound));
        }
        if !source.is_dir() {
            return Err(FspError::IO(std::io::ErrorKind::NotADirectory));
        }
        Ok(Self { source })
    }

    /// Convert a WinFSP path to a real filesystem path
    fn resolve_path(&self, file_name: &U16CStr) -> PathBuf {
        let path_str = file_name.to_string_lossy();
        let path_str = path_str.trim_start_matches('\\');
        let path_str = path_str.replace('\\', "/");
        self.source.join(path_str)
    }

    /// Convert metadata to FileInfo
    fn metadata_to_file_info(metadata: &fs::Metadata, file_info: &mut FileInfo) {
        let attrs = metadata.file_attributes();
        file_info.file_attributes = attrs;

        file_info.file_size = metadata.len();
        file_info.allocation_size = ((metadata.len() + 4095) / 4096) * 4096;

        // Convert SystemTime to Windows FILETIME (100-nanosecond intervals since 1601-01-01)
        if let Ok(created) = metadata.created() {
            file_info.creation_time = systemtime_to_filetime(created);
        }
        if let Ok(accessed) = metadata.accessed() {
            file_info.last_access_time = systemtime_to_filetime(accessed);
        }
        if let Ok(modified) = metadata.modified() {
            file_info.last_write_time = systemtime_to_filetime(modified);
            file_info.change_time = systemtime_to_filetime(modified);
        }

        file_info.index_number = 0;
    }

    /// Open or create a file handle for I/O operations
    fn open_file_handle(&self, path: &Path, write: bool) -> std::io::Result<fs::File> {
        if write {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(false)
                .open(path)
        } else {
            fs::File::open(path)
        }
    }
}

impl winfsp::filesystem::FileSystemContext for PassthroughFS {
    type FileContext = Arc<FileContext>;

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _security_descriptor: Option<&mut [c_void]>,
        _reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity> {
        let path = self.resolve_path(file_name);
        trace!("get_security_by_name: {:?}", path);

        match fs::metadata(&path) {
            Ok(metadata) => {
                let attrs = metadata.file_attributes();
                Ok(FileSecurity {
                    reparse: false,
                    sz_security_descriptor: 0,
                    attributes: attrs,
                })
            }
            Err(e) => {
                debug!("get_security_by_name failed for {:?}: {}", path, e);
                Err(FspError::from(e))
            }
        }
    }

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        _granted_access: u32,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext> {
        let path = self.resolve_path(file_name);
        trace!("open: {:?}", path);

        match fs::metadata(&path) {
            Ok(metadata) => {
                Self::metadata_to_file_info(&metadata, file_info.as_mut());
                Ok(Arc::new(FileContext::new(path)))
            }
            Err(e) => {
                debug!("open failed for {:?}: {}", path, e);
                Err(FspError::from(e))
            }
        }
    }

    fn close(&self, context: Self::FileContext) {
        trace!("close: {:?}", context.path);
        // Drop the file handle
        *context.file.write() = None;
    }

    fn cleanup(&self, context: &Self::FileContext, file_name: Option<&U16CStr>, flags: u32) {
        trace!("cleanup: {:?}, flags: 0x{:x}", context.path, flags);

        // Handle deletion
        if FspCleanupFlags::FspCleanupDelete.is_flagged(flags) {
            let path = if let Some(name) = file_name {
                self.resolve_path(name)
            } else {
                context.path.clone()
            };

            trace!("attempting to delete: {:?}", path);
            if let Ok(metadata) = fs::metadata(&path) {
                let result = if metadata.is_dir() {
                    fs::remove_dir(&path)
                } else {
                    fs::remove_file(&path)
                };

                if let Err(e) = result {
                    error!("failed to delete {:?}: {}", path, e);
                }
            }
        }
    }

    fn read(&self, context: &Self::FileContext, buffer: &mut [u8], offset: u64) -> Result<u32> {
        trace!("read: {:?}, offset: {}, len: {}", context.path, offset, buffer.len());

        let mut file_lock = context.file.write();
        if file_lock.is_none() {
            // Open file on demand
            match self.open_file_handle(&context.path, false) {
                Ok(f) => *file_lock = Some(f),
                Err(e) => return Err(FspError::from(e)),
            }
        }

        let file = file_lock.as_mut().unwrap();
        
        if let Err(e) = file.seek(SeekFrom::Start(offset)) {
            return Err(FspError::from(e));
        }

        match file.read(buffer) {
            Ok(n) => Ok(n as u32),
            Err(e) => Err(FspError::from(e)),
        }
    }

    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        _write_to_eof: bool,
        _constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> Result<u32> {
        trace!("write: {:?}, offset: {}, len: {}", context.path, offset, buffer.len());

        let mut file_lock = context.file.write();
        if file_lock.is_none() {
            // Open file on demand for writing
            match self.open_file_handle(&context.path, true) {
                Ok(f) => *file_lock = Some(f),
                Err(e) => return Err(FspError::from(e)),
            }
        }

        let file = file_lock.as_mut().unwrap();

        if let Err(e) = file.seek(SeekFrom::Start(offset)) {
            return Err(FspError::from(e));
        }

        match file.write(buffer) {
            Ok(n) => {
                // Update file info
                if let Err(e) = file.sync_all() {
                    warn!("failed to sync file: {}", e);
                }
                
                if let Ok(metadata) = fs::metadata(&context.path) {
                    Self::metadata_to_file_info(&metadata, file_info);
                }
                
                Ok(n as u32)
            }
            Err(e) => Err(FspError::from(e)),
        }
    }

    fn get_file_info(&self, context: &Self::FileContext, file_info: &mut FileInfo) -> Result<()> {
        trace!("get_file_info: {:?}", context.path);

        match fs::metadata(&context.path) {
            Ok(metadata) => {
                Self::metadata_to_file_info(&metadata, file_info);
                Ok(())
            }
            Err(e) => Err(FspError::from(e)),
        }
    }

    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        file_attributes: u32,
        _creation_time: u64,
        _last_access_time: u64,
        _last_write_time: u64,
        _last_change_time: u64,
        file_info: &mut FileInfo,
    ) -> Result<()> {
        trace!("set_basic_info: {:?}", context.path);

        // For passthrough, we only handle basic attribute changes
        if file_attributes != 0 && file_attributes != u32::MAX {
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                use windows::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

                let _ = fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
                    .open(&context.path)?;
            }
        }

        // Refresh file info
        if let Ok(metadata) = fs::metadata(&context.path) {
            Self::metadata_to_file_info(&metadata, file_info);
        }

        Ok(())
    }

    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> Result<()> {
        trace!("set_file_size: {:?}, new_size: {}", context.path, new_size);

        let file = fs::OpenOptions::new()
            .write(true)
            .open(&context.path)?;

        file.set_len(new_size)?;

        if let Ok(metadata) = fs::metadata(&context.path) {
            Self::metadata_to_file_info(&metadata, file_info);
        }

        Ok(())
    }

    fn read_directory(
        &self,
        context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32> {
        trace!("read_directory: {:?}", context.path);

        let dir_buffer = DirBuffer::new();
        let _lock = dir_buffer.acquire(marker.is_none(), None)?;

        // Read directory entries
        let entries = match fs::read_dir(&context.path) {
            Ok(entries) => entries,
            Err(e) => return Err(FspError::from(e)),
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("failed to read directory entry: {}", e);
                    continue;
                }
            };

            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            let mut dir_info: DirInfo<255> = DirInfo::new();
            if let Err(e) = dir_info.set_name(&*file_name_str) {
                warn!("failed to set name for {}: {:?}", file_name_str, e);
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                Self::metadata_to_file_info(&metadata, dir_info.file_info_mut());
            }

            if dir_buffer.acquire(false, None).is_err() {
                break;
            }
            if let Err(e) = _lock.write(&mut dir_info) {
                trace!("buffer full, stopping directory enumeration: {:?}", e);
                break;
            }
        }

        drop(_lock);
        Ok(dir_buffer.read(marker, buffer))
    }

    fn get_volume_info(&self, out_volume_info: &mut VolumeInfo) -> Result<()> {
        trace!("get_volume_info");

        // Set some reasonable defaults
        out_volume_info.total_size = 1024 * 1024 * 1024 * 1024; // 1TB
        out_volume_info.free_size = 512 * 1024 * 1024 * 1024;   // 512GB
        out_volume_info.set_volume_label("MusFuse");

        Ok(())
    }

    fn create(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        _granted_access: u32,
        file_attributes: u32,
        _security_descriptor: Option<&[c_void]>,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _extra_buffer_is_reparse_point: bool,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext> {
        let path = self.resolve_path(file_name);
        trace!("create: {:?}", path);

        let is_directory = (create_options & 0x00000001) != 0; // FILE_DIRECTORY_FILE

        if path.exists() {
            return Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_COLLISION.0));
        }

        if is_directory {
            fs::create_dir(&path)?;
        } else {
            let _attrs = file_attributes;
            // Note: File attributes would be set here in a full implementation
            
            let file = fs::File::create(&path)?;
            drop(file);
        }

        match fs::metadata(&path) {
            Ok(metadata) => {
                Self::metadata_to_file_info(&metadata, file_info.as_mut());
                Ok(Arc::new(FileContext::new(path)))
            }
            Err(e) => Err(FspError::from(e)),
        }
    }

    fn rename(
        &self,
        _context: &Self::FileContext,
        file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> Result<()> {
        let old_path = self.resolve_path(file_name);
        let new_path = self.resolve_path(new_file_name);
        trace!("rename: {:?} -> {:?}", old_path, new_path);

        if new_path.exists() && !replace_if_exists {
            return Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_COLLISION.0));
        }

        if new_path.exists() && replace_if_exists {
            if new_path.is_dir() {
                fs::remove_dir_all(&new_path)?;
            } else {
                fs::remove_file(&new_path)?;
            }
        }

        fs::rename(&old_path, &new_path)?;
        Ok(())
    }

    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        delete_file: bool,
    ) -> Result<()> {
        trace!("set_delete: {:?}, delete: {}", context.path, delete_file);

        if delete_file {
            // Check if directory is empty
            if context.path.is_dir() {
                match fs::read_dir(&context.path) {
                    Ok(mut entries) => {
                        if entries.next().is_some() {
                            return Err(FspError::NTSTATUS(STATUS_DIRECTORY_NOT_EMPTY.0));
                        }
                    }
                    Err(e) => return Err(FspError::from(e)),
                }
            }
        }

        Ok(())
    }
}

/// Convert SystemTime to Windows FILETIME format
fn systemtime_to_filetime(time: SystemTime) -> u64 {
    const UNIX_EPOCH_IN_FILETIME: u64 = 116444736000000000;
    const TICKS_PER_SECOND: u64 = 10_000_000;

    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => {
            let ticks = duration.as_secs() * TICKS_PER_SECOND
                + u64::from(duration.subsec_nanos()) / 100;
            UNIX_EPOCH_IN_FILETIME + ticks
        }
        Err(_) => 0,
    }
}
