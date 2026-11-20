# 🧪 Bayzip Automated Test Suite & Quality Report

<p align="center">
  <img src="https://img.shields.io/badge/tests-90%2F90%20passing-success?style=flat-square&logo=checkmarx&logoColor=white" alt="90/90 Automated Tests Passing" />
  <img src="https://img.shields.io/badge/typecheck-0%20errors-0078D6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript 0 Errors" />
  <img src="https://img.shields.io/badge/duration-4.29s-F59E0B?style=flat-square&logo=speedtest&logoColor=white" alt="Execution Time 4.29s" />
  <img src="https://img.shields.io/badge/coverage-100%25%20Passing-10B981?style=flat-square&logo=shield&logoColor=white" alt="100% Passing" />
  <img src="https://img.shields.io/badge/audit-60%2F60%20verified-8B5CF6?style=flat-square&logo=target&logoColor=white" alt="60/60 Audit Findings Verified" />
</p>

---

## 📋 Executive Summary

Bayzip enforces strict automated regression and security testing across all container engines, multi-core parallel unpackers, storage-aware NTFS I/O subsystems, cryptographic protocols, path sanitizers, and OS priority controllers. Every commit and release milestone is validated against:

1. **Cryptographic Stream Integrity**: Chunked AES-256-GCM AEAD encryption, async `PipedAesBufReader` read-ahead decryption roundtrip, wrong-password rejection, and premature EOF stream truncation detection.
2. **Deep-Defense Path Traversal Defense (Zip-Slip Proof)**: Denial of parent traversal (`..`), drive prefixes, root paths, DOS device aliases (`CLOCK$`, `CONFIG$`, `CON`, `AUX`, etc.), NTFS Alternate Data Streams (`:`), null bytes, illegal control characters, and external `bsdtar` symlink/hardlink/FIFO/socket/device entries.
3. **Container Framing, Seekable `.bay`/`.bayg` & Exact-Byte Alignment**: RFC 8878 Zstd seek table parsing and `locate_bay_payload_end` framing around `BAYTOC`/`BAYETOC` trailers, parallel Zstd frame decoding, solid `.7z` stream draining, and multi-member/multi-frame `.tar.gz`/`.tar.zst` alignment.
4. **Memory-Bounded Parsing & Storage-Aware I/O**: Strict 64 MB TOC cap (`MAX_TOC_SIZE`), `512 MB` `.7z` dictionary RAM budget, `96 MB` parallel `.bay` sliding window, Win32 `FileAllocationInfo` preallocation (`>= 64 KB`), byte-budgeted `AsyncFileWriterPool`, and deterministic `abort_and_join()` rollback cleanup.
5. **Hardware Scaling & Gamer Preset Limits**: Compression and extraction worker scaling (`StorageDeviceSeekPenaltyProperty` + `STORAGE_BUS_TYPE` across NVMe, SATA SSD, HDD, and USB) and Gamer Preset (`.bayg`) Zstandard Level 3 / `8 MB` frame enforcement.
6. **Thread-Local OS Priority Guards**: Prevention of UI and gaming thread priority inversion across nested scopes and parallel worker threads.
7. **Frontend Type-Safety**: 100% strict TypeScript compilation with zero diagnostic errors.

---

