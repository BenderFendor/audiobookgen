# Voxtral INT4 benchmark suite

- git commit: `5464e732f07bbdf4ad159879894a6d640078276c`
- GPU: NVIDIA GeForce RTX 3060 (cc 8.6)
- torch 2.13.0+cu130, torchao 0.17.0+cu130, hqq 0.2.8.post1, CUDA 13.0
- model load: 128.1 s

| profile | sentence | frames | FPS | wall RTF | decode RTF | prefill s | backbone s | acoustic s | loop overhead s | codec s |
|---|---|---|---|---|---|---|---|---|---|---|
| compatibility | short | 35 | 12.8 | 2.05 | 0.98 | 0.55 | 1.07 | 1.65 | 0.00 | 0.80 |
| compatibility | medium | 67 | 17.5 | 0.81 | 0.71 | 0.47 | 1.94 | 1.89 | 0.01 | 0.04 |
| compatibility | long | 166 | 16.8 | 0.77 | 0.74 | 0.32 | 5.06 | 4.81 | 0.02 | 0.03 |
| compatibility | dialogue | 81 | 16.5 | 0.81 | 0.76 | 0.30 | 2.50 | 2.42 | 0.01 | 0.02 |
| compatibility | numbers | 165 | 17.1 | 0.76 | 0.73 | 0.32 | 4.95 | 4.70 | 0.01 | 0.03 |
| quality | short | 36 | 7.4 | 1.81 | 1.70 | 0.29 | 1.09 | 3.80 | 0.00 | 0.02 |
| quality | medium | 99 | 7.8 | 1.64 | 1.60 | 0.30 | 2.94 | 9.74 | 0.01 | 0.02 |
| quality | long | 193 | 7.8 | 1.64 | 1.61 | 0.32 | 5.69 | 19.19 | 0.02 | 0.03 |
| quality | dialogue | 58 | 7.3 | 1.78 | 1.71 | 0.30 | 1.81 | 6.13 | 0.01 | 0.02 |
| quality | numbers | 242 | 7.6 | 1.67 | 1.65 | 0.33 | 7.57 | 24.43 | 0.02 | 0.04 |
| balanced | short | 36 | 22.6 | 6.46 | 0.55 | 16.98 | 0.52 | 1.07 | 0.00 | 0.01 |
| balanced | medium | 118 | 22.5 | 1.51 | 0.56 | 9.01 | 1.90 | 3.35 | 0.01 | 0.02 |
| balanced | long | 196 | 24.5 | 0.58 | 0.51 | 1.10 | 2.76 | 5.22 | 0.01 | 0.03 |
| balanced | dialogue | 57 | 22.4 | 0.62 | 0.56 | 0.27 | 0.89 | 1.65 | 0.00 | 0.02 |
| balanced | numbers | 208 | 24.1 | 0.54 | 0.52 | 0.30 | 3.01 | 5.59 | 0.02 | 0.03 |

## compatibility aggregate
- mean FPS: 16.1
- mean wall RTF: 1.04
- mean decode RTF: 0.78
- frame-time split: backbone 50%, acoustic 50%, loop overhead 0%

## quality aggregate
- mean FPS: 7.6
- mean wall RTF: 1.71
- mean decode RTF: 1.66
- frame-time split: backbone 23%, acoustic 77%, loop overhead 0%

## balanced aggregate
- mean FPS: 23.2
- mean wall RTF: 1.94
- mean decode RTF: 0.54
- frame-time split: backbone 35%, acoustic 65%, loop overhead 0%
- one-time compile warmup: 20.4 s
