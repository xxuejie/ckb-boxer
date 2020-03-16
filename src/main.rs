use bytes::Bytes;
use ckb_app_config::{ExitCode, Setup};
use ckb_build_info::Version;
use ckb_chain::chain::{ChainController, ChainService};
use ckb_logger::{error, info};
use ckb_shared::{
    shared::{Shared, SharedBuilder},
    Snapshot,
};
use ckb_types::{
    packed::{Block, BlockReader},
    prelude::*,
};
use ckb_verification::{GenesisVerifier, HeaderResolverWrapper, HeaderVerifier, Verifier};
use clap::{App, Arg, SubCommand};
use crossbeam_channel::select;
use faster_hex::hex_decode;
use std::io;
use std::sync::Arc;
use std::thread;

fn main() {
    let matches = App::new("ckb-boxer")
        .arg(
            Arg::with_name("config-dir")
                .global(true)
                .short("C")
                .value_name("path")
                .takes_value(true)
                .help(
                    "Runs as if ckb was started in <path> instead of the current working directory.",
                ),
        )
        .subcommand(
            SubCommand::with_name("run").about("Runs boxer"),
        )
        .get_matches();

    let setup = Setup::from_matches(&matches).expect("unable to create setup");
    let _setup_guard = setup
        .setup_app(&Version::default())
        .expect("unable to setup app");

    let args = setup.run(&matches).expect("prepare run args");

    if args.config.sentry.is_enabled() {
        panic!("CKB boxer must run with sentry disabled!");
    }

    let (shared, table) = SharedBuilder::with_db_config(&args.config.db)
        .consensus(args.consensus)
        .tx_pool_config(args.config.tx_pool)
        .notify_config(args.config.notify)
        .store_config(args.config.store)
        .build()
        .map_err(|err| {
            eprintln!("Run error: {:?}", err);
            ExitCode::Failure
        })
        .expect("shared builder setup failure");
    GenesisVerifier::new()
        .verify(shared.consensus())
        .expect("genesis verification failure");

    let chain_service = ChainService::new(shared.clone(), table);
    let chain_controller = chain_service.start(Some("ChainService"));

    println!("0000TIPH{:016x}", shared.snapshot().tip_header().number());

    let new_block_receiver = shared
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
                        if let Err(msg) =
                            handle_new_block(&chain_controller, &shared, payload.as_bytes())
                        {
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

fn handle_new_block(
    chain: &ChainController,
    shared: &Shared,
    payload: &[u8],
) -> Result<(), String> {
    let mut raw = Vec::new();
    raw.resize(payload.len() / 2, 0);
    hex_decode(payload, &mut raw).map_err(|e| format!("Hex decode error: {:?}", e))?;
    let raw = Bytes::from(raw);
    BlockReader::verify(&raw, false)
        .map_err(|e| format!("Molecule verification error: {:?}", e))?;
    let block = Block::new_unchecked(raw).into_view();
    let header = block.header();

    // Verify header
    let snapshot: &Snapshot = &shared.snapshot();
    let resolver = HeaderResolverWrapper::new(&header, snapshot);
    HeaderVerifier::new(snapshot, &shared.consensus())
        .verify(&resolver)
        .map_err(|e| format!("Header verification error: {:?}", e))?;

    // Verify and insert block
    chain
        .process_block(Arc::new(block))
        .map_err(|e| format!("Process block error: {:?}", e))?;

    Ok(())
}
