# AUDIT.md — Preparação para auditoria independente

**Versão:** 0.3.0
**Estado:** pronto para auditoria da DEVNET; nenhuma auditoria externa foi realizada
**Relacionamento:** `THREAT_MODEL.md §11` (aceitações temporárias), ADR-0009, ADR-0011, ADR-0012, `SECURITY.md`

---

## 1. Objetivo

Este documento reúne o necessário para uma auditoria independente da implementação de referência: escopo e prioridades, fronteiras de confiança, invariantes a atacar, como reproduzir, e o resultado da revisão interna já feita.

Três aceitações temporárias do modelo de ameaças dependem de auditoria para deixar a DEVNET:

| Aceitação | Componente | Decisão |
| --- | --- | --- |
| THR-PRIV-001 | RingCT (`rz-privacy`, `rz-core/src/private.rs`) | ADR-0009 |
| THR-PRIV-002, THR-P2P-005 | canal cifrado, Tor, Dandelion++ (`rz-p2p`, `rz-node`) | ADR-0010, ADR-0011 |
| THR-CON-001/002 | consenso Zero-BFT (`rz-core/src/consensus.rs`, `rz-chain`) | ADR-0012 |

A revisão interna (§7) **não substitui** a auditoria externa.

## 2. Escopo e prioridade

| Prioridade | Componente | Arquivos | Linhas¹ | O que olhar primeiro |
| --- | --- | --- | --- | --- |
| 1 | Privacidade transacional | `crates/rz-privacy/src/*`, `crates/rz-core/src/private.rs` | ~1 000 + ~450 | CLSAG (vínculo de imagem de chave, desafios agregados), hash-to-point, compromissos de Pedersen e geradores, integração dos Bulletproofs (64 bits), prova de excesso, contabilidade da oferta privada |
| 2 | Consenso Zero-BFT | `crates/rz-core/src/consensus.rs`, `crates/rz-chain/src/bft.rs`, `crates/rz-chain/src/chain.rs` | ~2 400 | regras de trava e rodada, verificação de `Commit`, sorteio do proponente, recomputação do conjunto na época, punição |
| 3 | Transição de estado e ZERO | `crates/rz-core/src/state.rs`, `tx.rs`, `block.rs`, `governance.rs` | ~4 000 | invariante monetária, aritmética, nonce/repetição, determinismo, raiz do estado |
| 4 | Canal cifrado e proteção de IP | `crates/rz-p2p/src/secure.rs`, `socks.rs`, `client.rs` | ~700 | handshake (X25519, assinatura do respondedor), derivação de chaves, nonces, preenchimento, SOCKS5 com DNS remoto |
| 5 | Codificação canônica | `crates/rz-codec`, `Decode` de todos os tipos | ~400 | unicidade de codificação, limites antes de alocar |
| 6 | Superfície de negação de serviço do Node | `crates/rz-node/src/node.rs`, `rz-p2p/src/ratelimit.rs`, `score.rs`, `rz-chain/src/mempool.rs` | ~2 300 | limites de fila, mempool, Dandelion++, sincronização |
| 7 | Grande Mercado e Pool | `crates/rz-core/src/market.rs`, partes de mercado em `state.rs` | ~1 300 | leilão (preço, prioridade, arredondamento, poeira), reservas e reembolsos, conservação por ativo, Pool monotônico, limites do livro |
| 8 | Comunidades e nomes | `crates/rz-core/src/community.rs`, partes em `state.rs` | ~900 | verificação de aprovações por limiar, reconhecimento via governança, isolamento, esqueletos de nomes |
| 9 | Wallet | `crates/rz-wallet` | ~1 200 | seleção de notas, recusa de conexão direta, manuseio de chaves, unidades e preços do mercado |

¹ Aproximado, incluindo testes internos aos arquivos.

