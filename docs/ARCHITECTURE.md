# ARCHITECTURE.md

# Rede Zero / Exonet

**Versão:** 0.1.0
**Status:** Arquitetura conceitual — fase de definição
**Natureza:** Especificação arquitetural inicial
**Relacionamento:** `REQUIREMENTS.md v0.2.0` + `THREAT_MODEL.md v0.1.0`

---

# 1. Objetivo

Este documento define a arquitetura inicial da **Rede Zero** e da **Exonet**.

Seu objetivo é transformar os requisitos e o modelo de ameaças em uma estrutura técnica coerente, identificando:

* componentes;
* responsabilidades;
* relações entre componentes;
* fluxos de comunicação;
* limites de confiança;
* mecanismos de segurança;
* pontos de dependência;
* decisões ainda não definidas.

Este documento não representa ainda uma implementação final.

Nenhuma tecnologia específica deve ser adotada apenas por aparecer neste documento. Quando uma decisão depender de testes, benchmarks, auditorias ou protótipos, ela deverá permanecer explicitamente em estado de definição.

---

# 2. Princípio arquitetural central

A Rede Zero deve ser projetada para continuar funcionando sem depender de:

* uma pessoa;
* uma empresa;
* um servidor;
* um administrador;
* uma chave-mestra;
* uma implementação única;
* um repositório específico;
* uma infraestrutura específica;
* um Node específico.

A arquitetura deve buscar:

> **Nenhum ponto único de controle e nenhum ponto único de falha capaz de determinar sozinho o destino da Rede Zero.**

---

# 3. Visão geral

A arquitetura conceitual é:

```text
                         INTERNET
                            │
             ┌──────────────┴──────────────┐
             │                             │
             │          EXONET             │
             │                             │
             │   ┌─────────────────────┐   │
             │   │     REDE ZERO       │   │
             │   │                     │   │
             │   │ ┌─────────────────┐ │   │
             │   │ │      P2P        │ │   │
             │   │ └────────┬────────┘ │   │
             │   │          │          │   │
             │   │ ┌────────▼────────┐ │   │
             │   │ │   Blockchain    │ │   │
             │   │ └────────┬────────┘ │   │
             │   │          │          │   │
             │   │ ┌────────▼────────┐ │   │
             │   │ │     Consensus   │ │   │
             │   │ └─────────────────┘ │   │
             │   │                     │   │
             │   │ ┌─────────────────┐ │   │
             │   │ │ Identity/Crypto │ │   │
             │   │ └─────────────────┘ │   │
             │   │                     │   │
             │   │ ┌───────┐ ┌──────┐ │   │
             │   │ │ Wallet│ │ ZERO │ │   │
             │   │ └───────┘ └──────┘ │   │
             │   │                     │   │
             │   │ ┌─────────────────┐ │   │
             │   │ │ Grande Mercado  │ │   │
             │   │ └─────────────────┘ │   │
             │   │                     │   │
             │   │ ┌─────────────────┐ │   │
             │   │ │   Communities   │ │   │
             │   │ └─────────────────┘ │   │
             │   └─────────────────────┘   │
             │                             │
             └─────────────────────────────┘
```

A Internet fornece infraestrutura de comunicação.

A Exonet fornece o ambiente de protocolo.

A Rede Zero fornece as regras, estado, economia e serviços fundamentais desse ambiente.

---

# 4. Camadas da arquitetura

A Rede Zero será organizada conceitualmente em camadas.

```text
┌─────────────────────────────────────┐
│ Aplicações                          │
│ Comunidades / Grande Mercado        │
├─────────────────────────────────────┤
│ Interface                            │
│ Navegador Zero / outras interfaces  │
├─────────────────────────────────────┤
│ Serviços da Rede                    │
│ identidade / governança / reputação │
├─────────────────────────────────────┤
│ Estado da Rede                      │
│ blockchain / transações / ZERO      │
├─────────────────────────────────────┤
│ Consenso                            │
│ validação / finalização             │
├─────────────────────────────────────┤
│ P2P                                 │
│ descoberta / propagação / transporte│
├─────────────────────────────────────┤
│ Privacidade                         │
│ criptografia / anonimização         │
├─────────────────────────────────────┤
│ Internet                            │
│ infraestrutura de transporte        │
└─────────────────────────────────────┘
```

As camadas devem possuir interfaces bem definidas para reduzir acoplamento.

---

# 5. Node

Um **Node** é uma instância participante da infraestrutura da Rede Zero.

