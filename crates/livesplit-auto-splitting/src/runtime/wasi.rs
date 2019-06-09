use super::Environment;
use std::{cell::Cell, io, time::Instant};
use wasmer_runtime::Ctx;
use wasmer_runtime_core::memory::Memory;

macro_rules! wasi_try {
    ($expr:expr) => {{
        let res: Result<_, crate::runtime::wasi::types::__wasi_errno_t> = $expr;
        match res {
            Ok(val) => {
                log::info!("wasi::wasi_try::val: {:?}", val);
                val
            }
            Err(err) => {
                log::info!("wasi::wasi_try::err: {:?}", err);
                return err;
            }
        }
    }};
    ($expr:expr; $e:expr) => {{
        let opt: Option<_> = $expr;
        wasi_try!(opt.ok_or($e))
    }};
}

mod fs;
mod ptr;
mod types;

pub use fs::FileSystem;
use ptr::{Array, WasmPtr};
use types::*;

/// ### `fd_prestat_get()`
/// Get metadata about a preopened file descriptor
/// Input:
/// - `__wasi_fd_t fd`
///     The preopened file descriptor to query
/// Output:
/// - `__wasi_prestat *buf`
///     Where the metadata will be written
pub fn fd_prestat_get(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    buf: WasmPtr<__wasi_prestat_t>,
) -> __wasi_errno_t {
    log::info!("wasi::fd_prestat_get: fd={}", fd);
    let memory = ctx.memory(0);

    let prestat_ptr = wasi_try!(buf.deref(memory));

    if fd != 3 {
        return __WASI_EBADF;
    }

    // let state = get_wasi_state(ctx);
    prestat_ptr.set(__wasi_prestat_t {
        pr_type: __WASI_PREOPENTYPE_DIR,
        u: PrestatEnum::Dir {
            pr_name_len: "/".len() as u32,
        }
        .untagged(),
    });

    __WASI_ESUCCESS
    // __WASI_EBADF
}

pub fn fd_prestat_dir_name(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    path: WasmPtr<u8, Array>,
    path_len: u32,
) -> __wasi_errno_t {
    log::info!(
        "wasi::fd_prestat_dir_name: fd={}, path_len={}",
        fd,
        path_len
    );
    let memory = ctx.memory(0);
    let path_chars = wasi_try!(path.deref(memory, 0, path_len));

    if fd != 3 {
        return __WASI_EBADF;
    }

    let path = "/";
    for (c, p) in path.bytes().zip(path_chars) {
        p.set(c);
    }

    __WASI_ESUCCESS
}

/// ### `fd_filestat_get()`
/// Get the metadata of an open file
/// Input:
/// - `__wasi_fd_t fd`
///     The open file descriptor whose metadata will be read
/// Output:
/// - `__wasi_filestat_t *buf`
///     Where the metadata from `fd` will be written
pub fn fd_filestat_get(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    buf: WasmPtr<__wasi_filestat_t>,
) -> __wasi_errno_t {
    log::info!("wasi::fd_filestat_get");
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.fd_filestat_get(memory, fd, buf)
}

/// ### `environ_sizes_get()`
/// Return command-line argument data sizes.
/// Outputs:
/// - `size_t *environ_count`
///     The number of environment variables.
/// - `size_t *environ_buf_size`
///     The size of the environment variable string data.
pub fn environ_sizes_get(
    ctx: &mut Ctx,
    environ_count: WasmPtr<u32>,
    environ_buf_size: WasmPtr<u32>,
) -> __wasi_errno_t {
    log::info!("wasi::environ_sizes_get");
    let memory = ctx.memory(0);

    let environ_count = wasi_try!(environ_count.deref(memory));
    let environ_buf_size = wasi_try!(environ_buf_size.deref(memory));

    environ_count.set(0);
    environ_buf_size.set(0);

    __WASI_ESUCCESS
}

