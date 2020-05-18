#[macro_use]
extern crate derive_more;

use ckb_app_config::{AppConfig, ExitCode, Setup, SetupGuard};
use ckb_build_info::Version;
use ckb_chain::chain::ChainService;
use ckb_error::Error as CkbError;
use ckb_shared::shared::{Shared, SharedBuilder};
use ckb_types::core::BlockView;
use ckb_verification::{GenesisVerifier, Verifier};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, PartialEq, Clone, Eq, Display)]
pub enum Error {
    #[display(fmt = "ckb error: {}", "_0")]
    CKB(String),
    #[display(fmt = "exit: {}", "_0")]
    Exit(i32),
    #[display(fmt = "other error: {}", "_0")]
    Other(String),
}

impl From<CkbError> for Error {
    fn from(c: CkbError) -> Self {
        Error::CKB(c.to_string())
    }
}

impl From<ExitCode> for Error {
    fn from(c: ExitCode) -> Self {
        Error::Exit(c as i32)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Other(s.to_string())
    }
}

impl std::error::Error for Error {}

pub struct Boxer {
    #[allow(dead_code)]
    guard: SetupGuard,
    shared: Shared,
    chain: ChainService,
}

impl Boxer {
    pub fn create<P: AsRef<Path>>(config_dir: P) -> Result<Boxer, Error> {
        let config = AppConfig::load_for_subcommand(config_dir, "run")?;
        let setup = Setup {
            subcommand_name: "run".to_string(),
            config,
            is_sentry_enabled: false,
        };
        let guard = setup.setup_app(&Version::default())?;
        let consensus = setup.consensus()?;
        let config = setup.config.into_ckb()?;
        let (shared, table) = SharedBuilder::with_db_config(&config.db)
            .consensus(consensus)
            .tx_pool_config(config.tx_pool)
            .notify_config(config.notify)
            .store_config(config.store)
            .build()?;
        GenesisVerifier::new().verify(shared.consensus())?;
        let chain = ChainService::new(shared.clone(), table);
        Ok(Boxer {
            guard,
            shared,
            chain,
        })
    }

    pub fn shared(&self) -> Shared {
        self.shared.clone()
    }

    pub fn process_block(&mut self, block: &BlockView) -> Result<bool, Error> {
        let success = self.chain.external_process_block(Arc::new(block.clone()))?;
        Ok(success)
    }
}