**Fora do escopo:** pontes de ativos externos (não existem; ADR-0014 §4), Comunidades, Defesa, Agente Zero, Navegador Zero (ainda não implementados), e a política monetária (A DEFINIR).

## 3. Arquitetura e fronteiras de confiança

```text
          Wallet (chaves locais)                 Node
   ┌───────────────────────────┐     ┌──────────────────────────────────────┐
   │ rz-wallet                  │     │ rz-node                              │
   │  assina localmente         │ TCP │  handshake → HELLO → mensagens       │
   │  varre notas localmente    │────▶│  limite de taxa, pontuação, banimento│
   └───────────────────────────┘ (1) │  mempool ─▶ Dandelion++ (haste/fluff)│
                                      │  consenso (thread própria, fila      │
        outros Nodes ◀──────────(2)──▶│   limitada) ─▶ rz-chain::Bft         │
                                      │  Chain::commit ─▶ rz-core (estado)   │
                                      │  disco: blocos com certificado   (3) │
                                      └──────────────────────────────────────┘
```

| Fronteira | Entrada não confiável | Defesa |
| --- | --- | --- |
| (1) Wallet → Node | transações, consultas | canal cifrado; o Node não conhece chaves; a Wallet baixa **todas** as saídas e varre localmente; transações privadas não revelam remetente, destinatário ou valor |
| (2) Node ↔ Node | qualquer mensagem P2P | limite de quadro (4 MiB) antes de ler; decodificação canônica com limites; limite de taxa; pontuação e quarentena; validação completa antes de repassar; blocos só com `Commit` verificado |
| (3) Disco | arquivos de blocos | reprocessados por `Chain::commit` ao iniciar (certificado e regras completas); entradas inválidas descartadas |
| Governança | parâmetros aprovados | validados após aplicados (`ProtocolParams::validate`), com limites superiores para temporizadores |

O relógio local só controla temporizadores de rodada e Dandelion++; **nenhuma regra de validade usa relógio** (THR-CON-006).

## 4. Invariantes a atacar

Cada invariante lista onde é aplicada e os testes que a exercitam. Uma violação de qualquer uma é, no mínimo, severidade alta.

