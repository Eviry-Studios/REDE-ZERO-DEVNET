# spec/P2P.md — Protocolo P2P da DEVNET

**Versão:** 0.1.0 (DEVNET), `P2P_VERSION = 1`
**Relacionamento:** `SPECIFICATIONS.md §23–§30`, ADR-0007, THR-P2P-001..006, THR-PRIV-002..004
**Implementação de referência:** `crates/rz-p2p` (mensagens) e `crates/rz-node` (orquestração)

---

## 1. Transporte

TCP. Cada mensagem é enviada como:

```text
u32 comprimento (big-endian) ‖ enc(Message)
```

`comprimento ≤ 4 MiB`. O comprimento é verificado **antes** de ler o corpo; mensagens maiores encerram a conexão (`AT-P2P-004`).

O canal **não é cifrado** na DEVNET (THR-P2P-005, aceitação temporária). A integridade dos dados protocolares vem das assinaturas.

## 2. Mensagens

| Tag | Mensagem | Conteúdo |
| --- | --- | --- |
| `0x00` | `HELLO` | `u16 p2p_version, string network_id, fixed[32] genesis, u64 height, u16 listen_port` |
| `0x01` | `PING` | `u64 nonce` |
| `0x02` | `PONG` | `u64 nonce` |
| `0x03` | `GET_PEERS` | — |
| `0x04` | `PEERS` | `list<PeerAddr>` (máx. 32) |
| `0x05` | `TRANSACTION` | `Transaction` |
| `0x06` | `BLOCK` | `Block` |
| `0x07` | `GET_BLOCKS` | `u64 from_height, u32 max` |
| `0x08` | `BLOCKS` | `list<Block>` (máx. 64) |
| `0x09` | `GET_ACCOUNT` | `Address` |
| `0x0a` | `ACCOUNT` | `Address, u64 balance, u64 nonce, u64 height` |
| `0x0b` | `TX_RESULT` | `TxId, bool accepted, string reason` (máx. 256) |
| `0x0c` | `GET_STATUS` | — |
| `0x0d` | `STATUS` | `u64 height, BlockId tip, u64 finalized_height, u32 peers, u32 mempool` |
| `0x0e` | `REJECT` | `string reason` (máx. 256), seguido de encerramento |
| `0x0f` | `GET_OUTPUTS` | `u64 from, u32 max` |
| `0x10` | `OUTPUTS` | `u64 start, list<OutputData>` (máx. 4096) |
| `0x11` | `GET_KEY_IMAGES` | `u64 from, u32 max` |
| `0x12` | `KEY_IMAGES` | `u64 start, list<fixed[32]>` (máx. 8192) |
| `0x13` | `STEM_TRANSACTION` | `Transaction` em fase de haste (seção 4) |

`PeerAddr = fixed[16] (IPv6; IPv4 mapeado) ‖ u16 porta`.

Tags desconhecidas são mensagens inválidas (`AT-P2P-003`).

## 3. Handshake

1. Ao conectar, **ambos** os lados enviam `HELLO` como primeira mensagem.
2. Cada lado verifica o `HELLO` recebido:
   * `p2p_version` igual;
   * `network_id` igual;
   * `genesis` igual ao hash do Genesis local.
3. Em caso de divergência, envia `REJECT` com o motivo e encerra (`AT-P2P-002`, `AT-GEN-002`).
4. Qualquer mensagem diferente de `HELLO` antes do handshake é violação de protocolo.
5. O handshake deve completar em até 10 segundos.

O `HELLO` não contém versão de software, sistema operacional, fuso horário, idioma nem identificadores persistentes (THR-PRIV-003). `listen_port = 0` indica um cliente que não aceita conexões (ex.: Wallet).

## 4. Propagação

### 4.1 Transações — Dandelion++ (ADR-0010)

```text
Wallet ──TRANSACTION──► Node A ──STEM──► Node B ──STEM──► … ──(1/10)──► FLOR: mempool + TRANSACTION para todos
                         │                 │
                         └─ embargo 10–20 s: se não houver flor, difunde
```

