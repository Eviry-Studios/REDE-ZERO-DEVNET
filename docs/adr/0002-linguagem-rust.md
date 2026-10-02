# ADR-0002 — Rust como linguagem da implementação de referência

**Estado:** Aceita
**Escopo:** DEVNET (implementação de referência)
**Data:** 2026-09-29
**Relacionamento:** REQ-003, REQ-004, REQ-084, SPEC-001, THR-P2P-003, THR-TX-005

## Contexto

É necessário escolher uma linguagem para o primeiro protótipo. O código processa entrada não confiável da rede (THR-P2P-003) e precisa ser determinístico (THR-TX-005).

## Decisão

A implementação de referência da DEVNET será escrita em **Rust (edição 2021, toolchain estável)**, organizada como um workspace Cargo com crates pequenos e de responsabilidade única.

Regras:

* `#![forbid(unsafe_code)]` em todos os crates do projeto;
* nenhum `panic` em caminhos que processam dados recebidos da rede;
* nenhum ponto flutuante na função de transição de estado;
* dependências externas mínimas, fixadas via `Cargo.lock` e justificadas nesta pasta.

## Alternativas consideradas

* **Go** — boa concorrência e simplicidade, mas com coletor de lixo e menor garantia de ausência de erros em tempo de compilação.
* **C/C++** — sem segurança de memória por padrão; risco inaceitável para parsing de rede.
* **TypeScript/Python** — adequados a ferramentas, não à implementação de consenso.

## Consequências

* Segurança de memória sem coletor de lixo.
* Curva de aprendizado maior para novos contribuidores.
* `SPEC-001` continua valendo: a especificação deve ser implementável em outras linguagens. O código Rust **não é** a especificação; em caso de divergência, a especificação prevalece ou é corrigida por processo formal.

## Dependências aprovadas

| Crate | Uso | Justificativa |
| --- | --- | --- |
| `ed25519-dalek` | Assinaturas | Implementação amplamente auditada de Ed25519 (ADR-0003) |
| `blake3` | Hash | Implementação oficial de BLAKE3 (ADR-0003) |
| `rand_core` / `getrandom` | Geração de chaves | Fonte de entropia do sistema operacional |
| `curve25519-dalek` | Aritmética em Ristretto255 | Mesma base auditada de `ed25519-dalek` (ADR-0009) |
| `bulletproofs` | Provas de faixa | Implementação de referência da equipe dalek, amplamente usada (ADR-0009) |
| `merlin` | Transcrições de provas | Exigida por `bulletproofs`; construção STROBE padronizada |

Rede, CLI, codificação e persistência usam apenas a biblioteca padrão, para reduzir superfície de ataque da cadeia de suprimentos (THR-DEV-001).
