use common::{io::new_socket, sha_256, stream::Stream};

#[tokio::main]
async fn main() {
    let file_path = std::env::args()
        .nth(1)
        .expect("payload file required as first argument");

    let data = std::fs::read(&file_path).expect("unable to read payload file");
    let shasum = sha_256(&data);

    let addr = new_socket(None);
    let mut stream = Stream::new(&addr);
    stream.connect().await.expect("unable to connect");

    let hex_string: String = shasum.iter().map(|b| format!("{:02X}", b)).collect();
    println!("sending data with sha256sum of  '{hex_string}'",);

    stream.send(&data).await.expect("failed to send payload");

    let reply = stream.recv().await.expect("failed to receive reply");
    let reply_sha = sha_256(&reply);

    let hex_string: String = reply_sha.iter().map(|b| format!("{:02X}", b)).collect();
    println!("received data with sha256sum of '{hex_string}'");
}
