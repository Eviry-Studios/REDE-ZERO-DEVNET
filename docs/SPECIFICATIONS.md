# SPECIFICATIONS.md

# Rede Zero / Exonet

**Versão:** 0.1.0
**Status:** Especificação técnica inicial
**Natureza:** Especificação funcional e protocolar
**Relacionamento:**

* `REQUIREMENTS.md v0.2.0`
* `THREAT_MODEL.md v0.1.0`
* `ARCHITECTURE.md v0.1.0`

---

# 1. Objetivo

Este documento define comportamentos técnicos verificáveis da Rede Zero.

Enquanto `REQUIREMENTS.md` define **o que a Rede Zero deve ser** e `ARCHITECTURE.md` define **como seus componentes se relacionam**, este documento começa a definir:

* formatos;
* regras;
* estados;
* operações;
* validações;
* interfaces;
* condições de aceitação;
* comportamentos em caso de erro.

A especificação deve ser suficientemente precisa para permitir que diferentes equipes implementem componentes compatíveis.

---

# 2. Princípios

## SPEC-001 — Implementação independente

A especificação não deve depender de uma única implementação.

Duas implementações diferentes devem poder produzir comportamento compatível quando seguem as mesmas regras.

---

## SPEC-002 — Determinismo

Operações protocolárias determinísticas devem produzir o mesmo resultado quando recebem:

* o mesmo estado;
* os mesmos dados;
* as mesmas regras;
* as mesmas entradas.

---

## SPEC-003 — Verificação independente

Sempre que possível, um participante deve poder verificar uma afirmação sem precisar confiar na entidade que a produziu.

---

## SPEC-004 — Falha segura

Dados inválidos, incompletos ou não verificáveis não devem produzir autorização automática.

---

## SPEC-005 — Versionamento

Toda especificação protocolar deverá possuir uma identificação de versão.

---

# 3. Identidade criptográfica

A identidade de um Node será baseada em criptografia assimétrica.

Conceitualmente:

```text
Private Key
     │
     ├── permanece protegida
     │
     ▼
Public Key
     │
     ▼
Node ID
```

---

## SPEC-ID-001 — Chave privada

Cada Node deverá possuir material criptográfico privado necessário para autenticar suas operações.

A chave privada:

* não deve ser transmitida como parte normal do protocolo;
* não deve ser armazenada em texto aberto quando houver alternativa segura;
* não deve ser necessária para outros Nodes verificarem operações.

---

## SPEC-ID-002 — Chave pública

A chave pública correspondente poderá ser utilizada para verificar assinaturas produzidas pelo Node.

---

## SPEC-ID-003 — Node ID

O Node ID deverá ser derivado deterministicamente do identificador criptográfico definido pelo protocolo.

A função exata de derivação permanece **A DEFINIR**.

---

## SPEC-ID-004 — Identidade civil

O protocolo não deverá exigir associação entre:

```text
Node ID → identidade civil
```

---

## SPEC-ID-005 — Rotação

O protocolo deverá prever mecanismo para substituição de chaves comprometidas ou inutilizadas.

O procedimento exato permanece **A DEFINIR**.

---

# 4. Wallet

A Wallet controla as chaves utilizadas para movimentação de ZERO.

---

## SPEC-WAL-001 — Separação

A Wallet e o Node devem poder funcionar como componentes separados.

---

## SPEC-WAL-002 — Assinatura local

Quando possível, operações que exigem autorização monetária devem ser assinadas localmente pela Wallet.

---

## SPEC-WAL-003 — Não custódia

Um Node não deve precisar possuir a chave privada da Wallet para transmitir uma transação.

---

## SPEC-WAL-004 — Verificação

Uma transação assinada deverá permitir que terceiros verifiquem sua autenticidade.

---

# 5. Identificadores

O protocolo deverá possuir identificadores para:

* Nodes;
* Wallets;
* transações;
* blocos;
* Comunidades;
* versões;
* propostas;
* incidentes.