Um Node pode possuir diferentes capacidades.

A arquitetura não deve assumir que todos os participantes possuem hardware, banda ou disponibilidade iguais.

## 5.1 Tipos conceituais

A arquitetura poderá utilizar diferentes categorias:

### Node leve

Participa da rede sem armazenar necessariamente todo o estado.

Responsabilidades possíveis:

* comunicação;
* verificação limitada;
* acesso à Exonet;
* interação com outros Nodes.

### Node completo

Mantém uma cópia substancial do estado da Rede Zero e participa da validação.

### Node validador

Executa as funções necessárias ao mecanismo de consenso.

### Node de aplicação

Hospeda ou executa componentes de uma Comunidade ou outro serviço distribuído.

Um mesmo computador poderá exercer mais de uma função.

Os tipos definitivos serão determinados posteriormente.

---

# 6. Identidade do Node

Cada Node deverá possuir uma identidade criptográfica própria.

Conceitualmente:

```text
Node
 │
 ├── chave pública
 ├── chave privada
 └── Node ID
```

O **Node ID** deverá ser derivado de material criptográfico verificável.

A identidade do Node não representa necessariamente uma identidade civil.

A Rede Zero não deve exigir:

```text
Node ID → nome real
Node ID → CPF
Node ID → endereço residencial
Node ID → identidade civil
```

O objetivo é permitir que a rede reconheça uma identidade criptográfica sem precisar conhecer a pessoa por trás dela.

---

# 7. Carteira

A carteira deve ser arquiteturalmente separada do Node.

```text
┌─────────────┐
│   WALLET    │
│             │
│ chaves ZERO │
└──────┬──────┘
       │
       │ assinatura
       ▼
┌─────────────┐
│    NODE     │
│             │
│ comunicação │
│ protocolo   │
└─────────────┘
```

O Node não deve precisar possuir as chaves privadas da carteira do usuário.

Uma operação monetária deverá seguir, conceitualmente:

```text
Usuário
   │
   ▼
Carteira
   │
   │ cria + assina
   ▼
Transação
   │
   ▼
Node
   │
   ▼
Rede P2P
```

O Node transmite e verifica a transação, mas não deve ser automaticamente o custodiante dos fundos.

---

# 8. P2P

A Rede Zero deverá utilizar comunicação entre participantes sem depender de um servidor central obrigatório.

Conceito:

```text
      Node A
      /    \
     /      \
 Node B────Node C
   │          │
   │          │
 Node D────Node E
```

Cada Node poderá conhecer múltiplos pares.

A comunicação P2P deverá permitir:

* descoberta de pares;
* propagação de transações;
* propagação de blocos;
* sincronização;
* descoberta de serviços;
* recuperação após desconexões.

Nenhum Node individual deve ser indispensável para a continuidade da rede.

---

# 9. Descoberta de pares

A descoberta de pares deverá evitar dependência obrigatória de um servidor central.

Possíveis mecanismos a estudar:

* DHT;
* descoberta distribuída;
* bootstrap distribuído;
* listas de pares assinadas;
* mecanismos híbridos.

A tecnologia definitiva permanece **A DEFINIR**.

O mecanismo deverá considerar:

* ataques Sybil;
* eclipse attacks;
* nós maliciosos;
* disponibilidade;
* privacidade;
* resistência à manipulação da descoberta.

---

# 10. Blockchain

A blockchain representa o estado verificável da Rede Zero.

Conceitualmente:

```text
Bloco N-1
   │
   ▼
Bloco N
   │
   ▼
Bloco N+1
```

Cada bloco deverá permitir verificar sua relação com o estado anterior.

A blockchain deverá representar, entre outros:

* transações;
* saldos/estado monetário;
* regras do protocolo;
* eventos de governança;
* mudanças de protocolo;
* informações necessárias aos serviços nativos.

A estrutura exata dos blocos permanece **A DEFINIR**.

---

# 11. Estado da Rede

A Rede Zero deverá possuir um estado determinístico verificável.

Conceitualmente:

```text
Estado anterior
       │
       │ transações + regras
       ▼
Função determinística
       │
       ▼
Novo estado
```

Dois Nodes honestos que processam exatamente:

```text
mesmo estado anterior
+
mesmas transações
+
mesmas regras
```

devem chegar ao mesmo resultado.

Essa propriedade será fundamental para impedir estados conflitantes.

---

# 12. Consenso

O mecanismo de consenso é responsável por determinar qual estado da rede é considerado válido.

O mecanismo deverá tratar:

* dupla utilização de fundos;
* Nodes maliciosos;
* participantes offline;
* forks;
* reorganizações;
* censura;
* ataques Sybil;
* concentração de poder.

O algoritmo definitivo de consenso ainda não está escolhido.

A decisão deverá ser baseada em:

* segurança;
* descentralização;
* disponibilidade;
* desempenho;
* custo;
* resistência a ataques;
* facilidade de implementação;
* facilidade de auditoria.

---

# 13. Verificação independente

A arquitetura deve favorecer a regra:

> **Não confie; verifique.**

Um Node não deve aceitar uma informação apenas porque outro Node declarou que ela é válida.

Sempre que possível:

```text
Mensagem
   │
   ▼
Verificação criptográfica
   │
   ▼
Verificação das regras
   │
   ▼
Aceitar / rejeitar
```

---

# 14. ZERO

**ZERO** é a unidade monetária nativa da Rede Zero.

A arquitetura monetária deve reconhecer:

```text
Rede Zero
   │
   └── ZERO
```

Não haverá uma segunda moeda nativa oficial da Rede Zero.

Ativos provenientes de outras redes serão tratados como **ativos externos**, não como novas moedas nativas.

---

# 15. Política monetária

A política definitiva de ZERO ainda precisa ser especificada.

Deverá definir:

* emissão inicial;
* emissão futura;
* limite ou ausência de limite;
* recompensas;
* taxas;
* distribuição;
* unidades mínimas;
* regras de criação;
* regras de destruição, se existirem.

Nenhum mecanismo de emissão poderá permitir criação arbitrária de ZERO por um participante.

---

# 16. Taxas

A rede poderá utilizar taxas mínimas para determinadas operações.

Conceitualmente:

```text
Usuário
   │
   ▼
Operação
   │
   ▼
Taxa em ZERO
   │
   ▼
Mecanismo de distribuição
```

As regras exatas de distribuição ainda precisam ser definidas.

As taxas não devem criar um mecanismo que permita a um administrador escolher arbitrariamente quem recebe recursos.

---

# 17. Privacidade

Privacidade será tratada como uma propriedade arquitetural transversal.

Ela não deverá ser adicionada apenas posteriormente.

```text
P2P
 │
 ├── privacidade de rede
 │
Blockchain
 │
 ├── privacidade de transação
 │
Wallet
 │
 ├── proteção das chaves
 │
Aplicações
 │
 └── minimização de metadados
```

---

# 18. Privacidade das transações

A arquitetura deverá buscar proteção contra:

* identificação direta do remetente;
* identificação direta do destinatário;
* análise de valores;
* ligação entre transações;
* reutilização de identificadores;
* análise estatística.

O mecanismo criptográfico definitivo ainda será definido.

Possíveis famílias tecnológicas a estudar incluem:

* assinaturas em grupo;
* provas de conhecimento-zero;
* compromissos criptográficos;
* endereços furtivos;
* esquemas de anonimização.

Nenhuma delas é adotada definitivamente neste documento.

---

# 19. Privacidade de rede

A privacidade da blockchain não é suficiente para proteger a identidade de um participante.

A arquitetura deverá considerar separadamente:

```text
Privacidade da transação
+
Privacidade da comunicação
+
Minimização de metadados
```

Transportes como Tor ou I2P poderão ser estudados.

A utilização de uma camada de anonimização não deverá ser tratada como garantia de anonimato absoluto.

---

# 20. Exonet

A Exonet é a camada de rede sobre a infraestrutura da Internet na qual a Rede Zero opera.

```text
Internet
   │
   ▼
Transporte
   │
   ▼
Exonet
   │
   ▼
Rede Zero
```

A Exonet deverá possuir:

* identidade própria;
* endereçamento próprio;
* protocolo próprio;
* regras próprias;
* serviços próprios;
* mecanismo próprio de participação.

A Internet continuará sendo a infraestrutura física/lógica utilizada para transportar os dados.

---

# 21. Acesso à Exonet

O acesso à Exonet deverá depender da implementação do protocolo da Rede Zero.

A arquitetura não deve tentar descobrir se alguém está usando:

* Chrome;
* Firefox;
* Edge;
* outro navegador.

Em vez disso, deverá verificar se a comunicação apresenta os requisitos criptográficos e protocolares necessários.

Assim:

```text
Interface compatível
       │
       ▼
Identidade criptográfica
       │
       ▼
Protocolo Exonet
       │
       ▼
Rede Zero
```

