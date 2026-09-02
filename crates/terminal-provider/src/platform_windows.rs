//! Windows ConPTY platform session (M8-A platform split): the ConPTY
//! implementation moved out of lib.rs so the Unix openpty path can share
//! the same session contract.

use std::os::raw::c_void;
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
};

use crate::{MAX_WRITE_BYTES, OutputEvent, PtyError};

/// Send+Sync wrapper for Win32 kernel handles (moved from lib.rs; the
/// reader thread carries the raw value as usize for the session lifetime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawHandle(*mut c_void);

unsafe impl Send for RawHandle {}
unsafe impl Sync for RawHandle {}

impl RawHandle {
    fn is_null(self) -> bool {
        self.0.is_null()
    }
}

/// Windows ConPTY session state (one session = console + process + pipes).
#[derive(Debug)]
pub struct PlatformSession {
    pseudo_console: RawHandle,
    process: RawHandle,
    read_handle: Option<RawHandle>,
    write_handle: Option<RawHandle>,
    /// Idempotency guard: Session::terminate runs the teardown and the
    /// platform Drop runs it again on unwind — closing an already-closed
    /// handle corrupts the heap (0xC0000374).
    io_terminated: bool,
}

/// Default shell: %COMSPEC% resolved through the environment (CreateProcessW
/// does not expand environment variables; the old inline code expanded it
/// Rust-side — kept here for parity).
pub fn default_shell() -> String {
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

/// Resolve a shell name to an executable path (Windows list). Accepts
/// the cross-platform schema union: pwsh (PowerShell Core) maps to the
/// same executable as powershell on Windows; sh/bash/zsh are Unix-only
/// and rejected here (mirror of the Unix resolve_shell).
pub fn resolve_shell(shell: Option<&str>) -> Result<String, PtyError> {
    match shell.unwrap_or("default") {
        "default" | "cmd" => Ok(default_shell()),
        "powershell" | "pwsh" => Ok("powershell.exe".to_string()),
        _ => Err(PtyError::InvalidShell),
    }
}

/// Size in bytes of a thread attribute list with one attribute.
fn attribute_size() -> usize {
    let mut size = 0usize;
    unsafe {
        // First call with a null list only returns the required size.
        let _ = InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
    }
    size
}

/// Zeroed allocation large enough for the attribute list.
fn allocate_zeroed(size_bytes: usize) -> *mut c_void {
    let layout = std::alloc::Layout::from_size_align(size_bytes.max(1), 8).expect("layout");
    unsafe { std::alloc::alloc_zeroed(layout) as *mut c_void }
}

impl PlatformSession {
    /// Spawn a shell under a fresh ConPTY. All acquired handles are
    /// released on the error path.
    pub fn spawn(
        shell_path: &str,
        cols: u16,
        rows: u16,
        cwd: Option<&str>,
    ) -> Result<Self, PtyError> {
        unsafe {
            let mut input_read: HANDLE = std::ptr::null_mut();
            let mut input_write: HANDLE = std::ptr::null_mut();
            let mut output_read: HANDLE = std::ptr::null_mut();
            let mut output_write: HANDLE = std::ptr::null_mut();
            if CreatePipe(&mut input_read, &mut input_write, std::ptr::null(), 0) == 0 {
                return Err(PtyError::SpawnUnavailable);
            }
            if CreatePipe(&mut output_read, &mut output_write, std::ptr::null(), 0) == 0 {
                CloseHandle(input_read);
                CloseHandle(input_write);
                return Err(PtyError::SpawnUnavailable);
            }
            let mut pseudo_console: HPCON = 0;
            let size = COORD {
                X: cols as i16,
                Y: rows as i16,
            };
            let created =
                CreatePseudoConsole(size, input_read, output_write, 0, &mut pseudo_console);
            if created != 0 {
                CloseHandle(input_read);
                CloseHandle(input_write);
                CloseHandle(output_read);
                CloseHandle(output_write);
                return Err(PtyError::SpawnUnavailable);
            }

            // Extended startup info with the pseudoconsole attribute list.
            let mut startup_info: STARTUPINFOEXW = std::mem::zeroed();
            startup_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup_info.StartupInfo.hStdInput = input_read;
            startup_info.StartupInfo.hStdOutput = output_write;
            startup_info.StartupInfo.hStdError = output_write;

            let attribute_size = attribute_size();
            let attribute_list = allocate_zeroed(attribute_size);
            let mut initialized_size = attribute_size;
            if InitializeProcThreadAttributeList(attribute_list, 1, 0, &mut initialized_size) == 0 {
                CloseHandle(input_read);
                CloseHandle(input_write);
                CloseHandle(output_read);
                CloseHandle(output_write);
                ClosePseudoConsole(pseudo_console);
                std::alloc::dealloc(
                    attribute_list as *mut u8,
                    std::alloc::Layout::from_size_align(attribute_size.max(1), 8).expect("layout"),
                );
                return Err(PtyError::SpawnUnavailable);
            }
            if UpdateProcThreadAttribute(
                attribute_list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                pseudo_console as *const c_void,
                std::mem::size_of::<HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ) == 0
            {
                CloseHandle(input_read);
                CloseHandle(input_write);
                CloseHandle(output_read);
                CloseHandle(output_write);
                ClosePseudoConsole(pseudo_console);
                DeleteProcThreadAttributeList(attribute_list);
                std::alloc::dealloc(
                    attribute_list as *mut u8,
                    std::alloc::Layout::from_size_align(attribute_size.max(1), 8).expect("layout"),
                );
                return Err(PtyError::SpawnUnavailable);
            }
            startup_info.lpAttributeList = attribute_list;

            let mut process_info: PROCESS_INFORMATION = std::mem::zeroed();
            // CreateProcessW rewrites lpCommandLine in place (it resolves the
            // executable path into the buffer), so it must be a mutable
            // buffer — passing a read-only pointer corrupts the heap.
            let mut command_line = widestring(shell_path);
            let cwd_wide = cwd.map(widestring);
            let created = CreateProcessW(
                std::ptr::null(),
                command_line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT,
                std::ptr::null(),
                cwd_wide
                    .as_deref()
                    .map_or(std::ptr::null(), |dir| dir.as_ptr()),
                &startup_info.StartupInfo,
                &mut process_info,
            );
            DeleteProcThreadAttributeList(attribute_list);
            std::alloc::dealloc(
                attribute_list as *mut u8,
                std::alloc::Layout::from_size_align(attribute_size.max(1), 8).expect("layout"),
            );
            if created == 0 {
                CloseHandle(input_read);
                CloseHandle(input_write);
                CloseHandle(output_read);
                CloseHandle(output_write);
                ClosePseudoConsole(pseudo_console);
                return Err(PtyError::SpawnUnavailable);
            }
            CloseHandle(process_info.hThread);
            CloseHandle(input_read);
            CloseHandle(output_write);

            Ok(Self {
                pseudo_console: RawHandle(pseudo_console as *mut c_void),
                process: RawHandle(process_info.hProcess as *mut c_void),
                read_handle: Some(RawHandle(output_read as *mut c_void)),
                write_handle: Some(RawHandle(input_write as *mut c_void)),
                io_terminated: false,
            })
        }
    }

    /// Reader token: the output read handle as usize (Send-safe; the
    /// session keeps the handle alive for the reader lifetime).
    pub fn reader_token(&self) -> usize {
        self.read_handle.map_or(0, |handle| handle.0 as usize)
    }

    /// Write data to the ConPTY input pipe.
    pub fn write(&self, data: &str) -> Result<(), PtyError> {
        let bytes = data.as_bytes();
        if bytes.len() > MAX_WRITE_BYTES {
            return Err(PtyError::WriteTooLarge);
        }
        let handle = self.write_handle.ok_or(PtyError::StateUnavailable)?.0 as HANDLE;
        let mut written: u32 = 0;
        let ok = unsafe {
            WriteFile(
                handle,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || written as usize != bytes.len() {
            return Err(PtyError::Io);
        }
        Ok(())
    }

    /// Resize the pseudo console.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        let ok = unsafe { ResizePseudoConsole(self.pseudo_console.0 as isize, size) };
        if ok != 0 {
            return Err(PtyError::Io);
        }
        Ok(())
    }

    /// Read the ConPTY output pipe and forward chunks as events (polling
    /// PeekNamedPipe reader — never blocks indefinitely, M6-C1 deadlock
    /// fix).
    pub fn spawn_reader(
        session_id: String,
        handle: usize,
        event_tx: std::sync::mpsc::SyncSender<OutputEvent>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            let mut seq = 0u64;
            loop {
                if stop.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let mut available = 0u32;
                let peek_ok = unsafe {
                    windows_sys::Win32::System::Pipes::PeekNamedPipe(
                        handle as *mut c_void,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if peek_ok == 0 {
                    break;
                }
                if available == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                let mut read = 0u32;
                let result = unsafe {
                    ReadFile(
                        handle as *mut c_void,
                        buffer.as_mut_ptr(),
                        buffer.len() as u32,
                        &mut read,
                        std::ptr::null_mut(),
                    )
                };
                if result == 0 || read == 0 {
                    break;
                }
                seq += 1;
                if event_tx
                    .try_send(OutputEvent {
                        session_id: session_id.clone(),
                        seq,
                        data: String::from_utf8_lossy(&buffer[..read as usize]).into_owned(),
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    /// Teardown phase 1 (stop flag handled by the caller): close the write
    /// pipe, kill the process, close the console. The read handle stays
    /// open until close_read - the reader thread may be mid-poll on it
    /// (closing it early corrupts the heap when the handle is reused).
    pub fn terminate_io(&mut self) {
        if self.io_terminated {
            return;
        }
        self.io_terminated = true;
        unsafe {
            if let Some(handle) = self.write_handle.take() {
                CloseHandle(handle.0 as HANDLE);
            }
            if !self.process.is_null() {
                let _ = TerminateProcess(self.process.0 as HANDLE, 1);
                CloseHandle(self.process.0 as HANDLE);
                self.process = RawHandle(std::ptr::null_mut());
            }
            ClosePseudoConsole(self.pseudo_console.0 as isize);
        }
    }

    /// Teardown phase 2: the reader thread is joined; close the read pipe.
    pub fn close_read(&mut self) {
        unsafe {
            if let Some(handle) = self.read_handle.take() {
                CloseHandle(handle.0 as HANDLE);
            }
        }
    }
}

impl Drop for PlatformSession {
    fn drop(&mut self) {
        // The reader is owned by the Session and joined before close_read;
        // dropping the platform session only needs the I/O teardown.
        self.terminate_io();
        self.close_read();
    }
}

/// UTF-16 wide string for the Win32 API.
fn widestring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