/// ### `environ_get()`
/// Read environment variable data.
/// The sizes of the buffers should match that returned by [`environ_sizes_get()`](#environ_sizes_get).
/// Inputs:
/// - `char **environ`
///     A pointer to a buffer to write the environment variable pointers.
/// - `char *environ_buf`
///     A pointer to a buffer to write the environment variable string data.
pub fn environ_get(
    ctx: &mut Ctx,
    environ: WasmPtr<WasmPtr<u8, Array>, Array>,
    environ_buf: WasmPtr<u8, Array>,
) -> __wasi_errno_t {
    unimplemented!()
}

/// ### `args_sizes_get()`
/// Return command-line argument data sizes.
/// Outputs:
/// - `size_t *argc`
///     The number of arguments.
/// - `size_t *argv_buf_size`
///     The size of the argument string data.
pub fn args_sizes_get(
    ctx: &mut Ctx,
    argc: WasmPtr<u32>,
    argv_buf_size: WasmPtr<u32>,
) -> __wasi_errno_t {
    log::info!("wasi::args_sizes_get");
    let memory = ctx.memory(0);

    let argc = wasi_try!(argc.deref(memory));
    let argv_buf_size = wasi_try!(argv_buf_size.deref(memory));

    let argc_val = 0u32;
    let argv_buf_size_val = 0u32;
    argc.set(argc_val);
    argv_buf_size.set(argv_buf_size_val);

    log::info!("=> argc={}, argv_buf_size={}", argc_val, argv_buf_size_val);

    __WASI_ESUCCESS
}

/// ### `args_get()`
/// Read command-line argument data.
/// The sizes of the buffers should match that returned by [`args_sizes_get()`](#args_sizes_get).
/// Inputs:
/// - `char **argv`
///     A pointer to a buffer to write the argument pointers.
/// - `char *argv_buf`
///     A pointer to a buffer to write the argument string data.
///
pub fn args_get(
    ctx: &mut Ctx,
    argv: WasmPtr<WasmPtr<u8, Array>, Array>,
    argv_buf: WasmPtr<u8, Array>,
) -> __wasi_errno_t {
    log::info!("wasi::args_get");
    let memory = ctx.memory(0);
    let result = write_buffer_array(memory, &[], argv, argv_buf);
    result
}

/// ### `fd_write()`
/// Write data to the file descriptor
/// Inputs:
/// - `__wasi_fd_t`
///     File descriptor (opened with writing) to write to
/// - `const __wasi_ciovec_t *iovs`
///     List of vectors to read data from
/// - `u32 iovs_len`
///     Length of data in `iovs`
/// Output:
/// - `u32 *nwritten`
///     Number of bytes written
/// Errors:
///
pub fn fd_write(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    iovs: WasmPtr<__wasi_ciovec_t, Array>,
    iovs_len: u32,
    nwritten: WasmPtr<u32>,
) -> __wasi_errno_t {
    log::info!("wasi::fd_write: fd={}", fd);
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.fd_write(memory, fd, iovs, iovs_len, nwritten)
}

/// ### `fd_seek()`
/// Update file descriptor offset
/// Inputs:
/// - `__wasi_fd_t fd`
///     File descriptor to mutate
/// - `__wasi_filedelta_t offset`
///     Number of bytes to adjust offset by
/// - `__wasi_whence_t whence`
///     What the offset is relative to
/// Output:
/// - `__wasi_filesize_t *fd`
///     The new offset relative to the start of the file
pub fn fd_seek(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    offset: __wasi_filedelta_t,
    whence: __wasi_whence_t,
    newoffset: WasmPtr<__wasi_filesize_t>,
) -> __wasi_errno_t {
    log::info!("wasi::fd_seek: fd={}, offset={}", fd, offset);
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.fd_seek(memory, fd, offset, whence, newoffset)
}

pub fn proc_exit(ctx: &mut Ctx, code: __wasi_exitcode_t) {
    unimplemented!()
}

/// ### `fd_fdstat_get()`
/// Get metadata of a file descriptor
/// Input:
/// - `__wasi_fd_t fd`
///     The file descriptor whose metadata will be accessed
/// Output:
/// - `__wasi_fdstat_t *buf`
///     The location where the metadata will be written
pub fn fd_fdstat_get(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    buf_ptr: WasmPtr<__wasi_fdstat_t>,
) -> __wasi_errno_t {
    log::info!(
        "wasi::fd_fdstat_get: fd={}, buf_ptr={}",
        fd,
        buf_ptr.offset()
    );
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.fd_fdstat_get(memory, fd, buf_ptr)
}