Uma interface externa que não implemente o protocolo não terá acesso legítimo aos serviços da Exonet.

---

# 22. Navegador Zero

O **Navegador Zero** será a interface principal para interação humana com a Exonet.

Ele não será a própria Exonet.

```text
Navegador Zero
       │
       ▼
Protocolo Exonet
       │
       ▼
Rede Zero
```

A arquitetura deverá permitir futuramente outras interfaces compatíveis.

Isso evita que o Navegador Zero se transforme em um ponto único de controle.

---

# 23. Endereçamento da Exonet

A Exonet deverá possuir um sistema próprio de identificação de recursos.

Uma representação conceitual poderá ser:

```text
zero://grande-mercado

zero://comunidade/tecnologia

zero://comunidade/desenvolvedores

zero://comunidade/jogos
```

A sintaxe definitiva ainda será definida.

---

# 24. Comunidades

Comunidades são aplicações/ambientes distribuídos dentro da Exonet.

Uma Comunidade poderá possuir:

* interface;
* conteúdo;
* lógica;
* estado;
* usuários;
* regras próprias compatíveis com o protocolo.

Arquitetura:

```text
Comunidade
│
├── Frontend
│
├── Backend / lógica
│
├── Estado
│
└── Nodes responsáveis pela execução
```

---

# 25. Distribuição das Comunidades

Uma Comunidade não deve depender necessariamente de uma única máquina.

Quando tecnicamente aplicável:

```text
             Comunidade
                  │
       ┌──────────┼──────────┐
       ▼          ▼          ▼
     Node A     Node B     Node C
```

Isso aumenta a disponibilidade e reduz a dependência de um único operador.

A estratégia exata de replicação ainda será definida.

---

# 26. Criação de Comunidades

A criação de uma Comunidade deverá ocorrer através de um protocolo definido.

O processo conceitual será:

```text
Proposta
   │
   ▼
Validação técnica
   │
   ▼
Regras objetivas
   │
   ▼
Governança, quando exigida
   │
   ▼
Publicação
```

Não deverá existir um administrador capaz de excluir arbitrariamente uma Comunidade por decisão pessoal.

---

# 27. Grande Mercado

O **Grande Mercado** será o mecanismo de negociação nativo da Rede Zero.

Ele deverá existir como **protocolo**, e não apenas como uma página ou servidor.

```text
Usuário
   │
   ▼
Interface
   │
   ▼
Grande Mercado
   │
   ▼
Protocolos da Rede Zero
```

Isso permite múltiplas interfaces compatíveis sem criar múltiplos mercados oficiais.

---

# 28. Único mecanismo oficial de negociação

A arquitetura prevê um único Grande Mercado oficial dentro da Rede Zero.

Isso não significa que uma interface específica seja obrigatória.

Significa que as regras econômicas oficiais devem estar no protocolo do Grande Mercado.

```text
Interface A ─┐
Interface B ─┼──► Grande Mercado
Interface C ─┘
```

---

# 29. Pool permanente

A arquitetura econômica prevê um único Pool oficial.

Regra conceitual:

> **Qualquer ativo depositado no Pool permanece no Pool.**

Não deverá existir uma função administrativa capaz de retirar arbitrariamente os ativos.

```text
Ativo
 │
 ▼
Pool
 │
 └── permanece no Pool
```

Essa regra deverá ser garantida pelo protocolo, e não pela promessa de um administrador.

---

# 30. Ativos externos

O Pool poderá receber ativos originados de outras redes.

Esses ativos continuam sendo ativos externos.

```text
Rede externa
      │
      ▼
Mecanismo de integração
      │
      ▼
Pool da Rede Zero
```

A integração com outras blockchains será uma das áreas de maior risco técnico.

A arquitetura deverá estudar:

* bridges;
* light clients;
* provas criptográficas;
* atomic swaps;
* mecanismos sem custodiante;
* verificabilidade independente.

Uma ponte centralizada não deverá ser considerada automaticamente compatível com o princípio de descentralização.

---

# 31. Governança

A governança será implementada como parte do protocolo.

Conceito:

```text
Proposta
   │
   ▼
Análise
   │
   ▼
Discussão
   │
   ▼
Testes
   │
   ▼
Votação
   │
   ▼
Resultado verificável
   │
   ▼
Adoção
```

Nenhum indivíduo deverá possuir uma chave-mestra para substituir esse processo.

---

# 32. Votação

O sistema de votação deverá considerar simultaneamente:

* Sybil;
* concentração econômica;
* participação;
* segurança;
* verificabilidade.

