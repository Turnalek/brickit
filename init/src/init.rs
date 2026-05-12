mod nitro;
mod system;

use std::{
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
    process::{Child, Command},
};

use common::{copy_bidirectional, create_core_socket, create_raw_socket, new_vsock_raw};

use nitro::init_platform;
use nix::sys::socket::{accept, bind, listen, socket, AddressFamily, Backlog, SockFlag, SockType};
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

const PORT: u32 = 9001;
const IP_PATH: &str = "/usr/sbin/ip";

fn main() {
    boot();

    dmesg("Brickit Booted".to_string());

    run_ip("tuntap add enclave_egress mode tun", "tuntap add failed");
    run_ip("link set enclave_egress up", "unable to bring up interface");
    run_ip("route add default dev enclave_egress", "unable to route");

    let ip_link = run_cmd(IP_PATH, "link show")
        .expect("unable to run ip command")
        .wait_with_output()
        .expect("ip program failed to finish");
    eprintln!(
        "{}",
        std::str::from_utf8(&ip_link.stdout).expect("invalid utf-8")
    );

    let cid = get_local_cid().expect("unable to get local cid");
    dmesg(format!("CID is {cid:?}"));

    let addr = new_vsock_raw(cid, PORT, VMADDR_NO_FLAGS);
    let core_socket = create_core_socket().expect("unable to create core socket");

    bind(core_socket.as_raw_fd(), &addr).expect("unable to bind core socket");

    // rust stdlib uses a 128 connection backlog
    listen(
        &core_socket,
        Backlog::new(1).expect("unable to set backlog"),
    )
    .expect("unable to listen on core socket");

    loop {
        println!("awaiting connection");
        let stream_fd = accept(core_socket.as_raw_fd()).expect("unable to accept on core socket");
        let stream = unsafe { OwnedFd::from_raw_fd(stream_fd) };

        println!("connection accepted, creating raw socket");

        let sock_fd = create_raw_socket("enclave_egress").expect("unable to create raw socket");

        println!("enclave egress running");
        copy_bidirectional(sock_fd.as_fd(), stream.as_fd(), false);
    }
}

fn run_ip(args: &str, fail_str: &str) {
    let ip_exit = run_cmd(IP_PATH, args)
        .expect("unable to run ip command")
        .wait()
        .expect("ip program failed to finish");
    assert!(ip_exit.success(), "{}", fail_str);
}

fn run_cmd(cmd_path: &str, args: &str) -> std::io::Result<Child> {
    // ip is dynlinked so we need to execute it with the right loader
    // TODO: figure out why the kernel doesn't look at /lib/ld-musl-x86 itself since it matches that in .interop of ip
    Command::new("/lib/ld-musl-x86")
        .env_clear()
        .arg(cmd_path)
        .args(args.split(" "))
        .spawn()
}
