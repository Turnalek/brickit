use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::{Duration, SystemTime},
};

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

    println!(
        "download complete\n\tstatus: {}\n\tsize: {}\n\tduration: {:?}",
        dl.status(),
        dl.bytes().unwrap().len(),
        SystemTime::now().duration_since(start).unwrap(),
    );
}
