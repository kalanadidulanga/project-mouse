//! `ProcessMonitor` via `CreateToolhelp32Snapshot` (WINDOWS-API). ~1-3 ms; the caller
//! cadence-limits how often it runs.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use crate::platform::ProcessMonitor;

#[derive(Default)]
pub struct WindowsProcessMonitor;

impl WindowsProcessMonitor {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessMonitor for WindowsProcessMonitor {
    fn running_process_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        unsafe {
            let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(h) => h,
                Err(_) => return names,
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    let name = wide_to_string(&entry.szExeFile);
                    if !name.is_empty() {
                        names.push(name);
                    }
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        names
    }
}

fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
