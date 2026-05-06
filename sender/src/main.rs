use std::os::fd::AsRawFd;

use common::{VMADDR_NO_FLAGS, new_vsock_raw, sha_256};
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, connect, send, socket};

const PORT: u32 = 9001;
const CID: u32 = 1;

fn main() {
    let file_path = std::env::args()
        .nth(1)
        .expect("payload file required as first argument");

    let data = std::fs::read(&file_path).expect("unable to read payload file");
    let shasum = sha_256(&data);
    let hex_string: String = shasum.iter().map(|b| format!("{:02X}", b)).collect();

    let addr = new_vsock_raw(CID, PORT, VMADDR_NO_FLAGS);
    let core_socket = socket(
        AddressFamily::Vsock,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )
    .expect("unable to create core socket");
    connect(core_socket.as_raw_fd(), &addr).expect("unable to connect to vsock");

    // TODO: figure out if this is a vhost-device-vsock/qemu bug
    println!("connected, waiting 50ms due to vhost bug");
    std::thread::sleep(std::time::Duration::from_millis(50));
    for _i in 0..5 {
        send_msg(core_socket.as_raw_fd(), &data, &hex_string);
    }
}

fn send_msg(core_socket: i32, data: &[u8], hex_string: &str) {
    println!(
        "sending data with size: {} sha256sum: '{hex_string}'",
        data.len()
    );

    let header = (data.len() as u64).to_le_bytes();
    let bytes =
        send(core_socket, &header, MsgFlags::empty()).expect("unable to send payload header");
    assert_eq!(header.len(), bytes);

    let mut total = 0;

    while total < data.len() {
        let bytes =
            send(core_socket, &data, MsgFlags::empty()).expect("unable to send payload data");

        eprintln!("send {bytes} bytes of payload");
        total += bytes;
    }

    eprintln!("done sending, total sent bytes {total}");
}