| # | Invariante | Onde | Testes |
| --- | --- | --- | --- |
| INV-1 | `Σ saldos + oferta privada + bloqueios + depósitos + vínculos + desvinculações = total_supply` após cada bloco; só a punição reduz `total_supply` | `State::check_supply`, `block::execute` | AT-MONEY-*, `governance_tests`, `extreme_signed_values_never_panic` |
| INV-2 | Oferta privada nunca negativa; nenhuma transação privada cria valor | `plan_private`, prova de excesso, Bulletproofs | `rz-core private::tests`, `shield_with_inflated_amount_rejected` |
| INV-3 | Uma nota só é gasta uma vez (imagem de chave única, ligada ao anel) | CLSAG, `key_images` | `private_transfer_end_to_end` (AT-DS-001 privado) |
| INV-4 | Toda codificação aceita é a única possível (sem maleabilidade) | `rz-codec`, `Decode` | `rz-p2p/tests/robustness.rs` (canonicidade sob mutação) |
| INV-5 | Nenhuma entrada de rede provoca pânico ou alocação proporcional ao tamanho declarado | decodificadores, `read_frame`, `SecureReader` | `robustness.rs`, AT-P2P-003/004 |
| INV-6 | Dois blocos diferentes nunca são finalizados na mesma altura com < 1/3 do poder bizantino | `bft.rs`, `Commit::verify` | `bft_tests` (voto duplo, partição) |
| INV-7 | Um bloco só entra na cadeia com pré-compromissos de > 2/3 do poder do conjunto vigente | `Chain::commit` | AT-CON-002, `malicious_blocks_rejected_and_peer_banned` |
| INV-8 | A mesma infração é punida uma única vez; punição exige evidência com assinaturas válidas | `evidence_id`, `punished` | `double_vote_slashes_bond_and_jails`, `equivocation_report_slashes_contribution` |
| INV-9 | Assinaturas nunca valem em outro contexto ou outra rede | `ctx ‖ network_id ‖ payload` | AT-ID-004..006, `wrong_genesis_rejected` |
| INV-10 | A transição de estado é determinística | sem relógio, aleatoriedade ou ordem indefinida | AT-DET-* |
| INV-11 | Mensagens de consenso hostis não crescem a memória sem limite nem são amplificadas | `FUTURE_ROUND_WINDOW`, `keep_future_vote`, fila limitada | `bft_tests` (limites de memória) |
| INV-12 | Todo bloco válido cabe num quadro P2P | `MAX_BLOCK_TX_BYTES` | `oversized_block_rejected` |
| INV-13 | Com < 1/3 do poder bizantino, a rede volta a finalizar blocos após a estabilização | `bft.rs` (votos conflitantes, gossip da prova de trava) | `randomized_adversarial_*` |
| INV-14 | O Pool nunca diminui; não existe operação de saída | `state.rs` (`add_to_pool` é o único ponto de escrita) | AT-POOL-002/003, `randomized_market_invariants` |
| INV-15 | Cada ativo externo se conserva: saldos + reservas de venda + Pool = oferta | `MarketState::check_assets` | `randomized_market_invariants` |
| INV-16 | O resultado do leilão não depende da ordem das transações no bloco | `market::clear` | `block_order_does_not_change_market_outcome`, `result_independent_of_input_order` |
| INV-17 | Manifestações de Comunidade exigem o limiar da regra declarada; nenhuma operação de Comunidade altera consenso, saldos de terceiros ou outras Comunidades | `DecisionRule::verify`, `state.rs` | AT-COM-003, `decision_rule_threshold` |
| INV-18 | A posição comunitária não altera a apuração oficial | `tally` ignora `positions` | `community_position_verifiable_and_non_binding` |

## 5. Como reproduzir

```bash
# Tudo o que a CI roda (Rust estável; a CI usa a versão estável mais recente)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p rz-crypto --example vectors     # vetores publicados

# Robustez por mutação com mais iterações e outras sementes
RZ_FUZZ_ITERS=20000 RZ_FUZZ_SEED=7 cargo test --release -p rz-p2p --test robustness

# Simulação adversarial do consenso com mais sementes
RZ_BFT_SEEDS=400 cargo test --release -p rz-chain randomized

# Propriedades do Grande Mercado com mais sementes
RZ_MARKET_SEEDS=300 cargo test --release -p rz-core randomized_market

# DEVNET local com 4 validadores (tolera 1 falha)
scripts/devnet.sh 4
```

Os testes de robustez (`crates/rz-p2p/tests/robustness.rs`) não dependem de ferramentas externas. Partindo de um corpus com **todos** os tipos de mensagem e transação (inclusive privadas, votos, propostas e blocos com certificado), eles aplicam mutações e verificam INV-4 e INV-5. Mutantes que ainda decodificam passam pela verificação completa do estado e devem ser rejeitados. Transações assinadas com valores extremos exercitam a aritmética.

Harnesses para `cargo-fuzz`/libFuzzer podem ser construídos sobre as mesmas funções (`Message::from_canonical_bytes`, `State::check_transaction`, `apply_block`, `SecureReader::read_message`); ainda não fazem parte do repositório.

## 6. Dependências criptográficas

| Crate | Versão | Uso |
| --- | --- | --- |
| `ed25519-dalek` | 2.2.0 | assinaturas (verificação estrita) |
| `blake3` | 1.8.7 | hash com separação de domínio (`derive_key`) |
| `curve25519-dalek` | 4.1.3 | Ristretto255 (notas, CLSAG, Pedersen) |
| `bulletproofs` | 5.0.0 | provas de faixa de 64 bits |
| `merlin` | 3.0.0 | transcrições das provas |
| `x25519-dalek` | 2.0.1 | troca de chaves do canal |
| `chacha20poly1305` | 0.10.1 | cifragem autenticada do canal |
| `rand_core` | 0.6.4 | `OsRng` |

