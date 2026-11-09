use std::ffi::c_void;
use std::mem::size_of;

use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
    IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
    StorageDeviceTemperatureProperty,
};

pub fn system_disk_temp() -> Option<f32> {
    let index = system_disk_index().unwrap_or(0);
    drive_temp(index).or_else(|| {
        (0..4)
            .filter(|candidate| *candidate != index)
            .find_map(drive_temp)
    })
}

fn system_disk_index() -> Option<u32> {
    unsafe {
        let mut path: Vec<u16> = vec![92, 92, 46, 92];
        path.extend("C:".encode_utf16());
        path.push(0);
        let handle = CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;
        let mut output = [0u8; 512];
        let mut returned = 0u32;
        let result = DeviceIoControl(
            handle,
            IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
            None,
            0,
            Some(output.as_mut_ptr() as *mut c_void),
            output.len() as u32,
            Some(&mut returned),
            None,
        );
        let _ = CloseHandle(handle);
        if result.is_err() || returned < 12 {
            return None;
        }
        let count = u32::from_ne_bytes(output[0..4].try_into().ok()?);
        if count == 0 {
            return None;
        }
        Some(u32::from_ne_bytes(output[8..12].try_into().ok()?))
    }
}

fn drive_temp(index: u32) -> Option<f32> {
    unsafe {
        let mut path: Vec<u16> = vec![92, 92, 46, 92];
        path.extend(format!("PhysicalDrive{index}").encode_utf16());
        path.push(0);

        let handle = CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceTemperatureProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut output = [0u8; 512];
        let mut returned = 0u32;
        let result = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const c_void),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(output.as_mut_ptr() as *mut c_void),
            output.len() as u32,
            Some(&mut returned),
            None,
        );
        let _ = CloseHandle(handle);
        if result.is_err() || returned < 26 {
            return None;
        }

        let count = u16::from_ne_bytes(output[12..14].try_into().ok()?) as usize;
        let mut best = None;
        for i in 0..count.min(8) {
            let offset = 24 + i * 16;
            if offset + 4 > returned as usize {
                break;
            }
            let temp = i16::from_ne_bytes(output[offset + 2..offset + 4].try_into().ok()?);
            if temp > 0 && temp <= 125 {
                best = Some(best.map_or(temp, |current: i16| current.max(temp)));
            }
        }
        best.map(|temp| temp as f32)
    }
}

