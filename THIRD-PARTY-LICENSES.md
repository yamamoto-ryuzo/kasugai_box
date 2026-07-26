# Third-Party Licenses

`kasugai_box` itself is released under the [MIT License](./LICENSE).

This project uses the following third-party Rust crates. Their respective licenses are compatible with the MIT license of `kasugai_box` and do not impose copyleft obligations on this application.

## License Summary

The following summary is generated from `cargo-license` and lists each license expression and the crates under that expression.

### (Apache-2.0 OR MIT)

The largest group of dependencies is dual-licensed under Apache-2.0 OR MIT. Notable crates include:

`aes`, `anyhow`, `apple-native-keyring-store`, `async-broadcast`, `async-channel`, `async-executor`, `async-io`, `async-lock`, `async-process`, `async-recursion`, `async-signal`, `async-task`, `async-trait`, `atomic-waker`, `autocfg`, `base64`, `bitflags`, `block-buffer`, `block-padding`, `blocking`, `bstr`, `bumpalo`, `cbc`, `cc`, `cfg-if`, `cipher`, `concurrent-queue`, `core-foundation`, `core-foundation-sys`, `cpufeatures`, `crossbeam-utils`, `crypto-common`, `digest`, `dirs`, `dirs-sys`, `displaydoc`, `enumflags2`, `enumflags2_derive`, `equivalent`, `errno`, `event-listener`, `event-listener-strategy`, `fastrand`, `find-msvc-tools`, `fnv`, `foreign-types`, `foreign-types-shared`, `form_urlencoded`, `futures`, `futures-channel`, `futures-core`, `futures-executor`, `futures-io`, `futures-lite`, `futures-macro`, `futures-sink`, `futures-task`, `futures-util`, `getrandom`, `hashbrown`, `hermit-abi`, `hex`, `hkdf`, `hmac`, `http`, `httparse`, `httpdate`, `hyper-tls`, `idna`, `idna_adapter`, `indexmap`, `inout`, `ipnet`, `itoa`, `js-sys`, `keyring`, `keyring-core`, `libc`, `lock_api`, `log`, `mime`, `native-tls`, `normpath`, `num`, `num-bigint`, `num-complex`, `num-integer`, `num-iter`, `num-rational`, `num-traits`, `once_cell`, `opener`, `openssl-macros`, `openssl-probe`, `ordered-stream`, `parking`, `parking_lot`, `parking_lot_core`, `percent-encoding`, `pin-project-lite`, `piper`, `pkg-config`, `polling`, `proc-macro-crate`, `proc-macro2`, `quote`, `regex`, `regex-automata`, `regex-syntax`, `reqwest`, `rustls-pki-types`, `rustversion`, `scopeguard`, `secret-service`, `security-framework`, `security-framework-sys`, `serde`, `serde_core`, `serde_derive`, `serde_json`, `serde_path_to_error`, `serde_repr`, `serde_urlencoded`, `sha2`, `shlex`, `signal-hook-registry`, `smallvec`, `socket2`, `stable_deref_trait`, `syn`, `system-configuration`, `system-configuration-sys`, `tempfile`, `thiserror`, `thiserror-impl`, `tokio-rustls`, `toml_datetime`, `toml_edit`, `toml_parser`, `typenum`, `url`, `utf8_iter`, `uuid`, `vcpkg`, `version_check`, `wasm-bindgen`, `wasm-bindgen-futures`, `wasm-bindgen-macro`, `wasm-bindgen-macro-support`, `wasm-bindgen-shared`, `web-sys`, `windows-link`, `windows-native-keyring-store`, `windows-registry`, `windows-result`, `windows-strings`, `windows-sys`, `windows-targets`, `windows_aarch64_gnullvm`, `windows_aarch64_msvc`, `windows_i686_gnu`, `windows_i686_gnullvm`, `windows_i686_msvc`, `windows_x86_64_gnu`, `windows_x86_64_gnullvm`, `windows_x86_64_msvc`, `zbus-secret-service-keyring-store`, `zeroize`

### MIT

`axum`, `axum-core`, `bytes`, `endi`, `generic-array`, `h2`, `http-body`, `http-body-util`, `hyper`, `hyper-util`, `libredox`, `memoffset`, `mio`, `openssl-sys`, `redox_syscall`, `redox_users`, `schannel`, `slab`, `synstructure`, `tokio`, `tokio-macros`, `tokio-native-tls`, `tokio-util`, `tower`, `tower-http`, `tower-layer`, `tower-service`, `tracing`, `tracing-attributes`, `tracing-core`, `try-lock`, `uds_windows`, `want`, `winnow`, `zbus`, `zbus_macros`, `zbus_names`, `zmij`, `zvariant`, `zvariant_derive`, `zvariant_utils`

### MIT OR Unlicense

`aho-corasick`, `byteorder`, `csv`, `csv-core`, `memchr`

### Apache-2.0

`openssl`, `sync_wrapper`

### Apache-2.0 AND ISC

`ring`

### Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT

`linux-raw-sys`, `rustix`, `wasi`

### Apache-2.0 OR BSL-1.0

`ryu`

### Apache-2.0 OR ISC OR MIT

`hyper-rustls`, `rustls`

### Apache-2.0 OR LGPL-2.1-or-later OR MIT

`r-efi`

### BSD-2-Clause

`kamadak-exif`, `mutate_once`

### BSD-3-Clause

`subtle`

### BSD-3-Clause AND MIT

`matchit`

### ISC

`rustls-webpki`, `untrusted`

### MPL-2.0

`option-ext`

### Unicode-3.0

`icu_collections`, `icu_locale_core`, `icu_normalizer`, `icu_normalizer_data`, `icu_properties`, `icu_properties_data`, `icu_provider`, `litemap`, `potential_utf`, `tinystr`, `writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`, `zerotrie`, `zerovec`, `zerovec-derive`

### Special Cases

- `(Apache-2.0 OR MIT) AND BSD-3-Clause`: `encoding_rs`
- `(Apache-2.0 OR MIT) AND Unicode-3.0`: `unicode-ident`
- `N/A`: `kasugai_box`（本ソフトウェア自身）

## Notes

- すべての依存クレートは **MIT または Apache-2.0 系の寛容なライセンス**、あるいは `BSD-2-Clause` / `BSD-3-Clause` / `ISC` / `MPL-2.0` / `Unicode-3.0` です。
- `GPL` / `LGPL` などの強いコピーレフトを持つ依存は含まれていません。
- バイナリ配布時は各クレートの著作権表示・ライセンス条文を保持する必要があります。詳細は各クレートのソースまたは crates.io を参照してください。