O modelo definitivo ainda não foi escolhido.

A regra:

```text
1 carteira = 1 voto
```

não deverá ser adotada automaticamente, pois permitiria criação massiva de identidades.

Da mesma forma:

```text
1 ZERO = 1 voto
```

não deverá ser adotada automaticamente, pois pode concentrar poder em grandes detentores.

O modelo deverá ser estudado e testado.

---

# 33. Reputação

A reputação será associada à identidade criptográfica do Node.

Ela poderá considerar:

* histórico de participação;
* operações válidas;
* contribuição;
* disponibilidade;
* comportamento protocolar;
* violações verificadas.

A reputação não representa uma identidade civil.

Também não deverá representar autoridade permanente.

---

# 34. Comportamento atual

A arquitetura deve evitar que reputação histórica substitua a verificação atual.

Um Node com histórico excelente ainda deverá ser submetido às regras normais.

```text
Reputação histórica
        +
Comportamento atual
        +
Evidências verificáveis
        │
        ▼
Decisão protocolar
```

---

# 35. Defesa da Rede Zero

A defesa deverá ser distribuída.

Nenhum participante deve possuir sozinho poder ilimitado de defesa.

A arquitetura seguirá:

```text
Detectar
   ↓
Limitar
   ↓
Isolar
   ↓
Preservar evidências
   ↓
Corrigir
   ↓
Recuperar
   ↓
Aprender
```

---

# 36. Modos de defesa

A arquitetura mantém os três níveis conceituais definidos no modelo de ameaças.

## Modo 0 — Normal

Operação normal.

## Modo 1 — Vigilância

Anomalia identificada.

A rede aumenta observação sem presumir automaticamente que houve ataque.

## Modo 2 — Incidente

Existe evidência suficiente de comportamento hostil ou incompatível.

Podem ser aplicados:

* isolamento;
* limitação;
* redução de privilégios;
* quarentena;
* preservação de evidências.

## Modo 3 — Guerra Cibernética

Estado excepcional de defesa máxima diante de ameaça grave e persistente à Exonet ou aos seus participantes.

Esse modo permanece estritamente defensivo.

---

# 37. Guerra Cibernética

O termo **Guerra Cibernética** não representa autorização para atacar sistemas externos.

Não deverá permitir:

* invasão;
* malware;
* destruição;
* roubo de informações;
* retaliação automática;
* identificação invasiva de pessoas.

Seu objetivo é:

```text
proteger a Rede
+
proteger participantes
+
proteger consenso
+
proteger privacidade
+
proteger disponibilidade
+
preservar evidências
+
recuperar o sistema
```

---

# 38. Credenciais defensivas

Durante incidentes graves, Nodes com histórico verificável poderão receber credenciais defensivas temporárias.

Essas credenciais poderão permitir:

* diagnóstico;
* isolamento;
* análise;
* recuperação;
* coordenação defensiva;
* preservação de evidências.

A regra arquitetural será:

> **Mais reputação pode significar maior capacidade de ajudar na defesa, mas não maior soberania sobre a rede.**

As credenciais deverão ser:

* temporárias;
* limitadas;
* verificáveis;
* revogáveis;
* auditáveis.

---

# 39. Encerramento de incidentes

Nenhum modo emergencial deverá permanecer indefinidamente.

A arquitetura deverá possuir um protocolo de encerramento.

```text
Incidente
   │
   ▼
Contenção
   │
   ▼
Recuperação
   │
   ▼
Avaliação
   │
   ▼
Encerramento
   │
   ▼
Operação normal
```

O encerramento não deverá apagar evidências históricas.

---

# 40. Registro de defesa

Contribuições defensivas verificadas poderão ser registradas na identidade criptográfica do Node.

Um registro conceitual poderá conter:

```text
Incident ID
Node ID
função exercida
período
evidências
resultado
```

O registro não deverá revelar automaticamente a identidade civil do participante.

---

# 41. Desenvolvimento do protocolo

O desenvolvimento da Rede Zero deverá ser público.

Arquitetura de desenvolvimento:

```text
Requisitos
    ↓
Threat Model
    ↓
Architecture
    ↓
Especificações
    ↓
Protótipos
    ↓
Testes
    ↓
Implementação
    ↓
Auditoria
    ↓
Release
```

O GitHub pode ser utilizado como infraestrutura de desenvolvimento, mas não deve ser uma dependência necessária para a operação da Rede Zero.

---

# 42. Atualização do protocolo

