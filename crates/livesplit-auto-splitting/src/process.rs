use quick_error::quick_error;
// use winapi::shared::minwindef::{BOOL, DWORD};
// use winapi::um::{
//     handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
//     memoryapi::{ReadProcessMemory, VirtualQueryEx},
//     processthreadsapi::{GetProcessTimes, OpenProcess},
//     psapi::GetModuleFileNameExW,
//     tlhelp32::{
//         CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
//         MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPPROCESS,
//     },
//     winnt::{
//         HANDLE, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_GUARD, PAGE_NOACCESS,
//         PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
//     },
// };

use proc_maps::{get_process_maps, MapRange, Pid};
use read_process_memory::{CopyAddress, TryIntoProcessHandle, ProcessHandle};
use sysinfo::{ProcessExt, SystemExt};

use std::collections::HashMap;
use std::ffi::OsString;
// use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::{mem, ptr, result, slice};

type Address = usize;
pub type Offset = isize;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        ListProcesses {}
        ProcessDoesntExist {}
        ListModules {}
        OpenProcess {}
        ModuleDoesntExist {}
        ReadMemory {}
    }
}

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Process {
    pid: Pid,
    handle: ProcessHandle,
    modules: HashMap<String, Address>
}

impl Process {
    pub fn path(&self) -> Option<PathBuf> {
        let mut system = sysinfo::System::new();
        system.refresh_process(self.pid);
        system.get_process_list().get(&0).map(|proc| proc.exe().to_path_buf())
    }

    pub fn with_name(name: &str) -> Result<Self> {
        let mut system = sysinfo::System::new();
        system.refresh_processes();

        let mut best_process = None::<(Pid, u64)>;
        for (pid, proc) in system.get_process_list() {
            if proc.name() == name {
                if best_process.map_or(true, |(_, current)| proc.start_time() > current) {
                    best_process = Some((proc.pid(), proc.start_time()));
                }
            }
        }

        if let Some((pid, _)) = best_process {
            Self::with_pid(pid)
        } else {
            Err(Error::ProcessDoesntExist)
        }
    }

    pub fn with_pid(pid: Pid) -> Result<Self> {
        let modules: HashMap<_,_> = get_process_maps(pid).map_err(|e| Error::ProcessDoesntExist)?
            .into_iter().filter(|range| range.filename().is_some()).map(|range| {
                (range.filename().clone().unwrap(), range.start())
            }).collect();
        let handle = (pid as read_process_memory::Pid).try_into_process_handle().map_err(|e| Error::ProcessDoesntExist)?;

        Ok(Self {
            pid,
            handle,
            modules
        })
    }

    pub fn module_address(&self, module: &str) -> Result<Address> {
        self.modules
            .get(module)
            .cloned()
            .ok_or(Error::ModuleDoesntExist)
    }

    pub fn read_buf(&self, address: Address, buf: &mut [u8]) -> Result<()> {
        self.handle.copy_address(address, buf).map_err(|_| Error::ReadMemory)
    }

    pub fn read<T: Copy>(&self, address: Address) -> Result<T> {
        // TODO Unsound af
        unsafe {
            let mut res = mem::uninitialized();
            let buf = slice::from_raw_parts_mut(mem::transmute(&mut res), mem::size_of::<T>());
            self.read_buf(address, buf).map(|_| res)
        }
    }

    fn memory_pages(&self, all: bool) -> Result<impl Iterator<Item = MapRange> + '_> {
        Ok(get_process_maps(self.pid).map_err(|e| Error::ProcessDoesntExist)?
            .into_iter().filter(move |range| all || range.is_read()))
    }

    pub fn scan_signature(&self, signature: &str) -> Result<Option<Address>> {
        let signature = Signature::new(signature);

        let mut page_buf = Vec::<u8>::new();

        for page in self.memory_pages(false)? {
            let base = page.start();
            let len = page.size();
            page_buf.clear();
            page_buf.reserve(len);
            unsafe {
                page_buf.set_len(len);
            }
            self.read_buf(base, &mut page_buf)?;
            if let Some(index) = signature.scan(&page_buf) {
                return Ok(Some(base + index as Address));
            }
        }
        Ok(None)
    }
}

struct Signature {
    bytes: Vec<u8>,
    mask: Vec<bool>,
    skip_offsets: [usize; 256],
}

impl Signature {
    fn new(signature: &str) -> Self {
        let mut bytes_iter = signature.bytes().filter_map(|b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 0xA),
            b'A'..=b'F' => Some(b - b'A' + 0xA),
            b'?' => Some(0x10),
            _ => None,
        });
        let (mut bytes, mut mask) = (Vec::new(), Vec::new());

        while let (Some(a), Some(b)) = (bytes_iter.next(), bytes_iter.next()) {
            let sig_byte = (a << 4) | b;
            let is_question_marks = a == 0x10 && b == 0x10;
            bytes.push(sig_byte);
            mask.push(is_question_marks);
        }

        let mut skip_offsets = [0; 256];

        let mut unknown = 0;
        let end = bytes.len() - 1;
        for (i, (&byte, mask)) in bytes.iter().zip(&mask).enumerate().take(end) {
            if !mask {
                skip_offsets[byte as usize] = end - i;
            } else {
                unknown = end - i;
            }
        }

        if unknown == 0 {
            unknown = bytes.len();
        }

        for offset in &mut skip_offsets[..] {
            if unknown < *offset || *offset == 0 {
                *offset = unknown;
            }
        }

        Self {
            bytes,
            mask,
            skip_offsets,
        }
    }

    fn scan(&self, buf: &[u8]) -> Option<usize> {
        let mut current = 0;
        let end = self.bytes.len() - 1;
        while current <= buf.len() - self.bytes.len() {
            let rem = &buf[current..];
            if rem
                .iter()
                .zip(&self.bytes)
                .zip(&self.mask)
                .all(|((&buf, &search), &mask)| buf == search || mask)
            {
                return Some(current);
            }
            let offset = self.skip_offsets[rem[end] as usize];
            current += offset;
        }
        None
    }
}
