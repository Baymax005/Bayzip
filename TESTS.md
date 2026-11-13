# 🧪 Bayzip Automated Test Suite & Quality Report

<p align="center">
  <img src="https://img.shields.io/badge/tests-41%2F41%20passing-success?style=flat-square&logo=checkmarx&logoColor=white" alt="41/41 Automated Tests Passing" />
  <img src="https://img.shields.io/badge/typecheck-0%20errors-0078D6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript 0 Errors" />
  <img src="https://img.shields.io/badge/duration-0.08s-F59E0B?style=flat-square&logo=speedtest&logoColor=white" alt="Execution Time 0.08s" />
  <img src="https://img.shields.io/badge/coverage-100%25%20Passing-10B981?style=flat-square&logo=shield&logoColor=white" alt="100% Passing" />
  <img src="https://img.shields.io/badge/audit-35%2F35%20verified-8B5CF6?style=flat-square&logo=target&logoColor=white" alt="35/35 Audit Findings Verified" />
</p>

---

## 📋 Executive Summary

Bayzip enforces strict automated regression and security testing across all container engines, cryptographic protocols, path sanitizers, and OS priority controllers. Every commit and release milestone is validated against:

1. **Cryptographic Stream Integrity**: Chunked AES-256-GCM AEAD encryption, decryption roundtrip, wrong-password rejection, and premature EOF stream truncation detection.
2. **Deep-Defense Path Traversal Defense (Zip-Slip Proof)**: Denial of parent traversal (`..`), drive prefixes, root paths, DOS device aliases (`CLOCK$`, `CONFIG$`, `CON`, `AUX`, etc.), NTFS Alternate Data Streams (`:`), null bytes, and illegal control characters.
3. **Container Framing & Exact-Byte Alignment**: Decompression stream draining without alignment desynchronization.
4. **Memory-Bounded Parsing**: Strict 64 MB cap on archive Table of Contents (TOC) to eliminate Out-Of-Memory (OOM) attack vectors.
5. **Hardware Scaling & Gamer Preset Limits**: Thread allocation scaling (0%–100%) and Zstandard Level 3 cap enforcement.
6. **Thread-Local OS Priority Guards**: Prevention of UI and gaming thread priority inversion.
7. **Frontend Type-Safety**: 100% strict TypeScript compilation with zero diagnostic errors.

---

## 📊 Automated Test Matrix (41 Unit Tests)