Cada identificador deverá ser:

* único dentro de seu domínio;
* verificável;
* determinístico quando apropriado;
* resistente a colisões.

Os algoritmos definitivos permanecem **A DEFINIR**.

---

# 6. Transações

Uma transação representa uma operação válida sobre o estado da Rede Zero.

Estrutura conceitual:

```text
Transaction
│
├── Version
├── Type
├── Inputs / State references
├── Outputs / State changes
├── Authorization data
├── Fee
└── Metadata protocolar mínima
```

A estrutura final poderá ser diferente após os testes.

---

# 7. Identificador da transação

O identificador da transação deverá ser derivado deterministicamente de seu conteúdo canônico.

Conceitualmente:

```text
Transaction
     │
     ▼
Canonical Encoding
     │
     ▼
Hash
     │
     ▼
Transaction ID
```

Alterar qualquer campo relevante deverá produzir um identificador diferente.

---

# 8. Codificação canônica

Dados protocolários que participam de hashes, assinaturas ou consenso deverão possuir representação canônica.

Dois Nodes não poderão interpretar o mesmo objeto válido como duas sequências diferentes de bytes para fins de consenso.

---

# 9. Assinaturas

Operações que exigem autorização deverão possuir dados criptográficos suficientes para verificar essa autorização.

Conceitualmente:

```text
Mensagem
   │
   ▼
Assinatura
   │
   ▼
Chave pública
   │
   ▼
VERIFICAÇÃO
```

O algoritmo de assinatura definitivo permanece **A DEFINIR**.

---

# 10. Validação de transação

Uma transação recebida deverá passar por validações antes de ser aceita.

Fluxo:

```text
Transação recebida
       │
       ▼
Formato válido?
       │
       ├── NÃO → REJEITAR
       │
       ▼
Assinatura válida?
       │
       ├── NÃO → REJEITAR
       │
       ▼
Estado permite operação?
       │
       ├── NÃO → REJEITAR
       │
       ▼
Taxa válida?
       │
       ├── NÃO → REJEITAR
       │
       ▼
Regras do protocolo válidas?
       │
       ├── NÃO → REJEITAR
       │
       ▼
ACEITAR
```

---

# 11. Double Spend

O protocolo deverá impedir que o mesmo recurso monetário seja utilizado de forma incompatível em duas operações aceitas no mesmo estado final.

Exemplo conceitual:

```text
Saldo A = 10 ZERO

TX1 → gasta 10 ZERO
TX2 → gasta os mesmos 10 ZERO
```

O estado final não poderá considerar simultaneamente as duas operações como válidas quando forem mutuamente exclusivas.

---

# 12. Estado

A Rede Zero deverá possuir uma representação determinística do estado.

Conceitualmente:

```text
State N
   +
Valid Transactions
   │
   ▼
State Transition
   │
   ▼
State N+1
```

A função de transição deverá ser determinística.

---

# 13. Máquina de estado

A Rede Zero poderá ser modelada como:

```text
S(n+1) = F(S(n), B(n+1))
```

Onde:

* `S(n)` = estado anterior;
* `B(n+1)` = novo bloco;
* `F` = função determinística de transição;
* `S(n+1)` = novo estado.

A definição matemática completa será especificada posteriormente.

---

# 14. Estado inválido

Um Node não deverá aceitar uma transição que produza estado incompatível com as regras do protocolo.

Exemplos:

* criação monetária não autorizada;
* saldo negativo;
* assinatura inválida;
* gasto inexistente;
* transação duplicada incompatível;
* regra de consenso violada.

---

# 15. Blocos

Um bloco representa uma unidade ordenada de atualização do estado.

Estrutura conceitual:

```text
Block
│
├── Version
├── Height
├── Previous Block ID
├── Timestamp / temporal data
├── Transactions
├── State commitment
├── Consensus data
└── Block ID
```

