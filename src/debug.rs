use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use ethabi::Contract;
use graph::blockchain::block_stream::BlockWithTriggers;
use graph::blockchain::{Blockchain, DataSourceTemplate};
use graph::components::store::{DeploymentId, DeploymentLocator};
use graph::data::subgraph::{Mapping, TemplateSource, UnifiedMappingApiVersion};
use graph::prelude::web3::transports::Http;
use graph::prelude::web3::types::{Block, Bytes, H160, H256, U256};
use graph::prelude::web3::Web3;
use graph::prelude::{
    CancelGuard, ChainStore, DeploymentHash, Link, LoggerFactory, MappingABI, MappingBlockHandler,
    MappingCallHandler, MappingEventHandler, StopwatchMetrics, SubgraphManifest,
};
use graph::prometheus::{CounterVec, GaugeVec, Opts};
use graph::semver::Version;
use graph_chain_ethereum::chain::TriggersAdapter;
use graph_chain_ethereum::{
    Chain, EthereumAdapter, ProviderEthRpcMetrics, SubgraphEthRpcMetrics, Transport,
};
use graph_core::subgraph::instance_manager::{process_block, IndexingContext, IndexingInputs};
use graph_mock::MockMetricsRegistry;
use graph_runtime_test::common::{mock_context, mock_data_source};
use slog::Logger;

use crate::subgraph_store::MockSubgraphStore;
use crate::writable_store::MockWritableStore;

