#[cfg(target_os = "windows")]
pub fn boost_current_thread_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
    };

    unsafe {
        let thread = GetCurrentThread();
        let _ = SetThreadPriority(thread, THREAD_PRIORITY_HIGHEST);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn boost_current_thread_priority() {}
