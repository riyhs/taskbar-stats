use std::ffi::c_void;
use std::mem::size_of;
use std::slice;

use windows::core::{w, PCWSTR};
use windows::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessorPerformanceInformation,
};
use windows::Win32::Foundation::{FILETIME, NTSTATUS};
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetActiveProcessorCount, GetSystemTimes, ALL_PROCESSOR_GROUPS,
};
use windows::Win32::System::WindowsProgramming::{
    DRIVE_FIXED, SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
};

use crate::disk;
use crate::gpu::{GpuInfo, GpuSampler};

pub const HISTORY_LEN: usize = 60;

#[derive(Clone)]
pub struct History {
    samples: [f32; HISTORY_LEN],
    next: usize,
    filled: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            samples: [0.0; HISTORY_LEN],
            next: 0,
            filled: 0,
        }
    }
}

impl History {
    pub fn push(&mut self, value: f32) {
        if !value.is_finite() {
            return;
        }
        self.samples[self.next] = value.clamp(0.0, 1.0);
        self.next = (self.next + 1) % HISTORY_LEN;
        self.filled = (self.filled + 1).min(HISTORY_LEN);
    }

    pub fn recent(&self) -> Vec<f32> {
        let start = (self.next + HISTORY_LEN - self.filled) % HISTORY_LEN;
        (0..self.filled)
            .map(|i| self.samples[(start + i) % HISTORY_LEN])
            .collect()
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub cpu_load: f32,
    pub cpu_temp: Option<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub net_down_bps: f64,
    pub net_up_bps: f64,
    pub cores: Vec<f32>,
    pub gpus: Vec<GpuInfo>,
    pub disk_temp: Option<f32>,
    pub cpu_history: Vec<f32>,
    pub mem_history: Vec<f32>,
    pub disk_history: Vec<f32>,
    pub net_history: Vec<f32>,
}

pub struct Sampler {
    previous: Vec<CpuTimes>,
    previous_system: Option<(u64, u64, u64)>,
    temperature: Option<TemperatureQuery>,
    gpu: GpuSampler,
    network: Option<NetworkCounters>,
    snapshot: Snapshot,
    cpu_history: History,
    mem_history: History,
    disk_history: History,
    net_history: History,
    net_peak: f64,
}

#[derive(Clone, Copy, Default)]
struct CpuTimes {
    idle: i64,
    kernel: i64,
    user: i64,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            previous: Vec::new(),
            previous_system: None,
            temperature: TemperatureQuery::new(),
            gpu: GpuSampler::new(),
            network: NetworkCounters::new(),
            snapshot: Snapshot::default(),
            cpu_history: History::default(),
            mem_history: History::default(),
            disk_history: History::default(),
            net_history: History::default(),
            net_peak: 1.0,
        }
    }

    pub fn gpu_names(&self) -> Vec<String> {
        self.gpu.names()
    }

    pub fn sample(&mut self) -> Snapshot {
        let system_load = self.system_load();
        let times = cpu_times();
        let mut cores = Vec::new();
        let mut total_busy = 0.0f64;
        let mut total_all = 0.0f64;

        if self.previous.len() == times.len() {
            for (now, before) in times.iter().zip(self.previous.iter()) {
                let idle = (now.idle - before.idle).max(0) as f64;
                let kernel = (now.kernel - before.kernel).max(0) as f64;
                let user = (now.user - before.user).max(0) as f64;
                let all = kernel + user;
                let busy = (all - idle).max(0.0);
                let load = if all > 0.0 { busy / all } else { 0.0 };
                cores.push(load as f32);
                total_busy += busy;
                total_all += all;
            }
        } else {
            cores.resize(times.len(), 0.0);
        }
        self.previous = times;

        let cpu_load = system_load.map(|load| load * 100.0).unwrap_or_else(|| {
            if total_all > 0.0 {
                (total_busy / total_all * 100.0) as f32
            } else {
                0.0
            }
        });
        let (mem_used, mem_total) = memory();
        let (disk_used, disk_total) = disk();
        let cpu_temp = self.temperature.as_ref().and_then(|t| t.read());
        let gpus = self.gpu.sample();
        let disk_temp = disk::system_disk_temp();
        let (net_down_bps, net_up_bps) = self
            .network
            .as_ref()
            .and_then(NetworkCounters::read)
            .unwrap_or((0.0, 0.0));

        let mem_pct = if mem_total > 0 {
            mem_used as f32 / mem_total as f32
        } else {
            0.0
        };
        let disk_pct = if disk_total > 0 {
            disk_used as f32 / disk_total as f32
        } else {
            0.0
        };
        self.cpu_history.push(cpu_load / 100.0);
        self.mem_history.push(mem_pct);
        self.disk_history.push(disk_pct);
        self.net_peak = self.net_peak.max(net_down_bps + net_up_bps);
        let net_total = net_down_bps + net_up_bps;
        self.net_history.push(if self.net_peak > 0.0 {
            (net_total / self.net_peak) as f32
        } else {
            0.0
        });

        self.snapshot = Snapshot {
            cpu_load,
            cpu_temp,
            mem_used,
            mem_total,
            disk_used,
            disk_total,
            net_down_bps,
            net_up_bps,
            cores,
            gpus,
            disk_temp,
            cpu_history: self.cpu_history.recent(),
            mem_history: self.mem_history.recent(),
            disk_history: self.disk_history.recent(),
            net_history: self.net_history.recent(),
        };
        self.snapshot.clone()
    }

    fn system_load(&mut self) -> Option<f32> {
        let mut idle = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
            return None;
        }
        let now = (filetime(idle), filetime(kernel), filetime(user));
        let (prev_idle, prev_kernel, prev_user) = self.previous_system.replace(now)?;
        let idle_delta = now.0.saturating_sub(prev_idle);
        let total_delta = now
            .1
            .saturating_sub(prev_kernel)
            .saturating_add(now.2.saturating_sub(prev_user));
        let busy = total_delta.saturating_sub(idle_delta);
        if total_delta == 0 {
            return None;
        }
        Some((busy as f32 / total_delta as f32).clamp(0.0, 1.0))
    }
}

