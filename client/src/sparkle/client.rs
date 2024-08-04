use crate::args::{Action, Args, WalletAction};
use crate::wallet::*;
use cliclack::intro;
use console::style;
use sparkle_core::model::kasplex;
use sparkle_core::runtime::Runtime;
use sparkle_rpc_client::prelude::*;
use sparkle_rs::imports::*;
use sparkle_rs::kasplex::v1::Indexer as KasplexIndexer;
use sparkle_rs::result::Result;
use workflow_log::prelude::*;

#[derive(Default)]
pub struct Client;

impl Client {
    pub async fn run(&self, _runtime: &Runtime) -> Result<()> {
        let Args {
            action,
            wallet_file,
            node_url,
            sparkled_url,
            network_id,
            enable_debug_mode,
            trace_log_level,
        } = Args::parse();

        if trace_log_level {
            workflow_log::set_log_level(workflow_log::LevelFilter::Trace);
        }

        sparkle_core::debug::enable(enable_debug_mode);

        // ---

        let version = format!(
            "sparkle client v{}-{} (rusty-kaspa v{})",
            crate::VERSION,
            crate::GIT_DESCRIBE,
            kaspa_wallet_core::version()
        );

        let url = sparkled_url.unwrap_or_else(|| "ws://127.0.0.1:7878".to_string());

        match action {
            Action::Ping => {
                println!("{}", version);

                let client = SparkleRpcClient::try_new(url.as_str(), None)?;
                client.connect(None).await?;
                println!(
                    "Connected to {}",
                    client.url().unwrap_or_else(|| "🤷".to_string())
                );

                // ensure that sparkled network matches ours
                let _status = client.negotiate(&network_id).await?;

                println!("📡 Pinging...");
                client.ping().await?;
                println!("🥂 Ok...");
                client.disconnect().await?;
            }
            Action::Wallet { action } => {
                println!();
                crate::log::init();
                intro(style(version).on_black().cyan())?;

                let ctx = Context {
                    network_id,
                    node_url,
                    wallet_file,
                };

                match action {
                    WalletAction::Test => {
                        let wallet = Wallet::try_new(ctx, true).await?;
                        // wallet.wallet.utxo_processor();
                        log_info!("{:#?}", wallet.account);
                        // wallet.demo_deploy().await;
                        // wallet.demo_mint().await;
                    }
                    WalletAction::Test2 => {
                        let _wallet = Wallet::try_new(ctx, true).await?;
                        // wallet.wallet.utxo_processor();
                        // log_info!("{:#?}", wallet.account);
                        // wallet.demo_deploy().await;
                        // wallet.demo_mint().await;

                        // some fake address (placeholder)
                        let address = match network_id.network_type() {
                            NetworkType::Testnet => Address::try_from("kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhqrxplya").unwrap(),
                            NetworkType::Mainnet => Address::try_from("kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e").unwrap(),
                            _ => panic!("Unsupported network"),
                        };

                        let network = kasplex::v1::Network::try_from(&network_id)?;
                        let indexer = KasplexIndexer::try_new(network.into())?;
                        let mut tokens =
                            indexer.get_token_balance_list_by_address(&address).await?;
                        tokens.sort_by(|a, b| a.tick.cmp(&b.tick));
                    }
                }
            }
        }

        Ok(())
    }
}
