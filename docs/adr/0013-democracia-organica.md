# ADR-0013 — Democracia Orgânica: incorporação com notas de conformidade

**Estado:** Aceita (diretriz conceitual; regras concretas a especificar)
**Escopo:** Projeto
**Data:** 2026-09-29
**Relacionamento:** `docs/DEMOCRACIA_ORGANICA.md`, ADR-0008, ADR-0011, REQ-023..027, REQ-032..033, REQ-040..045, REQ-064..074, `ARCHITECTURE.md §32`, THR-BRW-002

## Contexto

Foi proposto o documento "Democracia Orgânica", que descreve três ideias:

* participação em dois níveis (indivíduos e Comunidades);
* endereçamento `zero://nome.tipo`;
* Navegador Zero com terminal e ambiente de desenvolvimento, instalação simples e execução distribuída sem confiança no Node.

A regra do projeto é: só entra o que não contradiz os documentos vigentes; divergências são reconciliadas sem reestruturar o que já existe.

## Análise

A proposta é **compatível na essência**: não cria autoridade central, preserva a autonomia das Comunidades dentro do protocolo e reconhece explicitamente os riscos de Sybil e de duplicação de Comunidades. Foram identificados sete pontos que exigem interpretação:

| Nota | Tema | Documento vigente | Síntese adotada |
| --- | --- | --- | --- |
| N-1 | Peso do voto comunitário | ADR-0008 (apuração bicameral) | Sinalização registrada e verificável, **não vinculante**; peso decisório só com nova ADR anti-Sybil |
| N-2 | "Voto individual" e percentuais | ARCHITECTURE §32 | Resultado oficial = ponderação bicameral; contagens por cabeça só informativas |
| N-3 | Nomes `zero://nome.tipo` | SPEC §5, §60; THR-BRW-002 | Nome registrado sobre identificador criptográfico; tipo verificado |
| N-4 | Tipo `.market` | REQ-032/033; Manifesto §11 | Reservado a interfaces do Grande Mercado; nunca uma segunda DEX |
| N-5 | "Membros" | REQ-023, 025, 027 | Filiação pseudônima e mínima; sem lista pública obrigatória |
| N-6 | Posição comunitária | REQ-043, REQ-073 | Mecanismo de decisão declarado no reconhecimento; posição só vale com prova conforme a declaração |
| N-7 | "Reconhecida pela Exonet" | Manifesto §3; ADR-0008 | Reconhecimento = proposta aprovada na categoria Comunidade |
| N-8 | Instalação simples | ADR-0011 | Node privado por padrão, Wallet local, Tor opcional |

## Decisão

Incorporar o documento como `docs/DEMOCRACIA_ORGANICA.md`, com o texto original preservado e as notas N-1 a N-8 na seção 15, que prevalecem em caso de dúvida. Nenhum documento vigente é alterado, exceto por referências cruzadas.

## Consequências

* A governança continua decidida pela apuração bicameral (ADR-0008).
* Abre três especificações futuras: `spec/COMMUNITIES.md`, `spec/NAMING.md`, `spec/RUNTIME.md`.
* Um eventual peso decisório para Comunidades passa a ter critério explícito de aceitação: demonstrar que criar Comunidades adicionais não multiplica influência.