fn filetime(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}

fn cpu_times() -> Vec<CpuTimes> {
    let count = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }.max(1) as usize;
    let mut buffer: Vec<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION> =
        vec![SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default(); count];
    let mut returned = 0u32;
    let size = (buffer.len() * size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>()) as u32;

    let status = unsafe {
        NtQuerySystemInformation(
            SystemProcessorPerformanceInformation,
            buffer.as_mut_ptr().cast::<c_void>(),
            size,
            &mut returned,
        )
    };
    if status.0 < 0 {
        let needed =
            (returned as usize / size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>()).max(1);
        buffer.resize(needed, SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default());
        let retry_size =
            (buffer.len() * size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>()) as u32;
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessorPerformanceInformation,
                buffer.as_mut_ptr().cast::<c_void>(),
                retry_size,
                &mut returned,
            )
        };
        if status.0 < 0 {
            return Vec::new();
        }
    }

    let actual = (returned as usize / size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>())
        .min(buffer.len());
    buffer[..actual]
        .iter()
        .map(|t| CpuTimes {
            idle: t.IdleTime,
            kernel: t.KernelTime,
            user: t.UserTime,
        })
        .collect()
}

fn memory() -> (u64, u64) {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_ok() {
        (
            status.ullTotalPhys.saturating_sub(status.ullAvailPhys),
            status.ullTotalPhys,
        )
    } else {
        (0, 0)
    }
}

fn disk() -> (u64, u64) {
    let mut free = 0u64;
    let mut total = 0u64;
    let drives = unsafe { GetLogicalDrives() };
    for letter in 0..26u16 {
        if drives & (1 << letter) == 0 {
            continue;
        }
        let root = [b'A' as u16 + letter, b':' as u16, b'\\' as u16, 0];
        if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } != DRIVE_FIXED
            || !disk::volume_has_extents(b'A' as u16 + letter)
        {
            continue;
        }
        let mut filesystem = [0u16; 16];
        if unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                None,
                None,
                None,
                None,
                Some(&mut filesystem),
            )
        }
        .is_err()
        {
            continue;
        }
        let filesystem = String::from_utf16_lossy(&filesystem);
        if !matches!(filesystem.trim_end_matches('\0'), "NTFS" | "ReFS") {
            continue;
        }
        let mut drive_total = 0u64;
        let mut drive_free = 0u64;
        if unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(root.as_ptr()),
                None,
                Some(&mut drive_total),
                Some(&mut drive_free),
            )
        }
        .is_ok()
        {
            total = total.saturating_add(drive_total);
            free = free.saturating_add(drive_free);
        }
    }
    if total > 0 {
        (total.saturating_sub(free), total)
    } else {
        (0, 0)
    }
}

