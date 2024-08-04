use cliclack::log;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_wallet_core::prelude::{
    AccountDescriptor, AccountId, ConnectRequest, Wallet as CoreWallet, WalletDescriptor,
};
use kaspa_wallet_core::rpc::RpcApi;
use pad::{Alignment, PadStr};
// use sparkle_core::inscription::{
//     demo_keypair, deploy_token_demo, mint_token_demo, TransactionDetails,
// };
use sparkle_rs::imports::*;
use sparkle_rs::monitor::monitor;
use sparkle_rs::result::Result;
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;
mod account;
use account::Account;

type AccountHashMap = HashMap<AccountId, Arc<AccountDescriptor>>;

pub struct Context {
    pub network_id: NetworkId,
    pub node_url: Option<String>,
    pub wallet_file: Option<String>,
}

pub struct Wallet {
    pub wallet: Arc<CoreWallet>,
    pub account: Option<Arc<AccountDescriptor>>,
}

impl Wallet {
    pub async fn try_new(context: Context, connect: bool) -> Result<Self> {
        let Context {
            network_id,
            node_url,
            wallet_file,
        } = context;

        let wallet = CoreWallet::default()
            .with_resolver(Default::default())
            .with_url(node_url.as_deref())
            .with_network_id(network_id)
            .to_arc();

        // check if user-supplied wallet exists
        if let Some(wallet_file) = wallet_file.as_ref() {
            if !wallet.exists(Some(wallet_file.as_str())).await? {
                return Err(Error::custom(format!(
                    "Wallet not found: `{}`",
                    wallet_file
                )));
            }
        }

        wallet.start().await?;

        if connect {
            let request = ConnectRequest::default()
                .with_network_id(&network_id)
                .with_url(node_url)
                .with_retry_on_error(false)
                .with_require_sync(false);

            wallet.as_api().connect_call(request).await?;

            log::success(format!(
                "Connected to `{network_id}` at `{}`",
                wallet.wrpc_client().url().unwrap_or_default()
            ))?;
        }

        let wallet_file = if let Some(wallet_file) = wallet_file {
            wallet_file
        } else {
            let wallet_descriptors = wallet.as_api().wallet_enumerate().await?;

            if wallet_descriptors.is_empty() {
                return Err(Error::custom(
                    "No wallets found, please use `kaspa-wallet` to create accounts",
                ));
            } else if wallet_descriptors.len() == 1 {
                wallet_descriptors.first().unwrap().filename.clone()
            } else {
                let mut selector = cliclack::select("Please select a wallet:");
                for WalletDescriptor { filename, title } in wallet_descriptors {
                    selector = selector.item(filename.clone(), title.as_deref().unwrap_or(""), "");
                }
                selector.interact().map_err(|_| Error::UserAbort)?
            }
        };

        let password = cliclack::password("Enter wallet password:")
            .mask('▪')
            .interact()
            .map_err(|_| Error::UserAbort)?;

        let spinner = cliclack::spinner();
        spinner.start("Loading wallet...");

        // let accounts =
        wallet
            .as_api()
            .wallet_open(password.into(), Some(wallet_file), true, true)
            .await?
            .unwrap();

        wallet.as_api().accounts_activate(None).await?;

        let accounts = wallet
            .as_api()
            .accounts_enumerate()
            .await?
            .into_iter()
            .map(|descriptor| Account::new(descriptor, &network_id))
            .collect::<Vec<_>>();

        if accounts.is_empty() {
            return Err(Error::custom(
                "Wallet has no accounts, please use `kaspa-wallet` to create an account",
            ));
        }

        let name_len = accounts.iter().fold(0, |acc, account| {
            // let AccountDescriptor { account_name, .. } = account.as_ref();
            account
                .descriptor
                .account_name
                .as_ref()
                .map(|name| name.len())
                .unwrap_or(0)
                .max(acc)
        });

        let balance_len = accounts.iter().fold((0, 0, 0), |acc, account| {
            let (a, b, c) = account
                .balance
                .as_ref()
                .map(|v| v.len())
                .unwrap_or((0, 0, 0));
            (a.max(acc.0), b.max(acc.1), c.max(acc.2))
        });

        // let balances = accounts.iter().map(|account|account)
        let account_map = AccountHashMap::from_iter(
            accounts
                .iter()
                .map(|account| (account.descriptor.account_id, account.descriptor.clone())),
        );

        spinner.stop("Loading wallet...");

        let account_id = if accounts.len() == 1 {
            accounts.first().unwrap().descriptor.account_id
        } else {
            let mut selector = cliclack::select("Please select an account:");
            for account in accounts {
                let Account {
                    descriptor,
                    short_id,
                    balance,
                    ..
                } = account;

                let descr = [
                    short_id.pad_to_width_with_alignment(9, Alignment::Left),
                    descriptor
                        .account_name
                        .clone()
                        .unwrap_or_else(|| "".to_string())
                        .pad_to_width(name_len),
                    balance
                        .map(|balance| balance.format(balance_len))
                        .unwrap_or(" - ".to_string()),
                ]
                .join("");

                selector = selector.item(descriptor.account_id, descr, "");
            }
            selector.interact().map_err(|_| Error::UserAbort)?
        };

        let account = account_map.get(&account_id).cloned(); //.unwrap().cloned();

        Ok(Self { wallet, account })
    }

