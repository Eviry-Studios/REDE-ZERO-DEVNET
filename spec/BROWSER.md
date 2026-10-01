# spec/BROWSER.md — Navegador Zero

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0017
**Relacionamento:** `SPECIFICATIONS.md §58–§59`, `§62`, REQ-064..067, AC-BRW-001..004, AT-BRW-001..003, THR-BRW-001/002, THR-COM-001, THR-PRIV-*, INV-008
**Implementação de referência:** `crates/rz-browser` (`zero-navegador`)

---

## 1. Papel

O Navegador Zero é **uma** interface para os protocolos da Exonet, não uma autoridade (REQ-065):

* não existe mensagem, campo ou regra do protocolo que identifique o software do cliente (AC-BRW-004);
* tudo o que o Navegador faz, qualquer interface alternativa pode fazer com a biblioteca da Wallet ou outra implementação do protocolo (REQ-066, AT-BRW-003);
* uma interface que não implementa o protocolo (rede, Genesis ou versão diferentes) não estabelece sessão (`spec/P2P.md §3`, AT-BRW-002).

## 2. Arquitetura

```text
navegador web do sistema ──HTTP loopback──▶ zero-navegador ──canal cifrado (Tor opcional)──▶ Node ─▶ Rede Zero
                                              │ chaves locais, pedidos, identificadores fixados
```

O processo `zero-navegador` roda na máquina da pessoa, escuta **só em loopback** e usa o navegador web do sistema como tela. Ele conecta ao Node com as mesmas regras da Wallet (ADR-0011): Node local, Tor, ou consentimento explícito para conexão direta.

| Origem | Conteúdo |
| --- | --- |
| `navegador.localhost:P` | interface do Navegador: páginas geradas localmente, **sem scripts** |
| `NOME.TIPO.localhost:P` | a publicação `zero://NOME.TIPO` |
| `bBASE32.id.localhost:P` | publicação sem nome, pelo identificador (base32 minúsculo, 52 caracteres) |

O cabeçalho `Host` precisa corresponder exatamente a uma dessas formas, com a porta do Navegador; qualquer outro host é recusado com `421` (defesa contra DNS rebinding). `127.0.0.1:P` redireciona para a interface.

## 3. Isolamento das publicações (`SPEC §62`)

Cada publicação tem sua própria origem; a política de mesma origem do navegador do sistema isola publicações entre si e da interface. Cabeçalhos de toda publicação:

```text
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; connect-src 'self';
  worker-src 'self'; object-src 'none'; frame-src 'none'; frame-ancestors 'none';
  base-uri 'none'; form-action 'self'
X-Content-Type-Options: nosniff        Referrer-Policy: no-referrer
Cross-Origin-Opener-Policy: same-origin    Cross-Origin-Resource-Policy: same-origin
Permissions-Policy: camera=(), microphone=(), geolocation=(), usb=(), serial=(), bluetooth=(), payment=()
X-DNS-Prefetch-Control: off            Cache-Control: no-store
```

Consequências:

* uma publicação **não alcança servidores externos**: o IP do leitor não vaza para terceiros pelo conteúdo;
* não acessa câmera, microfone, localização ou dispositivos;
* não é embutida em outra página (contra clickjacking).

Um link para a web comum, se clicado, sai da Exonet; isso não é impedido.

A interface usa `default-src 'none'`: nenhum script executa nela.

## 4. Resolução e fixação

1. `zero://NOME.comunidade` → Comunidade **reconhecida** → manifesto (`spec/CONTENT.md §4`) → `frontend`.
2. `zero://NOME.TIPO` → `target` do nome.
3. `zero://HEX` → o próprio identificador.

O objeto é baixado e verificado pedaço a pedaço (`spec/CONTENT.md §2`). No primeiro acesso o Navegador **fixa** o valor resolvido: o `target`, ou o manifesto no caso de Comunidades, que também fixa a versão (THR-COM-002). Se o valor mudar, o conteúdo **não é aberto**. Uma página explica a troca, e a pessoa decide na interface se aceita o novo identificador.

A página de resolução (`/ir`) mostra o identificador criptográfico e o dono do nome. Ela alerta quando o nome tem o mesmo esqueleto visual de outro já fixado, ou o mesmo nome com outro tipo (`spec/NAMING.md §3`, THR-BRW-002).

## 5. Pedidos de assinatura (integração com a Wallet)

Publicações nunca recebem chaves. A API da publicação, na própria origem:

| Rota | Efeito |
| --- | --- |
| `POST /.zero/pedido` (formulário) | cria um pedido; exige `Origin` igual à origem da publicação |
| `GET /.zero/pedido/ID` | estado do pedido, só para a origem que o criou |

```text
tipo=transferencia & para=ENDEREÇO & valor=ZERO
tipo=ordem & ativo=HEX|rede:ativo & lado=compra|venda & quantidade=Q & preco=ZERO_POR_UNIDADE [& blocos=N]
tipo=cancelar & ordem=HEX
tipo=voto & proposta=HEX & escolha=sim|nao|abstencao
tipo=assinatura & mensagem=TEXTO (≤1024 bytes)        // identidade por site, spec/CONTENT.md §6

resposta: {"pedido": ID, "estado": "pendente"|"aprovado"|"recusado"|"erro", "txid"?, "chave"?, "assinatura"?, "erro"?}
```

O pedido aparece em `navegador.localhost:P/pedidos` com:

* a origem que o fez;
* o conteúdo em linguagem clara, incluindo os avisos de privacidade de cada operação.

Ele só é executado quando a pessoa clica em **Aprovar** na interface. Formulários da interface exigem dois controles: o cabeçalho `Origin` da interface e um token aleatório gerado a cada execução, que nenhuma outra origem consegue ler. Uma publicação, portanto, não aprova os próprios pedidos (THR-BRW-001). Limites: 16 pedidos pendentes por origem, 256 no total.

## 6. Funções da interface (`SPEC §58`)

| Função | Página |
| --- | --- |
| identificação e conexão | `/` (rede, Node, altura, modo de conexão, endereço, fixados) |
| navegação | `/ir?endereco=` |
| Wallet | `/carteira` (saldo, transferência transparente; envios privados pela `zero-wallet`) |
| Grande Mercado | `/mercado`, `/mercado/ativo?id=` (Pool, livro, ordens próprias, nova ordem, cancelamento) |
| governança | `/governanca` (propostas e voto) |
| Comunidades | `/comunidade?nome=` (estado, regra, manifesto, versões) |
| defesa | `/defesa` |
| pedidos | `/pedidos`, `/pedidos/ID` |

## 7. Testes

`browser_devnet`:

* publicação, replicação e verificação; recusa de host estranho;
* cabeçalhos de isolamento; interface sem scripts;
* pedido de outra origem recusado; aprovação só com origem e token da interface;
* transferência executada; identidade por site verificável e diferente entre sites;
* troca de alvo bloqueada; interface alternativa (AT-BRW-001, AT-BRW-003).

`incompatible_interface_rejected` cobre AT-BRW-002. Testes unitários em `origin`, `http`, `html` e `pedido`.