Campos definitivos permanecem **A DEFINIR**.

---

# 16. Encadeamento

Cada bloco deverá referenciar criptograficamente seu predecessor.

```text
Genesis
   │
   ▼
Block 1
   │
   ▼
Block 2
   │
   ▼
Block 3
```

Alterar um bloco deverá invalidar, direta ou indiretamente, as referências criptográficas posteriores.

---

# 17. Altura

Blocos deverão possuir uma posição lógica na cadeia ou estrutura equivalente.

A representação da altura deverá ser determinística.

---

# 18. Genesis

A Rede Zero deverá possuir um estado inicial conhecido.

O bloco/estado Genesis deverá definir, entre outros:

* versão inicial;
* regras iniciais;
* estado monetário inicial;
* parâmetros iniciais;
* identificador da rede.

O conteúdo definitivo do Genesis será especificado posteriormente.

---

# 19. Consenso

O consenso deverá determinar qual sequência/estado é considerado válido pela rede.

A interface conceitual será:

```text
Candidates
    │
    ▼
Consensus Rules
    │
    ▼
Accepted State
```

O algoritmo específico permanece **A DEFINIR**.

---

# 20. Requisitos do consenso

O mecanismo escolhido deverá ser avaliado contra:

* double spend;
* forks;
* ataques Sybil;
* censura;
* Nodes maliciosos;
* partições;
* perda de participantes;
* concentração;
* disponibilidade.

---

# 21. Finalidade

O protocolo deverá definir quando uma transação pode ser considerada final.

A especificação deverá distinguir:

* transação recebida;
* transação incluída;
* transação confirmada;
* estado finalizado.

O mecanismo definitivo permanece **A DEFINIR**.

---

# 22. Forks

A Rede Zero deverá possuir regras determinísticas para lidar com estados concorrentes.

Conceitualmente:

```text
       Block N
       /     \
      /       \
   Chain A   Chain B
      \       /
       decisão
          │
          ▼
   estado aceito
```

O mecanismo de resolução dependerá do consenso escolhido.

---

# 23. P2P

Nodes deverão poder trocar mensagens protocolárias diretamente ou através da topologia definida pelo protocolo.

Mensagens conceituais:

```text
HELLO
PEER_DISCOVERY
TRANSACTION
BLOCK
STATE_REQUEST
STATE_RESPONSE
PING
PONG
PROTOCOL_VERSION
```

A lista definitiva de mensagens ainda será definida.

---

# 24. Identificação de protocolo

Durante a comunicação, um Node deverá conseguir determinar pelo menos:

* versão do protocolo;
* capacidade suportada;
* identificação necessária à comunicação;
* parâmetros de compatibilidade.

---

# 25. Mensagens inválidas

Mensagens que não respeitem:

* formato;
* tamanho máximo;
* autenticação exigida;
* versão;
* regras de protocolo;

deverão ser rejeitadas.

Mensagens inválidas repetidas poderão resultar em limitação ou isolamento do remetente.

---

# 26. Proteção contra flooding

Nodes deverão possuir mecanismos para limitar abuso de recursos.

Possíveis mecanismos:

* limites de taxa;
* limites de tamanho;
* priorização;
* penalização protocolar;
* desconexão;
* quarentena.

Os parâmetros deverão ser definidos por testes.

---

# 27. Sincronização

Um Node que entra na rede deverá conseguir obter o estado necessário para participar.

Fluxo conceitual:

```text
Novo Node
   │
   ▼
Descoberta de pares
   │
   ▼
Identificação da rede
   │
   ▼
Obtenção do estado
   │
   ▼
Verificação
   │
   ▼
Sincronização
   │
   ▼
Participação
```

---

# 28. Verificação durante sincronização

Um Node não deverá aceitar cegamente o estado fornecido por outro Node.

O estado deverá ser verificável através das regras do protocolo.

---

# 29. Eclipse Attack

