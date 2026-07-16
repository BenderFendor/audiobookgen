"""Typed Voxtral failures; required chunks must never disappear silently."""


class VoxtralError(RuntimeError):
    code = "generation_failed"


class CudaOutOfMemory(VoxtralError):
    code = "cuda_out_of_memory"


class CudaDeviceAssertion(VoxtralError):
    code = "cuda_device_assertion"


class InvalidToken(VoxtralError):
    code = "invalid_token"


class GenerationTimeout(VoxtralError):
    code = "generation_timeout"


class NoEndOfAudio(VoxtralError):
    code = "no_end_of_audio"


class EmptyWaveform(VoxtralError):
    code = "empty_waveform"


class NonFiniteWaveform(VoxtralError):
    code = "non_finite_waveform"


class CodecFailure(VoxtralError):
    code = "codec_failure"


class ModelUnavailable(VoxtralError):
    code = "model_unavailable"


class CancelledJob(VoxtralError):
    code = "cancelled"


def classify_cuda_error(error: RuntimeError) -> VoxtralError:
    message = str(error)
    lowered = message.lower()
    if "out of memory" in lowered:
        return CudaOutOfMemory(message)
    if "device-side assert" in lowered:
        return CudaDeviceAssertion(message)
    return VoxtralError(message)