## 📊 Automated Test Matrix (90 Unit Tests)

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
| 11 | `extractor::bay` | `test_drain_bay_entry_bytes_exact_match` | Verifies exact byte consumption when skipping/excluding entries during streaming `.bay` extraction (`V-SEC-02`). | `PASSED` |
| 12 | `extractor::bay` | `test_drain_bay_entry_bytes_unaligned_chunking` | Validates stream alignment when draining entries across arbitrary buffer chunk boundaries. | `PASSED` |
| 13 | `extractor::bay` | `test_drain_bay_entry_bytes_premature_eof` | Traps premature EOF during entry draining, preventing container stream desynchronization. | `PASSED` |
| 14 | `extractor::bay` | `test_drain_bay_entry_bytes_zero_length` | Handles 0-byte entries without stream stalling. | `PASSED` |
| 15 | `extractor::bay` | `test_drain_bay_entry_bytes_io_error` | Ensures underlying I/O errors are immediately surfaced during stream draining. | `PASSED` |
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
| 37 | `extractor::utils` | `test_cleanup_extracted_files_safely_removes_created_files` | Validates `cleanup_extracted_files` safely deletes newly extracted items without panicking on missing paths. | `PASSED` |
| 38 | `compressor::utils` | `test_is_path_excluded_dot_slash_prefix` | Ensures `./`, `/`, and backslash relative path prefixes normalize cleanly during exclusion matching. | `PASSED` |
| 39 | `compressor::utils` | `test_is_excluded_or_temp_artifact` | Verifies `.baytmp`, `.baypart`, output archive, and temp archive paths are excluded from self-inclusion. | `PASSED` |
| 40 | `inspector::preview` | `test_extract_entry_ext` | Validates extension parsing on nested paths containing dots in folder names and extensionless files. | `PASSED` |
| 41 | `inspector::preview` | `test_is_likely_utf8_text` | Verifies 0-byte empty text handling, trailing incomplete UTF-8 boundary tolerance, and binary NUL rejection. | `PASSED` |
| 42 | `crypto` | `test_chunked_aes_drop_and_finish` | Verifies `ChunkedAesWriter`/`ChunkedAesReader` `Drop` zeroizes plaintext buffers and base nonces safely after finish or early drop. | `PASSED` |
| 43 | `inspector` | `test_rejects_unencrypted_baytoc_on_bayenc_archive` | Rejects unencrypted `BAYTOC` trailers appended to `BAYENC` encrypted containers. | `PASSED` |
| 44 | `inspector` | `test_inspect_encrypted_bay_wrong_password_returns_err` | Ensures `inspect_archive` returns an explicit wrong-password error instead of an empty listing for encrypted `.bay` containers. | `PASSED` |
| 45 | `extractor::utils` | `test_should_abort_fallback_on_cancel_and_password` | Verifies `universal_extract` fallback chains abort immediately on user cancellation or password errors. | `PASSED` |
| 46 | `extractor::bay` | `test_extract_bay_stream_eof_drain_and_truncation_detection` | Ensures `.bay` extraction detects truncated streams missing `entry_type == 0` EOF markers and drains remaining decoder bytes. | `PASSED` |
| 47 | `tester` | `test_should_abort_test_fallback_blocks_cancel_and_password` | Ensures `test_archive_sync` fallback logic never masks cancellation or password errors. | `PASSED` |
| 48 | `tester` | `test_test_bay_stream_detects_missing_eof_marker` | Verifies `test_bay_stream` detects truncated `.bay` payloads missing a terminating EOF marker. | `PASSED` |
| 49 | `tester` | `test_test_tar_archive_delegates_xz_and_bz2` | Confirms `.xz`, `.txz`, `.bz2`, and `.tbz2` archives delegate cleanly through system tar payload testing. | `PASSED` |
| 50 | `tester` | `test_concurrent_test_task_guards_do_not_clobber_tokens` | Verifies overlapping `TesterTaskGuard` instances isolate cancellation tokens inside the multi-token registry. | `PASSED` |
| 51 | `priority` | `test_midflight_background_shield_toggle_syncs_process_priority` | Validates `set_background_shield` synchronizes active OS process priority mid-flight under `PROCESS_PRIORITY_STATE`. | `PASSED` |
| 52 | `registry` | `test_format_registration_stamp_determinism` | Verifies deterministic formatting of shell registration stamp strings across binary paths and version metadata. | `PASSED` |
| 53 | `registry` | `test_should_skip_startup_registration_when_stamp_and_files_match` | Confirms cached startup registration is skipped only when the stamp matches and registered shell artifacts exist on disk. | `PASSED` |
| 54 | `lib` | `test_parse_cli_args_gui_compress_and_extract` | Verifies CLI parsing for `--gui-compress` and `--extract` verbs across multi-target paths. | `PASSED` |
| 55 | `lib` | `test_is_mini_extract_invocation_skips_startup_sync` | Ensures mini-extract CLI invocations are detected reliably so startup background registry synchronization is skipped. | `PASSED` |
| 56 | `inspector` | `test_build_excluded_set_and_prefix_matching` | Verifies normalized `build_excluded_set` construction and `O(depth)` ancestor prefix exclusion without sibling false positives. | `PASSED` |
| 57 | `extractor::system_tar` | `test_finalize_staged_extraction_preserves_preexisting_and_filters_case_glob_segments` | Verifies staged `tar.exe` extraction filters case/glob/sub-component exclusions cleanly without overwriting preexisting destination files. | `PASSED` |
| 58 | `inspector::hardware` | `test_calculate_extract_writer_threads_by_drive_profile` | Verifies extraction writer thread scaling across NVMe SSD, SATA SSD, spinning HDD, USB flash, and unknown drives via seek-penalty probing. | `PASSED` |
| 59 | `inspector::hardware` | `test_calculate_extract_inflight_budget_and_chunk_size` | Verifies storage-calibrated async writer memory budgets (`48 MB` / `32 MB` / `16 MB`) and VACB-aligned chunk sizes (`1 MB` / `512 KB` / `256 KB`). | `PASSED` |
| 60 | `extractor::io` | `test_create_output_file_fast_preallocates_without_extending_eof` | Validates NTFS `FileAllocationInfo` preallocation (`>= 64 KB`) reserves clusters without extending logical EOF on 0-byte or streaming writes. | `PASSED` |
| 61 | `extractor::io` | `test_write_file_payload_direct_and_atomic_overwrite` | Confirms direct payload writes and atomic temporary-file overwrite semantics when destination files already exist. | `PASSED` |
| 62 | `extractor::io` | `test_directory_cache_creates_nested_and_tracks_ancestors` | Validates `DirectoryCache` deduplicates repeated directory/parent syscalls while tracking newly created ancestors for rollback cleanup. | `PASSED` |
| 63 | `extractor::io` | `test_async_writer_pool_writes_files_and_handles_case_duplicates` | Confirms `AsyncFileWriterPool` writes queued payloads across worker threads and serializes Windows case-insensitive duplicate paths (last-wins). | `PASSED` |
| 64 | `extractor::io` | `test_async_writer_pool_respects_byte_budget_and_propagates_errors` | Verifies bounded in-flight byte accounting enforces memory limits and surfaces worker write errors cleanly on `finish()`. | `PASSED` |
| 65 | `extractor::io` | `test_async_writer_pool_abort_joins_before_cleanup` | Ensures `abort_and_join()` drains and joins all writer workers before rollback file deletion runs, preventing open-handle races. | `PASSED` |
| 66 | `extractor` | `test_extraction_task_guard_records_bytes_isolated` | Confirms `ExtractionTaskGuard::bytes_extracted()` and `record_extracted_bytes()` isolate byte telemetry per extraction token. | `PASSED` |
| 67 | `extractor::zip` | `test_calculate_zip_worker_threads_by_storage_profile` | Verifies ZIP parallel worker selection (`2–8` NVMe, `2–6` SATA SSD, `1` HDD/USB or small `< 4`-entry archives). | `PASSED` |
| 68 | `extractor::zip` | `test_parallel_zip_extraction_multifile_nested_and_exclusions` | Validates multi-worker parallel `.zip` extraction across nested directories, exclusions, and both Stored and SIMD Deflate entries. | `PASSED` |
| 69 | `extractor::zip` | `test_parallel_zip_handles_case_duplicate_entries_last_wins` | Confirms parallel ZIP extraction detects case-insensitive duplicate paths and enforces deterministic last-wins output without races. | `PASSED` |
| 70 | `extractor::zip` | `test_parallel_zip_atomic_overwrite_and_rejects_zip_slip` | Verifies atomic overwrites of preexisting targets and rejection/skipping of Zip-Slip traversal entries in parallel ZIP extraction. | `PASSED` |
| 71 | `extractor::zip` | `test_parallel_zip_encrypted_password_error_and_rollback` | Ensures AES-encrypted `.zip` extraction enforces password validation and rolls back partial files cleanly on bad passwords. | `PASSED` |
| 72 | `extractor::bay` | `test_extract_bay_archive_unencrypted_zstd_multifile_and_large_streaming` | Validates unencrypted `.bay` Zstd extraction across multi-file small entries (`AsyncFileWriterPool`) and large streaming payloads (`> 2 MB`). | `PASSED` |
| 73 | `extractor::bay` | `test_extract_bay_archive_encrypted_v3_pipelined_roundtrip_and_wrong_password` | Verifies `BAYENC\x03` extraction via `PipedAesBufReader` read-ahead ring pipeline and clean wrong-password error reporting. | `PASSED` |
| 74 | `extractor::bay` | `test_extract_bay_roundtrip_with_real_baytoc_and_bayetoc_trailers` | Confirms end-to-end extraction on unencrypted `BAYTOC` and encrypted `BAYETOC` containers with real appended trailer headers. | `PASSED` |
| 75 | `extractor::sevenz` | `test_calculate_7z_parallelism_by_storage_and_dict_memory` | Verifies `.7z` block worker and `Lzma2ReaderMt` thread scaling under storage profiles, `512 MB` dictionary RAM caps, and collision constraints. | `PASSED` |
| 76 | `extractor::sevenz` | `test_parallel_7z_multiblock_extraction_exclusions_empty_files_and_zip_slip` | Validates LPT-scheduled multi-block `.7z` parallel extraction, empty-file creation, exclusion filtering, and Zip-Slip defense. | `PASSED` |
| 77 | `extractor::sevenz` | `test_solid_7z_extraction_drains_excluded_entries_without_desync` | Ensures solid `.7z` blocks drain excluded entry streams completely without corrupting subsequent files in the same block. | `PASSED` |
| 78 | `extractor::sevenz` | `test_7z_cross_block_case_duplicate_serializes_last_wins` | Confirms cross-block Windows case-insensitive duplicate paths serialize safely and preserve last-wins semantics. | `PASSED` |
| 79 | `extractor::sevenz` | `test_7z_encrypted_password_handling_and_roundtrip` | Validates AES-256 encrypted `.7z` extraction roundtrip and explicit error handling when passwords are missing or wrong. | `PASSED` |
| 80 | `extractor::tar` | `test_extract_plain_tar_multifile_large_exclusions_and_zip_slip` | Validates native `.tar` extraction across small async-pooled files, large streaming payloads, exclusions, and traversal rejection. | `PASSED` |
| 81 | `extractor::tar` | `test_extract_tar_gz_multimember_pigz_and_single_root_hoist` | Verifies multi-member (`pigz`-style) `.tar.gz` extraction via `zlib-rs` `MultiGzDecoder` and single-pass `hoist_single_root_dir`. | `PASSED` |
| 82 | `extractor::tar` | `test_extract_tar_zst_multiframe_roundtrip` | Validates concatenated multi-frame `.tar.zst` decompression and directory/file extraction integrity. | `PASSED` |
| 83 | `extractor::tar` | `test_extract_tar_atomic_overwrite_and_case_duplicates` | Confirms native TAR extraction handles case-duplicate entries deterministically and atomically overwrites existing files. | `PASSED` |
| 84 | `extractor::tar` | `test_extract_tar_rejects_non_tar_gz_and_zst_cleanly` | Ensures bare non-TAR `.gz` / `.zst` payloads reject cleanly so `universal_extract` delegates single-file decompression properly. | `PASSED` |
| 85 | `extractor::system_tar` | `test_parse_bsdtar_tvf_line_formats_spaces_and_security` | Verifies fail-closed `parse_bsdtar_tvf_line` parsing across US/BSD/European/ISO dates, filenames with spaces, and link/FIFO/socket/device blocking. | `PASSED` |
| 86 | `extractor::system_tar` | `test_extract_with_system_tar_single_pass_roundtrip_exclusions_and_security` | Validates single-pass `tar.exe` extraction roundtrip, exclusion staging, and security blocking on malformed archive entries. | `PASSED` |
| 87 | `inspector::bay` | `test_locate_bay_payload_end_and_read_zstd_seek_table_validation` | Validates `locate_bay_payload_end` and RFC 8878 `read_zstd_seek_table` parsing, CRC/checksum bounds, and malformed footer rejection. | `PASSED` |
| 88 | `extractor::bay` | `test_calculate_bay_seekable_workers_by_storage_and_memory_budget` | Verifies `calculate_bay_seekable_workers` worker and sliding-window slot calculation within the `96 MB` RAM ceiling across drive types. | `PASSED` |
| 89 | `extractor::bay` | `test_phase6_seekable_multiframe_bay_parallel_extract_preview_tester_and_corruption_rollback` | End-to-end Phase 6 test covering multi-frame seekable `.bay` compression, parallel extraction, preview, `tester.rs`, and frame corruption rollback. | `PASSED` |
| 90 | `extractor::bay` | `test_phase6_headless_gamer_preset_creates_multiframe_8mb_seekable_bayg` | Verifies `.bayg` Gamer Preset creates `8 MB` multi-frame RFC 8878 seekable containers with instant `BAYTOC` and full parallel extraction parity. | `PASSED` |

