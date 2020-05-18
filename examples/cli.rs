use bytes::Bytes;
use ckb_boxer::Boxer;
use ckb_logger::{error, info};
use ckb_shared::Snapshot;
use ckb_types::{
    packed::{Block, BlockReader},
    prelude::*,
};
use ckb_verification::{HeaderResolverWrapper, HeaderVerifier, Verifier};
use clap::{App, Arg};
use crossbeam_channel::select;
use faster_hex::hex_decode;
use std::io;
use std::thread;

fn main() {
    let matches = App::new("ckb-boxer")
        .arg(
            Arg::with_name("data-dir")
                .global(true)
                .short("d")
                .takes_value(true)
                .default_value("data")
                .help("Data dir for CKB"),
        )
        .get_matches();

    let mut boxer = Boxer::create(matches.value_of("data-dir").unwrap()).expect("boxer creation!");
    println!(
        "0000TIPH{:016x}",
        boxer.shared().snapshot().tip_header().number()
    );

    let new_block_receiver = boxer
        .shared()
        .notify_controller()
        .subscribe_new_block("BoxerBlockListener");
    thread::Builder::new()
        .name("BoxerBlockListener".to_string())
        .spawn(move || loop {
            select! {
                recv(new_block_receiver) -> msg => match msg {
                    Ok(block) => {
                        println!("0000NBLK{:x}", block.hash());
                    },
                    Err(e) => {
                        error!("Error listening for new blocks: {:?}", e);
                        break;
                    },
                }
            }
        })
        .expect("Start BoxerBlockListener thread failed");

    info!("ckb-boxer is now booted");

    loop {
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let input = input.trim();
                if input.len() < 8 {
                    error!("Invalid command: {}", input);
                    continue;
                }
                let id = &input[0..4];
                let method = &input[4..8];
                let payload = &input[8..];
                match method {
                    "NBLK" => {
                        if let Err(msg) = handle_new_block(&mut boxer, payload.as_bytes()) {
                            error!("Error processing new block: {}", msg);
                        }
                    }
                    _ => {
                        error!("Invalid method: {} for message: {}", method, id);
                    }
                }
            }
            Err(err) => {
                error!("Error reading input: {:?}, exiting...", err);
                break;
            }
        }
    }
}

fn handle_new_block(boxer: &mut Boxer, payload: &[u8]) -> Result<(), String> {
    let mut raw = Vec::new();
    raw.resize(payload.len() / 2, 0);
    hex_decode(payload, &mut raw).map_err(|e| format!("Hex decode error: {:?}", e))?;
    let raw = Bytes::from(raw);
    BlockReader::verify(&raw, false)
        .map_err(|e| format!("Molecule verification error: {:?}", e))?;
    let block = Block::new_unchecked(raw).into_view();
    let header = block.header();

    // Verify header
    let shared = boxer.shared();
    let snapshot: &Snapshot = &shared.snapshot();
    let resolver = HeaderResolverWrapper::new(&header, snapshot);
    HeaderVerifier::new(snapshot, &shared.consensus())
        .verify(&resolver)
        .map_err(|e| format!("Header verification error: {:?}", e))?;

    // Verify and insert block
    boxer
        .process_block(&block)
        .map_err(|e| format!("Process block error: {:?}", e))?;

    Ok(())
}
