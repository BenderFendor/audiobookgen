# Security policy

## Supported versions

AudiobookGen is pre-release software. Security fixes are applied to the current `main` branch and the latest tagged release once releases begin.

## Reporting a vulnerability

Do not open a public issue for vulnerabilities involving arbitrary file access, ZIP path traversal, command execution, unsafe EPUB scripting, model-download integrity, or exposure of private book text.

Use GitHub's private vulnerability reporting feature for this repository. Include:

- the affected commit or release
- operating system
- a minimal EPUB or protocol request when one is required to reproduce the issue
- expected and actual behavior
- whether the issue requires a malicious EPUB, local user access, or network position

Do not include copyrighted books or private narration output unless they are necessary and you have permission to share them.

## Security boundaries

AudiobookGen does not remove EPUB DRM, execute EPUB scripts, upload book text, or expose an HTTP inference server. EPUB imports are treated as untrusted ZIP containers. Kokoro inference runs in a dedicated local Python process over a structured stdin/stdout protocol. Export invokes the locally installed FFmpeg executable selected by the user or found in `PATH`.
