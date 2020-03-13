use ckb_app_config::{ExitCode, Setup};
use ckb_build_info::Version;
use ckb_chain::chain::ChainService;
use ckb_logger::info;
use ckb_shared::shared::SharedBuilder;
use ckb_verification::{GenesisVerifier, Verifier};
use clap::{App, Arg, SubCommand};
use std::sync::{Arc, Condvar, Mutex};

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
    let _chain_controller = chain_service.start(Some("ChainService"));

    info!("ckb-boxer is now booted");

    // Wait for exit
    let exit = Arc::new((Mutex::new(()), Condvar::new()));
    let e = Arc::clone(&exit);
    ctrlc::set_handler(move || {
        e.1.notify_all();
    })
    .expect("error setting Ctrl-C handler");
    let _guard = exit
        .1
        .wait(exit.0.lock().expect("locking"))
        .expect("waiting");
    info!("exiting...");
}
