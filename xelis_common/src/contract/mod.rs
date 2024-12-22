mod metadata;
mod opaque;
mod random;
mod output;

use std::collections::HashMap;
use anyhow::Context as AnyhowContext;
use indexmap::IndexMap;
use log::{debug, info};
use opaque::*;
use xelis_builder::EnvironmentBuilder;
use xelis_vm::{
    Constant,
    Context,
    FnInstance,
    FnParams,
    FnReturnType,
    OpaqueWrapper,
    Type,
    Value,
    ValueCell
};
use crate::{
    block::{Block, TopoHeight},
    crypto::{Address, Hash, PublicKey},
    transaction::ContractDeposit
};

pub use metadata::ContractMetadata;
pub use random::DeterministicRandom;
pub use output::*;

pub use opaque::{ContractStorage, StorageWrapper};

pub struct TransferOutput {
    // The destination key for the transfer
    pub destination: PublicKey,
    // The amount to transfer
    pub amount: u64,
    // The asset to transfer
    pub asset: Hash,
}

pub struct ChainState<'a> {
    // Are we in debug mode
    // used by the contract to print debug information
    pub debug_mode: bool,
    // The random number generator
    // It is deterministic so we can replay the contract
    pub random: DeterministicRandom,
    // Are we in mainnet
    pub mainnet: bool,
    // The contract hash
    pub contract: &'a Hash,
    // The topoheight of the block
    pub topoheight: TopoHeight,
    // Block hash in which the contract is executed
    pub block_hash: &'a Hash,
    // The block in which the contract is executed
    pub block: &'a Block,
    // Tx hash in which the contract is executed
    pub tx_hash: &'a Hash,
    // All deposits made by the caller
    pub deposits: &'a IndexMap<Hash, ContractDeposit>,
    // All the transfers generated by the contract
    pub transfers: Vec<TransferOutput>,
    // The storage of the contract
    // All the changes made by the contract are stored here
    pub storage: HashMap<Constant, Option<Constant>>
}

// Build the environment for the contract
pub fn build_environment<S: ContractStorage>() -> EnvironmentBuilder<'static> {
    debug!("Building environment for contract");
    register_opaque_types();

    let mut env = EnvironmentBuilder::default();

    env.get_mut_function("println", None, vec![Type::Any])
        .set_on_call(println_fn);

    env.get_mut_function("debug", None, vec![Type::Any])
        .set_on_call(debug_fn);

    // Opaque type but we provide getters
    let tx_type = Type::Opaque(env.register_opaque::<OpaqueTransaction>("Transaction"));
    let hash_type = Type::Opaque(env.register_opaque::<Hash>("Hash"));
    let address_type = Type::Opaque(env.register_opaque::<Hash>("Address"));
    let random_type = Type::Opaque(env.register_opaque::<OpaqueRandom>("Random"));
    let block_type = Type::Opaque(env.register_opaque::<OpaqueBlock>("Block"));
    let storage_type = Type::Opaque(env.register_opaque::<OpaqueStorage>("Storage"));

    // Transaction
    {
        env.register_native_function(
            "transaction",
            Some(tx_type.clone()),
            vec![],
            transaction,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "nonce",
            Some(tx_type.clone()),
            vec![],
            transaction_nonce,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "hash",
            Some(tx_type.clone()),
            vec![],
            transaction_hash,
            5,
            Some(hash_type.clone())
        );
        env.register_native_function(
            "source",
            Some(tx_type.clone()),
            vec![],
            transaction_source,
            5,
            Some(address_type.clone())
        );
        env.register_native_function(
            "fee",
            Some(tx_type.clone()),
            vec![],
            transaction_fee,
            5,
            Some(Type::U64)
        );
    }

    // Block
    {
        env.register_native_function(
            "block",
            Some(block_type.clone()),
            vec![],
            block,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "nonce",
            Some(block_type.clone()),
            vec![],
            block_nonce,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "timestamp",
            Some(block_type.clone()),
            vec![],
            block_timestamp,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "height",
            Some(block_type.clone()),
            vec![],
            block_height,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "extra_nonce",
            Some(block_type.clone()),
            vec![],
            block_extra_nonce,
            5,
            Some(Type::Array(Box::new(Type::U8)))
        );
        env.register_native_function(
            "hash",
            Some(block_type.clone()),
            vec![],
            block_hash,
            5,
            Some(hash_type.clone())
        );
        env.register_native_function(
            "miner",
            Some(block_type.clone()),
            vec![],
            block_miner,
            5,
            Some(address_type.clone())
        );
        env.register_native_function(
            "version",
            Some(block_type.clone()),
            vec![],
            block_version,
            5,
            Some(Type::U8)
        );
        env.register_native_function(
            "tips",
            Some(block_type.clone()),
            vec![],
            block_tips,
            5,
            Some(Type::Array(Box::new(hash_type.clone())))
        );
    }

    // Storage
    {
        env.register_native_function(
            "storage",
            Some(storage_type.clone()),
            vec![],
            storage,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "load",
            Some(storage_type.clone()),
            vec![("key", Type::U64)],
            storage_load::<S>,
            50,
            Some(Type::Optional(Box::new(Type::Any)))
        );
        env.register_native_function(
            "has",
            Some(storage_type.clone()),
            vec![("key", Type::U64)],
            storage_has::<S>,
            25,
            Some(Type::Bool)
        );
        env.register_native_function(
            "store",
            Some(storage_type.clone()),
            vec![("key", Type::U64), ("value", Type::Any)],
            storage_store::<S>,
            50,
            None
        );
        env.register_native_function(
            "delete",
            Some(storage_type.clone()),
            vec![("key", Type::U64)],
            storage_delete::<S>,
            50,
            None
        );
    }

    env.register_native_function(
        "get_contract_hash",
        None,
        vec![],
        get_contract_hash,
        5,
        Some(hash_type.clone())
    );

    env.register_native_function(
        "get_deposit_for_asset",
        None,
        vec![("asset", hash_type.clone())],
        get_deposit_for_asset,
        5,
        Some(Type::Optional(Box::new(Type::U64)))
    );

    env.register_native_function(
        "get_balance_for_asset",
        None,
        vec![("asset", hash_type.clone())],
        get_balance_for_asset,
        25,
        Some(Type::U64)
    );

    env.register_native_function(
        "transfer",
        Some(tx_type.clone()),
        vec![
            ("destination", address_type.clone()),
            ("amount", Type::U64),
            ("asset", hash_type.clone()),
        ],
        transfer,
        500,
        Some(Type::Bool)
    );

    // Hash
    env.register_native_function(
        "as_bytes",
        Some(hash_type.clone()),
        vec![],
        hash_as_bytes_fn,
        5,
        Some(Type::Array(Box::new(Type::U8)))
    );

    // Random number generator
    {
        env.register_native_function(
            "random",
            None,
            vec![],
            random_fn,
            5,
            Some(random_type.clone())
        );
        env.register_native_function(
            "next_u8",
            Some(random_type.clone()),
            vec![],
            random_u8,
            5,
            Some(Type::U8)
        );
        env.register_native_function(
            "next_u16",
            Some(random_type.clone()),
            vec![],
            random_u16,
            5,
            Some(Type::U16)
        );
        env.register_native_function(
            "next_u32",
            Some(random_type.clone()),
            vec![],
            random_u32,
            5,
            Some(Type::U32)
        );
        env.register_native_function(
            "next_u64",
            Some(random_type.clone()),
            vec![],
            random_u64,
            5,
            Some(Type::U64)
        );
        env.register_native_function(
            "next_u128",
            Some(random_type.clone()),
            vec![],
            random_u128,
            5,
            Some(Type::U128)
        );
        env.register_native_function(
            "next_u256",
            Some(random_type.clone()),
            vec![],
            random_u256,
            5,
            Some(Type::U256)
        );
        env.register_native_function(
            "next_bool",
            Some(random_type.clone()),
            vec![],
            random_bool,
            5,
            Some(Type::Bool)
        );
    }

    env
}

