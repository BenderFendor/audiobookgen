#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // webkit2gtk's DMA-BUF renderer crashes with Wayland protocol error 71 on
    // Hyprland; disable it before the webview starts (same fix as mangareader).
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    audiobookgen_desktop_lib::run();
}
