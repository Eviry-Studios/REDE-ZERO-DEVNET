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

### Adicionado — privacidade e governança

* Licença MIT.
* Proposta de governança bicameral (ADR-0008, `spec/GOVERNANCE.md`).
* Privacidade transacional RingCT sobre Ristretto255 (ADR-0009, `spec/PRIVACY.md`):
  * crate `rz-privacy` — endereços furtivos, compromissos de Pedersen, Bulletproofs, CLSAG, prova de excesso;
  * transações privadas, operação `Shield`, retirada (`unshield`), controle público da oferta privada;
  * Wallet privada por padrão com varredura local de notas.
* Privacidade de rede Dandelion++ (ADR-0010).
* `THREAT_MODEL.md` v0.2.0.

### Adicionado — governança e proteção de IP

* Governança bicameral implementada (ADR-0008 aceita): `LockStake`, `Unlock`, `Propose`, `Vote`, `ReportEquivocation`; parâmetros do protocolo no estado; apuração nas duas câmaras; depósito devolvido ou queimado; comandos de governança na Wallet.
* Proteção do IP do usuário (ADR-0011): canal cifrado e autenticado com preenchimento; Tor/I2P via SOCKS5 com endereços `.onion`; Node privado (`--no-listen`); Wallet recusa conexão direta a Node remoto sem consentimento; logs sem endereços de clientes.
* `THREAT_MODEL.md` v0.3.0.

### Protocolo

* `PROTOCOL_VERSION = 1`, `P2P_VERSION = 2` (DEVNET, não congelados). O Genesis passou a incluir parâmetros de governança.
* **Incompatível** com blocos anteriores à camada privada: `Transaction` passou a ter tag de tipo, e a raiz do estado inclui a parte privada. DEVNETs antigas devem ser recriadas (`rm -rf devnet-data`).
