use std::{
    ffi::CString,
    os::fd::{BorrowedFd, FromRawFd, OwnedFd},
};

use libc::{ifreq, open, IFF_NO_PI, IFF_TUN, O_RDWR};
use nix::{
    sys::socket::{socket, AddressFamily, SockFlag, SockType, SockaddrLike, VsockAddr},
    unistd::{read, write},
    NixPath,
};

/// VSOCK flag for talking to host if we deploy multiple enclave "horizontally" on the same VM.
pub const VMADDR_FLAG_TO_HOST: u8 = 0x01;
/// Don't specify any flags for a VSOCK.
pub const VMADDR_NO_FLAGS: u8 = 0x00;

#[derive(Debug, PartialEq, Eq, Clone)]
enum TrafficDirection {
    RawToVsock(bool),
    VsockToRaw(bool),
}

impl TrafficDirection {
    pub fn debug(&self) -> bool {
        match self {
            Self::RawToVsock(debug) => *debug,
            Self::VsockToRaw(debug) => *debug,
        }
    }
}

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

pub fn create_core_socket() -> Result<OwnedFd, nix::Error> {
    socket(
        AddressFamily::Vsock,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )
}

pub fn create_raw_socket(if_name: &str) -> Result<OwnedFd, nix::Error> {
    let if_name = CString::new(if_name).unwrap();
    let name_ptr = if_name.as_ptr();
    let name_len = if_name.len().min(nix::libc::IFNAMSIZ - 1);
    let mut ifr = ifreq {
        ifr_name: [0; nix::libc::IFNAMSIZ],
        ifr_ifru: unsafe { std::mem::zeroed() },
    };

    let tun_dev = CString::new("/dev/net/tun").unwrap();

    unsafe {
        let fd = open(tun_dev.as_ptr(), O_RDWR);
        if fd < 0 {
            panic!("unable to open /dev/net/tun");
        }
        std::ptr::copy_nonoverlapping(name_ptr, ifr.ifr_name.as_mut_ptr(), name_len);
        ifr.ifr_ifru.ifru_flags = IFF_TUN as i16 | IFF_NO_PI as i16;

        // Set flags to IFF_TUN
        // Using libc directly for the ioctl is common when nix lacks the specific macro:
        let ret = nix::libc::ioctl(fd, 0x400454ca, &ifr as *const ifreq);
        if ret < 0 {
            panic!("unable to ioctl tun");
        }

        Ok(OwnedFd::from_raw_fd(fd))
    }
}

// copies traffic in both directions between two sockets using threads
pub fn copy_bidirectional<'fd>(fd1: BorrowedFd<'fd>, fd2: BorrowedFd<'fd>, debug: bool) {
    std::thread::scope(|s| {
        let sfd = fd1.clone();
        let tfd = fd2.clone();
        s.spawn(move || {
            pipe_all(sfd, tfd, TrafficDirection::RawToVsock(debug))
                .expect("error piping from raw to vsock");
        });

        let sfd = fd1.clone();
        let tfd = fd2.clone();
        s.spawn(move || {
            pipe_all(tfd, sfd, TrafficDirection::VsockToRaw(debug))
                .expect("error piping from vsock to raw");
        });
    });
}

// sends all traffic from fd_from to fd_to
fn pipe_all(
    fd_from: BorrowedFd,
    fd_to: BorrowedFd,
    direction: TrafficDirection,
) -> Result<(), nix::Error> {
    let mut buf = [0u8; 65535];
    loop {
        let received = read(fd_from, &mut buf)?;

        if direction.debug() {
            eprint!(".");
            // debug_data(&buf[..received], &direction);
        }

        write(fd_to, &buf[..received])?;
    }
}
