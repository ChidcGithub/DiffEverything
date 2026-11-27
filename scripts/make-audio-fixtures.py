"""Create small WAV fixtures with different audio properties."""
from pathlib import Path
import math
import struct
import wave

root = Path(__file__).resolve().parent.parent
out = root / "fixtures" / "audio"
out.mkdir(parents=True, exist_ok=True)


def write_wav(path: Path, seconds: float, rate: int, channels: int, frequency: float) -> None:
    frames = int(seconds * rate)
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(channels)
        handle.setsampwidth(2)
        handle.setframerate(rate)
        payload = bytearray()
        for index in range(frames):
            value = int(0.35 * 32767 * math.sin(2 * math.pi * frequency * index / rate))
            for _ in range(channels):
                payload.extend(struct.pack("<h", value))
        handle.writeframes(payload)


write_wav(out / "old.wav", 0.25, 8000, 1, 440.0)
write_wav(out / "new.wav", 0.50, 16000, 2, 660.0)
print(out / "old.wav")
print(out / "new.wav")
