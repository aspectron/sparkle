use crate::constants::*;
use crate::model::kasplex::v1::krc20::{Op, TokenTransaction};
use crate::model::kasplex::v1::Protocol;

use kaspa_addresses::Address;
// use kaspa_consensus_client::UtxoEntry as ClientUTXO;
// use kaspa_consensus_core::sign::sign;
// use kaspa_consensus_core::subnets::SubnetworkId;
// use kaspa_consensus_core::tx::{
//     MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
//     UtxoEntry,
// };
use kaspa_hashes::Hash;
use kaspa_txscript::opcodes::codes::*;
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_script_hash_script};

// use kaspa_wallet_pskt::prelude::{lock_script_sig_templating, script_sig_to_address, unlock_utxos_as_pskb, Bundle, Signer, PSKT};

// use kaspa_wallet_core::utxo::UtxoEntryReference;
// use kaspa_wrpc_client::prelude::*;
use secp256k1::{rand, Secp256k1, SecretKey};
// use core::slice::SlicePattern;
use std::str::FromStr;
// use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TransactionDetails {
    pub script_sig: Vec<u8>,
    pub recipient: Address,
    pub secret_key: SecretKey,
    pub prev_tx_tid: Hash,
    pub prev_tx_score: u64,
}

pub fn demo_keypair() -> (secp256k1::SecretKey, secp256k1::PublicKey) {
    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
    (secret_key, public_key)
}

pub fn ascii_debug_payload(script_sig: &[u8]) {
    let ascii_string: String = script_sig
        .iter()
        .map(|&b| {
            if b.is_ascii() {
                b as char
            } else {
                '.' // Replace non-ASCII bytes with a placeholder
            }
        })
        .collect();
    println!();
    println!("Envelope debug: {}", ascii_string);
    println!();
}

fn redeem_pubkey(redeem_script: &[u8], pubkey: &[u8]) -> ScriptBuilderResult<Vec<u8>> {
    Ok(ScriptBuilder::new()
        .add_data(pubkey)?
        .add_op(OpCheckSig)?
        .add_op(OpFalse)?
        .add_op(OpIf)?
        .add_data(PROTOCOL_NAMESPACE.as_bytes())?
        // .add_data(&[1])?
        // Force OpPushData1 for metadata to be kasplex compliant
        // .add_data(&vec![0x12; 76])?
        .add_i64(0)?
        .add_data(redeem_script)?
        .add_op(OpEndIf)?
        .drain())
}

#[allow(dead_code)]
fn print_script_sig(script_sig: &[u8]) {
    let mut step = 0;
    let mut incrementing = true;

    for (index, value) in script_sig.iter().enumerate() {
        let overall_position = index * 2;
        let hex_string = format!("{:02x}", value);
        let decimal_value = format!("{:03}", value);
        let ascii_value = if *value >= 0x20 && *value <= 0x7e {
            *value as char
        } else {
            step = 0; // Reset step if the character is non-ASCII
            incrementing = true; // Reset incrementing
            '.'
        };
        let padding = " ".repeat(step * 2);
        println!(
            "{:03} 0x{} | {} | {}{}",
            overall_position, hex_string, decimal_value, padding, ascii_value
        );

        if *value >= 0x20 && *value <= 0x7e {
            if incrementing {
                if step < 10 {
                    step += 1;
                } else {
                    incrementing = false;
                    step -= 1;
                }
            } else if step > 0 {
                step -= 1;
            } else {
                incrementing = true;
                step += 1;
            }
        }
    }
}

pub fn deploy_token_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let transaction: TokenTransaction = TokenTransaction {
        protocol: Protocol::from_str("krc-20").unwrap(),
        op: Op::Deploy,
        tick: "TOITOI".to_string(),
        max: Some(100000000000000000),
        limit: Some(100000000000),
        pre: Some(100000000000),
        dec: Some(8),
        amount: None,
        from: None,
        to: None,
        op_score: None,
        hash_rev: None,
        fee_rev: None,
        tx_accept: None,
        op_accept: None,
        op_error: None,
        mts_add: None,
        mts_mod: None,
    };

    let json = serde_json::to_string(&transaction).unwrap();
    println!("{json}");
    let script_sig: Vec<u8> = redeem_pubkey(json.as_bytes(), &pubkey.serialize()[1..33]).unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