1. `TRANSACTION` vinda de um **cliente** (`listen_port = 0`): o Node valida a transação contra a ponta, responde `TX_RESULT` e a encaminha em **haste** (`STEM_TRANSACTION`) ao relay da época, sem inseri-la no mempool.
2. `STEM_TRANSACTION` recebida: se já estiver no mempool, é ignorada; se já estiver na haste local (ciclo), é difundida imediatamente; caso contrário é validada e, com probabilidade 1/10, difundida; senão, encaminhada ao relay da época (excluindo quem a enviou).
3. Relay da época: escolhido aleatoriamente entre os pares que aceitam conexões, mantido por 10 minutos ou até desconectar.
4. Sem relay disponível, a transação é difundida.
5. Embargo: cada Node da haste difunde a transação se ela não aparecer em fase de flor em 10–20 s (prazo aleatório).
6. `TRANSACTION` vinda de um **par** é fase de flor: entra no mempool e é difundida aos demais pares.

Transações em haste devem ser executáveis sobre a ponta atual (nonce igual ao da conta, notas não gastas).

### 4.2 Blocos

* Um bloco é repassado somente se foi importado com sucesso pela primeira vez.

### 4.3 Consultas de Wallet

`GET_OUTPUTS` e `GET_KEY_IMAGES` permitem à Wallet baixar **todas** as saídas privadas e imagens de chave gastas e fazer a varredura localmente (`spec/PRIVACY.md`), sem revelar ao Node quais notas lhe pertencem.

## 5. Sincronização

1. Após o handshake, se `height` do par for maior que a altura local, o Node envia `GET_BLOCKS { from_height: altura_local + 1, max: 64 }`.
2. O par responde `BLOCKS` com blocos consecutivos da sua cadeia preferida.
3. Cada bloco é **validado integralmente** antes de ser aceito (`SPEC §28`, `AT-SYNC-002`).
4. Se ainda houver diferença de altura, o processo se repete.
5. Ao receber um bloco cujo pai é desconhecido, o Node solicita blocos a partir da sua altura finalizada + 1.

Um par que envia blocos inválidos é penalizado (seção 6) — nenhum estado é aceito por declaração (`AT-SYNC-003`).

## 6. Proteção de recursos

| Mecanismo | Valor padrão |
| --- | --- |
| Tamanho máximo de mensagem | 4 MiB |
| Limite de taxa por conexão | balde de 400 mensagens, recarga de 200/s |
| Conexões de entrada | 32 |
| Conexões de saída | 8 |
| Quarentena após banimento | 10 minutos |

Pontuação local (não é reputação global):

| Infração | Pontos |
| --- | --- |
| Mensagem malformada | 25 |
| Transação com assinatura/formato inválido | 10 |
| Bloco inválido | 50 |
| Violação de protocolo | 50 |
| Excesso de taxa | 100 |

Com 100 pontos ou mais o par é desconectado e seu IP fica em quarentena (`SPEC §26`, `AT-P2P-005`).

Rejeições que podem ocorrer com pares honestos (transação duplicada, nonce já usado, pai desconhecido) **não** geram penalidade.

## 7. Descoberta

* Pares iniciais (*bootstrap*) por configuração.
* Após o handshake, `GET_PEERS`; os endereços recebidos são candidatos a novas conexões de saída.
* Um endereço anunciado é `ip_observado_da_conexão : listen_port` — o Node não confia em IPs declarados pelo próprio par.

## 8. Privacidade e registros

* Registros locais não armazenam endereços de pares além do necessário à operação.
* A origem de transações é protegida por Dandelion++ (seção 4.1). A Wallet ainda revela seu IP ao Node ao qual se conecta; recomenda-se Node próprio ou Tor.

## 9. Testes de aceitação cobertos

AT-P2P-001..005, AT-SYNC-001..003 e o teste de haste/embargo do Dandelion++ (em `crates/rz-node/tests`).
