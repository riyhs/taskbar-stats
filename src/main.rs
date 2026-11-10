#![windows_subsystem = "windows"]

mod app;
mod capture;
mod config;
mod disk;
mod gpu;
mod logging;
mod metrics;
mod render;
mod taskbar;
mod ui;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

fn main() {
    logging::init();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        logging::write(&format!("panic: {info}"));
        previous(info);
    }));

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Err(error) = app::run() {
            logging::write(&format!("fatal: {error}"));
        }
        CoUninitialize();
    }
}
