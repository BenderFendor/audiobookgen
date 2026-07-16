"""Load, quantize, measure, and serialize one Voxtral GPU model."""

import gc
import json
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any


# Bump when the quantization recipe or model construction changes in a way
# that invalidates previously written quantized-weight caches.
QUANT_CACHE_VERSION = 1

PROFILES: dict[str, dict[str, Any]] = {
    "balanced": {"flow_steps": 3, "cfg_alpha": 1.2, "compile": True, "max_frames": 500},
    "quality": {"flow_steps": 8, "cfg_alpha": 1.2, "compile": False, "max_frames": 700},
    "compatibility": {
        "flow_steps": 3,
        "cfg_alpha": 1.2,
        "compile": False,
        "max_frames": 350,
    },
}


class VoxtralInt4Runtime:
    def __init__(self, model_dir: Path, progress: Callable[[str], None]) -> None:
        import torch

        from .inference import enable_static_cache
        from .tekken import TekkenTokenizer

        if not torch.cuda.is_available():
            raise RuntimeError("Voxtral INT4 requires an NVIDIA CUDA GPU")
        major, minor = torch.cuda.get_device_capability()
        if (major, minor) < (8, 0):
            raise RuntimeError(
                "Voxtral INT4 requires CUDA compute capability 8.0 or newer"
            )

        self.model_dir = model_dir
        self.device = "cuda"
        torch.cuda.reset_peak_memory_stats()
        started = time.monotonic()

        model = self._load_quantized_cache(progress)
        if model is None:
            model = self._quantize_from_weights(progress)
            self._save_quantized_cache(model, progress)

        model.eval()
        gc.collect()
        torch.cuda.empty_cache()
        enable_static_cache(model, max_seq_len=900)

        self.model = model
        self.tokenizer = TekkenTokenizer(model_dir / "tekken.json")
        self.voice_dir = model_dir / "voice_embedding"
        self.load_seconds = time.monotonic() - started
        self.allocated_bytes = torch.cuda.memory_allocated()
        self.reserved_bytes = torch.cuda.memory_reserved()
        self.peak_allocated_bytes = torch.cuda.max_memory_allocated()
        self._compiled = False
        progress("Voxtral INT4 is loaded")

    def _quantize_from_weights(self, progress: Callable[[str], None]):
        """Slow path: CPU-first load and per-layer HQQ INT4 quantization."""
        import torch
        from safetensors.torch import load_file
        from torchao.quantization import Int4WeightOnlyConfig, quantize_

        from .load_model import _assign_weights
        from .model import VoxtralConfig, VoxtralTTS

        progress("loading Voxtral BF16 weights on CPU")
        state = load_file(
            str(self.model_dir / "consolidated.safetensors"), device="cpu"
        )
        model = VoxtralTTS(VoxtralConfig())
        _assign_weights(model, state)
        del state
        gc.collect()

        # Loading the complete BF16 graph on CUDA before quantization can exceed
        # a 12 GB card even though steady-state INT4 inference fits. Quantize one
        # backbone layer at a time, leaving the quality-sensitive acoustic and
        # codec modules on CPU until the backbone has been compressed.
        config = Int4WeightOnlyConfig(
            group_size=64,
            int4_packing_format="tile_packed_to_4d",
            int4_choose_qparams_algorithm="hqq",
        )
        progress("quantizing the language backbone layer by layer to HQQ INT4")
        for index, layer in enumerate(model.backbone.layers, start=1):
            layer.to(device="cuda", dtype=torch.bfloat16)
            quantize_(layer, config)
            progress(f"quantized backbone layer {index}/{len(model.backbone.layers)}")
        model.backbone.tok_embeddings.to(device="cuda", dtype=torch.bfloat16)
        model.backbone.norm.to(device="cuda", dtype=torch.bfloat16)

        progress("moving the acoustic transformer and codec to CUDA in BF16")
        model.acoustic.to(device="cuda", dtype=torch.bfloat16)
        model.codec.to(device="cuda", dtype=torch.bfloat16)
        model.audio_codebook_embeddings.to(device="cuda", dtype=torch.bfloat16)
        return model

    def _quant_cache_paths(self) -> tuple[Path, Path]:
        cache_dir = self.model_dir / "quantized-cache"
        return cache_dir / "int4-model.pt", cache_dir / "int4-model.json"

    def _quant_cache_meta(self) -> dict[str, Any]:
        """Identity of the quantization recipe; any mismatch invalidates the cache.

        The full 9 GB weights checksum was verified at install time; re-hashing
        it on every start would defeat the fast path, so file size and mtime
        stand in for weight identity here.
        """
        import torch

        from importlib.metadata import PackageNotFoundError, version

        def safe_version(name: str) -> str:
            try:
                return version(name)
            except PackageNotFoundError:
                return "missing"

        weights = self.model_dir / "consolidated.safetensors"
        stat = weights.stat()
        return {
            "cache_version": QUANT_CACHE_VERSION,
            "torch": torch.__version__,
            "torchao": safe_version("torchao"),
            "hqq": safe_version("hqq"),
            "weights_size": stat.st_size,
            "weights_mtime_ns": stat.st_mtime_ns,
            "quant": "int4-hqq-g64-tile_packed_to_4d",
        }

    def _load_quantized_cache(self, progress: Callable[[str], None]):
        """Fast path: restore already-quantized weights straight to CUDA."""
        import torch

        from .model import VoxtralConfig, VoxtralTTS

        cache_path, meta_path = self._quant_cache_paths()
        if not cache_path.is_file() or not meta_path.is_file():
            return None
        try:
            stored_meta = json.loads(meta_path.read_text(encoding="utf-8"))
            if stored_meta != self._quant_cache_meta():
                progress("quantized cache is stale; rebuilding")
                return None
            progress("loading quantized Voxtral weights from cache")
            with torch.device("meta"):
                model = VoxtralTTS(VoxtralConfig())
            # map_location must carry the device index: torchao wrapper
            # subclasses reject mixing bare "cuda" with "cuda:0" storages.
            state = torch.load(
                cache_path, map_location="cuda:0", weights_only=False
            )
            # load_state_dict validates shapes, but the reconstruction assigns
            # some official weights (codec qk_norm) with shapes that differ
            # from the constructed skeleton. Assign by name instead, exactly
            # like the slow path does, then prove nothing was left on meta.
            modules = dict(model.named_modules())
            for key, tensor in state.items():
                module_name, _, attr = key.rpartition(".")
                module = modules[module_name]
                if attr in module._parameters:
                    module._parameters[attr] = torch.nn.Parameter(
                        tensor, requires_grad=False
                    )
                elif attr in module._buffers:
                    module._buffers[attr] = tensor
                else:
                    raise KeyError(f"unknown cached tensor: {key}")
            leftovers = [
                name
                for name, parameter in model.named_parameters()
                if parameter.device.type == "meta"
            ] + [
                name
                for name, buffer in model.named_buffers()
                if buffer.device.type == "meta"
            ]
            if leftovers:
                raise RuntimeError(
                    f"cache restore left {len(leftovers)} tensors unassigned: "
                    + ", ".join(leftovers[:5])
                )
            return model
        except Exception as error:  # noqa: BLE001 — any cache failure falls back
            progress(f"quantized cache unusable ({error}); rebuilding")
            return None

    def _save_quantized_cache(self, model, progress: Callable[[str], None]) -> None:
        """Best-effort write; a failed save never fails the load."""
        import torch

        cache_path, meta_path = self._quant_cache_paths()
        try:
            progress("writing quantized weight cache")
            cache_path.parent.mkdir(parents=True, exist_ok=True)
            temp_path = cache_path.with_suffix(".pt.tmp")
            torch.save(model.state_dict(), temp_path)
            temp_path.replace(cache_path)
            meta_path.write_text(
                json.dumps(self._quant_cache_meta(), indent=2) + "\n",
                encoding="utf-8",
            )
        except Exception as error:  # noqa: BLE001 — cache write is optional
            progress(f"quantized cache write failed ({error}); continuing")

    def voices(self) -> list[str]:
        return sorted(path.stem for path in self.voice_dir.glob("*.pt"))

    @property
    def compiled(self) -> bool:
        return self._compiled

    def metrics(self) -> dict[str, Any]:
        import torch

        properties = torch.cuda.get_device_properties(0)
        return {
            "gpu": properties.name,
            "compute_capability": ".".join(
                map(str, torch.cuda.get_device_capability())
            ),
            "total_vram_bytes": properties.total_memory,
            "allocated_vram_bytes": torch.cuda.memory_allocated(),
            "reserved_vram_bytes": torch.cuda.memory_reserved(),
            "peak_allocated_vram_bytes": torch.cuda.max_memory_allocated(),
            "load_seconds": self.load_seconds,
            "torch": torch.__version__,
        }

    def generate(
        self,
        text: str,
        voice: str,
        profile_name: str = "balanced",
        seed: int = 0,
        collect_timings: bool = False,
    ):
        import torch

        from .inference import generate_speech_fast

        profile = PROFILES.get(profile_name)
        if profile is None:
            raise ValueError(f"unknown Voxtral profile: {profile_name}")
        if voice not in self.voices():
            raise ValueError(f"unknown Voxtral voice: {voice}")
        if profile["compile"] and not self._compiled:
            import os

            # Persist compiled kernels next to the model so the one-time
            # compile warmup is paid once per machine, not per process.
            os.environ.setdefault(
                "TORCHINDUCTOR_CACHE_DIR", str(self.model_dir / "inductor-cache")
            )
            self.model.backbone = torch.compile(
                self.model.backbone, mode="default", fullgraph=False
            )
            self.model.acoustic.predict_velocity = torch.compile(
                self.model.acoustic.predict_velocity, mode="default", fullgraph=False
            )
            self._compiled = True
        with torch.inference_mode():
            return generate_speech_fast(
                self.model,
                self.tokenizer,
                text,
                voice_name=voice,
                voice_dir=str(self.voice_dir),
                max_frames=profile["max_frames"],
                flow_steps=profile["flow_steps"],
                cfg_alpha=profile["cfg_alpha"],
                engine_profile=profile_name,
                seed=seed,
                collect_timings=collect_timings,
            )
