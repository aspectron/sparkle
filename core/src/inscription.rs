use crate::constants::*;
use crate::model::kasplex::v1::krc20::TokenTransaction;
use crate::model::kasplex::v1::Protocol;
// use api::rpc;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_hashes::Hash;
use kaspa_txscript::opcodes::codes::*;
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_script_hash_script, pay_to_script_hash_signature_script, pay_to_address_script};
// use kaspa_wallet_keys::publickey;
use secp256k1::{rand, PublicKey, Secp256k1, SecretKey};
use std::str::FromStr;
// use std::sync::mpsc::RecvTimeoutError;
use kaspa_consensus_core::sign::sign;
use kaspa_consensus_core::tx::{
    MutableTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput,
    TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
// use kaspa_consensus_core::tx::VerifiableTransaction;
// use kaspa_consensus_core::Hash;

// use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
// use kaspa_txscript::caches::Cache;
// use kaspa_txscript::SigCacheKey;

use kaspa_consensus_core::{subnets::SubnetworkId, tx::*};

use kaspa_wrpc_client::prelude::*;
// use kaspa_wrpc_client::result::Result;
use kaspa_wallet_core::tx::PaymentOutputs;
use kaspa_wallet_core::tx::GeneratorSettings;
use kaspa_wallet_core::tx::PaymentDestination;
use std::sync::Arc;
use kaspa_wallet_core::tx::PendingTransaction;
use kaspa_wallet_core::utxo::UtxoEntryReference;
use kaspa_wallet_core::tx::Generator;

use kaspa_consensus_client::UtxoEntry as ClientUTXO;


pub fn demo_keypair() -> (secp256k1::SecretKey, secp256k1::PublicKey, Address) {
    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
    // let script_pub_key = ScriptVec::from_slice(&public_key.serialize());
    // let spk = ScriptPublicKey::new(0, script_pub_key);
    // let address = extract_script_pub_key_address(&spk, Prefix::Testnet).unwrap();
    // public_key.serialize()[1..33]
    // let pay_address =
    let address = Address::new(Prefix::Testnet, kaspa_addresses::Version::PubKey, &public_key.x_only_public_key().0.serialize());

    // let address = Address::new(Prefix::Testnet, Version::PubKey, &public_key.to_bytes()[1..33]);
    // let p2pk = pay_to_address_script(&address);
    // let pubkey = ScriptPublicKey::new(0, script);
    // let address = extract_script_pub_key_address(&public_key, Prefix::Testnet).unwrap();
    (secret_key, public_key, address)
}

fn redeem_pubkey(redeem_script: &[u8], pubkey: &[u8]) -> ScriptBuilderResult<Vec<u8>> {
    Ok(ScriptBuilder::new()
        .add_data(pubkey)?
        .add_op(OpCheckSig)?
        .add_op(OpFalse)?
        .add_op(OpIf)?
        .add_data(PROTOCOL_NAMESPACE.as_bytes())?
        .add_data(&[0])?
        .add_data(redeem_script)?
        .add_op(OpEndIf)?
        .drain())
}

pub fn deploy_token_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let transaction: TokenTransaction = TokenTransaction {
        protocol: Protocol::from_str("krc-20").unwrap(),
        op: "deploy".to_string(),
        tick: "toitoi".to_string(),
        max: Some(21000000000),
        limit: Some(3010000),
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
    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh = extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

pub fn reveal_transaction(script_sig: Vec<u8>, recipient: Address, secret_key: SecretKey, prev_tx_tid: Hash, prev_tx_score: u64, amount: u64, gas: u64)
-> (PendingTransaction, Vec<UtxoEntry>, Transaction) {

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let mut unsigned_tx = Transaction::new(
        0,
        vec![TransactionInput {
            previous_outpoint: TransactionOutpoint {
                transaction_id: prev_tx_tid,
                index: 0,
            },
            signature_script: vec![],
            sequence: 0,
            sig_op_count: 1, // when signed it turns into 1
        }],
        vec![
            TransactionOutput {
                value: amount,
                script_public_key: pay_to_address_script(&recipient),
            },
        ],
        0,
        SubnetworkId::from_byte(0),
        0,
        vec![],
    );

    let entries = vec![UtxoEntry {
        amount: 1001 * SOMPI_PER_KASPA,
        script_public_key: redeem_lock_p2sh.clone(),
        block_daa_score: prev_tx_score,
        is_coinbase: false,
    }];

    // Signing the transaction with keypair.
    let tx_clone = unsigned_tx.clone();
    let entries_clone = entries.clone();
    let schnorr_key =
        secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes())
            .unwrap();
    let mut signed_tx = sign(
        MutableTransaction::with_entries(tx_clone, entries_clone),
        schnorr_key,
    );
    let signature = signed_tx.tx.inputs[0].signature_script.clone();

    // Prepend the signature to the unlock script.
    let script_sig = pay_to_script_hash_signature_script(script_sig.clone(), signature).unwrap();
    unsigned_tx.inputs[0]
        .signature_script
        .clone_from(&script_sig);
    signed_tx.tx.inputs[0].signature_script = script_sig;


    let network_id = NetworkId::from_str("testnet-11").unwrap();
    let utxo_entry = ClientUTXO {
        address: None,
        outpoint: TransactionOutpoint { transaction_id: prev_tx_tid,
            index: 0}.into(),
        amount: 1001,
        script_public_key: redeem_lock_p2sh.clone(),
        block_daa_score: prev_tx_score,
        is_coinbase: false,
    };

    // UtxoEntryReference::from(utxo);
    let utxo_entries: Vec<UtxoEntryReference> = vec![];
    let multiplexer = None;
    let sig_op_count = 1;
    let minimum_signatures = 1;
    let utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static> = Box::new(utxo_entries.into_iter());
    let source_utxo_context = None;
    let destination_utxo_context = None;
    let final_priority_fee = gas.into();
    let final_transaction_payload = None;
    let change_address: Address = recipient.clone();

    let final_transaction_destination = 
        PaymentDestination::PaymentOutputs(PaymentOutputs::from((recipient.clone(), amount)));

    let settings = GeneratorSettings {
        network_id,
        multiplexer,
        sig_op_count,
        minimum_signatures,
        change_address,
        utxo_iterator,
        source_utxo_context,
        destination_utxo_context,
        final_transaction_priority_fee: final_priority_fee,
        final_transaction_destination,
        final_transaction_payload,
    };
    let generator = Generator::try_new(settings, None, None).unwrap();

    let utxo_entry_ref_from_ref: Vec<UtxoEntryReference> = vec![UtxoEntryReference{utxo: Arc::new(utxo_entry.to_owned())}];

    (PendingTransaction::try_new(
        &generator,
        signed_tx.tx,
        utxo_entry_ref_from_ref,
        vec![recipient].into_iter().collect(),
        Some(amount),
        0,
        0,
        0,
        gas,
        gas,
        kaspa_wallet_core::tx::DataKind::Final,
    ).unwrap(), entries, unsigned_tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_and_verify_sign() {
        use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
        use kaspa_txscript::caches::Cache;
        use kaspa_txscript::SigCacheKey;
        use kaspa_txscript::TxScriptEngine;
        use kaspa_txscript_errors::TxScriptError;
        use kaspa_consensus_core::tx::VerifiableTransaction;

        // let secp = Secp256k1::new();
        // let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        // let test_pubkey = ScriptVec::from_slice(&public_key.serialize());

        let (secret_key, public_key, test_address) = demo_keypair();
        // let pubkey = ScriptVec::from_slice(&public_key.serialize());
        let test_address = Address::new(Prefix::Testnet, kaspa_addresses::Version::PubKey, &public_key.x_only_public_key().0.serialize());

        let (redeem_lock, script_sig) = deploy_token_demo(&public_key);
        let priority_fee_sompi = SOMPI_PER_KASPA;

        let prev_tx_id =
            TransactionId::from_str("770eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3")
                .unwrap();

        // let recipient = ScriptPublicKey::new(0, pubkey.clone());
        let (transaction, entries, unsigned_tx) = 
            reveal_transaction(script_sig, test_address, secret_key, prev_tx_id, 1000, priority_fee_sompi, priority_fee_sompi);

        let tx =
            MutableTransaction::with_entries(unsigned_tx, entries);
            // MutableTransaction::with_entries(transaction.transaction(), entries);

        let tx = tx.as_verifiable();
        let cache: Cache<SigCacheKey, bool> = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();

        let script_run: Result<(), TxScriptError> =
        tx.populated_inputs()
            .enumerate()
            .try_for_each(|(idx, (input, entry))| {
                TxScriptEngine::from_transaction_input(
                    &tx,
                    input,
                    idx,
                    entry,
                    &mut reused_values,
                    &cache,
                )?
                .execute()
            });

        eprintln!("{:?}", script_run.clone().err());
        assert!(script_run.is_ok());
    }
}