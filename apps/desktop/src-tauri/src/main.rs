#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // webkit2gtk's DMA-BUF renderer crashes with Wayland protocol error 71 on
    // Hyprland; disable it before the webview starts (same fix as mangareader).
    // Native Wayland webkit also missizes its viewport under Hyprland (content
    // clipped at the right edge, blank EPUB frame), so default to XWayland
    // unless the user has chosen a backend themselves.
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var_os("GDK_BACKEND").is_none()
        {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }

    audiobookgen_desktop_lib::run();
}