Uma atualização não deverá ocorrer simplesmente porque um desenvolvedor publicou código novo.

O fluxo conceitual será:

```text
Proposta
   ↓
Análise
   ↓
Discussão
   ↓
Implementação experimental
   ↓
Testes
   ↓
Auditoria
   ↓
Aprovação
   ↓
Adoção
```

A implementação definitiva deverá possuir identificação de versão.

---

# 43. Compatibilidade

As versões do protocolo deverão possuir regras claras de compatibilidade.

A arquitetura deverá distinguir:

* atualização compatível;
* atualização incompatível;
* mudança de regras econômicas;
* mudança de consenso;
* correção de segurança.

Nenhum mecanismo de atualização deverá permitir que uma única entidade force arbitrariamente uma alteração de consenso.

---

# 44. Segurança de software

A arquitetura de desenvolvimento deverá considerar:

* revisão de código;
* testes automatizados;
* testes de integração;
* fuzzing;
* auditoria;
* análise de dependências;
* builds reproduzíveis;
* assinaturas de releases;
* divulgação responsável de vulnerabilidades.

---

# 45. Dependências externas

Cada dependência externa deverá ser avaliada segundo:

```text
Dependência
   │
   ├── Quem controla?
   ├── Pode desaparecer?
   ├── Pode ser comprometida?
   ├── Pode censurar?
   ├── Existe alternativa?
   └── A Rede continua funcionando sem ela?
```

Dependências críticas únicas deverão ser evitadas.

---

# 46. GitHub e infraestrutura de desenvolvimento

O GitHub será considerado ferramenta de desenvolvimento, não parte essencial da Exonet.

A Rede Zero deverá continuar existindo mesmo que:

* o repositório original desapareça;
* o GitHub fique indisponível;
* o mantenedor original desapareça;
* ocorra um fork;
* outro grupo continue o desenvolvimento.

A documentação deverá permitir que terceiros reconstruam o conhecimento necessário.

---

# 47. Modelo de confiança

A arquitetura seguirá o princípio de confiança mínima.

Não confiar automaticamente em:

* usuários;
* Nodes;
* servidores;
* administradores;
* interfaces;
* desenvolvedores;
* repositórios;
* dependências;
* histórico de reputação.

Sempre que possível:

```text
Afirmação
   ↓
Prova
   ↓
Verificação independente
   ↓
Decisão
```

---

# 48. Fluxo de uma transação

Fluxo conceitual:

```text
Usuário
   │
   ▼
Carteira
   │
   │ cria transação
   │ assina
   ▼
Transação assinada
   │
   ▼
Node
   │
   ├── verifica assinatura
   ├── verifica regras
   └── verifica estado
   │
   ▼
Rede P2P
   │
   ▼
Nodes validadores
   │
   ▼
Consenso
   │
   ▼
Bloco
   │
   ▼
Novo estado
   │
   ▼
Confirmação
```

---

# 49. Fluxo de acesso à Exonet

```text
Usuário
   │
   ▼
Navegador Zero
   │
   ▼
Identidade criptográfica
   │
   ▼
Camada de transporte
   │
   ▼
P2P Exonet
   │
   ▼
Rede Zero
   │
   ├── Blockchain
   ├── Comunidades
   └── Grande Mercado
```

---

# 50. Fluxo de criação de uma Comunidade

```text
Proponente
   │
   ▼
Definição da Comunidade
   │
   ▼
Validação técnica
   │
   ▼
Verificação das regras
   │
   ▼
Processo de governança
   │
   ▼
Publicação
   │
   ▼
Distribuição entre Nodes
```

---

# 51. Fluxo de defesa

```text
Anomalia
   │
   ▼
Detecção
   │
   ▼
Análise
   │
   ├── falsa anomalia → normal
   │
   └── evidência suficiente
             │
             ▼
          Incidente
             │
             ▼
          Isolamento
             │
             ▼
      Preservação de evidências
             │
             ▼
          Recuperação
             │
             ▼
       Encerramento
```

---

# 52. Separação de responsabilidades

Os componentes devem possuir responsabilidades diferentes.

| Componente     | Responsabilidade principal      |
| -------------- | ------------------------------- |
| Wallet         | chaves e assinaturas            |
| Node           | comunicação e participação      |
| P2P            | comunicação entre participantes |
| Blockchain     | estado verificável              |
| Consenso       | acordo sobre estado             |
| Criptografia   | autenticidade e privacidade     |
| Navegador Zero | interface                       |
| Exonet         | ambiente/protocolo de rede      |
| Comunidades    | aplicações distribuídas         |
| Grande Mercado | negociação nativa               |
| Pool           | reserva permanente              |
| Governança     | evolução das regras             |
| Reputação      | histórico verificável           |
| Defesa         | proteção da rede                |

