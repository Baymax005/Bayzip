use std::io::{Read, Write};
use aes_gcm::{
    aead::{Aead, AeadInPlace, KeyInit, Payload},
    Aes256Gcm, Key, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const BAY_CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4 MB plaintext chunk
pub const BAY_ENC_V4_MAGIC: &[u8; 8] = b"BAYENC\x04\x00";
pub const BAY_ETOC_V4_MAGIC: &[u8; 8] = b"BAYETOC\x04";
pub const BAY_ENC_HEADER_LEN: usize = 32; // 8-byte magic + 16-byte salt + 8-byte base_nonce
pub const BAY_CHUNK_FLAG_MORE: u8 = 0x00;
pub const BAY_CHUNK_FLAG_FINAL: u8 = 0x01;
pub const BAY_ARGON2_MEMORY_KIB: u32 = 65_536; // 64 MiB
pub const BAY_ARGON2_TIME_COST: u32 = 3;
pub const BAY_ARGON2_PARALLELISM: u32 = 1;

const STREAM_KEY_DOMAIN: &[u8] = b"bayzip-v4-stream-key";
const TOC_KEY_DOMAIN: &[u8] = b"bayzip-v4-toc-key";

fn derive_bay_master_key(password: &str, salt: &[u8; 16]) -> std::io::Result<[u8; 32]> {
    let params = Params::new(
        BAY_ARGON2_MEMORY_KIB,
        BAY_ARGON2_TIME_COST,
        BAY_ARGON2_PARALLELISM,
        Some(32),
    )
    .map_err(|e| std::io::Error::other(format!("Invalid Argon2id parameters: {}", e)))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut master_key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut master_key)
        .map_err(|e| std::io::Error::other(format!("Argon2id key derivation failed: {}", e)))?;
    Ok(master_key)
}

fn derive_subkey(
    domain: &[u8],
    master_key: &[u8; 32],
    salt: &[u8; 16],
    base_nonce: &[u8; 8],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(master_key);
    hasher.update(salt);
    hasher.update(base_nonce);
    let mut digest = hasher.finalize();

    let mut subkey = [0u8; 32];
    subkey.copy_from_slice(&digest);
    digest.zeroize();
    subkey
}

/// Derives domain-separated `(stream_key, toc_key)` from a single memory-hard Argon2id pass.
pub fn derive_bay_keys(
    password: &str,
    salt: &[u8; 16],
    base_nonce: &[u8; 8],
) -> std::io::Result<([u8; 32], [u8; 32])> {
    let mut master_key = derive_bay_master_key(password, salt)?;
    let stream_key = derive_subkey(STREAM_KEY_DOMAIN, &master_key, salt, base_nonce);
    let toc_key = derive_subkey(TOC_KEY_DOMAIN, &master_key, salt, base_nonce);
    master_key.zeroize();
    Ok((stream_key, toc_key))
}

/// Derives only the archive-bound TOC key (`BAYETOC\x04`) from the header salt and base nonce.
pub fn derive_bay_toc_key(
    password: &str,
    salt: &[u8; 16],
    base_nonce: &[u8; 8],
) -> std::io::Result<[u8; 32]> {
    let (mut stream_key, toc_key) = derive_bay_keys(password, salt, base_nonce)?;
    stream_key.zeroize();
    Ok(toc_key)
}

pub fn build_chunk_aad(
    salt: &[u8; 16],
    base_nonce: &[u8; 8],
    chunk_index: u32,
    flags: u8,
) -> [u8; 37] {
    let mut aad = [0u8; 37];
    aad[..8].copy_from_slice(BAY_ENC_V4_MAGIC);
    aad[8..24].copy_from_slice(salt);
    aad[24..32].copy_from_slice(base_nonce);
    aad[32..36].copy_from_slice(&chunk_index.to_be_bytes());
    aad[36] = flags;
    aad
}

pub fn build_toc_aad(
    salt: &[u8; 16],
    base_nonce: &[u8; 8],
    toc_nonce: &[u8; 12],
) -> [u8; 44] {
    let mut aad = [0u8; 44];
    aad[..8].copy_from_slice(BAY_ETOC_V4_MAGIC);
    aad[8..24].copy_from_slice(salt);
    aad[24..32].copy_from_slice(base_nonce);
    aad[32..44].copy_from_slice(toc_nonce);
    aad
}

