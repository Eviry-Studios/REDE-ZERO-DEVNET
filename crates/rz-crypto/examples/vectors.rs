//! Imprime os vetores de teste publicados em `spec/CRYPTOGRAPHY.md §8`.
use rz_crypto::{context, hash, hex, SecretKey};

fn main() {
    let sk = SecretKey::from_seed([1u8; 32]);
    let pk = sk.public_key();
    println!("seed        = {}", hex::encode(&[1u8; 32]));
    println!("public_key  = {pk}");
    println!("node_id     = {}", pk.node_id());
    println!("address     = {}", pk.address());
    println!("H(TX_ID,\"\") = {}", hash(context::TX_ID, b""));
    let sig = sk.sign(context::TX_SIGNATURE, "rede-zero-devnet-1", b"zero");
    println!("signature   = {}", sig.to_hex());
}
