# Voxtral INT4 benchmark suite

- git commit: `1d75ecf5bd467496bc158ab08b713b24fa09a140`
- GPU: NVIDIA GeForce RTX 3060 (cc 8.6)
- torch 2.13.0+cu130, torchao 0.17.0+cu130, hqq 0.2.8.post1, CUDA 13.0
- model load: 5.0 s

| profile | sentence | frames | FPS | wall RTF | decode RTF | prefill s | backbone s | acoustic s | loop overhead s | codec s |
|---|---|---|---|---|---|---|---|---|---|---|
| balanced | short | 34 | 31.2 | 0.51 | 0.40 | 0.28 | 0.54 | 0.54 | 0.00 | 0.02 |
| balanced | medium | 61 | 33.8 | 0.44 | 0.37 | 0.30 | 0.88 | 0.91 | 0.01 | 0.01 |
| balanced | long | 142 | 31.3 | 0.43 | 0.40 | 0.32 | 2.27 | 2.25 | 0.02 | 0.03 |
| balanced | dialogue | 52 | 31.0 | 0.48 | 0.40 | 0.30 | 0.81 | 0.86 | 0.01 | 0.02 |
| balanced | numbers | 225 | 32.9 | 0.40 | 0.38 | 0.33 | 3.43 | 3.38 | 0.02 | 0.04 |

## balanced aggregate
- mean FPS: 32.0
- mean wall RTF: 0.45
- mean decode RTF: 0.39
- frame-time split: backbone 50%, acoustic 50%, loop overhead 0%
- one-time compile warmup: 59.1 s
