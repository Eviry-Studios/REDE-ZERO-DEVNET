# spec/CONTENT.md — Conteúdo da Exonet

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0017
**Relacionamento:** `SPECIFICATIONS.md §58–§62`, `ARCHITECTURE.md §23–§25`, REQ-064..070, THR-COM-002, THR-BRW-001/002, Manifesto §6
**Implementação de referência:** `crates/rz-core/src/content.rs`, `crates/rz-node/src/content.rs`, `crates/rz-wallet` (publicação)

---

## 1. Princípio

Interfaces, páginas e arquivos ficam **fora da cadeia**. A cadeia guarda só identificadores:

| Referência no estado | Aponta para |
| --- | --- |
| `target` de um nome `zero://nome.tipo` (tipo ≠ `comunidade`) | um objeto de conteúdo |
| `manifest_hash` de uma Comunidade (e o histórico de versões) | um manifesto (§4) |
| `frontend` de um manifesto estruturado | um objeto de conteúdo |

Qualquer cópia, vinda de qualquer Node, é verificada contra o identificador. Um Node não precisa ser confiável para servir conteúdo.

## 2. Objetos de conteúdo

```text
CHUNK_SIZE = 1 MiB      MAX_CHUNKS = 16      MAX_OBJECT = 16 MiB

ContentInfo { len: u64, chunks: list<fixed[32]> }    // len ∈ 1..=MAX_OBJECT, |chunks| = ⌈len / CHUNK_SIZE⌉
chunk_hash(p) = H("rede-zero/content-chunk/v1", p)
content_id    = H("rede-zero/content/v1", enc(ContentInfo))
```

O pedaço `i` tem `min(CHUNK_SIZE, len − i·CHUNK_SIZE)` bytes. Cada pedaço é verificado ao chegar, antes de ser guardado: um Node que envia um pedaço errado é detectado imediatamente e penalizado.

## 3. Pacote (`Bundle`)

Formato de um site ou interface dentro de um objeto:

```text
Bundle     { files: list<BundleFile> }          // 1..=1024 arquivos, caminhos em ordem estritamente crescente
BundleFile { path: string (≤256), mime: string (≤128), data: bytes }
```

`path`: relativo, só `[A-Za-z0-9._/-]`, sem segmento vazio, `.` ou `..`. A ordem obrigatória torna a codificação única. `index.html` é a página inicial.

## 4. Manifesto estruturado de Comunidade

```text
manifesto = "RZMANIF1" ‖ enc(Manifest)
Manifest { name: string, version: u32, description: string (≤1024),
           frontend: option<fixed[32]>, module: option<fixed[32]> }
manifest_hash = H("rede-zero/community-manifest/v1", manifesto)      // spec/COMMUNITIES.md §1
```

Manifestos em formato livre continuam válidos para o protocolo (ADR-0015); as interfaces apenas não conseguem abrir a Comunidade. `module` aponta para a lógica no Exonet Runtime (`spec/RUNTIME.md`). Tamanho máximo: 64 KiB.

## 5. Hospedagem e replicação nos Nodes

Um Node hospeda, dentro de uma cota local (`--content-quota-mb`, padrão 1024):

* manifestos referenciados por qualquer Comunidade no estado (declarada ou reconhecida);
* objetos referenciados por um nome, ou pela interface de um manifesto de Comunidade **reconhecida**.

Qualquer outro objeto é recusado: publicar exige primeiro uma referência paga na cadeia (taxa de nome ou declaração), o que limita o abuso de armazenamento.

| Mensagem | Uso |
| --- | --- |
| `GET_MANIFEST h` → `MANIFEST {h, bytes?}` | consulta; `MANIFEST` não solicitado = envio para hospedagem |
| `GET_CONTENT id` → `CONTENT_INFO {id, info?}` | consulta; `CONTENT_INFO` não solicitado = início de envio |
| `GET_CHUNK {id, i}` → `CHUNK {id, i, data?}` | pedaço; limite próprio de 32 pedidos, recarga 8/s por conexão |
| `HAVE_CONTENT id` | o remetente passou a hospedar o objeto |

Fluxo:

1. **Publicação:** o cliente envia `CONTENT_INFO` e os pedaços. O Node verifica cada pedaço, grava o objeto e responde `HAVE_CONTENT`; se recusar, responde `CONTENT_INFO {id, None}`.
2. **Difusão:** ao hospedar, o Node anuncia `HAVE_CONTENT` aos pares. Um par que considere o objeto referenciado pede a descrição e os pedaços a quem anunciou.
3. **Sob demanda:** um pedido de cliente por objeto ausente faz o Node perguntar aos pares (`GET_CONTENT`, `GET_MANIFEST`). O cliente repete a consulta até o objeto chegar.

Limites contra abuso: 4 downloads simultâneos e 1 por conexão; download abandonado após 15 s sem pedaço novo ou 120 s no total; só a conexão de origem pode enviar pedaços de um download; descrição ou pedaço que não conferem com o identificador geram penalidade (`spec/P2P.md §6`). Ao iniciar, o Node reverifica tudo o que está em disco e descarta o que não confere.

## 6. Identidade por site (interfaces)

Para autenticar uma pessoa numa publicação **sem revelar sua Wallet** e sem permitir que sites diferentes a correlacionem:

```text
site_secret(semente, endereço) = Ed25519.from_seed(H("rede-zero/exonet-identity/v1", semente ‖ endereço))
assinatura = Ed25519(ctx "rede-zero/exonet-login/v1" ‖ network_id ‖ str(endereço) ‖ str(mensagem))
```

`endereço` é a forma canônica `zero://nome.tipo` ou `zero://HEX`. A assinatura de um site não vale em outro, nem em outra rede. A publicação verifica com `verify_login`.

## 7. Privacidade

* O Node que serve um objeto sabe que alguém pediu aquele identificador, mas não quem, se o acesso for por Tor (ADR-0011).
* Publicações não podem contatar servidores externos (`spec/BROWSER.md §3`), então o conteúdo não revela o IP do leitor a terceiros.
* Nada sobre leitores é gravado na cadeia.

## 8. Testes

`content::tests` (identidade por pedaços, pacote canônico, manifesto, identidade por site), `rz-node content::tests` (armazenamento, reverificação, cota), `community_devnet` (manifesto e interface de Comunidade replicados), `browser_devnet` (publicação, recusa sem referência, replicação entre Nodes), `robustness.rs`.