/// Holds the header-bound TOC key derived alongside `ChunkedAesWriter` so `BAYETOC\x04`
/// reuses the single Argon2id derivation and binds cryptographically to the stream header.
pub struct BayTocContext {
    salt: [u8; 16],
    base_nonce: [u8; 8],
    toc_key: [u8; 32],
}

impl BayTocContext {
    pub fn encrypt_toc(&self, toc_json: &[u8]) -> std::io::Result<(Vec<u8>, [u8; 12])> {
        let mut toc_nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut toc_nonce);

        let key = Key::<Aes256Gcm>::from_slice(&self.toc_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&toc_nonce);
        let aad = build_toc_aad(&self.salt, &self.base_nonce, &toc_nonce);

        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: toc_json,
                    aad: &aad,
                },
            )
            .map_err(|e| std::io::Error::other(format!("TOC encryption error: {}", e)))?;

        Ok((ciphertext, toc_nonce))
    }
}

impl Drop for BayTocContext {
    fn drop(&mut self) {
        self.salt.zeroize();
        self.base_nonce.zeroize();
        self.toc_key.zeroize();
    }
}

pub fn decrypt_bay_toc(
    password: &str,
    header_salt: &[u8; 16],
    header_base_nonce: &[u8; 8],
    toc_nonce: &[u8; 12],
    ciphertext: &[u8],
) -> std::io::Result<Vec<u8>> {
    let mut toc_key = derive_bay_toc_key(password, header_salt, header_base_nonce)?;
    let key = Key::<Aes256Gcm>::from_slice(&toc_key);
    let cipher = Aes256Gcm::new(key);
    toc_key.zeroize();

    let nonce = Nonce::from_slice(toc_nonce);
    let aad = build_toc_aad(header_salt, header_base_nonce, toc_nonce);

    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Incorrect password or corrupted BAYETOC payload",
            )
        })
}

/// Streaming Chunked AEAD Writer (`BAYENC\x04\x00`):
/// Buffers up to 4 MB per chunk, binds chunk index and finality flag via AES-256-GCM AAD,
/// and permanently poisons on any I/O or crypto failure to eliminate unwind panics or nonce reuse.
pub struct ChunkedAesWriter<W: Write> {
    inner: Option<W>,
    cipher: Aes256Gcm,
    salt: [u8; 16],
    base_nonce: [u8; 8],
    toc_key: [u8; 32],
    chunk_index: u32,
    buffer: Vec<u8>,
    failed: bool,
}

impl<W: Write> ChunkedAesWriter<W> {
    pub fn new(mut inner: W, password: &str) -> std::io::Result<Self> {
        let mut salt = [0u8; 16];
        let mut base_nonce = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut base_nonce);

        // Derive keys first so header isn't emitted if derivation fails.
        let (mut stream_key, toc_key) = derive_bay_keys(password, &salt, &base_nonce)?;
        let key = Key::<Aes256Gcm>::from_slice(&stream_key);
        let cipher = Aes256Gcm::new(key);
        stream_key.zeroize();

        // Header: 8 bytes magic + 16 bytes salt + 8 bytes base_nonce = 32 bytes
        inner.write_all(BAY_ENC_V4_MAGIC)?;
        inner.write_all(&salt)?;
        inner.write_all(&base_nonce)?;

