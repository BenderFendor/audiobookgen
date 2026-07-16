# Voxtral INT4 benchmark suite

- git commit: `5cb464128c3810cb10864f7fecaaf5ac22a480e6`
- GPU: NVIDIA GeForce RTX 3060 (cc 8.6)
- torch 2.13.0+cu130, torchao 0.17.0+cu130, hqq 0.2.8.post1, CUDA 13.0
- model load: 6.3 s

| profile | sentence | frames | FPS | wall RTF | decode RTF | prefill s | backbone s | acoustic s | loop overhead s | codec s |
|---|---|---|---|---|---|---|---|---|---|---|
| compatibility | short | 42 | 17.4 | 1.16 | 0.72 | 0.58 | 1.55 | 0.86 | 0.00 | 0.22 |
| compatibility | medium | 66 | 22.5 | 0.62 | 0.56 | 0.29 | 1.96 | 0.97 | 0.01 | 0.02 |
| compatibility | long | 214 | 19.3 | 0.67 | 0.65 | 0.32 | 7.71 | 3.39 | 0.02 | 0.04 |
| compatibility | dialogue | 60 | 17.8 | 0.77 | 0.70 | 0.29 | 2.35 | 1.02 | 0.01 | 0.03 |
| compatibility | numbers | 213 | 20.5 | 0.63 | 0.61 | 0.33 | 7.10 | 3.25 | 0.02 | 0.04 |
| quality | short | 29 | 10.5 | 1.34 | 1.19 | 0.30 | 1.08 | 1.68 | 0.00 | 0.03 |
| quality | medium | 100 | 12.0 | 1.09 | 1.04 | 0.32 | 3.16 | 5.17 | 0.01 | 0.02 |
| quality | long | 174 | 11.6 | 1.11 | 1.08 | 0.32 | 5.73 | 9.27 | 0.02 | 0.03 |
| quality | dialogue | 52 | 10.8 | 1.24 | 1.16 | 0.32 | 1.77 | 3.03 | 0.00 | 0.02 |
| quality | numbers | 184 | 11.5 | 1.12 | 1.09 | 0.34 | 5.89 | 10.12 | 0.02 | 0.03 |
| balanced | short | 42 | 33.3 | 1.36 | 0.38 | 3.28 | 0.63 | 0.63 | 0.00 | 0.01 |
| balanced | medium | 79 | 33.8 | 1.12 | 0.37 | 4.70 | 1.19 | 1.14 | 0.01 | 0.02 |
| balanced | long | 143 | 33.5 | 0.62 | 0.37 | 2.81 | 2.15 | 2.10 | 0.01 | 0.03 |
| balanced | dialogue | 69 | 31.4 | 0.46 | 0.40 | 0.28 | 1.13 | 1.06 | 0.01 | 0.02 |
| balanced | numbers | 210 | 33.3 | 0.40 | 0.38 | 0.31 | 3.24 | 3.05 | 0.02 | 0.04 |

## compatibility aggregate
- mean FPS: 19.5
- mean wall RTF: 0.77
- mean decode RTF: 0.65
- frame-time split: backbone 68%, acoustic 31%, loop overhead 0%

## quality aggregate
- mean FPS: 11.3
- mean wall RTF: 1.18
- mean decode RTF: 1.11
- frame-time split: backbone 38%, acoustic 62%, loop overhead 0%

## balanced aggregate
- mean FPS: 33.0
- mean wall RTF: 0.79
- mean decode RTF: 0.38
- frame-time split: backbone 51%, acoustic 49%, loop overhead 0%
- one-time compile warmup: 64.3 s