A implementação deverá dificultar que um atacante isole um Node de todos os participantes honestos.

As estratégias exatas serão definidas após testes do mecanismo P2P.

---

# 30. Privacidade de rede

A comunicação deverá minimizar informações desnecessárias.

O protocolo deverá evitar exposição desnecessária de:

* endereço físico;
* identidade civil;
* relações entre Wallets;
* padrões comportamentais;
* informações não necessárias à operação.

---

# 31. Privacidade de transações

A implementação deverá permitir transações cujo conteúdo público seja minimizado de acordo com o modelo de privacidade escolhido.

Os elementos criptográficos específicos permanecem **A DEFINIR**.

---

# 32. Metadados

O protocolo deverá avaliar cada campo de dados segundo:

```text
É necessário?
      │
      ├── SIM → manter
      │
      └── NÃO → considerar remoção
```

O objetivo é evitar coleta de dados apenas por conveniência.

---

# 33. ZERO

ZERO será representado por uma unidade inteira mínima definida pelo protocolo.

Exemplo conceitual:

```text
1 ZERO = N unidades mínimas
```

O valor de `N` permanece **A DEFINIR**.

---

# 34. Criação de ZERO

Somente mecanismos explicitamente autorizados pelo protocolo poderão criar ZERO.

Nenhum Node deverá poder simplesmente declarar:

```text
saldo += X
```

sem uma transição válida de estado.

---

# 35. Integridade monetária

Toda alteração monetária deverá ser verificável.

O protocolo deverá permitir verificar:

```text
Estado anterior
+
regras monetárias
+
transações
=
Estado posterior
```

---

# 36. Taxas

Uma transação poderá conter uma taxa.

A taxa deverá:

* estar explicitamente representada;
* ser verificável;
* respeitar os limites definidos pelo protocolo;
* possuir destino/distribuição determinísticos.

---

# 37. Recompensas

Caso existam recompensas de Node, elas deverão ser produzidas apenas segundo regras protocolárias verificáveis.

Nenhum administrador poderá criar recompensas manualmente.

---

# 38. Grande Mercado

O Grande Mercado deverá ser implementado como lógica protocolar verificável.

Operações conceituais:

```text
CREATE_ORDER
CANCEL_ORDER
MATCH_ORDER
SETTLE_ORDER
```

A lista final dependerá do modelo de negociação escolhido.

---

# 39. Pool

O Pool será representado pelo estado da própria Rede Zero.

A regra fundamental será:

```text
Depósito válido
      ↓
Pool
      ↓
Ativo permanece no Pool
```

Não deverá existir uma operação administrativa genérica:

```text
WITHDRAW_FROM_POOL_BY_ADMIN
```

---

# 40. Permanência do Pool

Qualquer mecanismo de movimentação de ativos para fora do Pool deverá ser considerado incompatível com a regra de permanência, salvo se futuramente a especificação da comunidade alterar explicitamente esse requisito por meio do processo de governança previsto.

A implementação inicial deverá tratar a permanência como propriedade protocolar.

---

# 41. Ativos externos

Ativos externos deverão possuir identificadores que indiquem sua origem.

Conceitualmente:

```text
ExternalAsset
│
├── Network
├── Asset identifier
├── Verification method
└── Pool representation
```

A representação não deverá permitir confundir um ativo externo com ZERO.

---

# 42. Integração entre redes

Uma integração externa deverá possuir mecanismo verificável para demonstrar que determinado ativo realmente existe ou foi bloqueado na rede de origem.

A arquitetura deverá evitar depender exclusivamente de uma declaração de um custodiante.

---

# 43. Governança

Uma proposta deverá possuir identificador único.

Estrutura conceitual:

```text
Proposal
│
├── ID
├── Version
├── Author
├── Description
├── Parameters
├── Deposit
├── Created at
├── Voting period
└── Status
```

---

# 44. Depósito de proposta

