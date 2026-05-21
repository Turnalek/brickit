mod nitro;
mod system;

use std::{
    process::{Child, Command},
    time::Duration,
};

use common::{run_ip, run_looping, run_static, run_with_ld, IP_PATH};
use nitro::init_platform;
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

fn main() {
    boot();

    dmesg("Brickit Booted".to_string());

    let cid = get_local_cid().expect("unable to get local cid");
    dmesg(format!("CID is {cid:?}"));

    println!("running enclave egress");
    run_looping("/sender", &format!("{cid} {PORT} true"));

    loop {
        println!("waiting 5s before download...");
        std::thread::sleep(std::time::Duration::from_secs(5));

        println!("running download");
        // let ping = run_cmd("/usr/bin/ping", "-4 -A 109.123.250.238")
        let ping = run_static("/downer", "")
            .expect("unable to run ping")
            .wait_with_output()
            .expect("unable to collect ping output");
        println!(
            "{}",
            std::str::from_utf8(&ping.stdout).expect("invalid utf-8")
        );
    }
}