Nenhum componente deverá receber responsabilidades desnecessárias.

---

# 53. Princípio de menor privilégio

Cada componente deverá possuir apenas os privilégios necessários à sua função.

Exemplo:

```text
Wallet
→ acesso às próprias chaves

Node
→ acesso às funções de rede necessárias

Comunidade
→ acesso apenas aos recursos autorizados

Defesa
→ privilégios temporários e limitados
```

Nenhum componente deverá receber acesso administrativo universal por conveniência.

---

# 54. Falha segura

Quando uma operação não puder ser validada corretamente:

> **Falha de verificação deve resultar em rejeição ou suspensão da operação, e não em autorização automática.**

Exemplo:

```text
Prova inválida
     ↓
não verificável
     ↓
REJEITAR
```

e não:

```text
Prova inválida
     ↓
"provavelmente correta"
     ↓
ACEITAR
```

---

# 55. Isolamento

Um componente comprometido não deverá comprometer automaticamente toda a rede.

Exemplo:

```text
Node comprometido
      │
      ▼
Isolamento
      │
      ├── Node afetado
      │
      └── restante da rede continua
```

A arquitetura deverá favorecer compartimentalização.

---

# 56. Recuperação

A Rede Zero deverá ser projetada não apenas para prevenção, mas também para recuperação.

Fluxo:

```text
Detectar
   ↓
Conter
   ↓
Isolar
   ↓
Preservar evidências
   ↓
Corrigir
   ↓
Recuperar
   ↓
Verificar
   ↓
Retornar à operação
```

---

# 57. Pontos críticos que exigem pesquisa

Os seguintes componentes ainda não possuem implementação definida:

### Consenso

* algoritmo;
* finalização;
* tolerância a falhas;
* custos.

### Privacidade

* protocolo criptográfico;
* modelo de anonimização;
* proteção contra análise de tráfego.

### P2P

* descoberta;
* roteamento;
* proteção contra eclipse;
* mecanismos anti-Sybil.

### ZERO

* emissão;
* distribuição;
* taxas;
* recompensas.

### Governança

* sistema de votação;
* resistência a Sybil;
* proteção contra concentração.

### Grande Mercado

* modelo matemático;
* execução;
* liquidação;
* proteção contra manipulação.

### Pool

* modelo econômico;
* integração de ativos externos;
* provas de entrada;
* impossibilidade de retirada.

### Comunidades

* armazenamento;
* execução;
* replicação;
* atualização.

### Exonet

* endereçamento;
* descoberta;
* resolução de nomes;
* transporte.

### Navegador Zero

* arquitetura;
* sandbox;
* segurança;
* integração com carteira.

---

# 58. Tecnologias ainda não escolhidas

Nenhuma das seguintes decisões deve ser considerada definitiva nesta versão:

* linguagem de programação;
* banco de dados;
* algoritmo de consenso;
* algoritmo criptográfico específico;
* protocolo P2P;
* sistema de armazenamento;
* mecanismo de DHT;
* formato de bloco;
* formato de transação;
* sistema de endereçamento;
* mecanismo de governança;
* modelo matemático do Grande Mercado.

Essas decisões deverão ser justificadas posteriormente.

---

# 59. Estratégia de implementação

A implementação deverá ocorrer por etapas.

## Fase 1 — Fundamentos

* estruturas de dados;
* criptografia básica;
* identidade;
* carteira;
* transações;
* validação.

## Fase 2 — P2P

* conexão entre Nodes;
* descoberta;
* propagação;
* sincronização.

## Fase 3 — Blockchain

* blocos;
* estado;
* validação;
* persistência.

## Fase 4 — Consenso

* protótipo;
* simulação;
* ataques;
* testes de falha.

## Fase 5 — Privacidade

* transações privadas;
* comunicação privada;
* minimização de metadados.

## Fase 6 — Exonet

* endereçamento;
* serviços;
* Navegador Zero.

## Fase 7 — Comunidades

* publicação;
* distribuição;
* execução;
* replicação.

## Fase 8 — Grande Mercado

* mecanismo econômico;
* Pool;
* testes matemáticos;
* integração de ativos externos.

## Fase 9 — Governança

* propostas;
* votação;
* atualização;
* proteção contra captura.

