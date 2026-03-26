pub mod io;
pub mod stream;

/// Create a SHA256 hash digest of `buf`.
#[must_use]
pub fn sha_256(buf: &[u8]) -> [u8; 32] {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    hasher.update(buf);
    hasher.finalize().into()
}
