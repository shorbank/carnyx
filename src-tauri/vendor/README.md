# Pinned SQLCipher build

`libsqlite3-sys` is the published 0.38.2 crate, patched locally because it bundles
SQLCipher 4.14.0. Only its SQLCipher C amalgamation, headers, version constants in
the existing bindings, and SQLCipher license were updated. The unused plain SQLite
C amalgamation is omitted. Its build script and stable SQLite/codec FFI remain upstream.
Remove the patch when a reviewed release supplies the required SQLCipher version.

The sources were generated from the official
[SQLCipher v4.19.0 tag](https://github.com/sqlcipher/sqlcipher/tree/v4.19.0), based
on SQLite 3.53.4. Fixes since 4.14.0 include export alias escaping, initialization
cleanup and Windows logging issues; Carnyx uses a fixed `legacy` alias, native
`sqlite3_key`, and disables SQLCipher logging on its keyed connections.

Source archive:
`https://codeload.github.com/sqlcipher/sqlcipher/tar.gz/refs/tags/v4.19.0`

SHA-256: `7075f96cbabe45b4ecfc2e6b1745a625f856f695b0827a5506ce9ed85b906aa0`

Reproduction (C compiler, make and Tcl/Jim required, not needed for normal app builds):

```sh
./configure --with-tempstore=yes --disable-tcl --enable-threadsafe
make sqlite3.c
```

Copy `sqlite3.c`, `sqlite3.h`, `sqlite3ext.h`, and `LICENSE.md` into the `sqlcipher`
directory. The source hashes are checked by `scripts/verify-sqlcipher.py` in CI;
connections also reject any native cipher version other than 4.19.0 at runtime.
The retained FFI is SQLite's stable ABI; the application does not call newly added
SQLite APIs. Version metadata in `bindgen_bundled_version.rs` matches the new header.

macOS uses system CommonCrypto (no Homebrew runtime dependency). Windows uses
statically bundled OpenSSL from the locked `openssl-src` dependency; build tools
need MSVC and Perl. The Windows hosted runner already includes Perl. Avoid setting
`OPENSSL_DIR` on macOS, as upstream's build script treats that as an explicit
request to link to external OpenSSL.

Licenses: `libsqlite3-sys/LICENSE` (MIT), `sqlcipher/LICENSE.md` (BSD), and SQLite
public domain. Keep those notices when distributing the app.