## Fase 10 — Defesa

* detecção;
* isolamento;
* incidentes;
* recuperação.

---

# 60. Testes arquiteturais

Antes de uma implementação ser considerada pronta, deverão ser realizados testes contra pelo menos:

* Node malicioso;
* múltiplos Nodes maliciosos;
* Sybil;
* eclipse;
* DoS;
* perda de Nodes;
* partição de rede;
* tentativa de double spend;
* tentativa de criação inválida de ZERO;
* comprometimento de carteira;
* comprometimento de interface;
* manipulação de governança;
* manipulação do Grande Mercado;
* falha de integração externa;
* atualização maliciosa;
* corrupção de dados;
* perda de infraestrutura.

---

# 61. Critério de aceitação arquitetural

Uma arquitetura somente deverá avançar para implementação definitiva quando for possível responder, de maneira verificável:

1. Como funciona?
2. Por que funciona?
3. Contra qual ameaça foi projetada?
4. O que acontece quando falha?
5. Quem precisa ser confiado?
6. O que acontece se esse participante for malicioso?
7. Como a rede detecta o problema?
8. Como limita o impacto?
9. Como recupera?
10. Como outro grupo poderia implementar a mesma especificação?

---

# 62. Independência de implementação

A especificação da Rede Zero deverá descrever **comportamento e regras**, e não depender excessivamente de uma implementação específica.

Assim:

```text
ESPECIFICAÇÃO
      │
      ├── Implementação A
      ├── Implementação B
      └── Implementação C
```

Implementações diferentes poderão coexistir desde que respeitem o protocolo.

Essa propriedade reduz a dependência de um único código-fonte.

---

# 63. Independência do fundador

A arquitetura deve permitir que o projeto continue sem seu criador original.

Isso exige:

* documentação;
* código aberto;
* testes;
* especificações;
* histórico de decisões;
* múltiplos contribuidores;
* possibilidade de forks;
* múltiplas implementações.

Princípio:

> **A Rede Zero pertence ao protocolo e à comunidade que o mantém, não ao indivíduo que iniciou o projeto.**

---

# 64. Estado atual da arquitetura

Nesta versão, as seguintes características são consideradas princípios arquiteturais estabelecidos:

* Rede Zero descentralizada;
* Exonet como camada sobre a Internet;
* Node com identidade criptográfica;
* separação entre Node e Wallet;
* blockchain;
* consenso distribuído;
* ZERO como única moeda nativa;
* privacidade por padrão;
* Grande Mercado como protocolo;
* um único Pool permanente;
* governança comunitária;
* reputação baseada em identidade criptográfica;
* defesa distribuída;
* Guerra Cibernética exclusivamente defensiva;
* Navegador Zero como interface;
* Comunidades como aplicações da Exonet;
* desenvolvimento aberto;
* ausência de autoridade absoluta;
* ausência de dependência de uma única infraestrutura.

---

# 65. Decisões explicitamente adiadas

Ainda não foram definidos:

* algoritmo de consenso;
* criptografia final;
* modelo de emissão de ZERO;
* sistema de votação;
* resistência a Sybil;
* arquitetura definitiva do P2P;
* sistema de endereçamento;
* mecanismo de anonimização;
* estrutura final dos blocos;
* estrutura final das transações;
* matemática do Grande Mercado;
* matemática do Pool;
* integração com outras blockchains;
* armazenamento distribuído;
* execução das Comunidades;
* arquitetura final do Navegador Zero;
* requisitos mínimos de hardware;
* linguagens de programação;
* APIs;
* formatos de mensagens;
* protocolos de atualização.

Essas decisões deverão ser tomadas somente após análise técnica, protótipos e testes.

---

# 66. Próxima etapa

O próximo documento recomendado é:

```text
SPECIFICATIONS.md
```

Ele deverá começar a transformar a arquitetura em especificações técnicas verificáveis.

A ordem proposta é:

```text
REQUIREMENTS.md
       ↓
THREAT_MODEL.md
       ↓
ARCHITECTURE.md
       ↓
SPECIFICATIONS.md
       ↓
PROTOTYPES
       ↓
TESTS
       ↓
IMPLEMENTATION
```

A primeira especificação técnica deverá provavelmente abordar o núcleo da rede:

```text
SPECIFICATIONS
│
├── identidade criptográfica
├── transações
├── blocos
├── estado
├── validação
├── P2P
└── consenso
```

---

**Status:** `ARCHITECTURE.md v0.1.0 — Arquitetura conceitual inicial`
