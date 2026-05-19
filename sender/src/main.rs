use std::os::fd::AsRawFd;

use common::{
    VMADDR_NO_FLAGS, copy_bidirectional, create_core_socket, create_raw_socket, new_vsock_raw,
};
use nix::sys::socket::{SockaddrLike, connect};

const PORT: u32 = 9001;

pub fn host_egress(addr: &dyn SockaddrLike) {
    let proxy_fd = create_core_socket().expect("unable to create vsock");
    connect(proxy_fd.as_raw_fd(), addr).expect("unable to connect to vsock");

    let sock_fd = create_raw_socket("host_egress").expect("unable to create raw socket");

    let debug = false;
    println!("host egress running: {debug}");
    copy_bidirectional(sock_fd, proxy_fd);
}

fn main() {
    let cid: u32 = std::env::args()
        .nth(1)
        .unwrap_or("1".to_owned())
        .parse()
        .expect("unable to parse cid arg");
    let addr = new_vsock_raw(cid, PORT, VMADDR_NO_FLAGS);

    host_egress(&addr);
}
