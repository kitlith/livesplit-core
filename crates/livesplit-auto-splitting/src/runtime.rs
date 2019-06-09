// use crate::environment::{Environment, Imports};
use crate::pointer::PointerValue;
use crate::process::{Offset, Process};
use std::{error::Error, mem, str, thread, time::Duration};
// use wasmi::{
//     ExternVal, FuncInstance, FuncRef, MemoryRef, Module, ModuleInstance, ModuleRef, RuntimeValue,
// };
use wasmer_runtime::{func, imports, memory::MemoryView, Ctx, Func, Instance};

mod wasi;

pub struct Runtime {
    instance: Instance,
    environment: Environment,
    timer_state: TimerState,
    // should_start: Option<FuncRef>,
    // should_split: Option<FuncRef>,
    // should_reset: Option<FuncRef>,
    // is_loading: Option<FuncRef>,
    // game_time: Option<FuncRef>,
    // update: Option<FuncRef>,
    // disconnected: Option<FuncRef>,
    is_loading_val: Option<bool>,
    game_time_val: Option<f64>,
}

#[repr(u8)]
pub enum TimerState {
    NotRunning = 0,
    Running = 1,
    Finished = 2,
}

#[derive(Debug)]
pub enum TimerAction {
    Start,
    Split,
    Reset,
}

#[derive(Debug)]
enum EnvironmentError {
    InvalidProcessName,
    InvalidModuleName,
    InvalidPointerPathId,
    InvalidPointerType,
    TypeMismatch,
    Utf8DecodeError,
}

pub struct Environment {
    pub process_name: String,
    // TODO Undo pub
    pub pointer_paths: Vec<PointerPath>,
    pub tick_rate: Duration,
    pub process: Option<Process>,
    pub fs: wasi::FileSystem,
}

#[derive(Debug)]
pub struct PointerPath {
    pub module_name: String,
    pub offsets: Vec<i64>,
    // TODO Undo pub
    pub current: PointerValue,
    pub old: PointerValue,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            process_name: String::new(),
            pointer_paths: Vec::new(),
            tick_rate: Duration::from_secs(1) / 60,
            process: None,
            fs: wasi::FileSystem::new(),
        }
    }
}

impl Runtime {
    pub fn new(binary: &[u8]) -> Result<Self, Box<Error>> {
        let import_object = imports! {
            "env" => {
                "set_process_name" => func!(set_process_name),
                "push_pointer_path" => func!(push_pointer_path),
                "push_offset" => func!(push_offset),
                "get_u8" => func!(get_u8),
                "get_u16" => func!(get_u16),
                "get_u32" => func!(get_u32),
                "get_u64" => func!(get_u64),
                "get_i8" => func!(get_i8),
                "get_i16" => func!(get_i16),
                "get_i32" => func!(get_i32),
                "get_i64" => func!(get_i64),
                "get_f32" => func!(get_f32),
                "get_f64" => func!(get_f64),
                "scan_signature" => func!(scan_signature),
                "set_tick_rate" => func!(set_tick_rate),
                "print_message" => func!(print_message),
                "read_into_buf" => func!(read_into_buf),
            },
            "wasi_unstable" => {
                "args_get" => func!(wasi::args_get),
                "args_sizes_get" => func!(wasi::args_sizes_get),
                "clock_time_get" => func!(wasi::clock_time_get),
                "environ_get" => func!(wasi::environ_get),
                "environ_sizes_get" => func!(wasi::environ_sizes_get),
                "fd_close" => func!(wasi::fd_close),
                "fd_fdstat_get" => func!(wasi::fd_fdstat_get),
                "fd_filestat_get" => func!(wasi::fd_filestat_get),
                "fd_prestat_dir_name" => func!(wasi::fd_prestat_dir_name),
                "fd_prestat_get" => func!(wasi::fd_prestat_get),
                "fd_read" => func!(wasi::fd_read),
                "fd_seek" => func!(wasi::fd_seek),
                "fd_write" => func!(wasi::fd_write),
                "path_open" => func!(wasi::path_open),
                "proc_exit" => func!(wasi::proc_exit),
                "random_get" => func!(wasi::random_get),
            },
        };
        let mut instance = wasmer_runtime::instantiate(binary, &import_object).unwrap();

        let mut environment = Environment::new();
        instance.context_mut().data = &mut environment as *mut Environment as *mut _;
        if let Ok(func) = instance.func::<(), ()>("_start") {
            func.call()
                .map_err(|e| format!("Failed to run _start function: {}", e))?;
        }
        instance
            .call("configure", &[])
            .map_err(|e| format!("Failed to run configure function: {}", e))?;

        // let should_start = instance
        //     .export_by_name("should_start")
        //     .and_then(|e| e.as_func()?.clone().into());
        // let should_split = instance
        //     .export_by_name("should_split")
        //     .and_then(|e| e.as_func()?.clone().into());
        // let should_reset = instance
        //     .export_by_name("should_reset")
        //     .and_then(|e| e.as_func()?.clone().into());
        // let is_loading = instance
        //     .export_by_name("is_loading")
        //     .and_then(|e| e.as_func()?.clone().into());
        // let game_time = instance
        //     .export_by_name("game_time")
        //     .and_then(|e| e.as_func()?.clone().into());
        // let update = instance
        //     .export_by_name("update")
        //     .and_then(|e| e.as_func()?.clone().into());
        // let disconnected = instance
        //     .export_by_name("disconnected")
        //     .and_then(|e| e.as_func()?.clone().into());

        Ok(Self {
            instance,
            environment,
            timer_state: TimerState::NotRunning,
            // should_start,
            // should_split,
            // should_reset,
            // is_loading,
            // game_time,
            // update,
            // disconnected,
            is_loading_val: None,
            game_time_val: None,
        })
    }