fn write_bytes<T: io::Write>(
    mut write_loc: T,
    memory: &Memory,
    iovs_arr_cell: &[Cell<__wasi_ciovec_t>],
) -> Result<u32, __wasi_errno_t> {
    let mut bytes_written = 0;
    for iov in iovs_arr_cell {
        let iov_inner = iov.get();
        let bytes = iov_inner.buf.deref(memory, 0, iov_inner.buf_len)?;
        write_loc
            .write(&bytes.iter().map(|b_cell| b_cell.get()).collect::<Vec<u8>>())
            .map_err(|_| {
                write_loc.flush();
                __WASI_EIO
            })?;

        // TODO: handle failure more accurately
        bytes_written += iov_inner.buf_len;
    }
    write_loc.flush();
    Ok(bytes_written)
}

/// ### `fd_close()`
/// Close an open file descriptor
/// Inputs:
/// - `__wasi_fd_t fd`
///     A file descriptor mapping to an open file to close
/// Errors:
/// - `__WASI_EISDIR`
///     If `fd` is a directory
/// - `__WASI_EBADF`
///     If `fd` is invalid or not open (TODO: consider __WASI_EINVAL)
pub fn fd_close(ctx: &mut Ctx, fd: __wasi_fd_t) -> __wasi_errno_t {
    log::info!("wasi::fd_close: fd={}", fd);
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.fd_close(memory, fd)
}

/// ### `path_open()`
/// Open file located at the given path
/// Inputs:
/// - `__wasi_fd_t dirfd`
///     The fd corresponding to the directory that the file is in
/// - `__wasi_lookupflags_t dirflags`
///     Flags specifying how the path will be resolved
/// - `char *path`
///     The path of the file or directory to open
/// - `u32 path_len`
///     The length of the `path` string
/// - `__wasi_oflags_t o_flags`
///     How the file will be opened
/// - `__wasi_rights_t fs_rights_base`
///     The rights of the created file descriptor
/// - `__wasi_rights_t fs_rightsinheriting`
///     The rights of file descriptors derived from the created file descriptor
/// - `__wasi_fdflags_t fs_flags`
///     The flags of the file descriptor
/// Output:
/// - `__wasi_fd_t* fd`
///     The new file descriptor
/// Possible Errors:
/// - `__WASI_EACCES`, `__WASI_EBADF`, `__WASI_EFAULT`, `__WASI_EFBIG?`, `__WASI_EINVAL`, `__WASI_EIO`, `__WASI_ELOOP`, `__WASI_EMFILE`, `__WASI_ENAMETOOLONG?`, `__WASI_ENFILE`, `__WASI_ENOENT`, `__WASI_ENOTDIR`, `__WASI_EROFS`, and `__WASI_ENOTCAPABLE`
pub fn path_open(
    ctx: &mut Ctx,
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
    log::info!("wasi::path_open");
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.path_open(
        memory,
        dirfd,
        dirflags,
        path,
        path_len,
        o_flags,
        fs_rights_base,
        fs_rights_inheriting,
        fs_flags,
        fd,
    )
}

/// ### `fd_read()`
/// Read data from file descriptor
/// Inputs:
/// - `__wasi_fd_t fd`
///     File descriptor from which data will be read
/// - `const __wasi_iovec_t *iovs`
///     Vectors where data will be stored
/// - `u32 iovs_len`
///     Length of data in `iovs`
/// Output:
/// - `u32 *nread`
///     Number of bytes read
pub fn fd_read(
    ctx: &mut Ctx,
    fd: __wasi_fd_t,
    iovs: WasmPtr<__wasi_iovec_t, Array>,
    iovs_len: u32,
    nread: WasmPtr<u32>,
) -> __wasi_errno_t {
    log::info!("wasi::fd_read: fd={}", fd);
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0);
    env.fs.fd_read(memory, fd, iovs, iovs_len, nread)
}