---

## 🖥️ Live CLI Execution Log

### Backend Rust Test Suite (`cargo test`)
```text
running 90 tests
test compressor::tests::test_calculate_target_threads_scaling ... ok
test compressor::tests::test_calculate_zstd_level_gamer_preset_cap ... ok
test compressor::tests::test_is_gamer_preset_normalization ... ok
test compressor::tests::test_compression_task_guard_isolation ... ok
test compressor::utils::tests::test_is_path_excluded_dot_slash_prefix ... ok
test extractor::bay::tests::test_drain_bay_entry_bytes_exact_match ... ok
test extractor::bay::tests::test_calculate_bay_seekable_workers_by_storage_and_memory_budget ... ok
test extractor::bay::tests::test_drain_bay_entry_bytes_io_error ... ok
test extractor::bay::tests::test_drain_bay_entry_bytes_premature_eof ... ok
test extractor::bay::tests::test_drain_bay_entry_bytes_unaligned_chunking ... ok
test extractor::bay::tests::test_drain_bay_entry_bytes_zero_length ... ok
test compressor::utils::tests::test_is_excluded_or_temp_artifact ... ok
test extractor::io::tests::test_async_writer_pool_abort_joins_before_cleanup ... ok
test extractor::io::tests::test_async_writer_pool_respects_byte_budget_and_propagates_errors ... ok
test extractor::io::tests::test_async_writer_pool_writes_files_and_handles_case_duplicates ... ok
test extractor::io::tests::test_directory_cache_creates_nested_and_tracks_ancestors ... ok
test extractor::io::tests::test_create_output_file_fast_preallocates_without_extending_eof ... ok
test extractor::io::tests::test_write_file_payload_direct_and_atomic_overwrite ... ok
test extractor::sevenz::tests::test_7z_cross_block_case_duplicate_serializes_last_wins ... ok
test extractor::sevenz::tests::test_calculate_7z_parallelism_by_storage_and_dict_memory ... ok
test extractor::bay::tests::test_phase6_seekable_multiframe_bay_parallel_extract_preview_tester_and_corruption_rollback ... ok
test extractor::sevenz::tests::test_7z_encrypted_password_handling_and_roundtrip ... ok
test extractor::bay::tests::test_extract_bay_archive_unencrypted_zstd_multifile_and_large_streaming ... ok
test extractor::sevenz::tests::test_solid_7z_extraction_drains_excluded_entries_without_desync ... ok
test extractor::system_tar::tests::test_finalize_staged_extraction_preserves_preexisting_and_filters_case_glob_segments ... ok
test extractor::system_tar::tests::test_parse_bsdtar_tvf_line_formats_spaces_and_security ... ok
test extractor::tar::tests::test_extract_tar_atomic_overwrite_and_case_duplicates ... ok
test extractor::tar::tests::test_extract_tar_gz_multimember_pigz_and_single_root_hoist ... ok
test extractor::tar::tests::test_extract_plain_tar_multifile_large_exclusions_and_zip_slip ... ok
test extractor::tar::tests::test_extract_tar_rejects_non_tar_gz_and_zst_cleanly ... ok
test extractor::tests::test_extraction_task_guard_isolation ... ok
test extractor::tests::test_extraction_task_guard_records_bytes_isolated ... ok
test extractor::utils::tests::test_cleanup_extracted_files_safely_removes_created_files ... ok
test extractor::utils::tests::test_should_abort_fallback_on_cancel_and_password ... ok
test extractor::zip::tests::test_calculate_zip_worker_threads_by_storage_profile ... ok
test extractor::tar::tests::test_extract_tar_zst_multiframe_roundtrip ... ok
test crypto::tests::test_chunked_aes_roundtrip ... ok
test extractor::bay::tests::test_phase6_headless_gamer_preset_creates_multiframe_8mb_seekable_bayg ... ok
test crypto::tests::test_chunked_aes_wrong_password ... ok
test crypto::tests::test_chunked_aes_truncation_detection ... ok
test inspector::bay::tests::test_locate_bay_payload_end_and_read_zstd_seek_table_validation ... ok
test extractor::zip::tests::test_parallel_zip_atomic_overwrite_and_rejects_zip_slip ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_encrypted_toc_exceeding_file_bounds ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_file_too_small ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_oversized_encrypted_toc ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_toc_exceeding_file_bounds ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_zero_encrypted_toc_len ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_zero_toc_len ... ok
test inspector::bay::tests::test_read_bay_trailer_valid_toc ... ok
test extractor::bay::tests::test_extract_bay_stream_eof_drain_and_truncation_detection ... ok
test inspector::bay::tests::test_read_bay_trailer_rejects_oversized_toc ... ok
test inspector::preview::tests::test_extract_entry_ext ... ok
test inspector::preview::tests::test_is_likely_utf8_text ... ok
test inspector::tests::test_absolute_and_drive_paths ... ok
test inspector::tests::test_build_excluded_set_and_prefix_matching ... ok
test extractor::zip::tests::test_parallel_zip_handles_case_duplicate_entries_last_wins ... ok
test inspector::hardware::tests::test_calculate_extract_writer_threads_by_drive_profile ... ok
test inspector::hardware::tests::test_calculate_extract_inflight_budget_and_chunk_size ... ok
test inspector::tests::test_dos_reserved_devices ... ok
test inspector::tests::test_empty_and_dot_paths ... ok
test inspector::tests::test_invalid_characters ... ok
test inspector::tests::test_parent_traversal ... ok
test inspector::tests::test_rejects_unencrypted_baytoc_on_bayenc_archive ... ok
test inspector::tests::test_trailing_period_and_space_evasion ... ok
test inspector::tests::test_valid_paths ... ok
test priority::tests::test_background_shield_toggle ... ok
test priority::tests::test_concurrent_threads_priority_guards_restore_independently ... ok
test priority::tests::test_nested_scoped_priority_guards_unwind_correctly ... ok
test registry::tests::test_format_registration_stamp_determinism ... ok
test priority::tests::test_midflight_background_shield_toggle_syncs_process_priority ... ok
test registry::tests::test_should_skip_startup_registration_when_stamp_and_files_match ... ok
test priority::tests::test_scoped_priority_guard_restores_thread_priority ... ok
test tester::tests::test_concurrent_test_task_guards_do_not_clobber_tokens ... ok
test tester::tests::test_tester_task_guard_isolation ... ok
test tester::tests::test_test_tar_archive_delegates_xz_and_bz2 ... ok
test tester::tests::test_should_abort_test_fallback_blocks_cancel_and_password ... ok
test tester::tests::test_test_bay_stream_detects_missing_eof_marker ... ok
test tests::test_is_mini_extract_invocation_skips_startup_sync ... ok
test tests::test_parse_cli_args_gui_compress_and_extract ... ok
test types::tests::test_task_cancel_token_lifecycle ... ok
test tests::test_collect_system_specs ... ok
test types::tests::test_task_cancel_tokens_are_isolated ... ok
test extractor::zip::tests::test_parallel_zip_extraction_multifile_nested_and_exclusions ... ok
test crypto::tests::test_chunked_aes_drop_and_finish ... ok
test extractor::zip::tests::test_parallel_zip_encrypted_password_error_and_rollback ... ok
test extractor::bay::tests::test_extract_bay_roundtrip_with_real_baytoc_and_bayetoc_trailers ... ok
test extractor::sevenz::tests::test_parallel_7z_multiblock_extraction_exclusions_empty_files_and_zip_slip ... ok
test inspector::tests::test_inspect_encrypted_bay_wrong_password_returns_err ... ok
test extractor::system_tar::tests::test_extract_with_system_tar_single_pass_roundtrip_exclusions_and_security ... ok
test extractor::bay::tests::test_extract_bay_archive_encrypted_v3_pipelined_roundtrip_and_wrong_password ... ok

test result: ok. 90 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.29s
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
