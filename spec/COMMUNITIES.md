# spec/COMMUNITIES.md — Comunidades

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0015
**Relacionamento:** `SPECIFICATIONS.md §60–§62`, REQ-068..074, `docs/DEMOCRACIA_ORGANICA.md` N-1, N-5, N-6, N-7, AT-COM-001..004, THR-COM-001..003, INV-007
**Implementação de referência:** `crates/rz-core/src/community.rs`, `crates/rz-core/src/state.rs`

---

## 1. Registro

```text
DecisionRule { keys: list<PublicKey> (1..32, sem repetição), threshold: u8 (1..=len(keys)) }

Community {
  id            : fixed[32]     // TxId da DeclareCommunity
  name          : string        // spec/NAMING.md §1
  declarant     : Address
  manifest_hash : fixed[32]     // H("rede-zero/community-manifest/v1", manifesto)
  version       : u32           // começa em 1
  rule          : DecisionRule
  status        : u8            // 0 declarada, 1 reconhecida
  declared_at   : u64
  expires_at    : u64           // só relevante enquanto declarada
  history       : list<(u32 versão, fixed[32] manifesto, u64 altura)>  // ≤ 64, as mais recentes
}
```

O manifesto fica fora da cadeia (Manifesto §6). Ele descreve identidade, versão, componentes, regras e governança interna (`SPEC §60`); o hash na cadeia permite verificar qualquer cópia.

**Membros não são registrados** (N-5). As chaves de `rule` são pseudônimas e representam o mecanismo de decisão declarado. Como a Comunidade chega a uma decisão (voto direto, representantes, consenso) é escolha dela, dentro do protocolo (N-6).

## 2. Operações

| Tag | Operação | Campos | Regras |
| --- | --- | --- | --- |
| `0x40` | `DeclareCommunity` | `name, manifest_hash, rule` | nome válido e disponível (nem igual nem confundível com outra Comunidade, declarada ou reconhecida); regra válida; declarações pendentes `< max_pending`; `expires_at = h + declaration_ttl_blocks` |
| `0x41` | `CommunityPosition` | `community, proposal, choice, approvals` | Comunidade reconhecida; proposta pendente e em votação; aprovações válidas (§3) sobre `position_payload`; registra ou substitui a posição |
| `0x42` | `UpdateCommunity` | `community, manifest_hash, version, rule?, approvals` | Comunidade reconhecida; `version > atual`; aprovações pela regra **vigente** sobre `update_payload`; acrescenta ao histórico |

Qualquer conta pode enviar `CommunityPosition` e `UpdateCommunity` (paga a taxa): a autoridade vem das aprovações, não do remetente.

## 3. Aprovações

```text
Approval { key: PublicKey, signature: Signature }

position_payload = u8(1) ‖ community ‖ proposal ‖ u8(choice)
update_payload   = u8(2) ‖ community ‖ manifest_hash ‖ u32(version) ‖ option(rule)
assinatura       = Ed25519(ctx "rede-zero/community/v1" ‖ network_id ‖ payload)
```

Válido se toda aprovação é de uma chave da regra, não há chave repetida, toda assinatura confere e há pelo menos `threshold` chaves distintas.

## 4. Reconhecimento (N-7)

1. `DeclareCommunity` cria o registro com `status = declarada`.
2. `Propose` da categoria Comunidade com `content_hash = id` só é aceito se a declaração existe, está pendente e não venceu. Essa é a validação técnica. A validade da declaração é estendida até a ativação da proposta.
3. A apuração é a da governança (ADR-0008: bicameral, quórum de 5%, maioria simples).
4. Na ativação, `status = reconhecida`; o id também entra em `approved_communities`.

Declarações vencidas e não reconhecidas são removidas no fim do bloco, e o nome fica livre.

## 5. Posição comunitária (N-1)

`positions[(proposta, comunidade)] = escolha`. É registrada e verificável, e **não entra** na apuração oficial nem no quórum. Interfaces podem exibir a distribuição das posições (Democracia Orgânica §4) ao lado do resultado ponderado (N-2). A contagem considera apenas Comunidades reconhecidas.

## 6. Isolamento (`SPEC §61`)

As operações desta especificação só alteram:

* o registro da própria Comunidade (versão, manifesto, regra, histórico);
* `positions`;
* nomes (`spec/NAMING.md`), cuja taxa vai para o Pool.

Não existe operação de Comunidade sobre consenso, parâmetros, saldos de terceiros, regras monetárias ou outras Comunidades (AT-COM-003, INV-007). Remoção unilateral não existe (REQ-074); a remoção por governança está A DEFINIR.

## 7. Execução (fora do escopo desta versão)

O Exonet Runtime, que executa aplicações de Comunidades isoladas e em múltiplos Nodes (`SPEC §62`, Democracia Orgânica §9–§13), está A DEFINIR (`spec/RUNTIME.md`). O estado de uma Comunidade que a rede reconhece é o registro desta especificação, replicado pelo consenso: um Node não pode alterá-lo sozinho (hosting ≠ autoridade).

## 8. Parâmetros (`ProtocolParams.communities`)

| Campo | Padrão | Governança |
| --- | --- | --- |
| `name_fee` | 10 ZERO | `name_fee` (tag 22), constitucional |
| `declaration_ttl_blocks` | 1 296 000 (~30 dias com blocos de 2 s) | `community_declaration_ttl_blocks` (tag 23), constitucional |
| `max_pending` | 1 000 | — |

## 9. Raiz do estado

```text
community_root = H("rede-zero/community-root/v1",
    u64(n) ‖ Community*                        // em ordem de id
    u64(n) ‖ (u8 tipo ‖ string nome ‖ NameRecord)*   // em ordem de (tipo, nome)
    u64(n) ‖ (proposta ‖ comunidade ‖ u8 escolha)*)
```

Entra na raiz do estado depois de `market_root` (`spec/STATE.md §3`).

## 10. Consultas P2P

`GET_COMMUNITY` (0x1c) → `COMMUNITY` (0x1d); `RESOLVE` (0x1e) → `RESOLVED` (0x1f). Ver `spec/P2P.md`.

## 11. Testes de aceitação cobertos

AT-COM-001..003 (`crates/rz-core/src/governance_tests.rs`), AT-COM-004 e o fluxo completo com Nodes reais (`crates/rz-wallet/tests/community_devnet.rs`), regras (`community::tests`), robustez (`crates/rz-p2p/tests/robustness.rs`).
