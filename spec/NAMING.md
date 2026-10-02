# spec/NAMING.md — Nomes `zero://`

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0015
**Relacionamento:** `SPECIFICATIONS.md §5`, `§60`, `docs/DEMOCRACIA_ORGANICA.md` N-3, N-4, THR-BRW-002
**Implementação de referência:** `crates/rz-core/src/community.rs`

---

## 1. Forma e sintaxe

```text
zero://NOME.TIPO

NOME : 3..=32 caracteres de [a-z0-9-], sem hífen no início, no fim ou repetido
TIPO : comunidade | blog | app | market | forum | video | service
```

Nomes **reservados** a funções do protocolo, e qualquer nome com o mesmo esqueleto (§3) de um deles:

```text
zero rede-zero redezero exonet grande-mercado grandemercado mercado pool
wallet carteira governanca node agente-zero navegador-zero
```

## 2. O nome é uma camada sobre um identificador (N-3)

| Tipo | Resolve para | Quem registra |
| --- | --- | --- |
| `comunidade` | `id` da Comunidade **reconhecida** (`spec/COMMUNITIES.md`) | ninguém diretamente: vem do reconhecimento |
| demais | `target: fixed[32]` (hash de conteúdo, chave de aplicação etc.) | qualquer conta, por `RegisterName` |

O sufixo é parte do endereço e é verificado contra o tipo registrado: `zero://x.app` e `zero://x.comunidade` são endereços diferentes, e um `.app` não se passa por `.comunidade`. Publicações privadas não precisam de nome e podem usar diretamente o identificador criptográfico (Manifesto §10).

`.market` identifica interfaces do Grande Mercado e comércio que liquida por ele. Nenhuma publicação tem capacidade de liquidar ativos: a única liquidação é a do Grande Mercado (`spec/MARKET.md`). A restrição do N-4 é, portanto, estrutural.

## 3. Nomes confundíveis (THR-BRW-002)

```text
esqueleto(nome) = remove '-'
                  mapeia 0→o, 1→l, i→l, 3→e, 4→a, 5→s, 7→t, 8→b, 9→g
                  substitui "rn"→"m", "vv"→"w"
```

Um nome não pode ser registrado se já existir, **no mesmo tipo**, outro nome com o mesmo esqueleto. Exemplos: `banco`/`banc0`, `modern`/`rnodern`, `ciencia`/`c1enc1a`, `editorzero`/`editor-zero`. Para `comunidade`, a regra vale contra todas as Comunidades declaradas ou reconhecidas.

## 4. Operações

| Tag | Operação | Campos | Débito | Regras |
| --- | --- | --- | --- | --- |
| `0x43` | `RegisterName` | `name, kind, target` | taxa + `name_fee` (vai para o Pool) | sintaxe; tipo ≠ `comunidade`; nome disponível (§3) |
| `0x44` | `UpdateName` | `name, kind, target, new_owner?` | taxa | o nome existe e pertence ao remetente |

```text
NameRecord { owner: Address, target: fixed[32], registered_at: u64 }
```

Nomes não expiram nesta versão (renovação A DEFINIR).

## 5. Resolução

`RESOLVE { name, kind }` → `RESOLVED { height, name, kind, target?, owner? }` (`spec/P2P.md`). O Navegador Zero e outras interfaces devem:

* mostrar o identificador criptográfico resolvido;
* alertar quando um nome for parecido com outro conhecido, mesmo de outro tipo;
* permitir fixar o identificador (e, para Comunidades, a versão do manifesto; THR-COM-002).

## 6. Testes

`community::tests` (sintaxe, esqueletos, endereços), `names_registered_resolved_and_protected` (registro, confundíveis, dono), `community_devnet` (resolução em Nodes reais).