    pub fn sleep(&self) {
        thread::sleep(self.environment.tick_rate);
    }

    pub fn step(&mut self) -> Result<Option<TimerAction>, Box<Error>> {
        let mut just_connected = false;
        if self.environment.process.is_none() {
            self.environment.process = match Process::with_name(&self.environment.process_name) {
                Ok(p) => Some(p),
                Err(_) => return Ok(None),
            };
            log::info!(target: "Auto Splitter", "Hooked");
            just_connected = true;
        }

        if self.update_values(just_connected).is_err() {
            // TODO: Only checks for disconnected if we actually have pointer paths
            log::info!(target: "Auto Splitter", "Unhooked");
            self.environment.process = None;
            // if let Some(func) = &self.disconnected {
            //     FuncInstance::invoke(func, &[], &mut self.environment)?;
            // }
            return Ok(None);
        }
        // println!("{:#?}", self.environment);
        self.run_script()
    }

    pub fn set_state(&mut self, state: TimerState) {
        self.timer_state = state;
    }

    fn update_values(&mut self, just_connected: bool) -> Result<(), Box<Error>> {
        // let process = self
        //     .environment
        //     .process
        //     .as_mut()
        //     .expect("The process should be connected at this point");

        // for pointer_path in &mut self.environment.pointer_paths {
        //     let mut address = if !pointer_path.module_name.is_empty() {
        //         process.module_address(&pointer_path.module_name)?
        //     } else {
        //         0
        //     };
        //     let mut offsets = pointer_path.offsets.iter().cloned().peekable();
        //     if process.is_64bit() {
        //         while let Some(offset) = offsets.next() {
        //             address = (address as Offset).wrapping_add(offset) as u64;
        //             if offsets.peek().is_some() {
        //                 address = process.read(address)?;
        //             }
        //         }
        //     } else {
        //         while let Some(offset) = offsets.next() {
        //             address = (address as i32).wrapping_add(offset as i32) as u64;
        //             if offsets.peek().is_some() {
        //                 address = process.read::<u32>(address)? as u64;
        //             }
        //         }
        //     }
        //     match &mut pointer_path.old {
        //         PointerValue::U8(v) => *v = process.read(address)?,
        //         PointerValue::U16(v) => *v = process.read(address)?,
        //         PointerValue::U32(v) => *v = process.read(address)?,
        //         PointerValue::U64(v) => *v = process.read(address)?,
        //         PointerValue::I8(v) => *v = process.read(address)?,
        //         PointerValue::I16(v) => *v = process.read(address)?,
        //         PointerValue::I32(v) => *v = process.read(address)?,
        //         PointerValue::I64(v) => *v = process.read(address)?,
        //         PointerValue::F32(v) => *v = process.read(address)?,
        //         PointerValue::F64(v) => *v = process.read(address)?,
        //         PointerValue::String(_) => unimplemented!(),
        //     }
        // }

        // if just_connected {
        //     for pointer_path in &mut self.environment.pointer_paths {
        //         pointer_path.current.clone_from(&pointer_path.old);
        //     }
        // } else {
        //     for pointer_path in &mut self.environment.pointer_paths {
        //         mem::swap(&mut pointer_path.current, &mut pointer_path.old);
        //     }
        // }

        Ok(())
    }

