# Command Watchdog Report

- Command: `uv pip install --python /home/bender/.local/share/io.audiobookgen.desktop/worker-venv/bin/python --torch-backend auto /home/bender/projects/audiobookgen/services/tts-worker[voxtral]`
- CWD: `/home/bender/projects/audiobookgen`
- Exit code: `0`
- Timed out: `False`
- Duration: 2.696s
- Max RSS delta: 272744 KB
- Verdict (exit-code authoritative): success
- Warnings noted (do not flip verdict): none

## Stdout Tail
```

```

## Stderr Tail
```
Using Python 3.12.12 environment at: /home/bender/.local/share/io.audiobookgen.desktop/worker-venv
Resolved 118 packages in 1.26s
   Building hqq==0.2.8.post1
   Building audiobookgen-worker @ file:///home/bender/projects/audiobookgen/services/tts-worker
Downloading torchao (3.1MiB)
Downloading scipy (33.7MiB)
Downloading tiktoken (1.1MiB)
 Downloaded torchao
 Downloaded tiktoken
      Built audiobookgen-worker @ file:///home/bender/projects/audiobookgen/services/tts-worker
 Downloaded scipy
      Built hqq==0.2.8.post1
Prepared 7 packages in 1.10s
Uninstalled 1 package in 1ms
Installed 7 packages in 218ms
 + accelerate==1.14.0
 - audiobookgen-worker==0.1.0 (from file:///home/bender/projects/audiobookgen/target/debug/tts-worker)
 + audiobookgen-worker==0.1.0 (from file:///home/bender/projects/audiobookgen/services/tts-worker)
 + hqq==0.2.8.post1
 + psutil==7.2.2
 + scipy==1.18.0
 + tiktoken==0.13.0
 + torchao==0.17.0+cu130
```

## Next Step
- Command completed. Use this report as verification evidence.
