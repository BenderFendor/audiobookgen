# Voxtral INT4 benchmark suite

- git commit: `1d75ecf5bd467496bc158ab08b713b24fa09a140`
- GPU: NVIDIA GeForce RTX 3060 (cc 8.6)
- torch 2.13.0+cu130, torchao 0.17.0+cu130, hqq 0.2.8.post1, CUDA 13.0
- model load: 4.7 s

| profile | sentence | frames | FPS | wall RTF | decode RTF | prefill s | backbone s | acoustic s | loop overhead s | codec s |
|---|---|---|---|---|---|---|---|---|---|---|
| balanced | short | 34 | 33.2 | 8.22 | 0.38 | 21.31 | 0.49 | 0.53 | 0.00 | 0.02 |
| balanced | medium | 61 | 35.0 | 0.42 | 0.36 | 0.28 | 0.84 | 0.90 | 0.01 | 0.01 |
| balanced | long | 142 | 32.0 | 0.42 | 0.39 | 0.31 | 2.17 | 2.26 | 0.02 | 0.03 |
| balanced | dialogue | 52 | 31.4 | 0.47 | 0.40 | 0.29 | 0.80 | 0.85 | 0.01 | 0.02 |
| balanced | numbers | 225 | 34.2 | 0.39 | 0.37 | 0.35 | 3.24 | 3.31 | 0.02 | 0.04 |

## balanced aggregate
- mean FPS: 33.2
- mean wall RTF: 1.98
- mean decode RTF: 0.38
- frame-time split: backbone 49%, acoustic 51%, loop overhead 0%
- one-time compile warmup: 30.1 s
