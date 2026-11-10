use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::mem::size_of;
use std::slice;

use windows::core::{w, PCSTR};
use windows::Wdk::Graphics::Direct3D::{
    D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, D3DKMT_ADAPTER_PERFDATA,
    D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO,
    KMTQAITYPE_ADAPTERPERFDATA,
};
use windows::Win32::Foundation::{FreeLibrary, HANDLE, HMODULE, LUID, NTSTATUS};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
};

#[derive(Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub temp_c: Option<f32>,
    pub load_pct: Option<f32>,
}

struct Adapter {
    name: String,
    vendor: String,
    luid: LUID,
    luid_key: u64,
    nvml_index: Option<usize>,
}

pub struct GpuSampler {
    adapters: Vec<Adapter>,
    nvml: Option<Nvml>,
    load_query: Option<GpuLoadQuery>,
}

impl GpuSampler {
    pub fn new() -> Self {
        let mut adapters = enumerate_adapters();
        let nvml = Nvml::new();
        if let Some(nvml) = &nvml {
            let names = nvml.names();
            let mut next_nvidia = 0usize;
            for adapter in &mut adapters {
                if adapter.vendor != "NVIDIA" {
                    continue;
                }
                if let Some(index) = names
                    .iter()
                    .position(|name| normalized(name) == normalized(&adapter.name))
                {
                    adapter.nvml_index = Some(index);
                } else if next_nvidia < names.len() {
                    adapter.nvml_index = Some(next_nvidia);
                }
                next_nvidia += 1;
            }
        }
        Self {
            adapters,
            nvml,
            load_query: GpuLoadQuery::new(),
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.adapters
            .iter()
            .map(|adapter| adapter.name.clone())
            .collect()
    }

    pub fn sample(&self) -> Vec<GpuInfo> {
        let loads = self
            .load_query
            .as_ref()
            .map(|query| query.read())
            .unwrap_or_default();
        let nvml = self
            .nvml
            .as_ref()
            .map(|nvml| nvml.read_all())
            .unwrap_or_default();

        self.adapters
            .iter()
            .map(|adapter| {
                let reading = adapter.nvml_index.and_then(|index| nvml.get(index));
                let load = reading
                    .and_then(|reading| reading.load_pct)
                    .or_else(|| loads.get(&adapter.luid_key).copied())
                    .or(Some(0.0));
                let temp = reading
                    .and_then(|reading| reading.temp_c)
                    .or_else(|| adapter_temp(adapter.luid));
                GpuInfo {
                    name: adapter.name.clone(),
                    temp_c: temp,
                    load_pct: load,
                }
            })
            .collect()
    }
}

fn enumerate_adapters() -> Vec<Adapter> {
    let mut adapters = Vec::new();
    unsafe {
        let Ok(factory): Result<IDXGIFactory1, _> = CreateDXGIFactory1() else {
            return adapters;
        };
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            index += 1;
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & 2 != 0 {
                continue;
            }
            let name = String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .to_string();
            let vendor = match desc.VendorId {
                0x10DE => "NVIDIA",
                0x8086 => "INTEL",
                0x1002 => "AMD",
                _ => continue,
            };
            if vendor == "INTEL"
                && adapters
                    .iter()
                    .any(|existing: &Adapter| existing.vendor == vendor && existing.name == name)
            {
                continue;
            }
            adapters.push(Adapter {
                name,
                vendor: vendor.to_string(),
                luid: desc.AdapterLuid,
                luid_key: luid_key(desc.AdapterLuid),
                nvml_index: None,
            });
        }
    }
    adapters
}

fn luid_key(luid: LUID) -> u64 {
    ((luid.HighPart as u32 as u64) << 32) | luid.LowPart as u64
}

fn normalized(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn adapter_temp(luid: LUID) -> Option<f32> {
    unsafe {
        let mut open = D3DKMT_OPENADAPTERFROMLUID {
            AdapterLuid: luid,
            ..Default::default()
        };
        if D3DKMTOpenAdapterFromLuid(&mut open).0 < 0 {
            return None;
        }
        let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
        let mut query = D3DKMT_QUERYADAPTERINFO {
            hAdapter: open.hAdapter,
            Type: KMTQAITYPE_ADAPTERPERFDATA,
            pPrivateDriverData: &mut perf as *mut _ as *mut c_void,
            PrivateDriverDataSize: size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
        };
        let status: NTSTATUS = D3DKMTQueryAdapterInfo(&mut query);
        let _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
            hAdapter: open.hAdapter,
        });
        if status.0 < 0 {
            return None;
        }
        let raw = perf.Temperature;
        if raw == 0 {
            return None;
        }
        let celsius = if raw > 200 {
            raw as f32 / 10.0
        } else {
            raw as f32
        };
        (celsius > 0.0 && celsius <= 125.0).then_some(celsius)
    }
}

type NvmlInit = unsafe extern "C" fn() -> i32;
type NvmlShutdown = unsafe extern "C" fn() -> i32;
type NvmlCount = unsafe extern "C" fn(*mut u32) -> i32;
type NvmlHandle = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
type NvmlTemp = unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32;
type NvmlUtil = unsafe extern "C" fn(*mut c_void, *mut NvmlUtilization) -> i32;
type NvmlName = unsafe extern "C" fn(*mut c_void, *mut c_char, u32) -> i32;

#[repr(C)]
#[derive(Default)]
struct NvmlUtilization {
    gpu: u32,
    memory: u32,
}

