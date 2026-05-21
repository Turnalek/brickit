use std::{
    ffi::CString,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
    process::{Child, Command},
    time::Duration,
};

use libc::{ifreq, open, IFF_NO_PI, IFF_TUN, O_RDWR};
use nix::{
    sys::socket::{
        accept, bind, connect, listen, socket, AddressFamily, Backlog, SockFlag, SockType,
        SockaddrLike, VsockAddr,
    },
    unistd::{read, write},
    NixPath,
};

/// VSOCK flag for talking to host if we deploy multiple enclave "horizontally" on the same VM.
pub const VMADDR_FLAG_TO_HOST: u8 = 0x01;
/// Don't specify any flags for a VSOCK.
pub const VMADDR_NO_FLAGS: u8 = 0x00;

/// opens enclave side egress bridging using given cid and port
pub fn enclave_egress(cid: u32, port: u32) {
    setup_enclave_tunnel();

    let addr = new_vsock_raw(cid, port, VMADDR_NO_FLAGS);
    let core_socket = create_core_socket().expect("unable to create core socket");

    bind(core_socket.as_raw_fd(), &addr).expect("unable to bind core socket");

    // rust stdlib uses a 128 connection backlog
    listen(
        &core_socket,
        Backlog::new(1).expect("unable to set backlog"),
    )
    .expect("unable to listen on core socket");

    println!("awaiting initial vsock connection");
    let stream_fd = accept(core_socket.as_raw_fd()).expect("unable to accept on core socket");
    let stream = unsafe { OwnedFd::from_raw_fd(stream_fd) };
    let sock_fd = create_raw_socket("enclave_egress").expect("unable to create raw socket");
    println!("enclave egress running");
    copy_bidirectional(sock_fd, stream);
}

/// opens host side egress bridging at the specified address
pub fn host_egress(cid: u32, port: u32) {
    // NOTE: it's important we don't loop just connect here as that seems to cause EPIPE errors after it does connect
    let proxy_fd = loop {
        let addr = new_vsock_raw(cid, port, VMADDR_NO_FLAGS);
        let proxy_fd = create_core_socket().expect("unable to create vsock");

        println!("connecting to egress server vsock");
        if connect(proxy_fd.as_raw_fd(), &addr).is_ok() {
            break proxy_fd;
        }
        println!("connect failed, retrying in 200ms");
        std::thread::sleep(Duration::from_millis(200));
    };
    println!("connected to egress server vsock");

    let sock_fd = create_raw_socket("host_egress").expect("unable to create raw socket");

    let debug = false;
    println!("host egress running: {debug}");
    copy_bidirectional(sock_fd, proxy_fd);
}

