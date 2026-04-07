use std::collections::{BTreeMap, BTreeSet};

use alloy_consensus::constants::{EMPTY_ROOT_HASH, KECCAK_EMPTY};
use alloy_consensus::{Transaction, TxReceipt};
use alloy_primitives::{hex, keccak256, Address, BlockHash, BlockNumber, Bytes, B256, U256};
use alloy_rlp::{RlpDecodable, RlpEncodable};
use alloy_rpc_types_eth::Header;
use alloy_serde::{OtherFields, WithOtherFields};
use md5::{Digest as Md5Digest, Md5};
use revm::bytecode::opcode::OpCode;
use revm::state::Account;
use revm::Database;
use revm_inspectors::tracing::{
    types::{CallKind, CallLog, CallTraceNode, TraceMemberOrder},
    CallTraceArena,
};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

use crate::evm::db::EvmDb;
use crate::evm::primitive_types::{
    CitreaReceiptWithBloom, SealedBlock, TransactionSignedAndRecovered,
};
use crate::AccountData;

#[derive(Debug, Clone, PartialEq, RlpDecodable, RlpEncodable, Default)]
pub struct BlockStorageDiff {
    pub hash: B256,
    pub parent_hash: B256,
    pub new_accounts: Vec<NewAccount>,
    pub deleted_accounts: Vec<B256>,
    pub storage_diffs: Vec<AccountStorageDiff>,
    pub new_codes: Vec<NewCode>,
}

#[derive(Debug, Clone, PartialEq, RlpDecodable, RlpEncodable)]
pub struct NewCode {
    pub code_hash: B256,
    pub code: Bytes,
}

#[derive(Debug, Clone, PartialEq, RlpDecodable, RlpEncodable)]
pub struct NewAccount {
    pub address: B256,
    pub balance: U256,
    pub nonce: u64,
    pub code_hash: B256,
}

#[derive(Debug, Clone, PartialEq, RlpDecodable, RlpEncodable)]
pub struct AccountStorageDiff {
    pub address: B256,
    pub diffs: Vec<IndexValuePair>,
}

#[derive(Debug, Clone, PartialEq, RlpDecodable, RlpEncodable)]
pub struct IndexValuePair {
    pub index: B256,
    pub value: U256,
}