        Ok(Self {
            inner: Some(inner),
            cipher,
            salt,
            base_nonce,
            toc_key,
            chunk_index: 0,
            buffer: Vec::with_capacity(BAY_CHUNK_SIZE + 16),
            failed: false,
        })
    }

    fn poison(&mut self) {
        self.failed = true;
        self.buffer.zeroize();
        self.buffer.clear();
    }

    fn flush_chunk(&mut self, is_final: bool) -> std::io::Result<()> {
        if self.failed {
            return Err(std::io::Error::other(
                "Encrypted writer is poisoned after a prior error",
            ));
        }
        if !is_final && self.buffer.is_empty() {
            return Ok(());
        }

        let idx = self.chunk_index;
        let Some(next_idx) = idx.checked_add(1) else {
            self.poison();
            return Err(std::io::Error::other("Archive chunk count exceeded limit"));
        };
        // Advance chunk_index before encryption/output so a failed write can never reuse a nonce.
        self.chunk_index = next_idx;

        let flags = if is_final {
            BAY_CHUNK_FLAG_FINAL
        } else {
            BAY_CHUNK_FLAG_MORE
        };

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..8].copy_from_slice(&self.base_nonce);
        nonce_bytes[8..].copy_from_slice(&idx.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = build_chunk_aad(&self.salt, &self.base_nonce, idx, flags);

        if let Err(e) = self.cipher.encrypt_in_place(nonce, &aad, &mut self.buffer) {
            self.poison();
            return Err(std::io::Error::other(format!(
                "AES-256-GCM encryption error: {}",
                e
            )));
        }

        let Ok(chunk_len) = u32::try_from(self.buffer.len()) else {
            self.poison();
            return Err(std::io::Error::other("Encrypted chunk size exceeded u32"));
        };

        let write_res = match self.inner.as_mut() {
            Some(inner) => inner
                .write_all(&chunk_len.to_le_bytes())
                .and_then(|_| inner.write_all(&[flags]))
                .and_then(|_| inner.write_all(&self.buffer)),
            None => Err(std::io::Error::other("Encrypted writer inner already finished")),
        };

        if let Err(e) = write_res {
            self.poison();
            return Err(e);
        }

        self.buffer.zeroize();
        self.buffer.clear();
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<(W, BayTocContext)> {
        if self.failed {
            return Err(std::io::Error::other(
                "Encrypted writer is poisoned after a prior error",
            ));
        }
        self.flush_chunk(true)?;
        let Some(mut inner) = self.inner.take() else {
            self.poison();
            return Err(std::io::Error::other("Encrypted writer inner missing"));
        };
        if let Err(e) = inner.flush() {
            self.poison();
            return Err(e);
        }

        let toc_ctx = BayTocContext {
            salt: self.salt,
            base_nonce: self.base_nonce,
            toc_key: self.toc_key,
        };
        self.buffer.zeroize();
        self.salt.zeroize();
        self.base_nonce.zeroize();
        self.toc_key.zeroize();
        Ok((inner, toc_ctx))
    }
}

impl<W: Write> Drop for ChunkedAesWriter<W> {
    fn drop(&mut self) {
        self.buffer.zeroize();
        self.salt.zeroize();
        self.base_nonce.zeroize();
        self.toc_key.zeroize();
    }
}

impl<W: Write> Write for ChunkedAesWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.failed {
            return Err(std::io::Error::other(
                "Encrypted writer is poisoned after a prior error",
            ));
        }
        let mut written = 0;
        while written < buf.len() {
            let available = BAY_CHUNK_SIZE.saturating_sub(self.buffer.len());
            if available == 0 {
                self.flush_chunk(false)?;
                continue;
            }
            let to_copy = available.min(buf.len() - written);
            self.buffer.extend_from_slice(&buf[written..written + to_copy]);
            written += to_copy;

            if self.buffer.len() >= BAY_CHUNK_SIZE {
                self.flush_chunk(false)?;
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.failed {
            return Err(std::io::Error::other(
                "Encrypted writer is poisoned after a prior error",
            ));
        }
        self.flush_chunk(false)?;
        let flush_res = match self.inner.as_mut() {
            Some(inner) => inner.flush(),
            None => Ok(()),
        };
        if let Err(e) = flush_res {
            self.poison();
            return Err(e);
        }
        Ok(())
    }
}

/// Streaming Chunked AEAD Reader (`BAYENC\x04\x00`):
/// Verifies authenticated chunk sequence indices and the authenticated `BAY_CHUNK_FLAG_FINAL` bit,
/// keeping memory bounded to ~4 MB regardless of archive size.
pub struct ChunkedAesReader<R: Read> {
    inner: R,
    cipher: Aes256Gcm,
    salt: [u8; 16],
    base_nonce: [u8; 8],
    chunk_index: u32,
    buffer: Vec<u8>,
    plain_offset: usize,
    eof_reached: bool,
    failed: bool,
}

