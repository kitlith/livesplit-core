use super::{
    ptr::{Array, WasmPtr},
    read_bytes,
    types::*,
    write_bytes_to_string,
};
use std::{
    fs::File,
    io::{Seek, SeekFrom},
    time::SystemTime,
};
use wasmer_runtime::Memory;

pub struct FileSystem {
    files: Vec<Option<File>>,
}

impl FileSystem {
    pub fn new() -> Self {
        Self { files: vec![] }
    }

    pub fn fd_fdstat_get(
        &mut self,
        memory: &Memory,
        fd: __wasi_fd_t,
        buf_ptr: WasmPtr<__wasi_fdstat_t>,
    ) -> __wasi_errno_t {
        if fd != 3 {
            return __WASI_EBADF;
        }

        let stat = __wasi_fdstat_t {
            fs_filetype: __WASI_FILETYPE_DIRECTORY,
            fs_flags: 0,
            fs_rights_base: 0x1FFFFFFF, // all rights for now
            fs_rights_inheriting: 0x1FFFFFFF,
        };
        let buf = wasi_try!(buf_ptr.deref(memory));

        buf.set(stat);

        __WASI_ESUCCESS
    }

    pub fn fd_write(
        &mut self,
        memory: &Memory,
        fd: __wasi_fd_t,
        iovs: WasmPtr<__wasi_ciovec_t, Array>,
        iovs_len: u32,
        nwritten: WasmPtr<u32>,
    ) -> __wasi_errno_t {
        let iovs_arr_cell = wasi_try!(iovs.deref(memory, 0, iovs_len));
        let nwritten_cell = wasi_try!(nwritten.deref(memory));
        if fd < 1 || fd > 2 {
            return __WASI_EBADF;
        }

        let (bytes_written, text) = wasi_try!(write_bytes_to_string(memory, iovs_arr_cell));
        if fd == 1 {
            log::info!(target: "Auto Splitter", "{}", text);
        } else {
            log::error!(target: "Auto Splitter", "{}", text);
        }
        nwritten_cell.set(bytes_written);

        __WASI_ESUCCESS
    }

    pub fn path_open(
        &mut self,
        memory: &Memory,
        dirfd: __wasi_fd_t,
        dirflags: __wasi_lookupflags_t,
        path: WasmPtr<u8, Array>,
        path_len: u32,
        o_flags: __wasi_oflags_t,
        fs_rights_base: __wasi_rights_t,
        fs_rights_inheriting: __wasi_rights_t,
        fs_flags: __wasi_fdflags_t,
        fd: WasmPtr<__wasi_fd_t>,
    ) -> __wasi_errno_t {
        let file = File::open(r"livesplit-core\README.md").unwrap();
        let fd_val = if let Some((i, slot)) = self
            .files
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            *slot = Some(file);
            i + 4
        } else {
            let i = self.files.len();
            self.files.push(Some(file));
            i + 4
        };

        wasi_try!(fd.deref(memory)).set(fd_val as __wasi_fd_t);

        __WASI_ESUCCESS
    }

    pub fn fd_close(&mut self, memory: &Memory, fd: __wasi_fd_t) -> __wasi_errno_t {
        if let Some(slot) = (fd as usize)
            .checked_sub(4)
            .and_then(|i| self.files.get_mut(i))
        {
            *slot = None;
            __WASI_ESUCCESS
        } else {
            __WASI_EBADF
        }
    }

    pub fn fd_read(
        &mut self,
        memory: &Memory,
        fd: __wasi_fd_t,
        iovs: WasmPtr<__wasi_iovec_t, Array>,
        iovs_len: u32,
        nread: WasmPtr<u32>,
    ) -> __wasi_errno_t {
        let iovs_arr_cell = wasi_try!(iovs.deref(memory, 0, iovs_len));
        let nread_cell = wasi_try!(nread.deref(memory));

        if let Some(Some(file)) = (fd as usize).checked_sub(4).and_then(|i| self.files.get(i)) {
            let bytes_read = wasi_try!(read_bytes(file, memory, iovs_arr_cell));
            nread_cell.set(bytes_read);
            __WASI_ESUCCESS
        } else {
            __WASI_EBADF
        }
    }

    pub fn fd_filestat_get(
        &mut self,
        memory: &Memory,
        fd: __wasi_fd_t,
        buf: WasmPtr<__wasi_filestat_t>,
    ) -> __wasi_errno_t {
        let buf_cell = wasi_try!(buf.deref(memory));

        if let Some(Some(file)) = (fd as usize).checked_sub(4).and_then(|i| self.files.get(i)) {
            let meta = wasi_try!(file.metadata().map_err(|_| __WASI_EIO));

            buf_cell.set(__wasi_filestat_t {
                st_filetype: if meta.file_type().is_file() {
                    __WASI_FILETYPE_REGULAR_FILE
                } else if meta.file_type().is_dir() {
                    __WASI_FILETYPE_DIRECTORY
                } else if meta.file_type().is_symlink() {
                    __WASI_FILETYPE_SYMBOLIC_LINK
                } else {
                    __WASI_FILETYPE_UNKNOWN
                },
                st_size: meta.len(),
                st_atim: meta
                    .accessed()
                    .ok()
                    .and_then(|sys_time| sys_time.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos() as u64)
                    .unwrap_or(0),
                st_ctim: meta
                    .created()
                    .ok()
                    .and_then(|sys_time| sys_time.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos() as u64)
                    .unwrap_or(0),
                st_mtim: meta
                    .modified()
                    .ok()
                    .and_then(|sys_time| sys_time.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos() as u64)
                    .unwrap_or(0),
                ..__wasi_filestat_t::default()
            });

            __WASI_ESUCCESS
        } else {
            __WASI_EBADF
        }
    }

    pub fn fd_seek(
        &mut self,
        memory: &Memory,
        fd: __wasi_fd_t,
        offset: __wasi_filedelta_t,
        whence: __wasi_whence_t,
        newoffset: WasmPtr<__wasi_filesize_t>,
    ) -> __wasi_errno_t {
        let newoffset_cell = wasi_try!(newoffset.deref(memory));
        if let Some(Some(file)) = (fd as usize).checked_sub(4).and_then(|i| self.files.get(i)) {
            let seek_from = match whence {
                __WASI_WHENCE_CUR => SeekFrom::Current(offset),
                __WASI_WHENCE_END => SeekFrom::End(offset),
                __WASI_WHENCE_SET => SeekFrom::Start(offset as _),
                _ => return __WASI_EINVAL,
            };
            let mut file = file;
            let new_offset = wasi_try!(file.seek(seek_from).map_err(|_| __WASI_EIO));
            newoffset_cell.set(new_offset);
            __WASI_ESUCCESS
        } else {
            __WASI_EBADF
        }
    }
}