Uma proposta poderá exigir depósito simbólico em ZERO para reduzir spam.

O valor deverá ser definido posteriormente.

O protocolo deverá especificar o destino e eventual devolução desse depósito.

---

# 45. Votação

Uma votação deverá ser:

* verificável;
* auditável;
* resistente à alteração posterior;
* vinculada a uma proposta específica;
* limitada ao período definido.

---

# 46. Resultado de votação

O resultado deverá ser calculável deterministicamente.

Conceitualmente:

```text
Votes
  │
  ▼
Voting Rules
  │
  ▼
Threshold
  │
  ▼
Result
```

---

# 47. Sybil

O sistema de governança não deverá depender exclusivamente de identidade criptográfica para assumir que:

```text
1 Node = 1 pessoa
```

Uma identidade criptográfica comprova controle de uma chave, não identidade civil.

O mecanismo anti-Sybil permanece **A DEFINIR**.

---

# 48. Reputação

A reputação deverá ser derivada de eventos verificáveis.

Exemplos:

* participação válida;
* contribuições verificadas;
* violações protocolárias comprovadas;
* participação defensiva verificada.

A fórmula definitiva permanece **A DEFINIR**.

---

# 49. Penalizações

Uma penalização deverá possuir:

* causa identificável;
* regra correspondente;
* evidência verificável;
* efeito definido.

Não deverá existir penalização puramente baseada em opinião subjetiva.

---

# 50. Pesquisa e desenvolvimento

Atividades como:

* leitura do código;
* pesquisa;
* compilação;
* fuzzing;
* testes;
* análise;
* criação de ferramentas;
* divulgação responsável;

não deverão ser classificadas automaticamente como comportamento malicioso.

---

# 51. Detecção de incidentes

Um incidente deverá possuir um identificador.

Estrutura conceitual:

```text
Incident
│
├── ID
├── Timestamp
├── Evidence
├── Affected components
├── Severity
├── Status
└── Actions
```

---

# 52. Evidências

Uma ação defensiva significativa deverá, quando possível, possuir evidência verificável.

Não deverá ser suficiente:

```text
"Node X parece malicioso."
```

A arquitetura deve buscar:

```text
Comportamento observado
+
dados verificáveis
+
regra violada
=
evidência
```

---

# 53. Modos de defesa

Estados conceituais:

```text
NORMAL
   ↓
VIGILÂNCIA
   ↓
INCIDENTE
   ↓
GUERRA_CIBERNÉTICA
```

A transição entre estados deverá possuir regras explícitas.

---

# 54. Privilégios defensivos

Privilégios concedidos durante incidentes deverão possuir:

* escopo;
* duração;
* autoridade verificável;
* mecanismo de revogação;
* registro.

---

# 55. Ausência de retaliação

O protocolo de defesa não deverá possuir operações destinadas a:

* invadir computadores externos;
* instalar código malicioso;
* roubar dados;
* destruir infraestrutura externa.

A defesa deverá operar dentro dos limites da própria Rede Zero e dos sistemas voluntariamente participantes.

---

# 56. Encerramento de incidente

O encerramento deverá exigir condições verificáveis.

Conceitualmente:

```text
Threat contained?
       │
       ├── NÃO → continuar incidente
       │
       ▼
Systems recovered?
       │
       ├── NÃO → recuperação
       │
       ▼
Evidence preserved?
       │
       ▼
Incident closed
```

---

# 57. Registro de defesa

Uma contribuição defensiva poderá gerar um registro verificável associado ao Node.

O registro deverá distinguir:

```text
participou do incidente
```

de:

```text
contribuiu de maneira verificável para a defesa
```

Isso evita recompensar simplesmente a presença em um modo emergencial.

---

# 58. Navegador Zero

O Navegador Zero deverá implementar as interfaces necessárias para:

* identificação;
* conexão;
* navegação;
* Comunidades;
* Grande Mercado;
* Wallet;
* governança.