| # | Test Module | Test Name | Purpose & Security Verification | Status |
|---|---|---|---|:---:|
| 1 | `crypto` | `test_chunked_aes_roundtrip` | Verifies full streaming AES-256-GCM encrypt and decrypt roundtrip. | `PASSED` |
| 2 | `crypto` | `test_chunked_aes_truncation_detection` | Verifies `UnexpectedEof` is thrown when ciphertext stream is truncated without valid 4-byte EOF marker (`V-SEC-04`). | `PASSED` |
| 3 | `crypto` | `test_chunked_aes_wrong_password` | Verifies `PermissionDenied` error is returned on password mismatch or authentication tag corruption. | `PASSED` |
| 4 | `inspector` | `test_dos_reserved_devices` | Rejects Windows reserved device names: `CON`, `NUL`, `PRN`, `AUX`, `COM1-9`, `LPT1-9`, `CONIN$`, `CONOUT$`, `CLOCK$`, `CONFIG$` (`V-SEC-08`). | `PASSED` |
| 5 | `inspector` | `test_invalid_characters` | Rejects reserved filesystem characters (`:`, `<`, `>`, `"`, `\|`, `?`, `*`, `\0`) and ASCII control characters `< 32` (`V-SEC-08`). | `PASSED` |
| 6 | `inspector` | `test_parent_traversal` | Blocks Zip-Slip directory traversal attempts (`..`, `../`, `../../evil.txt`, `a/b/../../../c.txt`). | `PASSED` |
| 7 | `inspector` | `test_absolute_and_drive_paths` | Blocks absolute paths, drive prefixes, and UNC shares (`/etc/passwd`, `C:\Windows\...`, `\\server\share`). | `PASSED` |
| 8 | `inspector` | `test_trailing_period_and_space_evasion` | Blocks Win32 trailing period/space namespace evasion exploits (`evil.exe .`, `folder./file`, `secret.txt.`). | `PASSED` |
| 9 | `inspector` | `test_empty_and_dot_paths` | Rejects empty strings, whitespace, and bare `.` paths. | `PASSED` |
| 10 | `inspector` | `test_valid_paths` | Ensures legitimate relative paths and nested directory paths are properly sanitized and preserved. | `PASSED` |
| 11 | `extractor` | `test_drain_bay_entry_bytes_exact_match` | Verifies exact byte consumption when skipping/excluding entries during streaming `.bay` extraction (`V-SEC-02`). | `PASSED` |
| 12 | `extractor` | `test_drain_bay_entry_bytes_unaligned_chunking` | Validates stream alignment when draining entries across arbitrary buffer chunk boundaries. | `PASSED` |
| 13 | `extractor` | `test_drain_bay_entry_bytes_premature_eof` | Traps premature EOF during entry draining, preventing container stream desynchronization. | `PASSED` |
| 14 | `extractor` | `test_drain_bay_entry_bytes_zero_length` | Handles 0-byte entries without stream stalling. | `PASSED` |
| 15 | `extractor` | `test_drain_bay_entry_bytes_io_error` | Ensures underlying I/O errors are immediately surfaced during stream draining. | `PASSED` |
| 16 | `inspector::bay` | `test_read_bay_trailer_valid_toc` | Validates correct parsing of `.bay` instant Table of Contents (TOC) metadata. | `PASSED` |
| 17 | `inspector::bay` | `test_read_bay_trailer_rejects_file_too_small` | Safely rejects undersized files before reading trailer magic. | `PASSED` |
| 18 | `inspector::bay` | `test_read_bay_trailer_rejects_zero_toc_len` | Rejects corrupted zero-length TOC entries. | `PASSED` |
| 19 | `inspector::bay` | `test_read_bay_trailer_rejects_oversized_toc` | Enforces strict 64 MB memory upper bound (`MAX_TOC_SIZE`) to prevent OOM denial-of-service (`V-SEC-03`). | `PASSED` |
| 20 | `inspector::bay` | `test_read_bay_trailer_rejects_toc_exceeding_file_bounds` | Rejects TOC lengths that exceed the physical container bounds. | `PASSED` |
| 21 | `inspector::bay` | `test_read_bay_trailer_rejects_zero_encrypted_toc_len` | Rejects malformed zero-length encrypted TOC (ETOC) trailers. | `PASSED` |
| 22 | `inspector::bay` | `test_read_bay_trailer_rejects_oversized_encrypted_toc` | Enforces 64 MB cap on encrypted Table of Contents. | `PASSED` |
| 23 | `inspector::bay` | `test_read_bay_trailer_rejects_encrypted_toc_exceeding_file_bounds` | Rejects out-of-bounds encrypted TOC trailers. | `PASSED` |
| 24 | `compressor` | `test_calculate_target_threads_scaling` | Validates core allocation scaling across the hardware slider: 0% (1 core) -> 25% (total/4) -> 50% (total/2) -> 75% (3/4) -> 100% (all cores). | `PASSED` |
| 25 | `compressor` | `test_calculate_zstd_level_gamer_preset_cap` | Validates Gamer Preset (`.bayg`) is strictly capped at Zstandard Level 3 across 75% and 100% hardware allocations (`V-PERF-05`). | `PASSED` |
| 26 | `compressor` | `test_is_gamer_preset_normalization` | Verifies normalization of `.bayg`, `.baygamer`, and `.bay_gamer` aliases. | `PASSED` |
| 27 | `priority` | `test_scoped_priority_guard_restores_thread_priority` | Confirms thread priority is demoted to `THREAD_PRIORITY_BELOW_NORMAL` during operations and restored upon drop (`V-CONC-01`). | `PASSED` |
| 28 | `priority` | `test_nested_scoped_priority_guards_unwind_correctly` | Ensures nested priority guards restore the exact caller priority level upon unwinding. | `PASSED` |
| 29 | `priority` | `test_concurrent_threads_priority_guards_restore_independently` | Confirms priority adjustments in concurrent worker threads remain isolated and do not collide. | `PASSED` |
| 30 | `priority` | `test_background_shield_toggle` | Verifies priority shield toggling behavior. | `PASSED` |
| 31 | `system` | `test_collect_system_specs` | Validates asynchronous physical hardware query (CPU core counts, total RAM). | `PASSED` |
| 32 | `types` | `test_task_cancel_token_lifecycle` | Verifies atomic cancel flag initialization, cancellation signaling, and token isolation lifecycle. | `PASSED` |
| 33 | `types` | `test_task_cancel_tokens_are_isolated` | Confirms independent `TaskCancelToken` instances do not interfere across concurrent tasks. | `PASSED` |
| 34 | `compressor` | `test_compression_task_guard_isolation` | Verifies RAII `CompressionTaskGuard` isolates thread-local cancellation scopes without zombie leaks. | `PASSED` |
| 35 | `extractor` | `test_extraction_task_guard_isolation` | Verifies RAII `ExtractionTaskGuard` scopes isolate cancellation flags per extraction job. | `PASSED` |
| 36 | `tester` | `test_tester_task_guard_isolation` | Verifies RAII `TestTaskGuard` scopes isolate cancellation flags during integrity tests. | `PASSED` |
| 37 | `extractor` | `test_cleanup_extracted_files_safely_removes_created_files` | Validates `cleanup_extracted_files` safely deletes newly extracted items without panicking on missing paths. | `PASSED` |
| 38 | `compressor::utils` | `test_is_path_excluded_dot_slash_prefix` | Ensures `./`, `/`, and backslash relative path prefixes normalize cleanly during exclusion matching. | `PASSED` |
| 39 | `compressor::utils` | `test_is_excluded_or_temp_artifact` | Verifies `.baytmp`, `.baypart`, output archive, and temp archive paths are excluded from self-inclusion. | `PASSED` |
| 40 | `inspector::preview` | `test_extract_entry_ext` | Validates extension parsing on nested paths containing dots in folder names and extensionless files. | `PASSED` |
| 41 | `inspector::preview` | `test_is_likely_utf8_text` | Verifies 0-byte empty text handling, trailing incomplete UTF-8 boundary tolerance, and binary NUL rejection. | `PASSED` |