    // todo: commented out for migration to WalletAPI.
    // #[cfg(not(target_arch = "wasm32"))]
    // pub async fn demo_deploy(&self) {
    //     let (secret_key, public_key) = demo_keypair();
    //     let (redeem_lock, script_sig) = deploy_token_demo(&public_key);
    //     let p2sh = redeem_lock.clone();
    //     self.commit_reveal_chain(p2sh, redeem_lock, script_sig, secret_key, FEE_DEPLOY)
    //         .await;
    // }

    // #[cfg(not(target_arch = "wasm32"))]
    // pub async fn demo_mint(&self) {
    //     let (secret_key, public_key) = demo_keypair();
    //     let (redeem_lock, script_sig) = mint_token_demo(&public_key);
    //     let p2sh: Address = redeem_lock.clone();
    //     self.commit_reveal_chain(p2sh, redeem_lock, script_sig, secret_key, FEE_MINT)
    //         .await;
    // }

    // #[cfg(not(target_arch = "wasm32"))]
    // async fn commit_reveal_chain(
    //     &self,
    //     p2sh: Address,
    //     redeem_lock: Address,
    //     script_sig: Vec<u8>,
    //     secret_key: secp256k1::SecretKey,
    //     reveal_fee: u64,
    // ) {
    //     let payback_amount = 10 * SOMPI_PER_KASPA;
    //     let commit_fee = (SOMPI_PER_KASPA as f64 * 0.20) as u64;
    //     let commit_total_amount = reveal_fee + payback_amount;
    //     let account = self.account.clone().unwrap();
    //     let recipient = account.receive_address.clone().unwrap();
    //     println!("Destination address {}", recipient.clone());

    //     let monitor_handle: JoinHandle<_> =
    //         await_utxo_inclusion(p2sh, commit_total_amount, self.wallet.rpc_api());

    //     let send_request = AccountsSendRequest {
    //         account_id: account.account_id,
    //         wallet_secret: "111".into(),
    //         payment_secret: None,
    //         destination: PaymentOutputs::from((redeem_lock, commit_total_amount)).into(),
    //         priority_fee_sompi: commit_fee.into(),
    //         payload: None,
    //     };
    //     let txsent = self
    //         .wallet
    //         .as_api()
    //         .accounts_send(send_request)
    //         .await
    //         .unwrap();

    //     let commit_txid = txsent.final_transaction_id().unwrap();

    //     // Wait for commit UTXO
    //     match monitor_handle.await.unwrap() {
    //         Ok(tid) => {
    //             println!("Monitor task  01 completed successfully");

    //             assert!(commit_txid == tid);

    //             // Assume latest transaction ID is commit transaction.
    //             let txs = self
    //                 .wallet
    //                 .as_api()
    //                 .transactions_data_get_range(
    //                     account.account_id,
    //                     self.wallet.network_id().unwrap(),
    //                     0..10,
    //                 )
    //                 .await
    //                 .unwrap();

    //             let prev_tx: &Arc<kaspa_wallet_core::prelude::TransactionRecord> = txs
    //                 .transactions
    //                 .iter()
    //                 .find(|&tx| tx.id == tid)
    //                 .expect("Commit transaction");

