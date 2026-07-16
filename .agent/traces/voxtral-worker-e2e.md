# Command Watchdog Report

- Command: `python3 scripts/e2e_voxtral_worker.py --python /home/bender/.local/share/io.audiobookgen.desktop/worker-venv/bin/python --model-dir /mnt/Big storage/AudiobookGen/models/voxtral-4b-tts --profile compatibility`
- CWD: `/home/bender/projects/audiobookgen`
- Exit code: `0`
- Timed out: `False`
- Duration: 85.075s
- Max RSS delta: 16353940 KB
- Verdict (exit-code authoritative): success
- Warnings noted (do not flip verdict): none

## Stdout Tail
```
{"id": "voxtral-e2e", "type": "progress", "state": "loading-int4"}
{"id": "voxtral-e2e", "type": "progress", "state": "loading Voxtral BF16 weights on CPU"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantizing the language backbone layer by layer to HQQ INT4"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 1/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 2/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 3/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 4/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 5/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 6/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 7/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 8/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 9/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 10/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 11/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 12/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 13/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 14/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 15/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 16/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 17/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 18/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 19/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 20/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 21/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 22/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 23/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 24/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 25/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "quantized backbone layer 26/26"}
{"id": "voxtral-e2e", "type": "progress", "state": "moving the acoustic transformer and codec to CUDA in BF16"}
{"id": "voxtral-e2e", "type": "progress", "state": "Voxtral INT4 is loaded"}
{"id": "voxtral-e2e", "type": "progress", "state": "synthesizing"}
{"id": "voxtral-e2e", "type": "progress", "state": "writing"}
{"id": "voxtral-e2e", "type": "complete", "duration_ms": 5360, "sample_rate": 48000, "word_timings": [{"word": "The", "start_ms": 0, "end_ms": 319}, {"word": "quick", "start_ms": 319, "end_ms": 766}, {"word": "brown", "start_ms": 766, "end_ms": 1212}, {"word": "fox", "start_ms": 1212, "end_ms": 1531}, {"word": "jumps", "start_ms": 1531, "end_ms": 1978}, {"word": "over", "start_ms": 1978, "end_ms": 2361}, {"word": "the", "start_ms": 2361, "end_ms": 2680}, {"word": "lazy", "start_ms": 2680, "end_ms": 3063}, {"word": "dog,", "start_ms": 3063, "end_ms": 3446}, {"word": "and", "start_ms": 3446, "end_ms": 3765}, {"word": "the", "start_ms": 3765, "end_ms": 4084}, {"word": "audiobook", "start_ms": 4084, "end_ms": 4786}, {"word": "begins.", "start_ms": 4786, "end_ms": 5360}], "payload": {"output_path": "/tmp/tmppl45prj5/voxtral-worker.wav"}}
Real Voxtral worker E2E passed: 5.360s mono 48 kHz
```

## Stderr Tail
```
W0716 09:19:07.298000 365874 torch/utils/_pytree.py:630] <enum 'KernelPreference'> is an Enum subclass and is now natively supported by torch.compile as an opaque value type. Calling register_constant() on Enum subclasses is deprecated and will be an error in a future release.
Voxtral: 67 frames in 4.3s (15.5 fps, RTF=0.81)
```

## Next Step
- Command completed. Use this report as verification evidence.