impl<R: Read> ChunkedAesReader<R> {
    pub fn new(mut inner: R, password: &str) -> std::io::Result<Self> {
        let mut salt = [0u8; 16];
        inner.read_exact(&mut salt).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to read salt: {}", e),
            )
        })?;

        let mut base_nonce = [0u8; 8];
        inner.read_exact(&mut base_nonce).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to read base nonce: {}", e),
            )
        })?;

        let (mut stream_key, mut toc_key) = derive_bay_keys(password, &salt, &base_nonce)?;
        let key = Key::<Aes256Gcm>::from_slice(&stream_key);
        let cipher = Aes256Gcm::new(key);
        stream_key.zeroize();
        toc_key.zeroize();

        Ok(Self {
            inner,
            cipher,
            salt,
            base_nonce,
            chunk_index: 0,
            buffer: Vec::with_capacity(BAY_CHUNK_SIZE + 16),
            plain_offset: 0,
            eof_reached: false,
            failed: false,
        })
    }

    fn load_next_chunk(&mut self) -> std::io::Result<bool> {
        if self.failed {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Encrypted reader failed verification on a prior chunk",
            ));
        }
        if self.eof_reached {
            return Ok(false);
        }

        let mut hdr = [0u8; 5];
        match self.inner.read_exact(&mut hdr) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.failed = true;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Cryptographic Error: Encrypted archive stream truncated before authenticated final chunk",
                ));
            }
            Err(e) => {
                self.failed = true;
                return Err(e);
            }
        }

        let chunk_len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
        let flags = hdr[4];

        if flags != BAY_CHUNK_FLAG_MORE && flags != BAY_CHUNK_FLAG_FINAL {
            self.failed = true;
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Corrupted archive: Invalid encrypted chunk flags",
            ));
        }

        if !(16..=BAY_CHUNK_SIZE + 16).contains(&chunk_len) {
            self.failed = true;
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Corrupted archive: Chunk length exceeds bounds",
            ));
        }

        self.buffer.resize(chunk_len, 0u8);
        if let Err(e) = self.inner.read_exact(&mut self.buffer) {
            self.failed = true;
            self.buffer.zeroize();
            self.buffer.clear();
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("Incomplete chunk ciphertext: {}", e),
            ));
        }

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..8].copy_from_slice(&self.base_nonce);
        nonce_bytes[8..].copy_from_slice(&self.chunk_index.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = build_chunk_aad(&self.salt, &self.base_nonce, self.chunk_index, flags);

        if self.cipher.decrypt_in_place(nonce, &aad, &mut self.buffer).is_err() {
            self.failed = true;
            self.buffer.zeroize();
            self.buffer.clear();
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Incorrect password or corrupted archive",
            ));
        }

        self.plain_offset = 0;
        self.chunk_index = self.chunk_index.checked_add(1).ok_or_else(|| {
            self.failed = true;
            std::io::Error::other("Archive chunk count exceeded limit")
        })?;

        if flags == BAY_CHUNK_FLAG_FINAL {
            self.eof_reached = true;
            if self.buffer.is_empty() {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

impl<R: Read> Drop for ChunkedAesReader<R> {
    fn drop(&mut self) {
        self.buffer.zeroize();
        self.salt.zeroize();
        self.base_nonce.zeroize();
    }
}

impl<R: Read> Read for ChunkedAesReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        while self.plain_offset >= self.buffer.len() {
            if !self.load_next_chunk()? {
                return Ok(0);
            }
        }

        let available = self.buffer.len() - self.plain_offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy]
            .copy_from_slice(&self.buffer[self.plain_offset..self.plain_offset + to_copy]);
        self.plain_offset += to_copy;
        Ok(to_copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunked_aes_roundtrip() {
        let password = "test_password_123";
        let original_data = b"Hello, Bayzip secure encrypted world! Testing chunked AES streaming.";

        let mut encrypted = Vec::new();
        {
            let mut writer = ChunkedAesWriter::new(&mut encrypted, password).expect("Writer init");
            writer.write_all(original_data).expect("Write data");
            let (_, _toc_ctx) = writer.finish().expect("Finish writer");
        }
        assert_eq!(&encrypted[..8], BAY_ENC_V4_MAGIC);

        let mut decrypted = Vec::new();
        {
            let mut reader = ChunkedAesReader::new(&encrypted[8..], password).expect("Reader init");
            reader.read_to_end(&mut decrypted).expect("Read data");
        }

        assert_eq!(&decrypted, original_data);
    }

    #[test]
    fn test_chunked_aes_truncation_detection() {
        let password = "test_password_123";
        let original_data = b"Sensitive payload that must not be truncated without detection.";

        let mut encrypted = Vec::new();
        {
            let mut writer = ChunkedAesWriter::new(&mut encrypted, password).expect("Writer init");
            writer.write_all(original_data).expect("Write data");
            writer.finish().expect("Finish writer");
        }

        let truncated = &encrypted[8..encrypted.len() - 4];
        let mut reader = ChunkedAesReader::new(truncated, password).expect("Reader init");
        let mut decrypted = Vec::new();
        let res = reader.read_to_end(&mut decrypted);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn test_chunked_aes_wrong_password() {
        let password = "correct_password";
        let original_data = b"Secret data";

        let mut encrypted = Vec::new();
        {
            let mut writer = ChunkedAesWriter::new(&mut encrypted, password).expect("Writer init");
            writer.write_all(original_data).expect("Write data");
            writer.finish().expect("Finish writer");
        }

        let mut reader =
            ChunkedAesReader::new(&encrypted[8..], "wrong_password").expect("Reader init");
        let mut decrypted = Vec::new();
        let res = reader.read_to_end(&mut decrypted);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn test_chunked_aes_drop_and_finish() {
        let password = "drop_test_password";
        let secret_payload = b"Super secret un-flushed plaintext chunk data";

        let mut early_buf = Vec::new();
        {
            let mut writer = ChunkedAesWriter::new(&mut early_buf, password).expect("Writer init");
            writer
                .write_all(secret_payload)
                .expect("Write buffered plaintext");
        }

        let mut completed_buf = Vec::new();
        {
            let mut writer =
                ChunkedAesWriter::new(&mut completed_buf, password).expect("Writer init");
            writer.write_all(secret_payload).expect("Write data");
            let (returned, _toc_ctx) = writer.finish().expect("Finish writer");
            assert!(returned.len() > BAY_ENC_HEADER_LEN);
        }

        let mut reader = ChunkedAesReader::new(&completed_buf[8..], password).expect("Reader init");
        let mut partial = [0u8; 10];
        reader.read_exact(&mut partial).expect("Partial read");
        assert_eq!(&partial, &secret_payload[..10]);
        drop(reader);
    }

    #[test]
    fn test_chunked_aes_v4_rejects_midstream_truncation_and_flag_tampering() {
        let password = "stream_auth_password";
        let mut encrypted = Vec::new();
        {
            let mut writer = ChunkedAesWriter::new(&mut encrypted, password).expect("Writer init");
            writer.write_all(b"chunk-zero-payload").expect("Write chunk 0");
            writer.flush().expect("Flush chunk 0");
            writer.write_all(b"chunk-one-payload").expect("Write chunk 1");
            writer.finish().expect("Finish writer");
        }

        // Chunk 0 layout after 32-byte header: [len: 4B][flags: 1B][ciphertext: 18 + 16 = 34B]
        let chunk0_len = u32::from_le_bytes(encrypted[32..36].try_into().unwrap()) as usize;
        assert_eq!(chunk0_len, b"chunk-zero-payload".len() + 16);
        assert_eq!(encrypted[36], BAY_CHUNK_FLAG_MORE);
        let chunk0_end = 32 + 5 + chunk0_len;

        // 1. Mid-stream truncation after valid chunk 0 must fail UnexpectedEof
        let truncated_after_chunk0 = &encrypted[8..chunk0_end];
        let mut reader = ChunkedAesReader::new(truncated_after_chunk0, password).unwrap();
        let mut out = Vec::new();
        let err = reader.read_to_end(&mut out).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);

        // 2. Appending legacy forged 0u32 EOF marker after chunk 0 must still fail
        let mut forged_eof = encrypted[8..chunk0_end].to_vec();
        forged_eof.extend_from_slice(&0u32.to_le_bytes());
        let mut reader2 = ChunkedAesReader::new(forged_eof.as_slice(), password).unwrap();
        let mut out2 = Vec::new();
        assert!(reader2.read_to_end(&mut out2).is_err());

        // 3. Flipping chunk 0 flags byte from MORE to FINAL breaks AEAD AAD authentication
        let mut tampered_flag = encrypted[8..chunk0_end].to_vec();
        tampered_flag[32 - 8 + 4] = BAY_CHUNK_FLAG_FINAL;
        let mut reader3 = ChunkedAesReader::new(tampered_flag.as_slice(), password).unwrap();
        let mut out3 = Vec::new();
        let err3 = reader3.read_to_end(&mut out3).unwrap_err();
        assert_eq!(err3.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn test_chunked_aes_writer_poisoned_after_io_error_no_panic_or_nonce_reuse() {
        struct FailAfterBytes {
            remaining: usize,
        }

        impl Write for FailAfterBytes {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.remaining == 0 {
                    return Err(std::io::Error::other("simulated disk full"));
                }
                let n = self.remaining.min(buf.len());
                self.remaining -= n;
                if n < buf.len() {
                    return Err(std::io::Error::other("simulated disk full"));
                }
                Ok(n)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        // Allow only the 32-byte header, then fail on the first chunk write.
        let sink = FailAfterBytes { remaining: 32 };
        let mut writer = ChunkedAesWriter::new(sink, "poison_password").expect("Header write ok");
        writer.write_all(b"first-write").expect("Buffered");
        let first_err = writer.flush();
        assert!(first_err.is_err());

        // Subsequent writes, flushes, and finish must return poisoned errors without panicking.
        let second_write = writer.write_all(b"second-write-after-error");
        assert!(second_write.is_err());
        assert!(second_write.unwrap_err().to_string().contains("poisoned"));
        assert!(writer.flush().is_err());
        assert!(writer.finish().is_err());
    }

    #[test]
    fn test_bay_toc_v4_binding_rejects_cross_archive_grafting() {
        let password = "shared_password";
        let mut archive_a = Vec::new();
        let mut archive_b = Vec::new();

        let toc_ctx_b = {
            let mut w = ChunkedAesWriter::new(&mut archive_a, password).unwrap();
            w.write_all(b"archive-a").unwrap();
            let (_inner_a, _ctx_a) = w.finish().unwrap();

            let mut w2 = ChunkedAesWriter::new(&mut archive_b, password).unwrap();
            w2.write_all(b"archive-b").unwrap();
            let (_inner_b, ctx_b) = w2.finish().unwrap();
            ctx_b
        };

        let (enc_toc_b, toc_nonce_b) = toc_ctx_b.encrypt_toc(b"[{\"name\":\"b.txt\",\"size\":1,\"is_dir\":false}]").unwrap();

        let salt_a: [u8; 16] = archive_a[8..24].try_into().unwrap();
        let base_nonce_a: [u8; 8] = archive_a[24..32].try_into().unwrap();
        let salt_b: [u8; 16] = archive_b[8..24].try_into().unwrap();
        let base_nonce_b: [u8; 8] = archive_b[24..32].try_into().unwrap();

        // Decryption succeeds with archive B's own header salt & base_nonce
        let ok = decrypt_bay_toc(password, &salt_b, &base_nonce_b, &toc_nonce_b, &enc_toc_b);
        assert!(ok.is_ok());

        // Grafting archive B's encrypted TOC onto archive A's header fails authentication
        let graft_err =
            decrypt_bay_toc(password, &salt_a, &base_nonce_a, &toc_nonce_b, &enc_toc_b)
                .unwrap_err();
        assert_eq!(graft_err.kind(), std::io::ErrorKind::PermissionDenied);
    }
}
