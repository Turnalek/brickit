mod nitro;
mod system;

use std::os::fd::AsRawFd;

use common::{new_vsock_raw, sha_256};

use nitro::init_platform;
use nix::sys::socket::{
    accept, bind, listen, recv, socket, AddressFamily, Backlog, MsgFlags, SockFlag, SockType,
};
use system::{dmesg, freopen, get_local_cid, mount};

// Mount common filesystems with conservative permissions
fn init_rootfs() {
    use libc::{MS_NODEV, MS_NOEXEC, MS_NOSUID};
    let no_dse = MS_NODEV | MS_NOSUID | MS_NOEXEC;
    let no_se = MS_NOSUID | MS_NOEXEC;
    let args = [
        ("devtmpfs", "/dev", "devtmpfs", no_se, "mode=0755"),
        ("devpts", "/dev/pts", "devpts", no_se, ""),
        ("shm", "/dev/shm", "tmpfs", no_dse, "mode=0755"),
        ("proc", "/proc", "proc", no_dse, "hidepid=2"),
        ("tmpfs", "/run", "tmpfs", no_dse, "mode=0755"),
        ("tmpfs", "/tmp", "tmpfs", no_dse, ""),
        ("sysfs", "/sys", "sysfs", no_dse, ""),
        (
            "cgroup_root",
            "/sys/fs/cgroup",
            "tmpfs",
            no_dse,
            "mode=0755",
        ),
    ];
    for (src, target, fstype, flags, data) in args {
        match mount(src, target, fstype, flags, data) {
            Ok(()) => dmesg(format!("Mounted {target}")),
            Err(e) => eprintln!("{e}"),
        }
    }
}

// Initialize console with stdin/stdout/stderr
fn init_console() {
    let args = [
        ("/dev/console", "r", 0),
        ("/dev/console", "w", 1),
        ("/dev/console", "w", 2),
    ];
    for (filename, mode, file) in args {
        match freopen(filename, mode, file) {
            Ok(()) => {}
            Err(e) => eprintln!("{e}"),
        }
    }
}

fn boot() {
    init_rootfs();
    init_console();
    init_platform();
}

/// VSOCK flag for talking to host if we deploy multiple enclave "horizontally" on the same VM.
pub const VMADDR_FLAG_TO_HOST: u8 = 0x01;
/// Don't specify any flags for a VSOCK.
pub const VMADDR_NO_FLAGS: u8 = 0x00;

fn main() {
    boot();

    dmesg("Brickit Booted".to_string());

    let cid = get_local_cid().expect("unable to get local cid");
    dmesg(format!("CID is {cid:?}"));

    let addr = new_vsock_raw(cid, 3, VMADDR_NO_FLAGS);
    let core_socket = socket(
        AddressFamily::Vsock,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )
    .expect("unable to create core socket");

    bind(core_socket.as_raw_fd(), &addr).expect("unable to bind core socket");

    // rust stdlib uses a 128 connection backlog
    listen(&core_socket, Backlog::new(128).unwrap_or(Backlog::MAXCONN))
        .expect("unable to listen on core socket");

    loop {
        eprintln!("awaiting connection");
        let stream_fd = accept(core_socket.as_raw_fd()).expect("unable to accept on core socket");

        eprintln!("connection accepted, receiving header");
        let mut buf = [0u8; 8];
        let bytes = recv(stream_fd, &mut buf, MsgFlags::empty())
            .expect("unable to receive on socket stream");
        assert_eq!(8, bytes);
        let length = u64::from_le_bytes(buf);

        eprintln!(
            "received header, data length is {length} bytes, receiving data portion in 64kB chunks"
        );
        let mut buf = [0u8; 65535];
        let mut msg = Vec::new();

        while msg.len() < length as usize {
            let bytes = recv(stream_fd, &mut buf, MsgFlags::empty())
                .expect("unable to receive data on core stream");
            if bytes == 0 {
                break;
            }
            msg.extend_from_slice(&buf[0..bytes]);
        }

        let shasum = sha_256(&msg);
        let hex_string: String = shasum.iter().map(|b| format!("{:02X}", b)).collect();
        eprintln!("received msg len: {} sha256sum: '{hex_string}'", msg.len());
    }
}