pub fn mint_token_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let transaction: TokenTransaction = TokenTransaction {
        protocol: Protocol::from_str("krc-20").unwrap(),
        op: Op::Mint,
        tick: "TOITOI".to_string(),
        max: None,
        limit: None,
        pre: None,
        dec: None,
        amount: None,
        from: None,
        to: None,
        op_score: None,
        hash_rev: None,
        fee_rev: None,
        tx_accept: None,
        op_accept: None,
        op_error: None,
        mts_add: None,
        mts_mod: None,
    };

    let json = serde_json::to_string(&transaction).unwrap();
    println!("{json}");
    let script_sig: Vec<u8> = redeem_pubkey(json.as_bytes(), &pubkey.serialize()[1..33]).unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

pub fn payload_to_placeholder(payload: &[u8], pubkey: &secp256k1::PublicKey) -> Vec<u8> {
    let needle = &pubkey.serialize()[1..33];

    let position = payload
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("Public key present in payload");

    let placeholder = "{{pubkey}}";

    let mut result = payload.to_owned();
    result.splice(
        position..position + needle.len(),
        placeholder.as_bytes().to_vec(),
    );
    result
}

// todo: commented out for migration to WalletAPI.
// pub fn reveal_transaction(
//     TransactionDetails {
//         script_sig,
//         recipient,
//         secret_key,
//         prev_tx_tid,
//         prev_tx_score,
//     }: TransactionDetails,
//     payback_amount: u64,
//     reveal_fee: u64,
//     network_id: NetworkId,
// ) -> (PendingTransaction, Vec<UtxoEntry>, Transaction) {
//     let entry_total_amount = payback_amount + reveal_fee;
//     let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

//     let mut unsigned_tx = Transaction::new(
//         0,
//         vec![TransactionInput {
//             previous_outpoint: TransactionOutpoint {
//                 transaction_id: prev_tx_tid,
//                 index: 0,
//             },
//             signature_script: vec![],
//             sequence: 0,
//             sig_op_count: 1, // when signed it turns into 1
//         }],
//         vec![TransactionOutput {
//             value: payback_amount,
//             script_public_key: pay_to_address_script(&recipient),
//         }],
//         0,
//         SubnetworkId::from_byte(0),
//         0,
//         vec![],
//     );

//     let entries = vec![UtxoEntry {
//         amount: entry_total_amount,
//         script_public_key: redeem_lock_p2sh.clone(),
//         block_daa_score: prev_tx_score,
//         is_coinbase: false,
//     }];

//     // Signing the transaction with keypair.
//     let tx_clone = unsigned_tx.clone();
//     let entries_clone = entries.clone();
//     let schnorr_key =
//         secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes())
//             .unwrap();
//     let mut signed_tx = sign(
//         MutableTransaction::with_entries(tx_clone, entries_clone),
//         schnorr_key,
//     );
//     let signature = signed_tx.tx.inputs[0].signature_script.clone();

//     // Prepend the signature to the unlock script.
//     let script_sig = pay_to_script_hash_signature_script(script_sig.clone(), signature).unwrap();
//     unsigned_tx.inputs[0]
//         .signature_script
//         .clone_from(&script_sig);
//     signed_tx.tx.inputs[0].signature_script = script_sig;

//     let utxo_entry = ClientUTXO {
//         address: None,
//         outpoint: TransactionOutpoint {
//             transaction_id: prev_tx_tid,
//             index: 0,
//         }
//         .into(),
//         amount: entry_total_amount,
//         script_public_key: redeem_lock_p2sh.clone(),
//         block_daa_score: prev_tx_score,
//         is_coinbase: false,
//     };

//     // Transaction generator
//     let utxo_entries: Vec<UtxoEntryReference> = vec![];
//     let priority_utxo_entries = None;
//     let multiplexer = None;
//     let sig_op_count = 1;
//     let minimum_signatures = 1;
//     let utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static> =
//         Box::new(utxo_entries.into_iter());
//     let source_utxo_context = None;
//     let destination_utxo_context = None;
//     let final_priority_fee = reveal_fee.into();
//     let final_transaction_payload = None;
//     let change_address: Address = recipient.clone();

//     let final_transaction_destination = PaymentDestination::PaymentOutputs(PaymentOutputs::from((
//         recipient.clone(),
//         payback_amount,
//     )));

//     let settings = GeneratorSettings {
//         network_id,
//         multiplexer,
//         sig_op_count,
//         minimum_signatures,
//         change_address,
//         utxo_iterator,
//         priority_utxo_entries,
//         source_utxo_context,
//         destination_utxo_context,
//         final_transaction_priority_fee: final_priority_fee,
//         final_transaction_destination,
//         final_transaction_payload,
//     };
//     let generator = Generator::try_new(settings, None, None).unwrap();

