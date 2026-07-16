"""Maya1 engine: expressive voice-design TTS via GGUF + llama.cpp + SNAC.

Maya1 is a 3B Llama-style model that emits SNAC codec tokens. The voice is a
natural-language description (for example "40-year-old male, low pitch, warm")
and the text may contain inline emotion tags like <laugh> or <sigh>.

Runs quantized GGUF weights through llama-cpp-python so it fits comfortably on
12 GB GPUs (Q8_0 is near-lossless at ~3.4 GB) and still works on CPU. Needs
the optional [maya1] dependency extra.
"""

from __future__ import annotations

from pathlib import Path

from .base import (
    SAMPLE_RATE,
    GenerateResult,
    Progress,
    configure_hf_cache,
    estimated_word_timings,
    mock_result,
    trim_silence,
)

GGUF_REPO = "mradermacher/maya1-GGUF"
SNAC_REPO = "hubertsiuzdak/snac_24khz"
# Quantization -> GGUF filename in the repo. Q8_0 is the quality default;
# smaller quants trade quality for speed and disk space.
QUANTS = {
    "Q8_0": "maya1.Q8_0.gguf",
    "Q6_K": "maya1.Q6_K.gguf",
    "Q4_K_M": "maya1.Q4_K_M.gguf",
}
DEFAULT_QUANT = "Q8_0"

# Prompt scaffolding token ids from the maya1 reference implementation.
SOH_ID = 128259
EOH_ID = 128260
SOA_ID = 128261
SOS_ID = 128257
EOS_ID = 128258
TEXT_EOT_ID = 128009
BOS_ID = 128000
CODE_BASE = 128266
CODE_SPAN = 4096


