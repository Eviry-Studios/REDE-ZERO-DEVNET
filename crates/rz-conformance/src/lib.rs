//! Suíte de conformidade da Rede Zero (`spec/CONFORMANCE.md`, `SPEC §68–§69`).
//!
//! Os vetores ficam em `conformance/` na raiz do repositório, em texto
//! simples, para que qualquer implementação, em qualquer linguagem, os
//! execute. Este crate é o verificador de referência e o gerador.

pub mod format;
pub mod generate;
pub mod verify;

/// Rede usada nos vetores (DEVNET, sem valor).
pub const VECTOR_NETWORK: &str = "rede-zero-devnet-conformidade";

/// Arquivos da suíte e o que cada um cobre (`SPEC §69`).
pub const SUITES: &[(&str, &str)] = &[
    (
        "identidade.txt",
        "Identidade: chaves, endereços e assinaturas",
    ),
    ("hash.txt", "Hash com separação de domínio"),
    ("zero.txt", "ZERO: leitura e escrita de valores"),
    (
        "codificacao.txt",
        "Codificação canônica: aceitação e rejeição",
    ),
    (
        "transacoes.txt",
        "Transações: identificador, assinatura e regras sem estado",
    ),
    (
        "cadeia.txt",
        "Blocos e estado: transição determinística e rejeições",
    ),
    ("p2p.txt", "Mensagens P2P"),
    ("governanca.txt", "Governança: decaimento de pontos"),
    (
        "runtime.txt",
        "Exonet Runtime: validação e execução determinística",
    ),
    ("ponte.txt", "Ponte: prova de trabalho, Merkle e queima"),
    (
        "versoes.txt",
        "Atualizações verificáveis: manifesto e hashes",
    ),
];
