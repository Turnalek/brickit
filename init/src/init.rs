mod nitro;
mod system;

use common::{io::new_socket, stream};

use nitro::init_platform;
use stream::Listener;
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

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let mock_mode = args.nth(1).is_some();

    if !mock_mode {
        boot();
    } else {
        eprintln!("skipping boot block in mock run");
    }

    dmesg("Brickit Booted".to_string());

    let cid = if mock_mode {
        None
    } else {
        Some(get_local_cid().unwrap())
    };
    dmesg(format!("CID is {cid:?}"));

    let core_socket = new_socket(cid);
    let listener = Listener::listen(&core_socket).expect("unable to create listener");

    loop {
        eprintln!("awaiting connection");
        let mut stream = listener.accept().await.expect("error accepting");

        eprintln!("connection accepted, receiving data");
        let msg = stream.recv().await.expect("error receiving");

        eprintln!("received msg len: {}", msg.len());
        stream.send(&msg).await.expect("failed to send reply");
    }
}
