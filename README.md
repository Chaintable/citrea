# Chaintable write node

> Fork of [chainwayxyz/citrea](https://github.com/chainwayxyz/citrea), with Chaintable pipeline patches.

## Architecture

This repo runs the chain's execution layer with the [Chaintable pipeline](https://github.com/Chaintable/pipeline) tracer embedded. The tracer extracts block data — block headers, transactions, call traces, receipts, events, and state diffs — and ships it to **S3 + Kafka** (see pipeline's [architecture](https://github.com/Chaintable/pipeline/blob/main/docs/architecture.md)). Two consumption paths:

- **Block headers + state diffs** → Kafka + S3 → [leafage-evm](https://github.com/Chaintable/leafage-evm): a lightweight EVM executor serving state queries (`eth_call`, `eth_estimateGas`, …), no P2P sync, no tx storage (see its [architecture](https://github.com/Chaintable/leafage-evm#architecture)).
- **Block files** (transactions · call traces · receipts · events) → S3 → Chaintable's transaction/trace indexing pipeline.

```
Chaintable write node (this repo · producer, embeds pipeline tracer)
        │
        ├─ block headers + state diffs ──────────────────→ Kafka + S3 ─→ leafage-evm (EVM state queries)
        │
        └─ block files (tx · trace · receipts · events) ──→ S3 ─→ Chaintable indexing pipeline (tx/trace data)
```

---

# Citrea

**The first rollup that enhances the capabilities of Bitcoin blockspace with zero-knowledge technology, now [live on Bitcoin Mainnet](https://www.blog.citrea.xyz/citrea-mainnet-is-live/)! 🎉🍊🍋**

![](resources/assets/banner.png)

> [!WARNING]
> Citrea uses **BTC** as its native token. **There's no Citrea token**. Please beware of scams! \
> \
> Follow our [website](https://citrea.xyz) & [social media accounts](https://twitter.com/citrea_xyz) for announcements regarding the next phases of Citrea.



## What is Citrea?

Citrea is the first rollup that enhances the capabilities of Bitcoin blockspace with zero-knowledge technology, **making it possible to build everything on Bitcoin**.

Every transaction occurring on Citrea, is fully secured by zero-knowledge proofs and optimistically verified by Bitcoin via BitVM. The execution environment of Citrea is trustless with respect to Bitcoin and is accessible to all participants of the Bitcoin Network.

Citrea's vision is to build scalable infrastructure that advances Bitcoin into its next phase, the foundation for world's finance. Citrea represents **Bitcoin Security at Scale** with its execution shard that keeps the settlement and data availability on-chain, on-Bitcoin.

## Audits
Citrea completed a private audit with [Sigma Prime](https://sigmaprime.io/), find the audit report here: [Sigma Prime Audit Report](./audits/Sigma_Prime_Chainway_Citrea_Security_Assessment_Report_v2_2.pdf) 

Citrea also held a public bug bounty competition on [Cantina](https://cantina.xyz/), find the report here: [Cantina Competition Report](./audits/cantina_competition_citrea_jul2025.pdf)

To see all past audits, visit: [Audits & Security](https://docs.citrea.xyz/security/audits-inquiries)

## Bug Bounty
Citrea maintains active bug bounty programs. Before diving into the codebase, researchers should review the [Auditor's Guide](docs/auditors-guide.md), then head to the [Citrea Active Bug Bounties](https://docs.citrea.xyz/security/audits-inquiries#active-bug-bounties) to see the currently available bug bounties.

## FAQ

| Question                                         | Answer                                                                                                                      |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| How do I set up the development environment?     | [dev-setup.md](./docs/dev-setup.md)                                                                                         |
| How do I run Citrea for testing and development? | [run-dev.md](./docs/run-dev.md)                                                                                                     |
| Where can I read more about the architecture?    | Go to [SUMMARY.md](./docs/SUMMARY.md) and navigate from there to the subject you want to read about, or refer to [our official documentation](https://docs.citrea.xyz). |
| How do I run a Citrea mainnet node?              | [run-mainnet.md](./docs/run-mainnet.md) |
| How do I bridge to Citrea?                       | Follow our guide on [bridging to Citrea](https://docs.citrea.xyz/welcome/bridge-to-citrea) |

## Official Links

- [Website](https://citrea.xyz)
- [Docs](https://docs.citrea.xyz)
- [Blog](https://blog.citrea.xyz)
- [X](https://x.com/citrea_xyz)
- [Discord](https://discord.gg/citrea)

## Acknowledgments

- [Sovereign SDK](https://github.com/Sovereign-Labs/sovereign-sdk): Citrea is built on a forked version of the Sovereign SDK. We're deeply thankful to the development & support of Sovereign Labs through our journey.
- [Reth](https://github.com/paradigmxyz/reth): We use Reth crates in various components in Citrea. We're grateful for their rapid development & contribution to the Rust-Ethereum ecosystem.