// sets up new tuntap tun interface `enclave_egress` with localhost routing using `10.0.0.1/32` mask
// and default gw
fn setup_enclave_tunnel() {
    run_ip("tuntap add enclave_egress mode tun", "tuntap add failed");
    run_ip("link set lo up", "unable to bring up lo");
    run_ip("address add 10.0.0.1/32 dev lo", "ip assign to lo failed");
    run_ip("link set enclave_egress up", "unable to bring up egress");
    run_ip("route add default dev enclave_egress", "unable to route");

    // let ip_link = run_with_ld(IP_PATH, "a show dev lo")
    //     .expect("unable to run ip command")
    //     .wait_with_output()
    //     .expect("ip program failed to finish");
    // eprintln!(
    //     "{}",
    //     std::str::from_utf8(&ip_link.stdout).expect("invalid utf-8")
    // );
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
fn new_vsock_raw(cid: u32, port: u32, flags: u8) -> VsockAddr {
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

/// Copies traffic in both directions between two sockets using threads, only returns on panics
/// # Panics
/// Panics if any read/write operation panics
fn copy_bidirectional(rsock: OwnedFd, vsock: OwnedFd) {
    std::thread::scope(|s| {
        let sfd = rsock.as_fd();
        let tfd = vsock.as_fd();
        std::thread::Builder::new()
            .name("raw_to_vsock".to_owned())
            .spawn_scoped(s, move || {
                pipe_all(sfd, tfd).expect("error piping from raw to vsock");
            })
            .expect("unable to run scoped thread");

        let sfd = rsock.as_fd();
        let tfd = vsock.as_fd();
        std::thread::Builder::new()
            .name("vsock_to_raw".to_owned())
            .spawn_scoped(s, move || {
                // pipe_all(tfd, sfd, TrafficDirection::VsockToRaw(debug))
                pipe_frames(tfd, sfd).expect("error piping from vsock to raw");
            })
            .expect("unable to run scoped thread");
    });
}

// sends all traffic from fd_from to fd_to byte by byte
fn pipe_all(fd_from: BorrowedFd, fd_to: BorrowedFd) -> Result<(), nix::Error> {
    // NOTE: qemu has the same bug as aws nitro
    let mut buf = [0u8; 32000];

    loop {
        let received = read(fd_from, &mut buf)?;

        let mut sent = 0;
        while sent < received {
            sent += write(fd_to, &buf[sent..received])?;
        }
    }
}

// returns Some(size) of the first ip frame present in `buf` or None if no complete frame is found
// WARNING: assumes `buf` slice starts at frame boundary!
fn next_frame(buf: &[u8]) -> Option<usize> {
    let Ok((ip, _)) = etherparse::LaxIpSlice::from_slice(buf) else {
        return None;
    };

    let size: usize = if let Some(ip4) = ip.ipv4() {
        ip4.header().total_len()
    } else if let Some(ip6) = ip.ipv6() {
        ip6.header().payload_length() + 40 // ip6 40 bytes header + payload_length
    } else {
        panic!("invalid ip version??");
    }
    .into();

    if buf.len() < size {
        None
    } else {
        Some(size)
    }
}

// sends all traffic from fd_from to fd_to byte by ip frames waiting for completion on reads
fn pipe_frames(fd_from: BorrowedFd, fd_to: BorrowedFd) -> Result<(), nix::Error> {
    // NOTE: qemu has the same bug as aws nitro
    let mut buf = [0u8; 32000];
    let mut frame_size;
    let mut received = 0;

    loop {
        loop {
            received += read(fd_from, &mut buf[received..])?;

            if let Some(size) = next_frame(&buf[..received]) {
                frame_size = size;
                break;
            }
        }

        let mut sent = 0;
        loop {
            while sent < frame_size {
                sent += write(fd_to, &buf[sent..frame_size])?;
                // eprintln!("sent: {sent}");
            }

            if let Some(size) = next_frame(&buf[sent..received]) {
                frame_size = sent + size;
            } else {
                let tail_size = received - sent;
                // copy tail to start so we can continue on reads
                if sent < received {
                    buf.rotate_left(sent);
                }
                received = tail_size;
                break;
            }
        }
    }
}

pub const IP_PATH: &str = "/usr/sbin/ip";

/// run the `ip` utility via the `run_with_ld`
pub fn run_ip(args: &str, fail_str: &str) {
    let ip_exit = run_with_ld(IP_PATH, args)
        .expect("unable to run ip command")
        .wait()
        .expect("ip program failed to finish");
    assert!(ip_exit.success(), "{}", fail_str);
}

/// run a statically linked program and return the `Child` handle
pub fn run_static(cmd_path: &str, args: &str) -> std::io::Result<Child> {
    Command::new(cmd_path)
        .env_clear()
        .args(args.split(" "))
        .spawn()
}

/// run a statically linked program in a loop
pub fn run_looping(cmd_path: &str, args: &str) {
    let cmd_path = cmd_path.to_owned();
    let args = args.to_owned();

    std::thread::spawn(move || loop {
        match run_static(&cmd_path, &args) {
            Ok(mut child) => {
                let exit = child.wait(); // try to wait, restart  in any case
                eprintln!("process {cmd_path} exit {exit:?}");
            }
            Err(err) => eprintln!("error spawning process {cmd_path}: {err}"),
        }

        eprintln!("process {cmd_path} exited, restarting in 200ms");
        std::thread::sleep(Duration::from_millis(200));
    });
}

/// run a program with `/lib/ld-musl-x86` loader and return the `Child` handle
pub fn run_with_ld(cmd_path: &str, args: &str) -> std::io::Result<Child> {
    Command::new("/lib/ld-musl-x86")
        .env_clear()
        .arg(cmd_path)
        .args(args.split(" "))
        .spawn()
}
