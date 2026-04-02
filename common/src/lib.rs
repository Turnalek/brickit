use nix::sys::socket::{AddressFamily, SockaddrLike, VsockAddr};

/// VSOCK flag for talking to host if we deploy multiple enclave "horizontally" on the same VM.
pub const VMADDR_FLAG_TO_HOST: u8 = 0x01;
/// Don't specify any flags for a VSOCK.
pub const VMADDR_NO_FLAGS: u8 = 0x00;

#[repr(C)]
struct SockAddrVm {
    svm_family: libc::sa_family_t,
    svm_reserved1: libc::c_ushort,
    svm_port: libc::c_uint,
    svm_cid: libc::c_uint,
    // Field added [here](https://github.com/torvalds/linux/commit/3a9c049a81f6bd7c78436d7f85f8a7b97b0821e6)
    // but not yet in a version of libc we can use.
    svm_flags: u8,
    svm_zero: [u8; 3],
}

/// Create a new raw VsockAddr.
///
/// For flags see: [Add flags field in the vsock address](<https://lkml.org/lkml/2020/12/11/249>).
#[allow(unsafe_code)]
pub fn new_vsock_raw(cid: u32, port: u32, flags: u8) -> VsockAddr {
    let vsock_addr = SockAddrVm {
        svm_family: AddressFamily::Vsock as libc::sa_family_t,
        svm_reserved1: 0,
        svm_cid: cid,
        svm_port: port,
        svm_flags: flags,
        svm_zero: [0; 3],
    };
    let vsock_addr_len = size_of::<SockAddrVm>() as libc::socklen_t;
    let addr = unsafe {
        VsockAddr::from_raw(
            &vsock_addr as *const SockAddrVm as *const libc::sockaddr,
            Some(vsock_addr_len),
        )
        .unwrap()
    };
    addr
}

/// Create a SHA256 hash digest of `buf`.
#[must_use]
pub fn sha_256(buf: &[u8]) -> [u8; 32] {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    hasher.update(buf);
    hasher.finalize().into()
}
