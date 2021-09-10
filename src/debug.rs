use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use ethabi::Contract;
use graph::blockchain::block_stream::BlockWithTriggers;
use graph::blockchain::{Blockchain, ChainHeadUpdateListener, BlockStreamMetrics};
use graph::components::store::{DeploymentId, DeploymentLocator, BlockNumber};

use graph::data::subgraph::{DeploymentHash, Mapping, TemplateSource, UnifiedMappingApiVersion};
use graph::petgraph::graphmap::GraphMap;
use graph::prelude::s::{Definition, DirectiveDefinition, Document};
use graph::prelude::web3::transports::Http;
use graph::prelude::web3::types::{Block, Bytes, H160, H256, U256, Address};
use graph::prelude::web3::Web3;
use graph::prelude::{CancelGuard, ChainStore, EthereumCallCache, HostMetrics, Link, LinkResolver, LoggerFactory, MappingABI, MappingBlockHandler, MappingCallHandler, MappingEventHandler, MetricsRegistry, NodeId, Schema, StopwatchMetrics, SubgraphManifest, Histogram, HistogramOpts};
use graph::prometheus::{CounterVec, GaugeVec, Opts};
use graph::semver::Version;
use graph_chain_ethereum::chain::TriggersAdapter;
use graph_chain_ethereum::data_source::BaseDataSourceTemplate;
use graph_chain_ethereum::network::{EthereumNetworkAdapter, EthereumNetworkAdapters};

use graph_chain_ethereum::adapter::{EthereumLogFilter, EventSignature, EthereumCallFilter, FunctionSelector, EthereumBlockFilter};
use graph_chain_ethereum::adapter::LogFilterNode;
use graph_chain_ethereum::{Chain, EthereumAdapter, NodeCapabilities, ProviderEthRpcMetrics, SubgraphEthRpcMetrics, Transport, TriggerFilter};
use graph_core::subgraph::instance_manager::{process_block, IndexingContext, IndexingInputs, IndexingState, SubgraphInstanceMetrics};
use graph_core::subgraph::SubgraphInstance;

use graph_runtime_test::common::{mock_data_source};

use slog::Logger;
use crate::subgraph_store::MockSubgraphStore;
use crate::writable_store::MockWritableStore;

use graph::petgraph::Undirected;
use std::collections::hash_map::RandomState;