Versões exatas em `Cargo.lock` (76 pacotes no total). O código do projeto proíbe `unsafe` (`unsafe_code = "forbid"`). Builds de release mantêm `overflow-checks`.

## 7. Revisão interna de segurança

Feita antes da auditoria, com foco em negação de serviço, aritmética com entradas assinadas pelo atacante e canonicidade. Severidade segundo impacto na DEVNET com projeção para uma rede pública.

### 7.1 Achados corrigidos

| ID | Severidade | Componente | Achado | Correção |
| --- | --- | --- | --- | --- |
| RZ-IR-01 | Alta | Consenso + governança | Temporizadores e intervalo de bloco sem limite superior. Uma proposta constitucional aprovada com valor gigante faria todos os Nodes entrarem em pânico ao somar o prazo ao relógio. | `MAX_TIME_PARAM_MS` (10 min) em `ConsensusParams::validate`; soma de prazos sem estouro no Node (`after_ms`) |
| RZ-IR-02 | Média | `bft.rs` | Votos e propostas da altura seguinte eram guardados (até 4 096) e **repassados** conferindo só a assinatura, de qualquer chave. Havia amplificação pela rede e expulsão de mensagens legítimas. | Só validadores do conjunto vigente, só rodadas `≤ FUTURE_ROUND_WINDOW` |
| RZ-IR-03 | Média | `bft.rs` | Um validador bizantino podia assinar votos e propostas para rodadas arbitrárias; cada um ficava em memória e era repassado. | Propostas até `rodada + FUTURE_ROUND_WINDOW`; para rodadas futuras, só o voto mais alto de cada validador e tipo (preserva o salto de rodada com > 1/3) |
| RZ-IR-04 | Média | `node.rs` | A deduplicação marcava mensagens como vistas antes de o consenso aceitá-las. Uma mensagem descartada nunca era reconsiderada, nem quando retransmitida. A fila para a thread de consenso não tinha limite. | Marcadas só quando aceitas; fila limitada (8 192) com descarte; altura avança também por verificação periódica da cadeia |
| RZ-IR-05 | Média | `block.rs`, `mempool.rs` | Sem limite de bytes por bloco: um bloco com muitas transações privadas passava de 4 MiB e não podia ser proposto nem sincronizado. | Regra de consenso `MAX_BLOCK_TX_BYTES` (2 MiB); a seleção do mempool respeita o limite |
| RZ-IR-06 | Média | `rz-node` | O limite de taxa por conexão (400/200 msg/s) não escalava com o número de validadores: Nodes honestos podiam ser banidos por repassar votos. | Balde próprio para propostas e votos, `max(50, 8·n)`/s e rajada `max(200, 32·n)`; excedente descartado antes da verificação, **sem banimento** (`consensus_flood_dropped_without_ban`) |
| RZ-IR-07 | Baixa | `bft.rs`, `tx.rs` | Equivocação de proposta era reportada como dois cabeçalhos; numa re-proposta o bloco é de outro proponente, e a evidência não era aceita. | `ReportDoubleProposal` (0x23) com dois `SignedProposal` compactos; mesmo identificador de infração da evidência de cabeçalho (sem punição dupla) |
| RZ-IR-13 | **Alta** | `bft.rs` | **Vivacidade:** o voto conflitante de um bizantino era descartado. Enviando votos diferentes a cada metade da rede, um único bizantino impedia que metade dos honestos completasse a prova de > 2/3 de um bloco travado, e a rede parava para sempre, mesmo após a estabilização. Encontrado pela simulação adversarial (RZ-IR-12). | Até dois votos distintos por validador, rodada e tipo, cada um contando para o seu bloco (a contagem "qualquer voto" conta o validador uma vez). Segurança preservada pela interseção de quóruns |
| RZ-IR-14 | Média | `bft.rs` | **Vivacidade:** a retransmissão só reenviava votos da rodada atual. Pré-votos da rodada de trava perdidos antes da estabilização nunca eram recuperados, e a re-proposta com `pol_round` não era aceita. | A retransmissão inclui os pré-votos que provam o bloco válido (≤ n votos), como o gossip do Tendermint |