class Maya1Engine:
    name = "maya1"

    def __init__(self, mock: bool = False) -> None:
        self.mock = mock
        self._llm = None
        self._llm_path: Path | None = None
        self._snac = None
        self._snac_device: str | None = None

    def capabilities(self) -> dict[str, object]:
        return {
            "engine": self.name,
            "sample_rate": SAMPLE_RATE,
            "languages": ["en"],
            "voices": [],
            "voice_style": "description",
            "quants": list(QUANTS),
            "word_timings": False,
            "supports_speed": False,
            "mock": self.mock,
        }

    def installed(self, model_dir: Path, options: dict[str, object]) -> dict[str, object]:
        quant = self._quant(options)
        weights = model_dir / QUANTS[quant]
        ready = weights.is_file() or model_dir.joinpath("MOCK_MODEL").is_file()
        return {"installed": ready, "path": str(model_dir), "quant": quant}

    def ensure_model(
        self, model_dir: Path, options: dict[str, object], progress: Progress
    ) -> dict[str, object]:
        model_dir.mkdir(parents=True, exist_ok=True)
        if self.mock:
            (model_dir / "MOCK_MODEL").write_text("mock", encoding="utf-8")
            return {"installed": True, "path": str(model_dir), "mock": True}
        self._require_dependencies()
        configure_hf_cache(model_dir)
        from huggingface_hub import hf_hub_download

        quant = self._quant(options)
        progress("downloading")
        hf_hub_download(
            repo_id=GGUF_REPO,
            filename=QUANTS[quant],
            local_dir=model_dir,
            local_dir_use_symlinks=False,
        )
        progress("downloading-decoder")
        from snac import SNAC

        SNAC.from_pretrained(SNAC_REPO)
        return {"installed": True, "path": str(model_dir), "quant": quant, "mock": False}

    def generate(
        self,
        text: str,
        voice: str,
        speed: float,
        output_path: Path,
        model_dir: Path,
        options: dict[str, object],
        progress: Progress,
    ) -> GenerateResult:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        if self.mock:
            progress("synthesizing")
            return mock_result(text, speed, output_path)
        self._require_dependencies()
        configure_hf_cache(model_dir)
        import numpy as np
        import soundfile as sf

        description = voice.strip() or "Realistic adult narrator, neutral accent, warm and clear"
        progress("loading")
        llm = self._get_llm(model_dir, options)
        prompt_tokens = self._prompt_tokens(llm, description, text)

        progress("synthesizing")
        temperature = float(options.get("temperature", 0.4) or 0.4)
        # ~86 SNAC frames/s of audio, 7 tokens per frame; budget generously
        # from text length so long sentences are not cut off mid-word.
        max_tokens = min(12_000, max(1_400, len(text.split()) * 220))
        generated: list[int] = []
        for token in llm.generate(
            prompt_tokens,
            temp=temperature,
            top_p=0.9,
            repeat_penalty=1.1,
        ):
            if token == EOS_ID or len(generated) >= max_tokens:
                break
            generated.append(token)

        codes = [token for token in generated if token >= CODE_BASE]
        frames = len(codes) // 7
        if frames == 0:
            raise RuntimeError("maya1 produced no audio codes; try a shorter sentence or a simpler voice description")
        progress("decoding")
        audio = self._decode_snac(codes[: frames * 7], options)
        audio, _ = trim_silence(np.asarray(audio, dtype=np.float32))
        duration_ms = round(len(audio) / SAMPLE_RATE * 1000)
        sf.write(output_path, audio, SAMPLE_RATE, subtype="PCM_16")
        return GenerateResult(
            duration_ms=duration_ms,
            word_timings=estimated_word_timings(text, duration_ms),
        )

    def _prompt_tokens(self, llm, description: str, text: str) -> list[int]:
        formatted = f'<description="{description}"> {text}'
        text_tokens = llm.tokenize(formatted.encode("utf-8"), add_bos=False, special=False)
        return [SOH_ID, BOS_ID, *text_tokens, TEXT_EOT_ID, EOH_ID, SOA_ID, SOS_ID]

    def _decode_snac(self, codes: list[int], options: dict[str, object]):
        import torch

        level1: list[int] = []
        level2: list[int] = []
        level3: list[int] = []
        for index in range(0, len(codes), 7):
            slots = codes[index : index + 7]
            level1.append((slots[0] - CODE_BASE) % CODE_SPAN)
            level2.extend([(slots[1] - CODE_BASE) % CODE_SPAN, (slots[4] - CODE_BASE) % CODE_SPAN])
            level3.extend([
                (slots[2] - CODE_BASE) % CODE_SPAN,
                (slots[3] - CODE_BASE) % CODE_SPAN,
                (slots[5] - CODE_BASE) % CODE_SPAN,
                (slots[6] - CODE_BASE) % CODE_SPAN,
            ])
        snac, device = self._get_snac(options)
        with torch.inference_mode():
            tensors = [
                torch.tensor(level, dtype=torch.long, device=device).unsqueeze(0)
                for level in (level1, level2, level3)
            ]
            audio = snac.decode(tensors)
        return audio.squeeze().detach().cpu().numpy()

    def _get_llm(self, model_dir: Path, options: dict[str, object]):
        weights = model_dir / QUANTS[self._quant(options)]
        if not weights.is_file():
            raise RuntimeError("maya1 weights are missing; download the model from the Models page")
        if self._llm is None or self._llm_path != weights:
            from llama_cpp import Llama

            gpu_layers = 0 if str(options.get("device", "auto")) == "cpu" else -1
            self._llm = Llama(
                model_path=str(weights),
                n_ctx=16_384,
                n_gpu_layers=gpu_layers,
                logits_all=False,
                verbose=False,
            )
            self._llm_path = weights
        return self._llm

    def _get_snac(self, options: dict[str, object]):
        import torch

        wanted = str(options.get("device", "auto"))
        device = "cpu" if wanted == "cpu" else ("cuda" if torch.cuda.is_available() else "cpu")
        if self._snac is None or self._snac_device != device:
            from snac import SNAC

            self._snac = SNAC.from_pretrained(SNAC_REPO).eval().to(device)
            self._snac_device = device
        return self._snac, device

    @staticmethod
    def _quant(options: dict[str, object]) -> str:
        quant = str(options.get("quant", DEFAULT_QUANT) or DEFAULT_QUANT)
        if quant not in QUANTS:
            raise ValueError(f"unsupported maya1 quantization: {quant}")
        return quant

    @staticmethod
    def _require_dependencies() -> None:
        missing = []
        for module in ("llama_cpp", "snac", "torch"):
            try:
                __import__(module)
            except ImportError:
                missing.append(module)
        if missing:
            raise RuntimeError(
                "maya1 support is not installed yet; install it from the Models page "
                f"(missing: {', '.join(missing)})"
            )