//     let utxo_entry_ref_from_ref: Vec<UtxoEntryReference> = vec![UtxoEntryReference {
//         utxo: Arc::new(utxo_entry.to_owned()),
//     }];

//     (
//         PendingTransaction::try_new(
//             &generator,
//             signed_tx.tx,
//             utxo_entry_ref_from_ref,
//             vec![recipient].into_iter().collect(),
//             Some(payback_amount),
//             0,
//             0,
//             0,
//             reveal_fee,
//             reveal_fee,
//             kaspa_wallet_core::tx::DataKind::Final,
//         )
//         .unwrap(),
//         entries,
//         unsigned_tx,
//     )
// }

#[cfg(test)]
mod tests {
    // todo: commented out for migration to WalletAPI.
    // use super::*;
    // use faster_hex::hex_string;
    // use kaspa_addresses::{Address, Prefix};
    // use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
    // use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
    // use kaspa_consensus_core::tx::TransactionId;
    // use kaspa_consensus_core::tx::VerifiableTransaction;
    // use kaspa_txscript::caches::Cache;
    // use kaspa_txscript::SigCacheKey;
    // use kaspa_txscript::TxScriptEngine;
    // use kaspa_txscript_errors::TxScriptError;

    // fn print_script_sig(script_sig: &[u8]) {
    //     let mut step = 0;
    //     let mut incrementing = true;

    //     for (index, value) in script_sig.iter().enumerate() {
    //         let overall_position = index * 2;
    //         let hex_string = format!("{:02x}", value);
    //         let decimal_value = format!("{:03}", value);
    //         let ascii_value = if *value >= 0x20 && *value <= 0x7e {
    //             *value as char
    //         } else {
    //             step = 0; // Reset step if the character is non-ASCII
    //             incrementing = true; // Reset incrementing
    //             '.'
    //         };
    //         let padding = " ".repeat(step * 2);
    //         println!(
    //             "{:03} 0x{} | {} | {}{}",
    //             overall_position, hex_string, decimal_value, padding, ascii_value
    //         );

    //         if *value >= 0x20 && *value <= 0x7e {
    //             if incrementing {
    //                 if step < 10 {
    //                     step += 1;
    //                 } else {
    //                     incrementing = false;
    //                     step -= 1;
    //                 }
    //             } else if step > 0 {
    //                 step -= 1;
    //             } else {
    //                 incrementing = true;
    //                 step += 1;
    //             }
    //         }
    //     }
    // }

    // #[test]
    // pub fn test_and_verify_sign() {
    //     let (secret_key, public_key) = demo_keypair();
    //     // let pubkey = ScriptVec::from_slice(&public_key.serialize());
    //     let test_address = Address::new(
    //         Prefix::Testnet,
    //         kaspa_addresses::Version::PubKey,
    //         &public_key.x_only_public_key().0.serialize(),
    //     );

    //     let (_, script_sig) = deploy_token_demo(&public_key);
    //     let priority_fee_sompi = SOMPI_PER_KASPA;

    //     let template = payload_to_placeholder(&script_sig, &public_key);
    //     println!("Template {}", hex_string(&template[..]));
    //     println!("Template {:?}", template);
    //     print_script_sig(&template);

    //     let prev_tx_id = TransactionId::from_str(
    //         "770eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3",
    //     )
    //     .unwrap();

    //     let test_daa_score = 30310;
    //     let (_, entries, unsigned_tx) = reveal_transaction(
    //         TransactionDetails {
    //             script_sig,
    //             recipient: test_address,
    //             secret_key,
    //             prev_tx_tid: prev_tx_id,
    //             prev_tx_score: test_daa_score,
    //         },
    //         priority_fee_sompi,
    //         priority_fee_sompi,
    //         NetworkId::from_str("testnet-11").unwrap(),
    //     );

    //     print_script_sig(&unsigned_tx.inputs[0].signature_script);

    //     let tx = MutableTransaction::with_entries(unsigned_tx, entries);

    //     let tx = tx.as_verifiable();
    //     let cache: Cache<SigCacheKey, bool> = Cache::new(10_000);
    //     let mut reused_values = SigHashReusedValues::new();

    //     let script_run: Result<(), TxScriptError> =
    //         tx.populated_inputs()
    //             .enumerate()
    //             .try_for_each(|(idx, (input, entry))| {
    //                 TxScriptEngine::from_transaction_input(
    //                     &tx,
    //                     input,
    //                     idx,
    //                     entry,
    //                     &mut reused_values,
    //                     &cache,
    //                 )?
    //                 .execute()
    //             });

    //     eprintln!("{:?}", script_run.clone().err());
    //     assert!(script_run.is_ok());
    // }
}