fn println_fn(_: FnInstance, params: FnParams, context: &mut Context) -> FnReturnType {
    let state: &ChainState = context.get().context("chain state not found")?;
    if state.debug_mode {
        info!("{}", params[0].as_ref());
    }

    Ok(None)
}

fn debug_fn(_: FnInstance, params: FnParams, context: &mut Context) -> FnReturnType {
    let state: &ChainState = context.get().context("chain state not found")?;
    if state.debug_mode {
        debug!("{:?}", params[0].as_ref().as_value());
    }

    Ok(None)
}

fn get_contract_hash(_: FnInstance, _: FnParams, context: &mut Context) -> FnReturnType {
    let state: &ChainState = context.get().context("chain state not found")?;
    Ok(Some(Value::Opaque(OpaqueWrapper::new(state.contract.clone())).into()))
}

fn get_deposit_for_asset(_: FnInstance, params: FnParams, context: &mut Context) -> FnReturnType {
    let param = params[0].as_ref();
    let asset: &Hash = param
        .as_value()
        .as_opaque_type()
        .context("invalid asset")?;

    let chain_state: &ChainState = context.get().context("chain state not found")?;

    let mut opt = None;
    if let Some(deposit) = chain_state.deposits.get(asset) {
        match deposit {
            ContractDeposit::Public(amount) => {
                opt = Some(Value::U64(*amount).into());
            }
        }
    }

    Ok(Some(ValueCell::Optional(opt)))
}

fn get_balance_for_asset(_: FnInstance, _: FnParams, _: &mut Context) -> FnReturnType {
    Ok(None)
}

fn transfer(_: FnInstance, mut params: FnParams, context: &mut Context) -> FnReturnType {
    let state: &mut ChainState = context.get_mut()
        .context("chain state not found")?;

    let amount = params.remove(2)
        .into_owned()
        .to_u64()?;

    let asset: Hash = params.remove(1)
        .into_owned()
        .into_opaque_type()?;

    let destination: Address = params.remove(0)
        .into_owned()
        .into_opaque_type()?;

    state.transfers.push(TransferOutput {
        destination: destination.to_public_key(),
        amount,
        asset,
    });

    Ok(Some(Value::Boolean(true).into()))
}