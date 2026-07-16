# Platform support

## Desktop target

The application is structured for 64-bit Windows, macOS, and Linux through Tauri 2.

A practical minimum is a machine capable of running a current system webview, Rust/Tauri binaries, Python 3.10–3.13, and the current CPU build of PyTorch. The project does not set an artificial year cutoff because CPU instruction support and vendor Python wheels are the real constraints.

Baseline acceptance target:

- 64-bit operating system
- four logical CPU cores
- 8 GB RAM
- 4 GB free disk space before importing books
- a supported system webview

Recommended for full-book generation:

- 8 or more logical CPU cores
- 16 GB RAM
- SSD storage

AudiobookGen remains usable on a slower machine because generation is serialized, begins with the current chapter, resumes from sentence cache, and never requires the full book to be resident in memory. Faster-than-real-time generation is not guaranteed on the minimum target.

## Windows

- Windows 10/11 x64
- WebView2 runtime
- Python 3.10–3.13
- FFmpeg in `PATH` for exports
- Sleep inhibition uses a hidden PowerShell process calling `SetThreadExecutionState` during generation.

## macOS

- Intel or Apple Silicon on a Tauri-supported macOS release
- Python 3.10–3.13
- FFmpeg in `PATH` for exports
- Sleep inhibition uses `caffeinate`.

Release automation should build Intel and Apple Silicon artifacts separately before producing a universal package.

## Linux

- x86-64 initially; ARM64 is an intended CI/release target once Kokoro/PyTorch wheel coverage is verified
- WebKitGTK 4.1 and Tauri system libraries
- Python 3.10–3.13 and `espeak-ng`
- FFmpeg in `PATH` for exports
- Sleep inhibition uses `systemd-inhibit`; failure to acquire the inhibitor does not abort generation.

## Model and disk behavior

Kokoro and its Python runtime are not bundled into the base application installer. First use installs the private worker environment and then downloads the model. This keeps the initial app package smaller, makes the model license visible at installation time, and permits independent model cache repair.

Sentence WAVs are retained while they are needed for resumable generation and exports. A later cache-management screen should expose size limits and safe cleanup of segments that already exist in portable exports.