### 7.2 Achados abertos ou aceitos

| ID | Severidade | Componente | Achado | Situação |
| --- | --- | --- | --- | --- |
| RZ-IR-08 | Média | Consenso | Ataque de longo alcance com chaves já desvinculadas: um Node novo não distingue histórico alternativo. | **Mitigado** — pontos de verificação do operador (`--checkpoint ALTURA:ID`, subjetividade fraca): o Node recusa cadeias divergentes. O protocolo não embute checkpoints (sem autoridade central, REQ-005); a distribuição social deles fica com as Comunidades |
| RZ-IR-09 | Baixa | Consenso | Evidência só é punível enquanto houver vínculo ou desvinculação pendente. | **Por projeto** — o período de desvinculação (≈ 21 dias) define a janela |
| RZ-IR-10 | Média | Governança, consenso | Votos, bloqueios e vínculos são públicos e ligados a contas transparentes. | **Aceito** (ADR-0008, ADR-0012) — votação privada é pesquisa futura |
| RZ-IR-11 | Informativa | `rz-crypto/keyfile` | Chaves em arquivo sem cifragem (permissão `0600`). | **Aceito** na DEVNET |
| RZ-IR-12 | Informativa | Consenso | A máquina de estados não tem prova formal nem verificação de modelo. | **Parcial** — simulação adversarial aleatória (rede assíncrona com reordenação e perdas até a estabilização; bizantinos que equivocam propostas e votos para metades da rede). Com 800 cenários, segurança e vivacidade pós-estabilização se mantiveram; a simulação encontrou RZ-IR-13 e RZ-IR-14. Modelo TLA+/Apalache ainda recomendado |

## 8. Perguntas para a auditoria

1. **CLSAG e imagens de chave:** a construção em `rz-privacy/src/clsag.rs` vincula a imagem de chave à posição real no anel sem permitir imagens relacionadas (ataques do tipo "imagem de chave com torção")? O hash-to-point de `rz-privacy/src/hash.rs` é seguro para esse uso?
2. **Oferta privada:** existe sequência de `Shield`, transferências privadas e retiradas que crie valor ou deixe `shielded_supply` inconsistente?
3. **Bulletproofs:** os geradores e a transcrição (`merlin`) estão separados por domínio do resto do protocolo? Há reutilização de geradores Pedersen entre compromissos e provas que permita falsificação?
4. **Canal cifrado:** o handshake (responder autenticado, iniciador anônimo) resiste a interceptação quando a identidade é fixada (`--node-id`)? A contagem de nonces e o preenchimento de 256 bytes estão corretos?
5. **Zero-BFT:** as regras de trava/`pol_round` em `bft.rs` preservam segurança com a janela de rodadas futuras (RZ-IR-03) e com a contagem de votos conflitantes (RZ-IR-13)? A recomputação do conjunto na época e a punição imediata podem ser combinadas para quebrar INV-6?
6. **Grande Mercado:** o arredondamento do leilão (pagamento mínimo de 1 unidade para pares de valor nulo, reserva por teto) preserva a conservação em todos os casos? Há sequência de ordens que faça o leilão escolher um preço fora dos limites de alguma ordem executada?
7. **Determinismo:** algum caminho da transição de estado depende de ordem de `HashMap` ou de arredondamento?

## 9. Contato

Vulnerabilidades devem ser reportadas pelo relato privado do GitHub, conforme `SECURITY.md`. Achados de auditoria podem ser publicados após a correção, com crédito.
