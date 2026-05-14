use std::{
    ffi::CString,
    os::fd::{BorrowedFd, FromRawFd, OwnedFd},
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    time::{Duration, SystemTime},
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
pub enum TrafficDirection {
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
    // NOTE: qemu has the same bug as aws nitro
    let mut buf = [0u8; 1500];
    let debug = direction.debug();

    loop {
        let start = SystemTime::now();
        let received = read(fd_from, &mut buf)?;
        get_counters().read_add(
            SystemTime::now()
                .duration_since(start)
                .map_err(|_| nix::Error::UnknownErrno)?,
            &direction,
        );

        if debug {
            eprintln!("received {received} @ {direction:?}");
        }

        let mut sent = 0;
        while sent < received {
            let start = SystemTime::now();
            sent += write(fd_to, &buf[sent..received])?;
            get_counters().write_add(
                SystemTime::now()
                    .duration_since(start)
                    .map_err(|_| nix::Error::UnknownErrno)?,
                &direction,
            );

            if debug {
                eprintln!("sent {sent} @ {direction:?}");
            }
        }
    }
}

// copies traffic in both directions between two sockets using threads
pub fn copy_bidirectional_raw(fd1: i32, fd2: i32, debug: bool) {
    std::thread::scope(|s| {
        s.spawn(move || {
            pipe_all_raw(fd1, fd2, TrafficDirection::RawToVsock(debug))
                .expect("error piping from raw to vsock");
        });

        s.spawn(move || {
            pipe_all_raw(fd2, fd1, TrafficDirection::VsockToRaw(debug))
                .expect("error piping from vsock to raw");
        });
    });
}

// sends all traffic from fd_from to fd_to
fn pipe_all_raw(fd_from: i32, fd_to: i32, direction: TrafficDirection) -> Result<(), nix::Error> {
    // NOTE: qemu has the same bug as aws nitro
    let mut buf = [0u8; 32000];
    let debug = direction.debug();

    loop {
        let start = SystemTime::now();
        let received = unsafe {
            libc::read(
                fd_from,
                (&mut buf).as_mut_ptr().cast(),
                buf.len() as libc::size_t,
            ) as usize
        };

        get_counters().read_add(
            SystemTime::now()
                .duration_since(start)
                .map_err(|_| nix::Error::UnknownErrno)?,
            &direction,
        );

        if debug {
            eprintln!("received {received} @ {direction:?}");
        }

        let mut sent = 0;
        while sent < received {
            let start = SystemTime::now();
            sent += unsafe {
                libc::write(
                    fd_to,
                    (&buf[sent..received]).as_ptr().cast(),
                    received - sent,
                ) as usize
            };
            get_counters().write_add(
                SystemTime::now()
                    .duration_since(start)
                    .map_err(|_| nix::Error::UnknownErrno)?,
                &direction,
            );

            if debug {
                eprintln!("sent {sent} @ {direction:?}");
            }
        }
    }
}

#[derive(Debug, Default)]
struct TrafficCounters {
    reads: AtomicU64,
    writes: AtomicU64,
    vsock_to_raw_read: AtomicU64,
    vsock_to_raw_write: AtomicU64,
    raw_to_vsock_read: AtomicU64,
    raw_to_vsock_write: AtomicU64,
}

impl TrafficCounters {
    pub fn read_add(&self, value: Duration, direction: &TrafficDirection) -> u64 {
        self.reads.fetch_add(1, Ordering::SeqCst);
        match direction {
            TrafficDirection::RawToVsock(_) => self
                .raw_to_vsock_read
                .fetch_add(value.as_micros() as u64, Ordering::SeqCst),

            TrafficDirection::VsockToRaw(_) => self
                .vsock_to_raw_read
                .fetch_add(value.as_micros() as u64, Ordering::SeqCst),
        }
    }

    pub fn write_add(&self, value: Duration, direction: &TrafficDirection) -> u64 {
        self.writes.fetch_add(1, Ordering::SeqCst);
        match direction {
            TrafficDirection::RawToVsock(_) => self
                .raw_to_vsock_write
                .fetch_add(value.as_micros() as u64, Ordering::SeqCst),

            TrafficDirection::VsockToRaw(_) => self
                .vsock_to_raw_write
                .fetch_add(value.as_micros() as u64, Ordering::SeqCst),
        }
    }

    pub fn print(&self) {
        let reads = self.reads.load(Ordering::Relaxed) as f64;
        let writes = self.writes.load(Ordering::Relaxed) as f64;

        let vsock_to_raw_read = self.vsock_to_raw_read.load(Ordering::Relaxed) as f64;
        let vsock_to_raw_write = self.vsock_to_raw_write.load(Ordering::Relaxed) as f64;
        let raw_to_vsock_read = self.raw_to_vsock_read.load(Ordering::Relaxed) as f64;
        let raw_to_vsock_write = self.raw_to_vsock_write.load(Ordering::Relaxed) as f64;

        eprintln!("========COUNTERS========\nreads: {}\nvsock_to_raw_read: {}\nraw_to_vsock_read: {}\nwrites: {}\nvsock_to_raw_writes: {}\nraw_to_vsock_writes: {}\n========================",
            reads,
            vsock_to_raw_read / reads,
            raw_to_vsock_read / reads,
            writes,
            vsock_to_raw_write / writes,
            raw_to_vsock_write / writes);
    }
}

static COUNTERS: OnceLock<TrafficCounters> = OnceLock::new();

fn get_counters() -> &'static TrafficCounters {
    COUNTERS.get_or_init(|| TrafficCounters::default())
}

pub fn print_counters() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(5));
        get_counters().print();
    });
}
