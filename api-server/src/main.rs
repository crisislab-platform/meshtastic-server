mod proto;

use prost::Message;
use proto::meshtastic::FileInfo;

fn main() {
    let file_info = FileInfo {
        file_name: "test".to_string(),
        size_bytes: 123
    };

    let mut buffer = Vec::new();
    file_info.encode(&mut buffer).expect("Failed to encode");

    println!("encoded {} bytes: {:?}", buffer.len(), buffer);

    let decoded = FileInfo::decode(&buffer[..]).expect("Failed to decode");

    println!("decoded: {:?}", decoded);
}
