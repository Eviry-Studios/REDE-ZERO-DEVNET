# Changelog

Todas as alterações relevantes do projeto (`REQ-086`, `REQ-094`). O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Adicionado

* `docs/THREAT_MODEL.md` v0.1.0, completando a cadeia documental.
* Registros de decisão (ADR-0001 a ADR-0007) com escopo explícito.
* `SECURITY.md`, `CONTRIBUTING.md`.
* Especificações DEVNET: `spec/ENCODING.md`, `CRYPTOGRAPHY.md`, `TRANSACTIONS.md`, `STATE.md`, `BLOCKS.md`, `CONSENSUS.md`, `P2P.md`.
* Implementação de referência em Rust:
  * `rz-codec` — codificação canônica estrita;
  * `rz-crypto` — BLAKE3 com separação de domínio, Ed25519 estrito, identidades, arquivo de chave;
  * `rz-core` — transações, estado, ZERO, blocos, Genesis;
  * `rz-chain` — interface de consenso, autoridade rotativa (DEVNET), escolha de fork, finalidade, mempool;
  * `rz-p2p` — mensagens, enquadramento, handshake, limite de taxa, pontuação, cliente;
  * `rz-node` — node com sincronização, produção de blocos e persistência;
  * `rz-wallet` — wallet de linha de comando.
* `scripts/devnet.sh` para DEVNET local.

### Protocolo

* `PROTOCOL_VERSION = 1`, `P2P_VERSION = 1` (DEVNET, não congelados).
