use std::sync::Arc;

use graph::blockchain::block_stream::BlockWithTriggers;
use graph::prelude::web3::types::{Block, Bytes, H160, H256, U256};
use graph_chain_ethereum::Chain;
use graph_core::subgraph::instance_manager;

pub fn get_block() {
    let block = Block {
        hash: None,
        parent_hash: H256::from_low_u64_be(1),
        uncles_hash: H256::from_low_u64_be(1),
        author: H160::from_low_u64_be(1),
        state_root: H256::from_low_u64_be(1),
        transactions_root: H256::from_low_u64_be(1),
        receipts_root: H256::from_low_u64_be(1),
        number: None,
        gas_used: U256::one(),
        gas_limit: U256::one(),
        base_fee_per_gas: None,
        extra_data: Bytes::default(),
        logs_bloom: None,
        timestamp: U256::one(),
        difficulty: U256::one(),
        total_difficulty: None,
        seal_fields: vec!(Bytes::default()),
        uncles: vec!(H256::from_low_u64_be(1)),
        transactions: vec!(),
        size: None,
        mix_hash: None,
        nonce: None,
    };
    let block_finality = graph_chain_ethereum::chain::BlockFinality::Final(Arc::new(block));
    let _block_with_triggers: BlockWithTriggers<Chain> =
        BlockWithTriggers::new(block_finality, vec![]);

    println!("🦀");
}