use prometheus::core::GenericGauge;

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
    let _data_source = mock_data_source("build/Gravity", Version::new(0, 0, 4));

    let mock_subgraph_store = MockSubgraphStore {};

    let mock_writable_store = MockWritableStore {};

    let eth_rpc_metrics = SubgraphEthRpcMetrics {
        request_duration: Box::new(GaugeVec::new(Opts::new("str", "str"), &["str"]).unwrap()),
        errors: Box::new(CounterVec::new(Opts::new("str", "str"), &["str"]).unwrap()),
    };

    let metrics_registry = Arc::new(MockMetricsRegistry {});

    let stopwatch_metrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, graph::prelude::o!()),
        deployment_id.clone(),
        metrics_registry.clone(),
    );

    #[derive(Clone)]
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

    let metrics_registry = Arc::new(MockMetricsRegistry {});

    let metrics = ProviderEthRpcMetrics::new(metrics_registry.clone());

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
        ethrpc_metrics: Arc::new(eth_rpc_metrics.clone()),
        stopwatch_metrics: stopwatch_metrics.clone(),
        chain_store: Arc::new(chain_store.clone()),
        eth_adapter: Arc::new(eth_adapter.clone()),
        unified_api_version: UnifiedMappingApiVersion::try_from_versions(
            vec![&Version::new(0, 0, 4)].into_iter(),
        )
        .unwrap(),
    };

    let logger_factory = LoggerFactory {
        parent: logger.clone(),
        elastic_config: None,
    };

    let node_id = NodeId::new("d").unwrap();

    #[derive(Clone)]
    struct MockMetricsRegistry {}

    impl MetricsRegistry for MockMetricsRegistry {
        fn register(&self, _name: &str, _c: Box<dyn graph::prelude::Collector>) {
            unimplemented!()
        }

        fn unregister(&self, _metric: Box<dyn graph::prelude::Collector>) {
            unimplemented!()
        }

        fn global_counter(
            &self,
            _name: &str,
            _help: &str,
            _const_labels: HashMap<String, String>,
        ) -> Result<graph::prometheus::Counter, graph::prometheus::Error> {
            unimplemented!()
        }

        fn global_gauge(
            &self,
            _name: &str,
            _help: &str,
            _const_labels: HashMap<String, String>,
        ) -> Result<graph::prometheus::Gauge, graph::prometheus::Error> {
            unimplemented!()
        }
    }

    let mock_metrics_registry = MockMetricsRegistry {};

    let node_capabilities = NodeCapabilities {
        archive: false,
        traces: false,
    };

    let eth_network_adapter = EthereumNetworkAdapter {
        capabilities: node_capabilities,
        adapter: Arc::new(eth_adapter.clone()),
    };

    let eth_network_adapters = EthereumNetworkAdapters {
        adapters: vec![eth_network_adapter],
    };

    let chain_store = MockChainStore {};

    #[derive(Clone)]
    struct MockEthCallCache {}

    impl EthereumCallCache for MockEthCallCache {
        fn get_call(
            &self,
            _contract_address: ethabi::Address,
            _encoded_call: &[u8],
            _block: graph::blockchain::BlockPtr,
        ) -> Result<Option<Vec<u8>>, anyhow::Error> {
            unimplemented!()
        }

        fn set_call(
            &self,
            _contract_address: ethabi::Address,
            _encoded_call: &[u8],
            _block: graph::blockchain::BlockPtr,
            _return_value: &[u8],
        ) -> Result<(), anyhow::Error> {
            unimplemented!()
        }
    }

    let call_cache = MockEthCallCache {};

    #[derive(Clone)]
    struct MockChainHeadUpdateListener {}

    impl ChainHeadUpdateListener for MockChainHeadUpdateListener {
        fn subscribe(
            &self,
            _network: String,
            _logger: Logger,
        ) -> graph::blockchain::ChainHeadUpdateStream {
            unimplemented!()
        }
    }

    let chain_head_update_listener = MockChainHeadUpdateListener {};

    let chain = Chain {
        logger_factory: logger_factory.clone(),
        name: String::from("name"),
        node_id: node_id.clone(),
        registry: Arc::new(mock_metrics_registry.clone()),
        eth_adapters: Arc::new(eth_network_adapters.clone()),
        ancestor_count: 1,
        chain_store: Arc::new(chain_store.clone()),
        call_cache: Arc::new(call_cache.clone()),
        subgraph_store: Arc::new(mock_subgraph_store.clone()),
        chain_head_update_listener: Arc::new(chain_head_update_listener.clone()),
        reorg_threshold: 1,
        is_ingestible: true,
    };

    let contract = Contract {
        constructor: None,
        functions: HashMap::new(),
        events: HashMap::new(),
        receive: false,
        fallback: false,
    };

    let mapping_abi = MappingABI {
        name: String::from("name"),
        contract,
    };

    let mapping_block_handler = MappingBlockHandler {
        handler: String::from("handler"),
        filter: None,
    };

    let mapping_call_handler = MappingCallHandler {
        function: String::from("function"),
        handler: String::from("handler"),
    };

    let event_handlers = MappingEventHandler {
        event: String::from("event"),
        topic0: None,
        handler: String::from("handler"),
    };

    let link = Link {
        link: String::from("link"),
    };

    //Arc<Vec<graph_chain_ethereum::data_source::BaseDataSourceTemplate<graph::data::subgraph::Mapping>>>
    let mapping = Mapping {
        kind: String::from("kind"),
        api_version: Version::new(0, 0, 4),
        language: String::from("language"),
        entities: vec![String::from("entities")],
        abis: vec![Arc::new(mapping_abi)],
        block_handlers: vec![mapping_block_handler],
        call_handlers: vec![mapping_call_handler],
        event_handlers: vec![event_handlers],
        runtime: Arc::new(vec![255, 255, 255, 255]),
        link,
    };

    let template_source = TemplateSource {
        abi: String::from("abi"),
    };

    let data_source_template = BaseDataSourceTemplate {
        kind: String::from("kind"),
        network: None,
        name: String::from("name"),
        source: template_source,
        mapping,
    };

    let indexing_inputs: IndexingInputs<Chain> = IndexingInputs {
        deployment,
        features: BTreeSet::new(),
        start_blocks: vec![1],
        store: Arc::new(mock_writable_store),
        triggers_adapter: Arc::new(triggers_adapter),
        chain: Arc::new(chain),
        templates: Arc::new(vec![data_source_template]),
        unified_api_version: UnifiedMappingApiVersion::try_from_versions(
            vec![&Version::new(0, 0, 4)].into_iter(),
        )
        .unwrap(),
    };

    let deployment_hash = DeploymentHash::new("s").unwrap();

    let directive_definition = DirectiveDefinition::new("d".to_string());

    let definition = Definition::DirectiveDefinition(directive_definition);

    let document = Document {
        definitions: vec![definition],
    };

    let _schema = Schema {
        id: deployment_hash.clone(),
        document,
        interfaces_for_type: BTreeMap::new(),
        types_for_interface: BTreeMap::new(),
    };

    // TODO: mock ctx

    let mapping = serde_yaml::Mapping::new();

    #[derive(Clone)]
    struct MockLinkResolver {}

    #[async_trait]
    impl LinkResolver for MockLinkResolver {
        fn with_timeout(self, _timeout: std::time::Duration) -> Self
        where
            Self: Sized,
        {
            unimplemented!()
        }

        fn with_retries(self) -> Self
        where
            Self: Sized,
        {
            unimplemented!()
        }

        async fn cat(&self, _logger: &Logger, _link: &Link) -> Result<Vec<u8>, anyhow::Error> {
            unimplemented!()
        }

        async fn json_stream(
            &self,
            _logger: &Logger,
            _link: &Link,
        ) -> Result<graph::prelude::JsonValueStream, anyhow::Error> {
            unimplemented!()
        }
    }

    let link_resolver = MockLinkResolver{};

    let deployment = DeploymentLocator::new(DeploymentId::new(42), deployment_id.clone());

    let chain = Chain {
        logger_factory: logger_factory.clone(),
        name: String::from("name"),
        node_id,
        registry: Arc::new(mock_metrics_registry.clone()),
        eth_adapters: Arc::new(eth_network_adapters.clone()),
        ancestor_count: 1,
        chain_store: Arc::new(chain_store.clone()),
        call_cache: Arc::new(call_cache.clone()),
        subgraph_store: Arc::new(mock_subgraph_store.clone()),
        chain_head_update_listener: Arc::new(chain_head_update_listener.clone()),
        reorg_threshold: 1,
        is_ingestible: true,
    };

    let manifest = SubgraphManifest::<Chain>::resolve_from_raw(
        deployment.hash.clone(),
        mapping,
        // Allow for infinite retries for subgraph definition files.
        &link_resolver,
        &logger,
        Version::new(0, 0, 4),
    )
    .await;

    let host_builder = graph_runtime_wasm::RuntimeHostBuilder::<Chain>::new(
        chain.runtime_adapter(),
        Arc::new(link_resolver),
        Arc::new(mock_subgraph_store),
    );

    let stopwatch_metrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, graph::prelude::o!()),
        deployment_id.clone(),
        metrics_registry.clone(),
    );

    let host_metrics = Arc::new(HostMetrics::new(
        Arc::new(mock_metrics_registry.clone()),
        deployment.hash.as_str(),
        stopwatch_metrics.clone(),
    ));

    let instance = SubgraphInstance::from_manifest(
        &logger,
        manifest.unwrap(),
        host_builder,
        host_metrics.clone(),
    )
    .expect("Could not create instance from manifest.");

    // Arc<std::sync::RwLock<HashMap<DeploymentId, CancelGuard>>>

    let map: HashMap<DeploymentId, CancelGuard> = HashMap::new();
    let instances = Arc::new(RwLock::new(map));

    // GraphMap<graph_chain_ethereum::adapter::LogFilterNode, (), Undirected>

    let contracts_and_events_graph: GraphMap<LogFilterNode, (), Undirected> = GraphMap::new();

    let wildcard_events: HashSet<EventSignature, RandomState> = HashSet::new();

    let ethereum_log_filter = EthereumLogFilter{ contracts_and_events_graph, wildcard_events };

    let contract_addresses_function_signatures:  HashMap<Address, (BlockNumber, HashSet<FunctionSelector>)> = HashMap::new();

    let ethereum_call_filter = EthereumCallFilter{ contract_addresses_function_signatures };

    let ethereum_block_filter = EthereumBlockFilter{ contract_addresses: Default::default(), trigger_every_block: false };

    let trigger_filter = TriggerFilter{ log: ethereum_log_filter, call: ethereum_call_filter, block: ethereum_block_filter };

    let indexing_state = IndexingState{ logger: logger.clone(), instance, instances, filter: trigger_filter, entity_lfu_cache: Default::default() };

    let histogram = Histogram::with_opts(HistogramOpts { common_opts: Opts {
        namespace: "".to_string(),
        subsystem: "".to_string(),
        name: "".to_string(),
        help: "".to_string(),
        const_labels: Default::default(),
        variable_labels: vec![]
    }, buckets: vec![] }).unwrap();

    let subgraph_instance_metrics = SubgraphInstanceMetrics{
        block_trigger_count: Box::new(histogram.clone()),
        block_processing_duration: Box::new(histogram.clone()),
        block_ops_transaction_duration: Box::new(histogram.clone()),
        trigger_processing_duration: Box::new(histogram.clone())
    };

    let block_stream_metrics = Arc::new(BlockStreamMetrics {
        deployment_head: Box::new(GenericGauge::new(String::from("hello"), "f").unwrap()),
        deployment_failed: Box::new(GenericGauge::new(String::from("hello"), "f").unwrap()),
        reverted_blocks: Box::new(GenericGauge::new(String::from("hello"), "f").unwrap()),
        stopwatch: stopwatch_metrics.clone()
    });

    let indexing_context = IndexingContext {
        inputs: indexing_inputs,
        state: indexing_state,
        subgraph_metrics: Arc::new(subgraph_instance_metrics),
        host_metrics,
        block_stream_metrics
    };

    let triggers_adapter = TriggersAdapter {
        logger: logger.clone(),
        ethrpc_metrics: Arc::new(eth_rpc_metrics.clone()),
        stopwatch_metrics: stopwatch_metrics.clone(),
        chain_store: Arc::new(chain_store.clone()),
        eth_adapter: Arc::new(eth_adapter.clone()),
        unified_api_version: UnifiedMappingApiVersion::try_from_versions(
            vec![&Version::new(0, 0, 4)].into_iter(),
        )
            .unwrap(),
    };

    process_block(
        &logger,
        Arc::new(triggers_adapter),
        indexing_context,
        block_stream_cancel_handle.clone(),
        block_with_triggers,
    ).await.unwrap();

    println!("🦀");
}
