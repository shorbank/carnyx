"""Verify the pinned native SQLCipher sources on every desktop CI runner."""
from hashlib import sha256
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "src-tauri/vendor/libsqlite3-sys/sqlcipher"
expected = {
    "sqlite3.c": "8640c653acadf665cce6331646f60b5b74a4690746f2c4a2d8f688a0570a0c0c",
    "sqlite3.h": "8a9d1bff44d75174ca6dea3ea9bac50a6104d86facb566647b8bb839375b7b3a",
    "sqlite3ext.h": "ac9645e5c9ff0cf176efdd6e75cb5e98f46295d38e02db5c4d208826a39ab4be",
}
for name, digest in expected.items():
    if sha256((root / name).read_bytes()).hexdigest() != digest:
        raise SystemExit(f"SQLCipher source mismatch: {name}")
print("Pinned SQLCipher 4.19.0 sources verified")
