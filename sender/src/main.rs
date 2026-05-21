use common::{enclave_egress, host_egress};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut args = args.into_iter();
    args.next();

    let cid: u32 = args
        .next()
        .unwrap_or("1".to_owned())
        .parse()
        .expect("unable to parse cid arg");

    let port: u32 = args
        .next()
        .unwrap_or("9001".to_owned())
        .parse()
        .expect("unable to parse port arg");

    let enclave: bool = args
        .next()
        .unwrap_or("false".to_owned())
        .parse()
        .expect("unable to parse enclave arg");

    if enclave {
        enclave_egress(cid, port);
    } else {
        host_egress(cid, port);
    }
}