pub async fn get_block() {
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
        seal_fields: vec![Bytes::default()],
        uncles: vec![H256::from_low_u64_be(1)],
        transactions: vec![],

        size: None,
        mix_hash: None,
        nonce: None,
    };
    let block_finality = graph_chain_ethereum::chain::BlockFinality::Final(Arc::new(block));
    let block_with_triggers: BlockWithTriggers<Chain> =
        BlockWithTriggers::new(block_finality, vec![]);

    // TODO: Generalise and reuse all the mock args
    let logger = Logger::root(slog::Discard, graph::prelude::o!());

    let block_stream_canceler = CancelGuard::new();
    let block_stream_cancel_handle = block_stream_canceler.handle();

    let subgraph_id = "ipfsMap";

    let deployment_id = DeploymentHash::new(subgraph_id).expect("Could not create DeploymentHash.");

    let deployment = DeploymentLocator::new(DeploymentId::new(42), deployment_id.clone());

    // TODO: remove hardcoded path to wasm
    let data_source = mock_data_source("build/Gravity", Version::new(0, 0, 4));

    let mock_subgraph_store = MockSubgraphStore {};

    let mock_writable_store = MockWritableStore {};

    let eth_rpc_metrics = SubgraphEthRpcMetrics {
        request_duration: Box::new(GaugeVec::new(Opts::new("str", "str"), &["str"]).unwrap()),
        errors: Box::new(CounterVec::new(Opts::new("str", "str"), &["str"]).unwrap()),
    };

    let metrics_registry = Arc::new(MockMetricsRegistry::new());

    let stopwatch_metrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, graph::prelude::o!()),
        deployment_id.clone(),
        metrics_registry.clone(),
    );

    struct MockChainStore {}

    #[async_trait]
    impl ChainStore for MockChainStore {
        fn genesis_block_ptr(&self) -> Result<graph::blockchain::BlockPtr, anyhow::Error> {
            unimplemented!()
        }

        async fn upsert_block(
            &self,
            _block: graph::prelude::EthereumBlock,
        ) -> Result<(), anyhow::Error> {
            unimplemented!()
        }

        fn upsert_light_blocks(
            &self,
            _blocks: Vec<graph::prelude::LightEthereumBlock>,
        ) -> Result<(), anyhow::Error> {
            unimplemented!()
        }

        async fn attempt_chain_head_update(
            self: Arc<Self>,
            _ancestor_count: graph::prelude::BlockNumber,
        ) -> Result<Option<H256>, anyhow::Error> {
            unimplemented!()
        }

        fn chain_head_ptr(&self) -> Result<Option<graph::blockchain::BlockPtr>, anyhow::Error> {
            unimplemented!()
        }

        fn blocks(
            &self,
            _hashes: Vec<H256>,
        ) -> Result<Vec<graph::prelude::LightEthereumBlock>, anyhow::Error> {
            unimplemented!()
        }

        fn ancestor_block(
            &self,
            _block_ptr: graph::blockchain::BlockPtr,
            _offset: graph::prelude::BlockNumber,
        ) -> Result<Option<graph::prelude::EthereumBlock>, anyhow::Error> {
            unimplemented!()
        }

        fn cleanup_cached_blocks(
            &self,
            _ancestor_count: graph::prelude::BlockNumber,
        ) -> Result<Option<(graph::prelude::BlockNumber, usize)>, anyhow::Error> {
            unimplemented!()
        }

        fn block_hashes_by_block_number(
            &self,
            _number: graph::prelude::BlockNumber,
        ) -> Result<Vec<H256>, anyhow::Error> {
            unimplemented!()
        }

        fn confirm_block_hash(
            &self,
            _number: graph::prelude::BlockNumber,
            _hash: &H256,
        ) -> Result<usize, anyhow::Error> {
            unimplemented!()
        }

        fn block_number(
            &self,
            _block_hash: H256,
        ) -> Result<Option<(String, graph::prelude::BlockNumber)>, graph::prelude::StoreError>
        {
            unimplemented!()
        }

        async fn transaction_receipts_in_block(
            &self,
            _block_ptr: &H256,
        ) -> Result<
            Vec<graph::components::transaction_receipt::LightTransactionReceipt>,
            graph::prelude::StoreError,
        > {
            unimplemented!()
        }
    }

    let chain_store = MockChainStore {};

    let transport = Transport::RPC(Http::new("url").unwrap().1);
    let web3 = Web3::new(transport);

    let metrics_registry = Arc::new(MockMetricsRegistry::new());

    let metrics = ProviderEthRpcMetrics::new(metrics_registry);

    let eth_adapter = EthereumAdapter {
        logger: logger.clone(),
        url_hostname: Arc::new(String::from("hostname")),
        provider: String::from("provider"),
        web3: Arc::new(web3),
        metrics: Arc::new(metrics),
        supports_eip_1898: false,
    };

    let triggers_adapter = TriggersAdapter {
        logger: logger.clone(),
        ethrpc_metrics: Arc::new(eth_rpc_metrics),
        stopwatch_metrics,
        chain_store: Arc::new(chain_store),
        eth_adapter: Arc::new(eth_adapter),
        unified_api_version: UnifiedMappingApiVersion::try_from_versions(
            vec![&Version::new(0, 0, 4)].into_iter(),
        )
        .unwrap(),
    };

    let logger_factory = LoggerFactory {
        parent: logger.clone(),
        elastic_config: None,
    };

    // let chain = Chain {
    //     logger_factory: (),
    //     name: (),
    //     node_id: (),
    //     registry: (),
    //     eth_adapters: (),
    //     ancestor_count: (),
    //     chain_store: (),
    //     call_cache: (),
    //     subgraph_store: (),
    //     chain_head_update_listener: (),
    //     reorg_threshold: (),
    //     is_ingestible: (),
    // };

    // let indexing_inputs: IndexingInputs<Chain> = IndexingInputs {
    //     deployment,
    //     features: BTreeSet::new(),
    //     start_blocks: vec![1],
    //     store: Arc::new(mock_writable_store),
    //     triggers_adapter: Arc::new(triggers_adapter),
    //     chain: (),
    //     templates: (),
    //     unified_api_version: (),
    // };

    // let indexing_context = IndexingContext{ inputs: (), state: (), subgraph_metrics: (), host_metrics: (), block_stream_metrics: () };

    // process_block(
    //     &logger,
    //     Arc::new(triggers_adapter),
    //     ctx,
    //     block_stream_cancel_handle.clone(),
    //     block_with_triggers,
    // );

    println!("🦀");
}