/// ### `clock_time_get()`
/// Get the time of the specified clock
/// Inputs:
/// - `__wasi_clockid_t clock_id`
///     The ID of the clock to query
/// - `__wasi_timestamp_t precision`
///     The maximum amount of error the reading may have
/// Output:
/// - `__wasi_timestamp_t *time`
///     The value of the clock in nanoseconds
pub fn clock_time_get(
    ctx: &mut Ctx,
    clock_id: __wasi_clockid_t,
    precision: __wasi_timestamp_t,
    time: WasmPtr<__wasi_timestamp_t>,
) -> __wasi_errno_t {
    log::info!("wasi::clock_time_get");
    let memory = ctx.memory(0);

    let out_addr = wasi_try!(time.deref(memory));
    if clock_id != __WASI_CLOCK_MONOTONIC {
        return __WASI_ENOTCAPABLE;
    }
    lazy_static::lazy_static! {
        static ref INITIAL: Instant = Instant::now();
    }
    // TODO: Precision
    out_addr.set(INITIAL.elapsed().as_nanos() as _);

    __WASI_ESUCCESS
}

/// ### `random_get()`
/// Fill buffer with high-quality random data.  This function may be slow and block
/// Inputs:
/// - `void *buf`
///     A pointer to a buffer where the random bytes will be written
/// - `size_t buf_len`
///     The number of bytes that will be written
pub fn random_get(ctx: &mut Ctx, buf: WasmPtr<u8, Array>, buf_len: u32) -> __wasi_errno_t {
    log::info!("wasi::random_get");
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let memory = ctx.memory(0);

    let buf = wasi_try!(buf.deref(memory, 0, buf_len));

    unsafe {
        let u8_buffer = &mut *(buf as *const [_] as *mut [_] as *mut [u8]);
        rng.fill(u8_buffer);
    }

    __WASI_ESUCCESS
}

fn read_bytes<T: io::Read>(
    mut reader: T,
    memory: &Memory,
    iovs_arr_cell: &[Cell<__wasi_iovec_t>],
) -> Result<u32, __wasi_errno_t> {
    let mut bytes_read = 0;

    for iov in iovs_arr_cell {
        let iov_inner = iov.get();
        let bytes = iov_inner.buf.deref(memory, 0, iov_inner.buf_len)?;
        let raw_bytes: &mut [u8] = unsafe { &mut *(bytes as *const [_] as *mut [_] as *mut [u8]) };
        bytes_read += reader.read(raw_bytes).map_err(|_| __WASI_EIO)? as u32;
    }
    Ok(bytes_read)
}

fn write_bytes_to_string(
    memory: &Memory,
    iovs_arr_cell: &[Cell<__wasi_ciovec_t>],
) -> Result<(u32, String), __wasi_errno_t> {
    let mut bytes = Vec::new();
    let bytes_written = write_bytes(&mut bytes, memory, iovs_arr_cell)?;
    Ok((bytes_written, String::from_utf8_lossy(&bytes).into_owned()))
}

#[must_use]
fn write_buffer_array(
    memory: &Memory,
    from: &[Vec<u8>],
    ptr_buffer: WasmPtr<WasmPtr<u8, Array>, Array>,
    buffer: WasmPtr<u8, Array>,
) -> __wasi_errno_t {
    let ptrs = wasi_try!(ptr_buffer.deref(memory, 0, from.len() as u32));

    let mut current_buffer_offset = 0;
    for ((i, sub_buffer), ptr) in from.iter().enumerate().zip(ptrs.iter()) {
        ptr.set(WasmPtr::new(buffer.offset() + current_buffer_offset));

        let cells =
            wasi_try!(buffer.deref(memory, current_buffer_offset, sub_buffer.len() as u32 + 1));

        for (cell, &byte) in cells.iter().zip(sub_buffer.iter().chain([0].iter())) {
            cell.set(byte);
        }
        current_buffer_offset += sub_buffer.len() as u32 + 1;
    }

    __WASI_ESUCCESS
}