    fn run_script(&mut self) -> Result<Option<TimerAction>, Box<Error>> {
        self.instance.context_mut().data = &mut self.environment as *mut Environment as *mut _;

        if let Ok(func) = self.instance.func::<(), ()>("update") {
            // TODO: Don't panic
            func.call().unwrap();
        }

        match &self.timer_state {
            TimerState::NotRunning => {
                if let Ok(func) = self.instance.func::<(), i32>("should_start") {
                    let ret_val = func.call().unwrap();

                    if ret_val != 0 {
                        return Ok(Some(TimerAction::Start));
                    }
                }
            }
            TimerState::Running => {
                if let Ok(func) = self.instance.func::<(), i32>("is_loading") {
                    let ret_val = func.call().unwrap();

                    self.is_loading_val = Some(ret_val != 0);
                }
                if let Ok(func) = self.instance.func::<(), f64>("game_time") {
                    let ret_val = func.call().unwrap();

                    self.game_time_val = if ret_val.is_nan() {
                        None
                    } else {
                        Some(ret_val)
                    };
                }

                if let Ok(func) = self.instance.func::<(), i32>("should_split") {
                    let ret_val = func.call().unwrap();

                    if ret_val != 0 {
                        return Ok(Some(TimerAction::Split));
                    }
                }
                if let Ok(func) = self.instance.func::<(), i32>("should_reset") {
                    let ret_val = func.call().unwrap();

                    if ret_val != 0 {
                        return Ok(Some(TimerAction::Reset));
                    }
                }
            }
            TimerState::Finished => {
                if let Ok(func) = self.instance.func::<(), i32>("should_reset") {
                    let ret_val = func.call().unwrap();

                    if ret_val != 0 {
                        return Ok(Some(TimerAction::Reset));
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn is_loading(&self) -> Option<bool> {
        self.is_loading_val
    }

    pub fn game_time(&self) -> Option<f64> {
        self.game_time_val
    }
}

fn read_bytes(memory: &MemoryView<u8>, ptr: usize, len: usize) -> Vec<u8> {
    memory[ptr..][..len].iter().map(|c| c.get()).collect()
}

fn read_string(memory: &MemoryView<u8>, ptr: usize, len: usize) -> String {
    // TODO: Don't panic
    String::from_utf8(read_bytes(memory, ptr, len)).unwrap()
}

fn print_message(ctx: &mut Ctx, ptr: u32, len: u32) {
    let ptr = ptr as usize;
    let len = len as usize;
    let memory = ctx.memory(0).view();
    let message = read_string(&memory, ptr, len);
    log::info!(target: "Auto Splitter", "{}", message);
}

fn set_process_name(ctx: &mut Ctx, ptr: u32, len: u32) {
    let ptr = ptr as usize;
    let len = len as usize;
    let memory = ctx.memory(0).view();
    let env = unsafe { &mut *(ctx.data as *mut Environment) };

    env.process_name = read_string(&memory, ptr, len);
}

fn push_pointer_path(ctx: &mut Ctx, ptr: u32, len: u32, pointer_type: u32) -> u32 {
    use crate::pointer::{PointerType, PointerValue};
    use num_traits::FromPrimitive;

    let ptr = ptr as usize;
    let len = len as usize;
    let memory = ctx.memory(0).view();
    let env = unsafe { &mut *(ctx.data as *mut Environment) };

    let pointer_type = PointerType::from_u8(pointer_type as u8)
        .ok_or_else(|| EnvironmentError::InvalidPointerType)
        .unwrap();
    let current = match pointer_type {
        PointerType::U8 => PointerValue::U8(0),
        PointerType::U16 => PointerValue::U16(0),
        PointerType::U32 => PointerValue::U32(0),
        PointerType::U64 => PointerValue::U64(0),
        PointerType::I8 => PointerValue::I8(0),
        PointerType::I16 => PointerValue::I16(0),
        PointerType::I32 => PointerValue::I32(0),
        PointerType::I64 => PointerValue::I64(0),
        PointerType::F32 => PointerValue::F32(0.0),
        PointerType::F64 => PointerValue::F64(0.0),
        PointerType::String => PointerValue::String(String::new()),
    };

    let module_name = read_string(&memory, ptr, len);

    let id = env.pointer_paths.len();
    env.pointer_paths.push(PointerPath {
        module_name,
        offsets: Vec::new(),
        old: current.clone(),
        current,
    });

    id as _
}

fn push_offset(ctx: &mut Ctx, pointer_path_id: u32, offset: i64) {
    let pointer_path_id = pointer_path_id as usize;
    let env = unsafe { &mut *(ctx.data as *mut Environment) };

    let pointer_path = env
        .pointer_paths
        .get_mut(pointer_path_id)
        .ok_or_else(|| EnvironmentError::InvalidPointerPathId)
        .unwrap();
    pointer_path.offsets.push(offset);
}

fn get_u8(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> u32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::U8(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_u16(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> u32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::U16(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_u32(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> u32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::U32(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_u64(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> u64 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::U64(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_i8(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> i32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::I8(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_i16(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> i32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::I16(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_i32(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> i32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::I32(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_i64(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> i64 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::I64(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_f32(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> f32 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::F32(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn get_f64(ctx: &mut Ctx, pointer_path_id: u32, current: i32) -> f64 {
    get_val(pointer_path_id, current, ctx, |v| match *v {
        PointerValue::F64(v) => Some(v as _),
        _ => None,
    })
    .unwrap()
}

fn scan_signature(ctx: &mut Ctx, ptr: u32, len: u32) -> u64 {
    let ptr = ptr as usize;
    let len = len as usize;
    let memory = ctx.memory(0).view();
    let env = unsafe { &mut *(ctx.data as *mut Environment) };

    // TODO: Don't panic
    if let Some(process) = &env.process {
        let signature = read_string(&memory, ptr, len);
        let address = process.scan_signature(&signature).unwrap();
        return address.unwrap_or(0);
    }

    0
}

fn set_tick_rate(ctx: &mut Ctx, ticks_per_sec: f64) {
    log::info!("New Tick Rate: {:?}", ticks_per_sec);
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    env.tick_rate = Duration::from_nanos((1_000_000_000.0 / ticks_per_sec).round() as u64);
}

fn read_into_buf(ctx: &mut Ctx, address: u64, buf: u32, buf_len: u32) {
    let buf = buf as usize;
    let buf_len = buf_len as usize;
    let env = unsafe { &mut *(ctx.data as *mut Environment) };
    let memory = ctx.memory(0).view();

    // TODO: Don't panic
    let buf = &memory[buf..buf + buf_len];
    if let Some(process) = &env.process {
        let mut byte_buf = vec![0; buf.len()];
        process.read_buf(address, &mut byte_buf).unwrap();
        for (dst, src) in buf.iter().zip(byte_buf) {
            dst.set(src);
        }
    }
}

fn get_val<T>(
    pointer_path_id: u32,
    current: i32,
    ctx: &mut Ctx,
    convert: impl FnOnce(&PointerValue) -> Option<T>,
) -> Result<T, EnvironmentError> {
    let pointer_path_id = pointer_path_id as usize;
    let current = current != 0;
    let env = unsafe { &mut *(ctx.data as *mut Environment) };

    let pointer_path = env
        .pointer_paths
        .get(pointer_path_id)
        .ok_or_else(|| EnvironmentError::InvalidPointerPathId)
        .unwrap();
    let value = if current {
        &pointer_path.current
    } else {
        &pointer_path.old
    };

    convert(value).ok_or(EnvironmentError::TypeMismatch)
}

// fn into_memory(extern_val: ExternVal) -> Result<MemoryRef, Box<Error>> {
//     match extern_val {
//         ExternVal::Memory(memory) => Ok(memory),
//         _ => Err("Memory is not exported correctly".into()),
//     }
// }
