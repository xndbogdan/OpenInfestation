use std::ffi::{c_void, OsString};
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;

type Handle = *mut c_void;

const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const PROCESS_VM_READ: u32 = 0x0010;
const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

pub const RETAIL_IMAGE_BASE: usize = 0x0040_0000;
pub const RETAIL_MACHINE_I386: u16 = 0x014C;
pub const RETAIL_PE_TIMESTAMP: u32 = 0x396F_38A3;
pub const RETAIL_SIZE_OF_IMAGE: u32 = 0x0027_B000;
pub const RETAIL_SHA256: &str = "BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDCCC90BD575882921E4956";

#[repr(C)]
struct ProcessEntry32W {
    size: u32,
    usage: u32,
    process_id: u32,
    default_heap_id: usize,
    module_id: u32,
    threads: u32,
    parent_process_id: u32,
    priority_class_base: i32,
    flags: u32,
    exe_file: [u16; 260],
}

impl Default for ProcessEntry32W {
    fn default() -> Self {
        Self {
            size: size_of::<Self>() as u32,
            usage: 0,
            process_id: 0,
            default_heap_id: 0,
            module_id: 0,
            threads: 0,
            parent_process_id: 0,
            priority_class_base: 0,
            flags: 0,
            exe_file: [0; 260],
        }
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
    fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> Handle;
    fn ReadProcessMemory(
        process: Handle,
        address: *const c_void,
        buffer: *mut c_void,
        size: usize,
        bytes_read: *mut usize,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

struct OwnedHandle(Handle);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildFingerprint {
    pub machine: u16,
    pub timestamp: u32,
    pub image_base: u32,
    pub size_of_image: u32,
    pub expected_sha256: &'static str,
    pub structural_profile_verified: bool,
}

pub struct Process {
    handle: OwnedHandle,
    pub process_id: u32,
    pub exe_name: String,
    pub build: BuildFingerprint,
}

impl Process {
    pub fn attach(requested_pid: Option<u32>) -> Result<Self, String> {
        let (process_id, exe_name) = find_original_process(requested_pid)?;
        let handle =
            unsafe { OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return Err(format!(
                "could not open {exe_name} (PID {process_id}); run the inspector at the same privilege level"
            ));
        }
        let mut process = Self {
            handle: OwnedHandle(handle),
            process_id,
            exe_name,
            build: BuildFingerprint {
                machine: 0,
                timestamp: 0,
                image_base: 0,
                size_of_image: 0,
                expected_sha256: RETAIL_SHA256,
                structural_profile_verified: false,
            },
        };
        process.build = process.read_build_fingerprint()?;
        if !process.build.structural_profile_verified {
            return Err(format!(
                "unsupported Infest.exe build: machine={:#06X}, timestamp={:#010X}, image_base={:#010X}, size={:#010X}",
                process.build.machine,
                process.build.timestamp,
                process.build.image_base,
                process.build.size_of_image
            ));
        }
        Ok(process)
    }

    pub fn read_bytes(&self, address: usize, size: usize) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0u8; size];
        let mut read = 0usize;
        let ok = unsafe {
            ReadProcessMemory(
                self.handle.0,
                address as *const c_void,
                bytes.as_mut_ptr().cast(),
                size,
                &mut read,
            )
        };
        if ok == 0 || read != size {
            return Err(format!(
                "ReadProcessMemory({address:08X}, {size:#x}) read {read:#x} bytes"
            ));
        }
        Ok(bytes)
    }

    pub fn read_u32(&self, address: usize) -> Result<u32, String> {
        let bytes = self.read_bytes(address, 4)?;
        Ok(u32_at(&bytes, 0))
    }

    fn read_build_fingerprint(&self) -> Result<BuildFingerprint, String> {
        let dos = self.read_bytes(RETAIL_IMAGE_BASE, 0x40)?;
        if &dos[0..2] != b"MZ" {
            return Err("process image at 0x00400000 has no DOS MZ header".into());
        }
        let pe_offset = u32_at(&dos, 0x3C) as usize;
        if !(0x40..=0x1000).contains(&pe_offset) {
            return Err(format!("implausible PE header offset {pe_offset:#x}"));
        }
        let pe = self.read_bytes(RETAIL_IMAGE_BASE + pe_offset, 0x80)?;
        if &pe[0..4] != b"PE\0\0" {
            return Err("process image has no PE signature".into());
        }
        let machine = u16_at(&pe, 4);
        let timestamp = u32_at(&pe, 8);
        let optional_magic = u16_at(&pe, 24);
        if optional_magic != 0x010B {
            return Err(format!(
                "expected PE32 optional header, found {optional_magic:#06x}"
            ));
        }
        let image_base = u32_at(&pe, 24 + 28);
        let size_of_image = u32_at(&pe, 24 + 56);
        Ok(BuildFingerprint {
            machine,
            timestamp,
            image_base,
            size_of_image,
            expected_sha256: RETAIL_SHA256,
            structural_profile_verified: machine == RETAIL_MACHINE_I386
                && timestamp == RETAIL_PE_TIMESTAMP
                && image_base as usize == RETAIL_IMAGE_BASE
                && size_of_image == RETAIL_SIZE_OF_IMAGE,
        })
    }
}

fn find_original_process(requested_pid: Option<u32>) -> Result<(u32, String), String> {
    let mut candidates = find_original_processes()?;
    candidates.retain(|(pid, _)| requested_pid.is_none_or(|requested| requested == *pid));
    match candidates.len() {
        0 if requested_pid.is_some() => Err(format!(
            "PID {} is not a running Infest.exe",
            requested_pid.expect("checked")
        )),
        0 => Err("original Infest.exe is not running".into()),
        1 => Ok(candidates.remove(0)),
        _ => Err(format!(
            "multiple original Infest.exe processes are running ({}); select one with --pid",
            candidates
                .iter()
                .map(|(pid, name)| format!("{name}:{pid}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn find_original_processes() -> Result<Vec<(u32, String)>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err("CreateToolhelp32Snapshot failed".into());
    }
    let snapshot = OwnedHandle(snapshot);
    let mut entry = ProcessEntry32W::default();
    let mut candidates = Vec::new();
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while ok {
        let end = entry
            .exe_file
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(entry.exe_file.len());
        let name = OsString::from_wide(&entry.exe_file[..end])
            .to_string_lossy()
            .into_owned();
        if name.eq_ignore_ascii_case("infest.exe") {
            candidates.push((entry.process_id, name));
        }
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    Ok(candidates)
}

pub fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

pub fn plausible_heap_pointer(pointer: usize) -> bool {
    (0x1_0000..=0x7FFF_FFFF).contains(&pointer) && pointer & 3 == 0
}

#[cfg(test)]
mod tests {
    use super::plausible_heap_pointer;

    #[test]
    fn structural_pointers_are_bounded_and_aligned() {
        assert!(plausible_heap_pointer(0x1234_5678));
        assert!(!plausible_heap_pointer(0));
        assert!(!plausible_heap_pointer(0x1234_5679));
        assert!(!plausible_heap_pointer(0x8000_0000));
    }
}
