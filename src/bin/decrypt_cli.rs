/// decrypt_cli — packet decryption test tool
///
/// Rust port of c_src/decrypt_cli.c.
/// Uses the pure-Rust crypt functions in yuri::network::crypt directly
/// (no C FFI required).

use yuri::network::crypt::{populate_table, generate_key2, tk_crypt_dynamic};

fn debug(packet: &[u8]) {
    println!("Decrypted Packet");

    // Line 1: hex values with commas
    for b in packet {
        print!("0x{:02X}, ", b);
    }
    println!();

    println!();
    println!();

    // Line 2: hex values < 0x10 only (space-separated)
    for b in packet {
        if *b < 16 {
            print!("{:02X} ", b);
        }
    }

    println!();
    println!();

    // Line 3: printable ASCII chars or spaces
    for b in packet {
        if *b <= 32 || *b > 126 {
            print!(" ");
        } else {
            print!("{}", *b as char);
        }
    }
    println!();
    println!();

    // Line 4: table with field index, character, decimal, hex for printable chars
    println!("\nField:     Character:        Decimal:         Hex Value:");
    for (i, b) in packet.iter().enumerate() {
        if *b >= 33 && *b < 126 {
            println!("    {}             {}                 {}                {:02X}", i, *b as char, b, b);
        } else {
            println!("    {}                               {}              {:02X}", i, b, b);
        }
    }

    println!();
    println!();
}

fn main() {
    let mut refpacket: Vec<u8> = vec![0xAA, 0x00, 0x07, 0x3B, 0xBC, 0xB1, 0x87, 0xAD, 0x13, 0x9A];
    let refname = b"invicta";

    let mut table = vec![0u8; 0x401];
    populate_table(refname, &mut table);

    let mut key = [0u8; 10];
    generate_key2(&refpacket, &table, &mut key, false);

    tk_crypt_dynamic(&mut refpacket, &key);

    debug(&refpacket);
}
