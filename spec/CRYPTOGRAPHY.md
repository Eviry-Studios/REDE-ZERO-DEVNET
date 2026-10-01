# spec/CRYPTOGRAPHY.md — Criptografia

**Versão:** 0.1.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §3, §5, §9`, ADR-0003, THR-ID-002, THR-ID-003, THR-TX-004
**Implementação de referência:** `crates/rz-crypto`

---

## 1. Primitivas

| Função | Primitiva | Parâmetros |
| --- | --- | --- |
| Hash | BLAKE3, modo *derive-key* | saída de 32 bytes |
| Assinatura | Ed25519 (RFC 8032) | chave pública 32 bytes, assinatura 64 bytes |
| Entropia | gerador do sistema operacional | — |

## 2. Hash com separação de domínio

```text
H(ctx, dados) = BLAKE3_derive_key(context = ctx, key_material = dados)[0..32]
```

Todo hash protocolar **deve** usar um contexto da seção 3. O hash "puro" (sem contexto) não é usado pelo protocolo.

## 3. Contextos

| Constante | Valor | Uso |
| --- | --- | --- |
| `NODE_ID` | `rede-zero/node-id/v1` | Derivação do Node ID |
| `ADDRESS` | `rede-zero/address/v1` | Derivação do endereço de Wallet |
| `TX_ID` | `rede-zero/tx-id/v1` | Identificador de transação |
| `TX_SIGNATURE` | `rede-zero/tx-signature/v1` | Assinatura de transação |
| `BLOCK_ID` | `rede-zero/block-id/v1` | Identificador de bloco |
| `BLOCK_SIGNATURE` | `rede-zero/block-signature/v1` | Assinatura de bloco |
| `TX_ROOT` | `rede-zero/tx-root/v1` | Compromisso da lista de transações |
| `STATE_ROOT` | `rede-zero/state-root/v1` | Compromisso do estado |
| `GENESIS` | `rede-zero/genesis/v1` | Hash do Genesis |
| `VOTE` | `rede-zero/vote/v1` | Assinatura e identificador de voto Zero-BFT |
| `PROPOSAL` | `rede-zero/proposal/v1` | Assinatura e identificador de proposta Zero-BFT |
| `PROPOSER` | `rede-zero/proposer/v1` | Sorteio ponderado do proponente |
| `EVIDENCE` | `rede-zero/evidence/v1` | Identificador de infração punida |
| `ASSET_ID` | `rede-zero/asset/v1` | Identificador de ativo externo (`spec/MARKET.md §1`) |
| `MARKET_ROOT` | `rede-zero/market-root/v1` | Raiz da parte de mercado e Pool do estado |
| `COMMUNITY_APPROVAL` | `rede-zero/community/v1` | Aprovações da regra de decisão de uma Comunidade |
| `COMMUNITY_MANIFEST` | `rede-zero/community-manifest/v1` | Hash do manifesto de uma Comunidade |
| `COMMUNITY_ROOT` | `rede-zero/community-root/v1` | Raiz da parte de Comunidades e nomes do estado |
| `DEFENSE_ATTESTATION` | `rede-zero/defense/v1` | Atestações de validadores para decisões de defesa |
| `DEFENSE_ROOT` | `rede-zero/defense-root/v1` | Raiz da parte de defesa do estado |

Os contextos de consenso estão em `crates/rz-core/src/consensus.rs` (`spec/CONSENSUS.md`). Privacidade, governança e canal P2P usam contextos próprios, listados em `spec/PRIVACY.md`, `spec/GOVERNANCE.md` e `spec/P2P.md`.

Alterar o significado de um contexto exige novo sufixo de versão (`/v2`).

## 4. Identidades

```text
NodeId  = H(NODE_ID, chave_pública)
Address = H(ADDRESS, chave_pública)
```

A mesma chave produz identificadores diferentes nos dois papéis. O protocolo recomenda, ainda assim, chaves diferentes para Node e Wallet (`REQ-011`).

Nenhum identificador contém ou exige dado civil (`SPEC-ID-004`).

## 5. Mensagem assinada

A mensagem efetivamente assinada com Ed25519 é:

```text
M = string(ctx) ‖ string(network_id) ‖ payload
```

onde `string(x)` é a codificação canônica (`spec/ENCODING.md`, prefixo `u32` de comprimento) e `payload` é a codificação canônica do objeto sem a assinatura.

Consequências:

* uma assinatura de transação não é válida como assinatura de bloco (contexto);
* uma assinatura da DEVNET não é válida na TESTNET ou MAINNET (rede) — THR-ID-003;
* o prefixo de comprimento impede ambiguidade entre `ctx`, `network_id` e `payload`.

## 6. Validação de chaves públicas

Uma chave pública é válida se e somente se:

1. os 32 bytes decodificam um ponto válido da curva Edwards25519;
2. o ponto **não** é de ordem pequena.

Chaves inválidas são rejeitadas na decodificação (`InvalidValue`).

## 7. Verificação de assinatura

A verificação usa o modo **estrito**:

* `s` deve ser canônico (`s < ℓ`);
* `R` e a chave pública não podem ser de ordem pequena;
* a equação de verificação é a do RFC 8032 sem cofator.

Assinaturas que passam apenas na verificação permissiva são **inválidas** (THR-TX-004).

## 8. Vetores de teste

Semente: 32 bytes `0x01`.

| Item | Valor (hex) |
| --- | --- |
| seed | `0101010101010101010101010101010101010101010101010101010101010101` |
| chave pública | `8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c` |
| NodeId | `20a09f3a82f2ea8144e5b5fda01764841f287e572a210948d36cae69f034dd14` |
| Address | `3c5385626729bf35550029e68fc6ae6f88e24c542f5d63b3e24b2cd687dd1fb6` |
| `H(TX_ID, "")` | `669381f31efb09384e1c5ebbd279b748028bdbb3f99be8f9ee136030bbe0126e` |
| assinatura de `"zero"`, ctx `TX_SIGNATURE`, rede `rede-zero-devnet-1` | `b159c866eb087d49f04636c2b758fa08196053e1e6bd83b2f369bd9e99ebf72843864f6a2dcb73d18d2ce285bf01549b19f14562ab64e4caaf9af22448d2710a` |

Os vetores são verificados automaticamente em `crates/rz-crypto/src/keys.rs` (`published_vectors`) e podem ser impressos com:

```bash
cargo run -p rz-crypto --example vectors
```

## 9. Testes de aceitação cobertos

| Teste | Descrição |
| --- | --- |
| AT-ID-001 | Geração de identidade |
| AT-ID-002 | Determinismo do Node ID |
| AT-ID-003 | Chaves diferentes → identidades diferentes |
| AT-ID-004 | Assinatura válida |
| AT-ID-005 | Assinatura ou mensagem adulterada rejeitada |
| AT-ID-006 | Chave incorreta rejeitada |

## 10. Pendências

* Rotação de chaves (`SPEC-ID-005`) — **A DEFINIR**.
* Armazenamento cifrado de chaves — **A DEFINIR** (na DEVNET, chaves ficam em arquivo local com permissão restrita).
* Privacidade transacional — fora do escopo desta versão.