    //             // prev_tx.transaction_data.id()
    //             println!("prev_tx tid {:?}", prev_tx.id);
    //             println!("prev_tx daa score {:?}", prev_tx.block_daa_score);

    //             let (transaction, _, _) = reveal_transaction(
    //                 TransactionDetails {
    //                     script_sig,
    //                     recipient: recipient.clone(),
    //                     secret_key,
    //                     prev_tx_tid: prev_tx.id,
    //                     prev_tx_score: prev_tx.block_daa_score,
    //                 },
    //                 payback_amount,
    //                 reveal_fee,
    //                 self.wallet.network_id().unwrap(),
    //             );

    //             let submitted_reveal_tid = self
    //                 .wallet
    //                 .rpc_api()
    //                 .submit_transaction(transaction.rpc_transaction(), false)
    //                 .await
    //                 .expect("Reveal transaction submit");

    //             println!("Reveal transaction submitted {:?}", submitted_reveal_tid);

    //             println!();
    //             let t = self
    //                 .wallet
    //                 .rpc_api()
    //                 .get_mempool_entry(transaction.id(), false, false)
    //                 .await;
    //             println!("Mempool fetch {:?}", t);
    //             println!();

    //             let monitor_handle: JoinHandle<_> =
    //                 await_utxo_inclusion(recipient, payback_amount, self.wallet.rpc_api());

    //             match monitor_handle.await.unwrap() {
    //                 Ok(reveal_tid) => {
    //                     println!();
    //                     println!("Monitor task 02 completed successfully");

    //                     assert!(reveal_tid == submitted_reveal_tid);

    //                     let txs = self
    //                         .wallet
    //                         .as_api()
    //                         .transactions_data_get_range(
    //                             account.account_id,
    //                             self.wallet.network_id().unwrap(),
    //                             0..10,
    //                         )
    //                         .await
    //                         .unwrap();
    //                     let reveal_tx: &Arc<kaspa_wallet_core::prelude::TransactionRecord> = txs
    //                         .transactions
    //                         .iter()
    //                         .find(|&tx| tx.id == reveal_tid)
    //                         .expect("Reveal transaction");

    //                     println!("reveal tx {:?}", reveal_tx);
    //                 }
    //                 Err(e) => eprintln!("Monitor task failed: {:?}", e),
    //             }
    //         }
    //         Err(e) => eprintln!("Monitor task failed: {:?}", e),
    //     }
    // }
}

async fn query_utxo_presence(
    rpc_api: &Arc<dyn RpcApi>,
    expected_amount: u64,
    p2sh: &Address,
) -> Result<Option<TransactionId>> {
    let result = rpc_api.get_utxos_by_addresses(vec![p2sh.clone()]).await;
    match result {
        Ok(entries) => {
            for entry in entries {
                if let Some(address) = &entry.address {
                    if address == p2sh && entry.utxo_entry.amount == expected_amount {
                        return Ok(Some(entry.outpoint.transaction_id));
                    }
                }
            }
            Ok(None)
        }
        Err(e) => Err(Error::KaspaRpc(e)),
    }
}

use kaspa_consensus_core::Hash;

#[cfg(not(target_arch = "wasm32"))]
pub fn await_utxo_inclusion(
    p2sh: Address,
    expected_amount: u64,
    rpc_api: Arc<dyn RpcApi>,
) -> JoinHandle<Result<Hash>> {
    tokio::spawn(async move {
        let (listener, receiver) = monitor(p2sh.clone()).await.unwrap();
        loop {
            match query_utxo_presence(&rpc_api, expected_amount, &p2sh).await {
                Ok(Some(tid)) => {
                    println!("Expected UTXO found, stopping monitor");
                    listener
                        .stop()
                        .await
                        .map_err(|e| Error::ListenerError(e.to_string()))?;
                    return Ok(tid);
                }
                Ok(None) => {}
                Err(e) => return Err(Error::custom(e.to_string())),
            };

            match receiver.recv().await {
                Ok(_) => {
                    println!("New related UTXO");
                }
                Err(e) => {
                    eprintln!("Error in monitor: {}", e);
                }
            }
        }
    })
}
