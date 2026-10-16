use std::io::{Read, Write};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Key
};
use rand::RngCore;
use sha2::{Sha256, Digest};

pub const BAY_CHUNK_SIZE: usize = 1024 * 1024; // 1 MB plaintext chunk
pub const BAY_ENC_V3_MAGIC: &[u8; 8] = b"BAYENC\x03\x00";

/// PBKDF2-like key derivation using 10,000 rounds of SHA-256
pub fn derive_bay_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt);
    let mut current = hasher.finalize();

    for _ in 1..10_000 {
        let mut h = Sha256::new();
        h.update(&current);
        h.update(salt);
        current = h.finalize();
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&current);
    key
}

/// Streaming Chunked AEAD Writer:
/// Buffers uncompressed/compressed output into 1 MB chunks, encrypts each chunk
/// on-the-fly with AES-256-GCM using an incremental counter nonce, and writes
/// the resulting ciphertext to the underlying writer without storing large files in RAM.
pub struct ChunkedAesWriter<W: Write> {
    inner: W,
    cipher: Aes256Gcm,
    base_nonce: [u8; 8],
    chunk_index: u32,
    buffer: Vec<u8>,
}

impl<W: Write> ChunkedAesWriter<W> {
    pub fn new(mut inner: W, password: &str) -> std::io::Result<Self> {
        let mut salt = [0u8; 16];
        let mut base_nonce = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut base_nonce);

        // Header: 8 bytes magic + 16 bytes salt + 8 bytes base_nonce = 32 bytes
        inner.write_all(BAY_ENC_V3_MAGIC)?;
        inner.write_all(&salt)?;
        inner.write_all(&base_nonce)?;

        let key_bytes = derive_bay_key(password, &salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        Ok(Self {
            inner,
            cipher,
            base_nonce,
            chunk_index: 0,
            buffer: Vec::with_capacity(BAY_CHUNK_SIZE),
        })
    }

    fn flush_chunk(&mut self) -> std::io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..8].copy_from_slice(&self.base_nonce);
        nonce_bytes[8..].copy_from_slice(&self.chunk_index.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self.cipher.encrypt(nonce, self.buffer.as_slice())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("AES-256-GCM encryption error: {}", e)))?;

        let chunk_len = ciphertext.len() as u32;
        self.inner.write_all(&chunk_len.to_le_bytes())?;
        self.inner.write_all(&ciphertext)?;

        self.chunk_index = self.chunk_index.checked_add(1)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Archive chunk count exceeded limit"))?;
        self.buffer.clear();
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<W> {
        self.flush_chunk()?;
        // EOF marker: 4 zero bytes
        self.inner.write_all(&0u32.to_le_bytes())?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for ChunkedAesWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            let available = BAY_CHUNK_SIZE - self.buffer.len();
            let to_copy = available.min(buf.len() - written);
            self.buffer.extend_from_slice(&buf[written..written + to_copy]);
            written += to_copy;

            if self.buffer.len() >= BAY_CHUNK_SIZE {
                self.flush_chunk()?;
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_chunk()?;
        self.inner.flush()
    }
}

/// Streaming Chunked AEAD Reader:
/// Reads and decrypts 1 MB AES-256-GCM chunks on-the-fly, keeping memory strictly bounded
/// under ~2 MB regardless of archive size.
pub struct ChunkedAesReader<R: Read> {
    inner: R,
    cipher: Aes256Gcm,
    base_nonce: [u8; 8],
    chunk_index: u32,
    current_plain: Vec<u8>,
    plain_offset: usize,
    eof_reached: bool,
}

impl<R: Read> ChunkedAesReader<R> {
    pub fn new(mut inner: R, password: &str) -> std::io::Result<Self> {
        let mut salt = [0u8; 16];
        inner.read_exact(&mut salt)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Failed to read salt: {}", e)))?;

        let mut base_nonce = [0u8; 8];
        inner.read_exact(&mut base_nonce)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Failed to read base nonce: {}", e)))?;

        let key_bytes = derive_bay_key(password, &salt);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        Ok(Self {
            inner,
            cipher,
            base_nonce,
            chunk_index: 0,
            current_plain: Vec::new(),
            plain_offset: 0,
            eof_reached: false,
        })
    }

    fn load_next_chunk(&mut self) -> std::io::Result<bool> {
        if self.eof_reached {
            return Ok(false);
        }

        let mut len_bytes = [0u8; 4];
        match self.inner.read_exact(&mut len_bytes) {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.eof_reached = true;
                return Ok(false);
            },
            Err(e) => return Err(e),
        }

        let chunk_len = u32::from_le_bytes(len_bytes) as usize;
        if chunk_len == 0 {
            self.eof_reached = true;
            return Ok(false);
        }

        // Sanity check: prevent allocating insane buffer sizes on malformed data
        if chunk_len > 16 * 1024 * 1024 + 16 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Corrupted archive: Chunk length exceeds limit",
            ));
        }

        let mut ciphertext = vec![0u8; chunk_len];
        self.inner.read_exact(&mut ciphertext)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("Incomplete chunk ciphertext: {}", e)))?;

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..8].copy_from_slice(&self.base_nonce);
        nonce_bytes[8..].copy_from_slice(&self.chunk_index.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = self.cipher.decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Incorrect password or corrupted archive",
            ))?;

        self.current_plain = plaintext;
        self.plain_offset = 0;
        self.chunk_index = self.chunk_index.checked_add(1)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Archive chunk count exceeded limit"))?;

        Ok(true)
    }
}

impl<R: Read> Read for ChunkedAesReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        while self.plain_offset >= self.current_plain.len() {
            if !self.load_next_chunk()? {
                return Ok(0);
            }
        }

        let available = self.current_plain.len() - self.plain_offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.current_plain[self.plain_offset..self.plain_offset + to_copy]);
        self.plain_offset += to_copy;
        Ok(to_copy)
    }
}
