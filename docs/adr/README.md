# Registros de Decisão de Arquitetura (ADR)

Este diretório registra as decisões técnicas da Rede Zero, conforme `REQ-079` (histórico das decisões) e `REQ-086` (alterações documentadas).

Cada ADR descreve **contexto**, **decisão**, **alternativas consideradas**, **consequências** e **escopo** (DEVNET, TESTNET ou MAINNET).

Uma decisão com escopo `DEVNET` **não** vale automaticamente para outros ambientes: ela precisa de nova avaliação antes de ser herdada (ver `THREAT_MODEL.md §11`).

## Estados

* **Proposta** — em discussão;
* **Aceita** — em vigor no escopo indicado;
* **Substituída** — trocada por outra ADR (indicada no documento);
* **Rejeitada** — registrada para histórico.

## Índice

| ADR | Título | Escopo | Estado |
| --- | --- | --- | --- |
| [0001](0001-registro-de-decisoes.md) | Registro de decisões | Projeto | Aceita |
| [0002](0002-linguagem-rust.md) | Rust como linguagem da implementação de referência | DEVNET | Aceita |
| [0003](0003-criptografia.md) | Ed25519 e BLAKE3 | DEVNET | Aceita |
| [0004](0004-codificacao-canonica.md) | Codificação canônica binária própria | DEVNET | Aceita |
| [0005](0005-modelo-de-contas.md) | Modelo de contas com nonce e unidade mínima do ZERO | DEVNET | Aceita |
| [0006](0006-consenso-devnet.md) | Consenso da DEVNET: autoridade rotativa | DEVNET apenas | Substituída por 0012 |
| [0007](0007-p2p-devnet.md) | P2P da DEVNET sobre TCP | DEVNET | Aceita |
| [0008](0008-governanca-bicameral.md) | Governança bicameral com compromisso temporal | DEVNET → TESTNET | Aceita |
| [0009](0009-privacidade-transacional.md) | Privacidade transacional: RingCT sobre Ristretto255 | DEVNET | Aceita (auditoria pendente) |
| [0010](0010-privacidade-de-rede.md) | Privacidade de rede: Dandelion++ | DEVNET | Aceita |
| [0011](0011-protecao-do-ip.md) | Proteção do endereço IP do usuário | DEVNET | Aceita |
| [0012](0012-consenso-zero-bft.md) | Consenso Zero-BFT | DEVNET → TESTNET | Aceita (auditoria pendente) |
| [0013](0013-democracia-organica.md) | Democracia Orgânica: incorporação com notas de conformidade | Projeto | Aceita |
| [0014](0014-grande-mercado-e-pool.md) | Grande Mercado por leilão de preço uniforme e Pool permanente sem saída | DEVNET → TESTNET | Aceita |

## Modelo

Copie `template.md` e numere sequencialmente.
