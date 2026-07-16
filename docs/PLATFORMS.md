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

## GPU acceleration

Kokoro remains usable on CPU. Maya1 automatically builds `llama-cpp-python` with CUDA when AudiobookGen can find both an NVIDIA GPU and the CUDA compiler (`nvcc`); otherwise it installs the CPU fallback and says so on the Models page. The private PyTorch install uses uv's automatic backend selection, which chooses a compatible CUDA wheel from the detected driver.

The direct Voxtral 4B INT4 path has narrower requirements:

- Linux and Python 3.12
- NVIDIA CUDA GPU with compute capability 8.0 or newer
- 12 GB VRAM for the measured selective INT4 profile
- about 8 GB of model weights plus CUDA PyTorch, torchao, HQQ, and working storage

AudiobookGen reports the detected GPU and VRAM before use and blocks unmeasured sub-12-GB configurations. On the local RTX 3060 12 GB, the compatibility profile loaded at 3.676 GB peak PyTorch allocation and generated at a 3.812 GB peak; see [the measurement](../reports/benchmarks/voxtral-rtx3060-2026-07-16.md). Mistral's official BF16 vLLM recommendation remains at least 16 GB and is not the app default. Windows and macOS can still use Kokoro and Maya1; direct Voxtral is currently Linux/CUDA only.

## Model and disk behavior

Model runtimes are not bundled into the base application installer. First use installs private dependencies and then downloads the selected model after license acceptance. Model weights use the configured models volume and never enter the source tree.

Sentence WAVs are retained while they are needed for resumable generation and exports. A later cache-management screen should expose size limits and safe cleanup of segments that already exist in portable exports.