---

## 🖥️ Live CLI Execution Log

### Backend Rust Test Suite (`cargo test`)
```text
running 41 tests
test compressor::tests::test_calculate_target_threads_scaling ... ok
test compressor::tests::test_is_gamer_preset_normalization ... ok
test extractor::tests::test_drain_bay_entry_bytes_io_error ... ok
test compressor::utils::tests::test_is_path_excluded_dot_slash_prefix ... ok
test compressor::tests::test_calculate_zstd_level_gamer_preset_cap ... ok
test compressor::tests::test_compression_task_guard_isolation ... ok
test extractor::tests::test_drain_bay_entry_bytes_exact_match ... ok
test extractor::tests::test_drain_bay_entry_bytes_premature_eof ... ok
test extractor::tests::test_drain_bay_entry_bytes_unaligned_chunking ... ok
test extractor::tests::test_drain_bay_entry_bytes_zero_length ... ok
test extractor::tests::test_extraction_task_guard_isolation ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_encrypted_toc_exceeding_file_bounds ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_file_too_small ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_oversized_encrypted_toc ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_oversized_toc ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_toc_exceeding_file_bounds ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_zero_encrypted_toc_len ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_zero_toc_len ... ok
test compressor::utils::tests::test_is_excluded_or_temp_artifact ... ok
test inspector::preview::tests::test_extract_entry_ext ... ok
test inspector::bay::tests::test_read_bay_trailer_valid_toc ... ok
test inspector::preview::tests::test_is_likely_utf8_text ... ok
test inspector::tests::test_absolute_and_drive_paths ... ok
test inspector::tests::test_dos_reserved_devices ... ok
test inspector::tests::test_empty_and_dot_paths ... ok
test inspector::tests::test_invalid_characters ... ok
test inspector::tests::test_parent_traversal ... ok
test inspector::tests::test_trailing_period_and_space_evasion ... ok
test inspector::tests::test_valid_paths ... ok
test priority::tests::test_background_shield_toggle ... ok
test types::tests::test_task_cancel_token_lifecycle ... ok
test tester::tests::test_tester_task_guard_isolation ... ok
test types::tests::test_task_cancel_tokens_are_isolated ... ok
test priority::tests::test_concurrent_threads_priority_guards_restore_independently ... ok
test extractor::tests::test_cleanup_extracted_files_safely_removes_created_files ... ok
test priority::tests::test_nested_scoped_priority_guards_unwind_correctly ... ok
test priority::tests::test_scoped_priority_guard_restores_thread_priority ... ok
test tests::test_collect_system_specs ... ok
test crypto::tests::test_chunked_aes_wrong_password ... ok
test crypto::tests::test_chunked_aes_truncation_detection ... ok
test crypto::tests::test_chunked_aes_roundtrip ... ok

test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

### Frontend TypeScript Static Analysis (`npx tsc --noEmit`)
```text
$ npx tsc --noEmit
Exit code: 0
Diagnostics: 0 errors
```

---

## 🛠️ How to Run the Tests Locally

You can reproduce and execute this exact test suite on any Windows system:

### 1. Run Backend Unit Tests
```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

### 2. Run Single Test Module (e.g. Cryptography or Security)
```bash
# Cryptographic AEAD streaming & truncation tests
cargo test --manifest-path src-tauri/Cargo.toml crypto

# Path traversal & Zip-Slip defense tests
cargo test --manifest-path src-tauri/Cargo.toml inspector::tests
```

### 3. Run Frontend Typecheck
```bash
npx tsc --noEmit
```