#[derive(Clone, Default)]
struct NvmlReading {
    name: String,
    temp_c: Option<f32>,
    load_pct: Option<f32>,
}

struct Nvml {
    lib: HMODULE,
    devices: Vec<*mut c_void>,
    shutdown: NvmlShutdown,
    temp: NvmlTemp,
    util: NvmlUtil,
    name: NvmlName,
}

impl Nvml {
    fn new() -> Option<Self> {
        unsafe {
            let lib = LoadLibraryExW(
                w!("nvml.dll"),
                HANDLE::default(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
            .ok()?;
            let init: NvmlInit = load(lib, b"nvmlInit_v2\0")?;
            let shutdown: NvmlShutdown = load(lib, b"nvmlShutdown\0")?;
            let count: NvmlCount = load(lib, b"nvmlDeviceGetCount_v2\0")?;
            let get_handle: NvmlHandle = load(lib, b"nvmlDeviceGetHandleByIndex_v2\0")?;
            let temp: NvmlTemp = load(lib, b"nvmlDeviceGetTemperature\0")?;
            let util: NvmlUtil = load(lib, b"nvmlDeviceGetUtilizationRates\0")?;
            let name: NvmlName = load(lib, b"nvmlDeviceGetName\0")?;

            if init() != 0 {
                let _ = FreeLibrary(lib);
                return None;
            }
            let mut count_value = 0u32;
            if count(&mut count_value) != 0 || count_value == 0 {
                let _ = shutdown();
                let _ = FreeLibrary(lib);
                return None;
            }
            let mut devices = Vec::new();
            for index in 0..count_value {
                let mut handle = std::ptr::null_mut();
                if get_handle(index, &mut handle) == 0 && !handle.is_null() {
                    devices.push(handle);
                }
            }
            if devices.is_empty() {
                let _ = shutdown();
                let _ = FreeLibrary(lib);
                return None;
            }
            Some(Self {
                lib,
                devices,
                shutdown,
                temp,
                util,
                name,
            })
        }
    }

    fn names(&self) -> Vec<String> {
        self.read_all()
            .into_iter()
            .map(|reading| reading.name)
            .collect()
    }

    fn read_all(&self) -> Vec<NvmlReading> {
        self.devices
            .iter()
            .map(|handle| {
                let mut reading = NvmlReading::default();
                unsafe {
                    let mut temp = 0u32;
                    if (self.temp)(*handle, 0, &mut temp) == 0 && temp > 0 && temp < 150 {
                        reading.temp_c = Some(temp as f32);
                    }
                    let mut util = NvmlUtilization::default();
                    if (self.util)(*handle, &mut util) == 0 {
                        reading.load_pct = Some(util.gpu.min(100) as f32);
                    }
                    let mut name = [0u8; 96];
                    if (self.name)(
                        *handle,
                        name.as_mut_ptr().cast::<c_char>(),
                        name.len() as u32,
                    ) == 0
                    {
                        reading.name = CStr::from_ptr(name.as_ptr().cast::<c_char>())
                            .to_string_lossy()
                            .into_owned();
                    }
                }
                if reading.name.is_empty() {
                    reading.name = "GPU".into();
                }
                reading
            })
            .collect()
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.shutdown)();
            let _ = FreeLibrary(self.lib);
        }
    }
}

struct GpuLoadQuery {
    query: isize,
    counter: isize,
}

impl GpuLoadQuery {
    fn new() -> Option<Self> {
        unsafe {
            let mut query = 0isize;
            if PdhOpenQueryW(windows::core::PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let mut counter = 0isize;
            let status = PdhAddEnglishCounterW(
                query,
                w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut counter,
            );
            if status != 0 {
                let _ = PdhCloseQuery(query);
                return None;
            }
            let _ = PdhCollectQueryData(query);
            Some(Self { query, counter })
        }
    }

    fn read(&self) -> HashMap<u64, f32> {
        let mut values = HashMap::new();
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return values;
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
                return values;
            }
            if size == 0 || count == 0 {
                return values;
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
                return values;
            }
            let items = slice::from_raw_parts(
                words.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
                count as usize,
            );
            for item in items {
                if item.FmtValue.CStatus != 0 {
                    continue;
                }
                let value = item.FmtValue.Anonymous.doubleValue;
                if !value.is_finite() || value <= 0.0 {
                    continue;
                }
                let Ok(name) = item.szName.to_string() else {
                    continue;
                };
                let Some(luid) = parse_luid(&name) else {
                    continue;
                };
                *values.entry(luid).or_insert(0.0) += value as f32;
            }
            for value in values.values_mut() {
                *value = value.clamp(0.0, 100.0);
            }
        }
        values
    }
}

impl Drop for GpuLoadQuery {
    fn drop(&mut self) {
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

fn parse_luid(name: &str) -> Option<u64> {
    let rest = name.split("luid_").nth(1)?;
    let mut parts = rest.split('_');
    let high = u32::from_str_radix(parts.next()?.trim_start_matches("0x"), 16).ok()?;
    let low = u32::from_str_radix(parts.next()?.trim_start_matches("0x"), 16).ok()?;
    Some(((high as u64) << 32) | low as u64)
}

unsafe fn load<T: Copy>(lib: HMODULE, name: &[u8]) -> Option<T> {
    debug_assert!(name.ends_with(b"\0"));
    let address = GetProcAddress(lib, PCSTR(name.as_ptr()))?;
    Some(std::mem::transmute_copy(&address))
}