impl From<&[AccountData]> for BlockStorageDiff {
    fn from(accounts: &[AccountData]) -> Self {
        let mut new_accounts = Vec::new();
        let mut new_codes = Vec::new();
        let mut storage_diffs = Vec::new();

        for account in accounts {
            new_accounts.push(NewAccount {
                address: keccak256(account.address.0),
                balance: account.balance,
                nonce: account.nonce,
                code_hash: account.code_hash,
            });

            if !account.code.is_empty() {
                new_codes.push(NewCode {
                    code_hash: account.code_hash,
                    code: account.code.clone(),
                });
            }

            if !account.storage.is_empty() {
                let diffs = account
                    .storage
                    .iter()
                    .map(|(key, value)| IndexValuePair {
                        index: keccak256::<[u8; 32]>(key.to_be_bytes()),
                        value: *value,
                    })
                    .collect();
                storage_diffs.push(AccountStorageDiff {
                    address: keccak256(account.address.0),
                    diffs,
                });
            }
        }

        BlockStorageDiff {
            hash: B256::ZERO,
            parent_hash: EMPTY_ROOT_HASH,
            new_accounts,
            deleted_accounts: vec![],
            storage_diffs,
            new_codes,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(default)]
pub struct DebankBlock {
    pub id: BlockHash,
    pub height: BlockNumber,
    pub parent_id: BlockHash,
    pub base_fee_per_gas: Option<u64>,
    pub miner: Address,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub process_start_timestamp: u128,
}

impl From<&SealedBlock> for DebankBlock {
    fn from(block: &SealedBlock) -> Self {
        Self {
            id: block.header.hash(),
            height: block.header.number,
            parent_id: block.header.parent_hash,
            base_fee_per_gas: block.header.base_fee_per_gas,
            miner: block.header.beneficiary,
            gas_limit: block.header.gas_limit,
            gas_used: block.header.gas_used,
            timestamp: block.header.timestamp,
            process_start_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_millis(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(default)]
pub struct DebankTransaction {
    pub id: BlockHash,
    #[serde(rename = "from_addr")]
    pub from: Address,
    #[serde(rename = "to_addr")]
    pub to: Address,
    pub gas_limit: u64,
    pub gas_price: u128,
    pub gas_used: u64,
    pub status: bool,
    #[serde(rename = "max_fee_per_gas")]
    pub gas_fee_cap: u128,
    #[serde(rename = "max_priority_fee_per_gas")]
    pub gas_tip_cap: u128,
    pub input: Bytes,
    pub nonce: u64,
    #[serde(rename = "idx")]
    pub transaction_index: u64,
    pub value: U256,
}

impl DebankTransaction {
    pub fn from_parts(
        receipt: &CitreaReceiptWithBloom,
        tx: &TransactionSignedAndRecovered,
        block: &SealedBlock,
        transaction_index: u64,
    ) -> Self {
        let recovered: reth_primitives::Recovered<reth_primitives::TransactionSigned> =
            tx.clone().into();
        let gas_used = receipt.gas_used.max(1);
        let l1_fee = U256::from(block.l1_fee_rate) * U256::from(receipt.l1_diff_size);
        let effective_gas_price = recovered.effective_gas_price(block.header.base_fee_per_gas);
        let gas_price = (l1_fee / U256::from(gas_used)) + U256::from(effective_gas_price);

        Self {
            id: *recovered.hash(),
            from: recovered.signer(),
            to: recovered.to().unwrap_or_default(),
            gas_limit: recovered.gas_limit(),
            gas_price: gas_price.to(),
            gas_used: receipt.gas_used,
            status: receipt
                .receipt
                .receipt
                .status_or_post_state()
                .coerce_status(),
            gas_fee_cap: recovered.max_fee_per_gas(),
            gas_tip_cap: recovered.max_priority_fee_per_gas().unwrap_or_default(),
            input: recovered.input().clone(),
            nonce: recovered.nonce(),
            transaction_index,
            value: recovered.value(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DebankEvent {
    pub id: String,
    pub contract_id: Address,
    pub selector: String,
    pub topics: Vec<String>,
    pub data: Bytes,
    pub parent_trace_id: String,
    pub pos_in_parent_trace: usize,
    pub idx: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DebankTrace {
    pub id: String,
    pub from_addr: Address,
    pub gas_limit: u64,
    pub input: Bytes,
    pub to_addr: Address,
    pub value: U256,
    pub gas_used: u64,
    pub output: Bytes,
    #[serde(rename = "type")]
    pub call_create_type: String,
    pub call_type: String,
    pub tx_id: B256,
    pub parent_trace_id: String,
    pub pos_in_parent_trace: usize,
    pub self_storage_change: bool,
    pub storage_change: bool,
    pub subtraces: usize,
    pub trace_address: Vec<usize>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BlockValidation {
    pub validation_hash: i64,
    pub is_fork: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(default)]
pub struct BlockFile {
    pub block: DebankBlock,
    #[serde(rename = "txs")]
    pub transactions: Vec<DebankTransaction>,
    pub events: Vec<DebankEvent>,
    pub traces: Vec<DebankTrace>,
    pub error_events: Vec<DebankEvent>,
    pub error_traces: Vec<DebankTrace>,
    pub storage_contracts: Vec<Address>,
}

impl BlockFile {
    pub fn validation(&self) -> BlockValidation {
        let mut ids = Vec::new();
        ids.push(self.block.id.to_string());
        for transaction in &self.transactions {
            ids.push(transaction.id.to_string());
        }
        for event in &self.events {
            ids.push(event.id.clone());
        }
        for trace in &self.traces {
            ids.push(trace.id.clone());
        }
        BlockValidation {
            validation_hash: calc_validation_hash(&ids),
            is_fork: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DebankOutPut {
    pub block_file: BlockFile,
    pub header: WithOtherFields<Header>,
    pub state_diff: Bytes,
    pub validation_hash: i64,
}

trait DebankId {
    fn debank_id(&self) -> String;

    fn calculate_id(args: Vec<&str>) -> String {
        let mut hasher = Md5::new();
        for arg in args {
            hasher.update(arg.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

impl DebankId for DebankEvent {
    fn debank_id(&self) -> String {
        Self::calculate_id(vec![
            &self.parent_trace_id,
            &self.pos_in_parent_trace.to_string(),
        ])
    }
}

impl DebankId for DebankTrace {
    fn debank_id(&self) -> String {
        Self::calculate_id(vec![
            &self.tx_id.to_string(),
            &self.parent_trace_id,
            &self.pos_in_parent_trace.to_string(),
        ])
    }
}

fn calc_validation_hash(ids: &[String]) -> i64 {
    let mut sha1_sum = U256::ZERO;
    for id in ids {
        let mut hasher = Sha1::new();
        hasher.update(id.as_bytes());
        let hash_int = U256::from_str_radix(&hex::encode(hasher.finalize()), 16)
            .unwrap_or_else(|_| panic!("failed to convert id {id} to U256"));
        sha1_sum += hash_int;
    }
    let sha1_sum_str = sha1_sum.to_string();
    let last_6_digits = if sha1_sum_str.len() >= 6 {
        &sha1_sum_str[sha1_sum_str.len().saturating_sub(6)..]
    } else {
        &sha1_sum_str
    };
    last_6_digits.parse().unwrap_or(0)
}

impl From<&CallTraceNode> for DebankTrace {
    fn from(call_trace: &CallTraceNode) -> Self {
        let trace = &call_trace.trace;
        let call_create_type = match trace.kind {
            CallKind::Call
            | CallKind::StaticCall
            | CallKind::CallCode
            | CallKind::DelegateCall
            | CallKind::AuthCall => "call".to_string(),
            CallKind::Create | CallKind::EOFCreate => "create".to_string(),
            CallKind::Create2 => "create2".to_string(),
        };
        let call_type = if call_create_type == "call" {
            trace.kind.to_string().to_lowercase()
        } else {
            String::new()
        };
        let mut debank_trace = DebankTrace {
            from_addr: trace.caller,
            gas_limit: trace.gas_limit,
            input: trace.data.clone(),
            to_addr: trace.address,
            value: trace.value,
            gas_used: trace.gas_used,
            output: trace.output.clone(),
            call_create_type,
            call_type,
            subtraces: call_trace.children.len(),
            error: if trace.success {
                String::new()
            } else {
                format!("{:?}", trace.status)
            },
            ..Default::default()
        };
        for step in &trace.steps {
            if step.op == OpCode::SSTORE {
                debank_trace.self_storage_change = true;
                debank_trace.storage_change = true;
                break;
            }
        }
        debank_trace
    }
}

impl From<&CallLog> for DebankEvent {
    fn from(log: &CallLog) -> Self {
        let selector = log
            .raw_log
            .topics()
            .first()
            .map(|h| h.to_string())
            .unwrap_or_default();
        let topics = if log.raw_log.topics().len() > 1 {
            log.raw_log.topics()[1..]
                .iter()
                .map(|h| h.to_string())
                .collect()
        } else {
            vec![]
        };
        DebankEvent {
            selector,
            topics,
            data: log.raw_log.data.clone(),
            ..Default::default()
        }
    }
}

enum DebankTraceOrLog {
    Trace(DebankTraceNode),
    Log(DebankEvent),
}

struct DebankTraceNode {
    trace: DebankTrace,
    children: Vec<DebankTraceOrLog>,
    success: bool,
}

fn build_trace_node(
    tx_id: B256,
    parent_trace_id: String,
    pos_in_parent_trace: usize,
    node: &CallTraceNode,
    nodes: &[CallTraceNode],
    parent_success: bool,
    trace_address: Vec<usize>,
    log_index: &mut usize,
) -> DebankTraceNode {
    let mut debank_node = DebankTraceNode {
        trace: node.into(),
        children: Vec::new(),
        success: node.trace.success && parent_success,
    };
    debank_node.trace.trace_address = trace_address.clone();
    debank_node.trace.parent_trace_id = parent_trace_id;
    debank_node.trace.pos_in_parent_trace = pos_in_parent_trace;
    debank_node.trace.tx_id = tx_id;
    debank_node.trace.id = debank_node.trace.debank_id();

    let id = debank_node.trace.id.clone();
    let contract_id = node.execution_address();
    let mut child_trace_address = Vec::new();

    for pos in &node.ordering {
        match pos {
            TraceMemberOrder::Call(i) => {
                let child_node = &nodes[node.children[*i]];
                let mut child_address = trace_address.clone();
                child_address.push(*i);
                child_trace_address = child_address.clone();
                let child_trace = build_trace_node(
                    tx_id,
                    id.clone(),
                    debank_node.children.len(),
                    child_node,
                    nodes,
                    parent_success && debank_node.success,
                    child_address,
                    log_index,
                );
                if child_trace.trace.storage_change && child_node.trace.success {
                    debank_node.trace.storage_change = true;
                }
                debank_node
                    .children
                    .push(DebankTraceOrLog::Trace(child_trace));
            }
            TraceMemberOrder::Log(i) => {
                let mut child_event: DebankEvent = (&node.logs[*i]).into();
                child_event.pos_in_parent_trace = debank_node.children.len();
                child_event.contract_id = contract_id;
                child_event.parent_trace_id = id.clone();
                child_event.id = child_event.debank_id();
                child_event.idx = *log_index;
                if debank_node.success {
                    *log_index += 1;
                }
                debank_node
                    .children
                    .push(DebankTraceOrLog::Log(child_event));
            }
            TraceMemberOrder::Step(_) => {}
        }
    }

    if node.is_selfdestruct() {
        child_trace_address.last_mut().map(|last| *last += 1);
        debank_node.trace.subtraces += 1;
        let mut selfdestruct_trace = DebankTrace {
            from_addr: node.trace.selfdestruct_address.unwrap_or_default(),
            to_addr: node.trace.selfdestruct_refund_target.unwrap_or_default(),
            value: node
                .trace
                .selfdestruct_transferred_value
                .unwrap_or_default(),
            trace_address: child_trace_address,
            parent_trace_id: id.clone(),
            pos_in_parent_trace: debank_node.children.len(),
            tx_id,
            call_create_type: "suicide".to_string(),
            ..Default::default()
        };
        selfdestruct_trace.id = selfdestruct_trace.debank_id();
        debank_node
            .children
            .push(DebankTraceOrLog::Trace(DebankTraceNode {
                trace: selfdestruct_trace,
                children: vec![],
                success: parent_success && debank_node.success,
            }));
    }

    debank_node
}

fn finish_build_traces(
    node: &mut DebankTraceNode,
    traces: &mut Vec<DebankTrace>,
    error_traces: &mut Vec<DebankTrace>,
    events: &mut Vec<DebankEvent>,
    error_events: &mut Vec<DebankEvent>,
) {
    if node.success {
        traces.push(node.trace.clone());
    } else {
        error_traces.push(node.trace.clone());
    }

    for child in &mut node.children {
        match child {
            DebankTraceOrLog::Trace(trace) => {
                trace.trace.parent_trace_id = node.trace.id.clone();
                finish_build_traces(trace, traces, error_traces, events, error_events);
            }
            DebankTraceOrLog::Log(log) => {
                if node.success {
                    events.push(log.clone());
                } else {
                    error_events.push(log.clone());
                }
            }
        }
    }
}

pub fn build_debank_traces(
    tx_id: B256,
    traces: CallTraceArena,
    log_index: &std::cell::RefCell<usize>,
) -> (
    Vec<DebankTrace>,
    Vec<DebankTrace>,
    Vec<DebankEvent>,
    Vec<DebankEvent>,
) {
    let nodes = traces.into_nodes();
    if nodes.is_empty() {
        return (vec![], vec![], vec![], vec![]);
    }
    let mut top = build_trace_node(
        tx_id,
        String::new(),
        0,
        &nodes[0],
        &nodes,
        true,
        vec![],
        &mut log_index.borrow_mut(),
    );
    let mut traces = vec![];
    let mut error_traces = vec![];
    let mut events = vec![];
    let mut error_events = vec![];
    finish_build_traces(
        &mut top,
        &mut traces,
        &mut error_traces,
        &mut events,
        &mut error_events,
    );
    (traces, error_traces, events, error_events)
}

pub fn get_storage_contracts_from_changes(changes: &[(Address, Account)]) -> Vec<Address> {
    changes
        .iter()
        .filter_map(|(address, account)| {
            if account.changed_storage_slots().next().is_some() {
                Some(*address)
            } else {
                None
            }
        })
        .collect()
}

pub fn get_storage_contracts_from_genesis(accounts: &[AccountData]) -> Vec<Address> {
    accounts
        .iter()
        .filter_map(|account| {
            if account.storage.is_empty() {
                None
            } else {
                Some(account.address)
            }
        })
        .collect()
}

pub(crate) fn get_storage_diffs_from_changes<C: sov_modules_api::Context>(
    db: &mut EvmDb<'_, C>,
    changes: &[(Address, Account)],
) -> BlockStorageDiff {
    let mut new_accounts = Vec::new();
    let mut deleted_accounts = Vec::new();
    let mut storage_diffs = Vec::new();
    let mut new_codes = Vec::new();

    for (address, account) in changes {
        let prev_info = db.basic(*address).ok().flatten();

        if account.is_selfdestructed() {
            deleted_accounts.push(keccak256(address.0));
            continue;
        }

        let account_was_missing = prev_info.is_none();
        let current_code_hash = account.info.code_hash;
        let prev_code_hash = prev_info
            .as_ref()
            .map(|info| info.code_hash)
            .unwrap_or(KECCAK_EMPTY);

        if account_was_missing
            || prev_info.as_ref().is_some_and(|prev| {
                prev.balance != account.info.balance
                    || prev.nonce != account.info.nonce
                    || prev.code_hash != current_code_hash
            })
        {
            new_accounts.push(NewAccount {
                address: keccak256(address.0),
                balance: account.info.balance,
                nonce: account.info.nonce,
                code_hash: current_code_hash,
            });
        }

        let diffs = account
            .changed_storage_slots()
            .map(|(key, value)| IndexValuePair {
                index: keccak256::<[u8; 32]>(key.to_be_bytes()),
                value: value.present_value(),
            })
            .collect::<Vec<_>>();
        if !diffs.is_empty() {
            storage_diffs.push(AccountStorageDiff {
                address: keccak256(address.0),
                diffs,
            });
        }

        if current_code_hash != prev_code_hash {
            if let Some(code) = account.info.code.as_ref() {
                if !code.is_empty() {
                    new_codes.push(NewCode {
                        code_hash: current_code_hash,
                        code: code.original_bytes(),
                    });
                }
            }
        }
    }

    BlockStorageDiff {
        hash: B256::ZERO,
        parent_hash: B256::ZERO,
        new_accounts,
        deleted_accounts,
        storage_diffs,
        new_codes,
    }
}

pub fn account_changeset_from_state(state: &revm::state::EvmState) -> Vec<(Address, Account)> {
    state
        .iter()
        .map(|(address, account)| (*address, account.clone()))
        .collect()
}

pub fn header_from_sealed_block(block: &SealedBlock) -> WithOtherFields< Header> {
    let header = Header {
        inner: alloy_consensus::Header {
            parent_hash: block.header.parent_hash,
            ommers_hash: block.header.ommers_hash,
            beneficiary: block.header.beneficiary,
            state_root: block.header.state_root,
            transactions_root: block.header.transactions_root,
            receipts_root: block.header.receipts_root,
            logs_bloom: block.header.logs_bloom,
            difficulty: block.header.difficulty,
            number: block.header.number,
            gas_limit: block.header.gas_limit,
            gas_used: block.header.gas_used,
            timestamp: block.header.timestamp,
            extra_data: block.header.extra_data.clone(),
            mix_hash: block.header.mix_hash,
            nonce: block.header.nonce,
            base_fee_per_gas: block.header.base_fee_per_gas,
            withdrawals_root: block.header.withdrawals_root,
            blob_gas_used: block.header.blob_gas_used,
            excess_blob_gas: block.header.excess_blob_gas,
            parent_beacon_block_root: block.header.parent_beacon_block_root,
            requests_hash: block.header.requests_hash,
        },
        hash: block.header.hash(),
        total_difficulty: None,
        size: None,
    };
    WithOtherFields{
        inner: header,
        other: OtherFields::from_iter([(
            "l1FeeRate".to_string(),
            format!("{:#x}", block.l1_fee_rate).into(),
        )]),
    }
}

#[derive(Default)]
pub struct BlockStorageDiffBuilder {
    new_accounts: BTreeMap<B256, NewAccount>,
    deleted_accounts: BTreeSet<B256>,
    storage_diffs: BTreeMap<B256, BTreeMap<B256, U256>>,
    new_codes: BTreeMap<B256, NewCode>,
}

impl BlockStorageDiffBuilder {
    pub fn merge(&mut self, diff: BlockStorageDiff) {
        for account in diff.new_accounts {
            self.deleted_accounts.remove(&account.address);
            self.new_accounts.insert(account.address, account);
        }
        for deleted in diff.deleted_accounts {
            self.deleted_accounts.insert(deleted);
            self.new_accounts.remove(&deleted);
            self.storage_diffs.remove(&deleted);
        }
        for storage_diff in diff.storage_diffs {
            let entry = self.storage_diffs.entry(storage_diff.address).or_default();
            for pair in storage_diff.diffs {
                entry.insert(pair.index, pair.value);
            }
        }
        for code in diff.new_codes {
            self.new_codes.insert(code.code_hash, code);
        }
    }

    pub fn build(self, hash: B256, parent_hash: B256) -> BlockStorageDiff {
        BlockStorageDiff {
            hash,
            parent_hash,
            new_accounts: self.new_accounts.into_values().collect(),
            deleted_accounts: self.deleted_accounts.into_iter().collect(),
            storage_diffs: self
                .storage_diffs
                .into_iter()
                .map(|(address, diffs)| AccountStorageDiff {
                    address,
                    diffs: diffs
                        .into_iter()
                        .map(|(index, value)| IndexValuePair { index, value })
                        .collect(),
                })
                .collect(),
            new_codes: self.new_codes.into_values().collect(),
        }
    }
}
