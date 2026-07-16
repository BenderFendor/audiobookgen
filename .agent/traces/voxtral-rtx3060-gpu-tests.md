# Command Watchdog Report

- Command: `/home/bender/.local/share/io.audiobookgen.desktop/worker-venv/bin/python services/tts-worker/tests/gpu/test_voxtral_gpu.py`
- CWD: `/home/bender/projects/audiobookgen`
- Exit code: `0`
- Timed out: `False`
- Duration: 320.149s
- Max RSS delta: 15054792 KB
- Verdict (exit-code authoritative): success
- Warnings noted (do not flip verdict): none

## Stdout Tail
```
loading Voxtral BF16 weights on CPU
quantizing the language backbone layer by layer to HQQ INT4
quantized backbone layer 1/26
quantized backbone layer 2/26
quantized backbone layer 3/26
quantized backbone layer 4/26
quantized backbone layer 5/26
quantized backbone layer 6/26
quantized backbone layer 7/26
quantized backbone layer 8/26
quantized backbone layer 9/26
quantized backbone layer 10/26
quantized backbone layer 11/26
quantized backbone layer 12/26
quantized backbone layer 13/26
quantized backbone layer 14/26
quantized backbone layer 15/26
quantized backbone layer 16/26
quantized backbone layer 17/26
quantized backbone layer 18/26
quantized backbone layer 19/26
quantized backbone layer 20/26
quantized backbone layer 21/26
quantized backbone layer 22/26
quantized backbone layer 23/26
quantized backbone layer 24/26
quantized backbone layer 25/26
quantized backbone layer 26/26
moving the acoustic transformer and codec to CUDA in BF16
Voxtral INT4 is loaded
  67 frames in 4.4s (15.3 fps, RTF=0.82)
  67 frames in 3.9s (17.0 fps, RTF=0.74)
  111 frames in 15.2s (7.3 fps, RTF=1.71)
  119 frames in 64.8s (1.8 fps, RTF=6.81)
```

## Stderr Tail
```
W0716 09:02:12.964000 252263 torch/utils/_pytree.py:630] <enum 'KernelPreference'> is an Enum subclass and is now natively supported by torch.compile as an opaque value type. Calling register_constant() on Enum subclasses is deprecated and will be an error in a future release.
/home/bender/.local/share/io.audiobookgen.desktop/worker-venv/lib/python3.12/site-packages/torch/jit/_script.py:365: DeprecationWarning: `torch.jit.script_method` is deprecated. Please switch to `torch.compile` or `torch.export`.
  warnings.warn(
/home/bender/.local/share/io.audiobookgen.desktop/worker-venv/lib/python3.12/site-packages/torch/_functorch/_aot_autograd/autograd_cache.py:675: UserWarning: Int4TilePackedTo4dTensor does not implement _stable_hash_for_caching. For PT2-compatible tensor subclasses, it is recommended to implement _stable_hash_for_caching(self) -> str for stable AOT autograd caching.
  warn_once(
/home/bender/.local/share/io.audiobookgen.desktop/worker-venv/lib/python3.12/site-packages/torch/_inductor/lowering.py:2633: UserWarning: Torchinductor does not support code generation for complex operators. Performance may be worse than eager.
  warnings.warn(
In file included from /home/bender/.local/share/uv/python/cpython-3.12.12-linux-x86_64-gnu/include/python3.12/Python.h:12,
                 from /tmp/tmp4cpe7bj6/cuda_utils.c:9:
/home/bender/.local/share/uv/python/cpython-3.12.12-linux-x86_64-gnu/include/python3.12/pyconfig.h:1877:9: warning: ‘_POSIX_C_SOURCE’ redefined
 1877 | #define _POSIX_C_SOURCE 200809L
      |         ^~~~~~~~~~~~~~~
In file included from /usr/include/bits/libc-header-start.h:33,
                 from /usr/include/stdlib.h:26,
                 from /home/bender/.local/share/io.audiobookgen.desktop/worker-venv/lib/python3.12/site-packages/triton/backends/nvidia/include/cuda.h:56,
                 from /tmp/tmp4cpe7bj6/cuda_utils.c:1:
/usr/include/features.h:319:10: note: this is the location of the previous definition
  319 | # define _POSIX_C_SOURCE        202405L
      |          ^~~~~~~~~~~~~~~
W0716 09:06:22.655000 252263 torch/_inductor/utils.py:1953] [1/0] Not enough SMs to use max_autotune_gemm mode
.
----------------------------------------------------------------------
Ran 1 test in 315.802s

OK
```

## Next Step
- Command completed. Use this report as verification evidence.