struct NetworkCounters {
    query: isize,
    received: isize,
    sent: isize,
}

impl NetworkCounters {
    fn new() -> Option<Self> {
        unsafe {
            let mut query = 0isize;
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let mut received = 0isize;
            let mut sent = 0isize;
            let received_status = PdhAddEnglishCounterW(
                query,
                w!(r"\Network Interface(*)\Bytes Received/sec"),
                0,
                &mut received,
            );
            let sent_status = PdhAddEnglishCounterW(
                query,
                w!(r"\Network Interface(*)\Bytes Sent/sec"),
                0,
                &mut sent,
            );
            if received_status != 0 || sent_status != 0 {
                let _ = PdhCloseQuery(query);
                return None;
            }
            let _ = PdhCollectQueryData(query);
            Some(Self {
                query,
                received,
                sent,
            })
        }
    }

    fn read(&self) -> Option<(f64, f64)> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            Some((counter_sum(self.received)?, counter_sum(self.sent)?))
        }
    }
}

impl Drop for NetworkCounters {
    fn drop(&mut self) {
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

fn counter_sum(counter: isize) -> Option<f64> {
    unsafe {
        let mut size = 0u32;
        let mut count = 0u32;
        let status =
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
        if status != PDH_MORE_DATA && status != 0 {
            return None;
        }
        if size == 0 || count == 0 {
            return Some(0.0);
        }
        let mut words = vec![0u64; (size as usize).div_ceil(8)];
        let status = PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(words.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>()),
        );
        if status != 0 {
            return None;
        }
        let items = slice::from_raw_parts(
            words.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
            count as usize,
        );
        let mut total = 0.0;
        for item in items {
            if item.FmtValue.CStatus != 0 {
                continue;
            }
            let value = item.FmtValue.Anonymous.doubleValue;
            if value.is_finite() && value > 0.0 {
                total += value;
            }
        }
        Some(total)
    }
}

struct TemperatureQuery {
    query: isize,
    counter: isize,
    tenths_kelvin: bool,
}

impl TemperatureQuery {
    fn new() -> Option<Self> {
        unsafe {
            let mut query = 0isize;
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let mut counter = 0isize;
            let mut tenths_kelvin = true;
            let mut status = PdhAddEnglishCounterW(
                query,
                w!("\\Thermal Zone Information(*)\\High Precision Temperature"),
                0,
                &mut counter,
            );
            if status != 0 {
                tenths_kelvin = false;
                status = PdhAddEnglishCounterW(
                    query,
                    w!("\\Thermal Zone Information(*)\\Temperature"),
                    0,
                    &mut counter,
                );
            }
            if status != 0 {
                let _ = PdhCloseQuery(query);
                return None;
            }
            let _ = PdhCollectQueryData(query);
            Some(Self {
                query,
                counter,
                tenths_kelvin,
            })
        }
    }

    fn read(&self) -> Option<f32> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                None,
            );
            if status != PDH_MORE_DATA && status != 0 {
                return None;
            }
            if size == 0 || count == 0 {
                return None;
            }
            let mut words = vec![0u64; (size as usize).div_ceil(8)];
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                Some(words.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>()),
            );
            if status != 0 {
                return None;
            }
            let items = slice::from_raw_parts(
                words.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
                count as usize,
            );
            let mut best: Option<f32> = None;
            for item in items {
                if item.FmtValue.CStatus != 0 {
                    continue;
                }
                let value = item.FmtValue.Anonymous.doubleValue;
                if !value.is_finite() || value <= 0.0 {
                    continue;
                }
                let celsius = if self.tenths_kelvin {
                    (value / 10.0 - 273.15) as f32
                } else {
                    (value - 273.15) as f32
                };
                if !(-20.0..=130.0).contains(&celsius) {
                    continue;
                }
                if (celsius - 27.8).abs() < 0.15 {
                    continue;
                }
                best = Some(best.map_or(celsius, |b| b.max(celsius)));
            }
            best
        }
    }
}

impl Drop for TemperatureQuery {
    fn drop(&mut self) {
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

#[allow(dead_code)]
fn _status_is_ok(status: NTSTATUS) -> bool {
    status.0 >= 0
}