Ele não deverá possuir autoridade especial sobre o protocolo.

---

# 59. Interfaces alternativas

Uma aplicação diferente poderá implementar o mesmo protocolo.

Exemplo:

```text
Navegador Zero
       │
       ├──────────┐
       │          │
Interface B   Interface C
       │          │
       └────┬─────┘
            ▼
       Protocolo
```

Isso evita dependência de uma interface única.

---

# 60. Comunidades

Uma Comunidade deverá possuir identificador próprio.

A publicação deverá permitir verificar:

* identidade da Comunidade;
* versão;
* componentes;
* regras;
* estado;
* origem.

---

# 61. Segurança das Comunidades

Uma Comunidade comprometida não deverá possuir automaticamente capacidade de alterar:

* consenso;
* saldo de usuários;
* regras monetárias;
* identidade de Nodes;
* outras Comunidades.

---

# 62. Sandboxing

Aplicações de Comunidades deverão ser isoladas quando tecnicamente possível.

Uma Comunidade não deverá receber privilégios do sistema operacional ou da Rede Zero que não sejam necessários à sua função.

---

# 63. Atualizações

Uma atualização de protocolo deverá possuir:

```text
Version
+
Specification
+
Implementation
+
Tests
+
Release identifier
```

---

# 64. Verificação de atualização

Antes de aceitar uma atualização, um participante deverá conseguir verificar sua autenticidade e compatibilidade.

O mecanismo exato de assinatura/distribuição permanece **A DEFINIR**.

---

# 65. Compatibilidade

Cada versão deverá declarar sua compatibilidade com versões anteriores.

Categorias possíveis:

```text
COMPATIBLE
PARTIALLY_COMPATIBLE
INCOMPATIBLE
```

Os efeitos de cada categoria deverão ser formalizados posteriormente.

---

# 66. Genesis e identificação da rede

Cada implementação deverá distinguir claramente:

* rede principal;
* redes de teste;
* ambientes de desenvolvimento.

Nenhum Node deverá aceitar silenciosamente dados de uma rede diferente como se fossem da rede principal.

---

# 67. Testnet

Antes da rede principal deverão existir ambientes de teste.

Possíveis ambientes:

```text
DEVNET
TESTNET
STAGING
MAINNET
```

Os parâmetros poderão ser diferentes entre ambientes.

---

# 68. Determinismo entre implementações

Quando duas implementações receberem:

```text
mesmo bloco
+
mesmo estado
+
mesmas regras
```

elas deverão produzir o mesmo resultado.

Divergências deverão ser consideradas falhas de compatibilidade até investigação.

---

# 69. Testes de conformidade

Uma implementação deverá poder executar uma suíte de testes de conformidade.

Exemplo:

```text
Protocol Conformance Tests
│
├── Identity
├── Transactions
├── Blocks
├── State
├── Consensus interface
├── P2P
├── Wallet
├── ZERO
├── Governance
└── Security invariants
```

---

# 70. Invariantes

As seguintes propriedades deverão permanecer verdadeiras:

### INV-001

Nenhuma transação inválida pode produzir estado válido.

### INV-002

Nenhum Node individual possui autoridade monetária absoluta.

### INV-003

ZERO não pode ser criado arbitrariamente.

### INV-004

Uma assinatura inválida não pode autorizar uma operação protegida.

### INV-005

Uma transação incompatível com o estado não pode ser aceita.

### INV-006

Um Node comprometido não deve possuir automaticamente controle sobre outros Nodes.

### INV-007

Uma Comunidade não deve possuir autoridade sobre o consenso global.

### INV-008

Uma interface não deve possuir autoridade superior ao protocolo.

### INV-009

Credenciais defensivas devem possuir escopo limitado.

### INV-010

Modo emergencial não deve criar autoridade permanente.

---

# 71. Limites da especificação

Esta versão não define ainda:

* algoritmo de consenso;
* algoritmo criptográfico final;
* protocolo P2P definitivo;
* política monetária completa;
* fórmula de reputação;
* mecanismo anti-Sybil;
* algoritmo do Grande Mercado;
* matemática do Pool;
* bridge;
* sistema definitivo de armazenamento;
* linguagem de implementação.

Esses elementos serão tratados em especificações específicas.

---

# 72. Próximas especificações

A partir deste documento, a especificação poderá ser dividida em módulos:

```text
spec/
│
├── CRYPTOGRAPHY.md
├── IDENTITY.md
├── TRANSACTIONS.md
├── BLOCKS.md
├── STATE.md
├── CONSENSUS.md
├── P2P.md
├── PRIVACY.md
├── WALLET.md
├── ZERO.md
├── GOVERNANCE.md
├── REPUTATION.md
├── DEFENSE.md
├── EXONET.md
├── BROWSER.md
├── COMMUNITIES.md
├── MARKET.md
├── POOL.md
└── UPDATES.md
```

---

# 73. Ordem recomendada de implementação

A sequência técnica recomendada é:

```text
CRYPTOGRAPHY
     ↓
IDENTITY
     ↓
WALLET
     ↓
TRANSACTIONS
     ↓
STATE
     ↓
BLOCKS
     ↓
P2P
     ↓
CONSENSUS
     ↓
ZERO
     ↓
PRIVACY
     ↓
GOVERNANCE
     ↓
EXONET
     ↓
COMMUNITIES
     ↓
GRANDE MERCADO
     ↓
POOL
     ↓
DEFENSE
```

Essa ordem não significa que todos os componentes serão implementados linearmente. Alguns deverão ser prototipados em paralelo.

---

# 74. Critério de maturidade

Uma especificação não deverá ser considerada pronta simplesmente por estar escrita.

Ela deverá passar por:

```text
Especificação
     ↓
Revisão
     ↓
Protótipo
     ↓
Testes
     ↓
Ataques simulados
     ↓
Correções
     ↓
Nova revisão
     ↓
Especificação estável
```

---

# 75. Relação com os documentos anteriores

A hierarquia documental da Rede Zero é:

```text
MANIFESTO EXONET
      │
      ▼
REQUIREMENTS.md
      │
      ▼
THREAT_MODEL.md
      │
      ▼
ARCHITECTURE.md
      │
      ▼
SPECIFICATIONS.md
      │
      ▼
IMPLEMENTATION
```

Cada nível deve responder a uma pergunta diferente:

| Documento      | Pergunta                             |
| -------------- | ------------------------------------ |
| Manifesto      | Por quê?                             |
| Requirements   | O que deve existir?                  |
| Threat Model   | Contra o quê?                        |
| Architecture   | Como os componentes se organizam?    |
| Specifications | Quais são as regras técnicas exatas? |
| Implementation | Como o código realiza essas regras?  |

---

# 76. Status

**SPECIFICATIONS.md v0.1.0**

Status:

> **Especificação técnica inicial — não congelada.**

Nenhuma implementação deverá tratar este documento como protocolo definitivo até que as partes críticas tenham passado por protótipos, testes e revisão.

---

# 77. Próxima etapa

A próxima etapa deverá ser a criação das especificações especializadas.

A primeira deverá ser:

```text
spec/CRYPTOGRAPHY.md
```

seguida por:

```text
spec/IDENTITY.md
spec/WALLET.md
spec/TRANSACTIONS.md
spec/STATE.md
spec/BLOCKS.md
spec/P2P.md
spec/CONSENSUS.md
```

O objetivo será sair de conceitos como **“a Rede Zero precisa de criptografia”** para definições testáveis como:

```text
entrada
   ↓
operação criptográfica
   ↓
saída esperada
   ↓
regra de validação
   ↓
casos de erro
   ↓
teste de conformidade
```

**Status final:** `SPECIFICATIONS.md v0.1.0`
