use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::{Duration, SystemTime},
};

use sha2::Digest;

fn main() {
    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(109, 123, 250, 238), 0));
    let client = reqwest::blocking::ClientBuilder::new()
        .resolve_to_addrs("objdump.katona.me", &[addr])
        .build()
        .unwrap();
    let request = client
        .get("http://objdump.katona.me/jg.zip")
        .timeout(Duration::from_secs(300))
        .build()
        .expect("unable to build request");

    let start = SystemTime::now();
    let dl = client.execute(request).expect("unable to download");

    let status = dl.status();
    let bytes = dl.bytes().unwrap();
    let size = bytes.len();
    let ss = sha2::Sha256::digest(bytes);

    println!(
        "download complete\n\tstatus: {}\n\tsize: {}\n\tduration: {:?}\n\tsha256sum:{:x}",
        status,
        size,
        SystemTime::now().duration_since(start).unwrap(),
        ss,
    );
}
