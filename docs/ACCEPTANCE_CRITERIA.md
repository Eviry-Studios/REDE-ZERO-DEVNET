# ACCEPTANCE_CRITERIA.md

# Rede Zero / Exonet

**Versão:** 0.1.0
**Status:** Critérios de aceitação iniciais
**Natureza:** Especificação normativa de aceitação

**Relacionamento:**

* `REQUIREMENTS.md v0.2.0`
* `THREAT_MODEL.md v0.1.0`
* `ARCHITECTURE.md v0.1.0`
* `SPECIFICATIONS.md v0.1.0`

---

# 1. Objetivo

Este documento define as condições que uma implementação da Rede Zero deve satisfazer para ser considerada conforme às especificações do projeto.

Os critérios aqui definidos descrevem **o que deve ser verdadeiro**.

Este documento não define:

* como testar;
* quais ferramentas utilizar;
* qual ambiente utilizar;
* quais comandos executar;
* como implementar os testes;
* resultados de execuções;
* configuração de CI;
* casos individuais de teste.

Esses elementos pertencem aos documentos:

```text
ACCEPTANCE_CRITERIA.md
        ↓
TEST_PLANS.md
        ↓
TEST_CASES.md
        ↓
TEST_RESULTS.md
```

---

# 2. Princípios de aceitação

## AC-001 — Conformidade verificável

Toda propriedade normativa da Rede Zero deverá possuir uma forma objetiva de verificar sua conformidade.

**Referências:**

* REQ-016
* REQ-078
* SPEC-003

---

## AC-002 — Comportamento determinístico

Para entradas equivalentes sob as mesmas regras de protocolo, implementações compatíveis deverão produzir resultados protocolários equivalentes.

**Referências:**

* REQ-014
* REQ-016
* SPEC-002

---

## AC-003 — Rejeição segura

Operações que não satisfaçam as regras do protocolo deverão ser rejeitadas ou permanecer sem efeito válido.

Uma falha de verificação não poderá ser interpretada como autorização.

**Referências:**

* REQ-018
* THREAT_MODEL.md §33
* SPEC-004

---

## AC-004 — Ausência de autoridade absoluta

Nenhuma identidade, chave, Node, servidor, componente ou participante poderá possuir autoridade permanente e unilateral para:

* modificar arbitrariamente o estado global;
* criar ZERO fora das regras;
* confiscar fundos;
* substituir o consenso;
* alterar unilateralmente as regras fundamentais.

**Referências:**

* REQ-005
* REQ-089
* INV-002
* INV-010

---

# 3. Identidade criptográfica

## AC-ID-001 — Identidade baseada em criptografia

Cada Node deverá possuir uma identidade criptográfica verificável.

A identidade deverá possuir relação determinística com o material criptográfico definido pelo protocolo.

**Referências:**

* REQ-009
* SPEC-ID-001
* SPEC-ID-003

---

## AC-ID-002 — Separação da identidade civil

O funcionamento protocolar básico de um Node não poderá exigir uma identidade civil.

**Referências:**

* REQ-010
* REQ-027
* SPEC-ID-004

---

## AC-ID-003 — Verificação de identidade

Uma identidade criptográfica deverá poder ser autenticada segundo as regras do protocolo.

Uma identidade sem prova criptográfica válida não deverá ser considerada autenticada.

**Referências:**

* REQ-016
* SPEC-ID-002

---

## AC-ID-004 — Proteção da chave privada

A chave privada utilizada para autorizar operações protegidas deverá permanecer sob controle do componente autorizado pelo usuário.

O protocolo não poderá exigir sua transmissão a Nodes de terceiros.

**Referências:**

* REQ-012
* SPEC-WAL-002
* SPEC-WAL-003

---

## AC-ID-005 — Rotação de identidade

Quando o protocolo permitir rotação de chaves ou identidade, o processo deverá possuir regras verificáveis de associação, validade e transição.

**Referências:**

* SPEC-ID-005

---

# 4. Wallet

## AC-WAL-001 — Separação entre Wallet e Node

A posse de uma Wallet não deverá exigir que sua chave privada seja entregue ao Node.

**Referências:**

* REQ-011
* REQ-030
* SPEC-WAL-001

---

## AC-WAL-002 — Assinatura autorizada

Operações que exigem autorização da Wallet somente poderão ser consideradas autorizadas quando possuírem assinatura válida conforme o protocolo.

**Referências:**

* REQ-029
* SPEC-WAL-002
* INV-004

---

## AC-WAL-003 — Não custódia obrigatória

A arquitetura não poderá exigir custódia central dos fundos dos usuários para o funcionamento básico da Rede Zero.

**Referências:**

* REQ-030
* SPEC-WAL-003

---

## AC-WAL-004 — Recuperação definida

A Wallet deverá possuir um mecanismo documentado para recuperação ou restauração compatível com o modelo de segurança adotado.

**Referências:**

* REQ-031

---

# 5. Codificação e identificadores

## AC-CAN-001 — Codificação canônica

Dados protocolários que exigem representação canônica deverão possuir uma representação determinística e não ambígua.

**Referências:**

* SPEC-008
* SPEC-002

---

## AC-CAN-002 — Identificador determinístico

Objetos protocolários que possuam identificador criptográfico deverão produzir o mesmo identificador quando seus dados protegidos forem equivalentes.

**Referências:**

* SPEC-007
* SPEC-002

---

## AC-CAN-003 — Alteração detectável

Alterações em dados protegidos que afetem sua identidade deverão produzir resultado criptográfico diferente ou invalidar as provas associadas.

**Referências:**

* SPEC-007
* SPEC-009
* INV-004

---

# 6. Transações

## AC-TX-001 — Estrutura válida

Uma transação somente poderá ser aceita quando possuir todos os campos obrigatórios e obedecer à estrutura definida pelo protocolo.

**Referências:**

* REQ-016
* SPEC-010

---

## AC-TX-002 — Assinatura válida

Uma transação com assinatura inválida deverá ser rejeitada.

**Referências:**

* REQ-016
* THREAT_MODEL.md T-032
* INV-004

---

## AC-TX-003 — Integridade da transação

Alterações posteriores à assinatura em qualquer conteúdo protegido deverão invalidar a autorização correspondente.

**Referências:**

* SPEC-009
* SPEC-010

---

## AC-TX-004 — Estado válido

Uma transação somente poderá ser aceita quando sua execução for compatível com o estado protocolar atual.

**Referências:**

* REQ-018
* SPEC-013
* INV-005

---

## AC-TX-005 — Ausência de gasto inválido

Uma mesma unidade ou recurso monetário não poderá produzir duas utilizações incompatíveis no mesmo estado final aceito.

**Referências:**

* THREAT_MODEL.md T-001
* SPEC-011

---

## AC-TX-006 — Taxa válida

Uma transação deverá obedecer às regras de taxa definidas pelo protocolo.

**Referências:**

* SPEC-010
* SPEC-036

---

## AC-TX-007 — Transação duplicada

A retransmissão de uma transação já processada não poderá produzir uma segunda alteração monetária indevida.

**Referências:**

* SPEC-010
* SPEC-012

---

# 7. Estado da rede

## AC-STATE-001 — Estado válido

O estado da Rede Zero deverá obedecer aos invariantes definidos pelo protocolo.

**Referências:**

* REQ-014
* SPEC-013

---

## AC-STATE-002 — Transição determinística

A aplicação do mesmo conjunto de operações válidas ao mesmo estado inicial deverá produzir o mesmo estado resultante.

**Referências:**

* SPEC-013
* SPEC-002
* INV-001

---

## AC-STATE-003 — Estado inválido rejeitado

Uma transição que produza estado incompatível com as regras deverá ser rejeitada.

**Referências:**

* REQ-018
* SPEC-014
* INV-001

---

## AC-STATE-004 — Integridade verificável

O estado da rede deverá possuir mecanismos que permitam verificar sua integridade segundo as regras do protocolo.

**Referências:**

* REQ-016
* SPEC-014

---

# 8. Blocos e cadeia

## AC-BLOCK-001 — Bloco válido

Um bloco somente poderá ser aceito quando satisfizer todas as regras estruturais e de estado aplicáveis.

**Referências:**

* SPEC-015

---

## AC-BLOCK-002 — Encadeamento válido

Cada bloco deverá possuir referência válida ao estado ou bloco anterior conforme definido pelo protocolo.

**Referências:**

* SPEC-016

---

## AC-BLOCK-003 — Altura válida

A altura de um bloco deverá obedecer às regras de progressão da cadeia.

**Referências:**

* SPEC-017

---

## AC-BLOCK-004 — Adulteração detectável

A alteração de dados protegidos de um bloco deverá tornar inválidas as provas ou referências afetadas.

**Referências:**

* SPEC-016
* INV-001

---

## AC-BLOCK-005 — Genesis correto

Um Node deverá distinguir corretamente a rede para a qual está configurado por meio da identificação definida para o Genesis.

**Referências:**

* SPEC-018
* SPEC-066

---

# 9. Consenso

## AC-CON-001 — Consenso verificável

O mecanismo de consenso deverá possuir regras públicas e determinísticas suficientes para que Nodes compatíveis possam verificar suas decisões.

**Referências:**

* REQ-015
* SPEC-019
* SPEC-020

---

## AC-CON-002 — Candidatos inválidos

Dados incompatíveis com as regras de consenso não poderão ser considerados válidos apenas por terem sido apresentados por outro Node.

**Referências:**

* THREAT_MODEL.md T-004
* SPEC-019

---

## AC-CON-003 — Convergência

Sob as condições previstas pelo protocolo, Nodes honestos deverão convergir para um estado compatível.

**Referências:**

* REQ-017
* SPEC-021

---

## AC-CON-004 — Forks

Ramificações da cadeia deverão ser tratadas segundo regras determinísticas e públicas de seleção, finalização ou rejeição.

**Referências:**

* SPEC-022

---

# 10. P2P

## AC-P2P-001 — Protocolo identificável

Mensagens de rede deverão possuir identificação suficiente para que o Node determine como tratá-las conforme o protocolo.

**Referências:**

* SPEC-024

---

## AC-P2P-002 — Mensagens inválidas

Mensagens malformadas ou incompatíveis deverão ser rejeitadas sem produzir alteração inválida do estado.

**Referências:**

* REQ-018
* SPEC-025

---

## AC-P2P-003 — Limitação de recursos

O protocolo deverá possuir mecanismos para limitar entradas que possam consumir recursos de forma abusiva.

**Referências:**

* THREAT_MODEL.md T-026
* SPEC-026

---

## AC-P2P-004 — Sincronização verificável

Dados obtidos durante sincronização deverão ser verificados antes de serem considerados parte do estado local válido.

**Referências:**

* REQ-016
* SPEC-027
* SPEC-028

---

## AC-P2P-005 — Resistência a isolamento

A arquitetura deverá possuir mecanismos compatíveis com o modelo de ameaça para reduzir o risco de um Node ser isolado por um conjunto de pares maliciosos.

**Referências:**

* THREAT_MODEL.md T-024
* SPEC-029

---

# 11. ZERO

## AC-ZERO-001 — ZERO como unidade nativa

ZERO deverá ser a unidade monetária nativa da Rede Zero.

**Referências:**

* REQ-019
* SPEC-033

---

## AC-ZERO-002 — Exclusividade monetária nativa

Nenhum segundo ativo deverá ser tratado como unidade monetária nativa da Rede Zero.

Ativos externos deverão permanecer identificáveis como ativos externos.

**Referências:**

* REQ-020
* SPEC-033
* SPEC-041

---

## AC-ZERO-003 — Criação monetária controlada

ZERO somente poderá ser criado ou emitido através dos mecanismos expressamente autorizados pelo protocolo.

**Referências:**

* REQ-021
* SPEC-034
* INV-003

---

## AC-ZERO-004 — Integridade monetária

Nenhuma operação válida poderá criar, destruir ou modificar ZERO fora das regras monetárias definidas pelo protocolo.

**Referências:**

* REQ-021
* SPEC-035

---

## AC-ZERO-005 — Saldos válidos

O estado monetário não poderá aceitar saldos incompatíveis com as regras de representação e transição do protocolo.

**Referências:**

* SPEC-035

---

# 12. Taxas e recompensas

## AC-FEE-001 — Taxas verificáveis

Taxas deverão ser calculadas de acordo com regras determinísticas e públicas.

**Referências:**

* SPEC-036

---

## AC-FEE-002 — Recompensas verificáveis

Qualquer mecanismo de recompensa deverá possuir regras determinísticas de cálculo, elegibilidade e distribuição.

**Referências:**

* REQ-002
* SPEC-037

---

## AC-FEE-003 — Ausência de recompensa arbitrária

Nenhuma entidade deverá possuir capacidade unilateral de atribuir recompensas fora das regras protocoladas.

**Referências:**

* REQ-005
* SPEC-037

---

# 13. Privacidade

## AC-PRIV-001 — Privacidade por padrão

A arquitetura deverá minimizar a exposição de informações desnecessárias durante as operações normais.

**Referências:**

* REQ-023
* SPEC-030

---

## AC-PRIV-002 — Separação de identidade civil

O protocolo não deverá exigir associação entre identidade criptográfica e identidade civil para participação básica.

**Referências:**

* REQ-010
* REQ-027

---

## AC-PRIV-003 — Minimização de metadados

Somente metadados necessários ao funcionamento do protocolo deverão ser transmitidos ou armazenados.

**Referências:**

* REQ-025
* SPEC-032

---

## AC-PRIV-004 — Proteção de chaves privadas

Chaves privadas não deverão ser incluídas em mensagens ou estruturas protocolárias.

**Referências:**

* REQ-012
* SPEC-031

---

## AC-PRIV-005 — Privacidade de rede

O protocolo deverá fornecer mecanismos destinados a reduzir a exposição da origem e dos metadados de comunicação conforme o modelo de privacidade definido.

**Referências:**

* REQ-026
* SPEC-030

---

## AC-PRIV-006 — Ausência de promessa absoluta

A documentação e implementação não deverão tratar anonimato como propriedade absoluta garantida contra qualquer observador ou comprometimento de endpoint.

**Referências:**

* REQ-028
* THREAT_MODEL.md §27

---

# 14. Grande Mercado

## AC-MKT-001 — Grande Mercado como protocolo

O Grande Mercado deverá existir como função protocolar, e não depender de uma única interface.

**Referências:**

* REQ-032
* REQ-033
* SPEC-038

---

## AC-MKT-002 — Múltiplas interfaces

Interfaces compatíveis poderão acessar o Grande Mercado sem que uma única interface possua autoridade exclusiva sobre ele.

**Referências:**

* REQ-034
* SPEC-038

---

## AC-MKT-003 — Autorização de operações

Operações de negociação somente poderão ocorrer quando autorizadas pelas regras correspondentes.

**Referências:**

* SPEC-038

---

## AC-MKT-004 — Ausência de autoridade central de negociação

Nenhuma entidade administrativa deverá possuir capacidade unilateral de decidir quais operações econômicas válidas podem ocorrer fora das regras do protocolo.

**Referências:**

* REQ-035
* SPEC-038

---

# 15. Pool permanente

## AC-POOL-001 — Pool único

A arquitetura oficial deverá reconhecer um único Pool conforme definido pelo protocolo.

**Referências:**

* REQ-036
* SPEC-039

---

## AC-POOL-002 — Permanência dos ativos

Ativos depositados no Pool deverão permanecer sujeitos à regra de permanência definida pelo protocolo.

**Referências:**

* REQ-037
* SPEC-040

---

## AC-POOL-003 — Ausência de retirada administrativa

Não deverá existir mecanismo administrativo unilateral para retirar ativos permanentemente depositados no Pool.

**Referências:**

* REQ-038
* SPEC-040
* INV-002

---

## AC-POOL-004 — Separação entre ZERO e ativos externos

Ativos externos depositados no Pool não poderão ser confundidos com ZERO.

**Referências:**

* REQ-039
* SPEC-041

---

## AC-POOL-005 — Integração verificável

Qualquer mecanismo de integração com ativos externos deverá possuir regras verificáveis de autenticidade, representação e segurança.

**Referências:**

* THREAT_MODEL.md T-038
* THREAT_MODEL.md T-039
* SPEC-042

---

# 16. Governança

## AC-GOV-001 — Governança comunitária

Alterações protocolárias sujeitas à governança deverão seguir um processo definido e verificável de participação comunitária.

**Referências:**

* REQ-040
* SPEC-043

---

## AC-GOV-002 — Propostas verificáveis

Propostas deverão possuir identificação e estado verificáveis.

**Referências:**

* REQ-041
* SPEC-043

---

## AC-GOV-003 — Prevenção de spam

O mecanismo de propostas deverá possuir proteção contra criação ilimitada de propostas abusivas.

**Referências:**

* REQ-042
* SPEC-044

---

## AC-GOV-004 — Votação verificável

Votos deverão obedecer às regras de autenticidade, elegibilidade, período e contagem definidas pelo protocolo.

**Referências:**

* REQ-043
* SPEC-045

---

## AC-GOV-005 — Resultado determinístico

A mesma votação válida deverá produzir o mesmo resultado quando processada conforme as mesmas regras.

**Referências:**

* REQ-043
* SPEC-046

---

## AC-GOV-006 — Resistência a Sybil

A criação de múltiplas identidades não deverá conceder influência ilimitada simplesmente pela criação dessas identidades.

**Referências:**

* REQ-044
* THREAT_MODEL.md T-007
* SPEC-047

---

## AC-GOV-007 — Limitação de concentração

O modelo de governança deverá possuir mecanismos compatíveis com o objetivo de reduzir captura excessiva por concentração econômica ou estrutural.

**Referências:**

* REQ-045
* THREAT_MODEL.md T-014
* SPEC-047

---

# 17. Reputação

## AC-REP-001 — Reputação verificável

Registros de reputação deverão possuir origem e integridade verificáveis.

**Referências:**

* REQ-046
* SPEC-048

---

## AC-REP-002 — Histórico verificável

Eventos relevantes utilizados para reputação deverão possuir histórico verificável conforme as regras do protocolo.

**Referências:**

* REQ-047
* SPEC-048

---

## AC-REP-003 — Comportamento atual

A reputação histórica não poderá tornar um Node permanentemente imune às regras de segurança.

**Referências:**

* REQ-048
* THREAT_MODEL.md T-050
* SPEC-048

---

## AC-REP-004 — Penalização objetiva

Penalizações deverão estar vinculadas a comportamentos ou violações definidos pelo protocolo.

Não deverão depender exclusivamente de julgamento subjetivo ou preferência pessoal.

**Referências:**

* REQ-049
* SPEC-049

---

## AC-REP-005 — Pesquisa não é ataque

Atividades legítimas de pesquisa, auditoria, desenvolvimento e análise não poderão ser classificadas automaticamente como comportamento malicioso.

**Referências:**

* REQ-050
* SPEC-050

---

# 18. Segurança e incidentes

## AC-SEC-001 — Defesa em camadas

A arquitetura deverá possuir mecanismos independentes ou complementares de proteção.

**Referências:**

* REQ-051
* THREAT_MODEL.md TM-004

---

## AC-SEC-002 — Detecção de anomalias

Comportamentos anômalos deverão poder ser diferenciados de operações normais segundo critérios definidos.

**Referências:**

* REQ-052
* SPEC-051

---

## AC-SEC-003 — Isolamento

Componentes comprovadamente incompatíveis poderão ser isolados sem conceder ao mecanismo de isolamento autoridade arbitrária sobre toda a rede.

**Referências:**

* REQ-053
* THREAT_MODEL.md T-055
* SPEC-053

---

## AC-SEC-004 — Evidência

A aplicação de medidas de segurança deverá possuir evidência verificável compatível com a ação realizada.

**Referências:**

* REQ-054
* TM-006
* SPEC-052

---

## AC-SEC-005 — Recuperação

Incidentes não deverão deixar a rede permanentemente dependente do estado emergencial.

**Referências:**

* REQ-055
* SPEC-055

---

# 19. Defesa da Exonet

## AC-DEF-001 — Finalidade defensiva

Os mecanismos de defesa deverão existir para proteger a Rede Zero, seus participantes, seu estado e sua continuidade.

**Referências:**

* REQ-056
* SPEC-055

---

## AC-DEF-002 — Ausência de retaliação externa

O protocolo não deverá possuir mecanismo destinado a:

* invadir sistemas externos;
* instalar malware;
* roubar informações;
* destruir infraestrutura externa;
* executar retaliação automática.

**Referências:**

* REQ-056
* THREAT_MODEL.md §36
* SPEC-055

---

## AC-DEF-003 — Escalonamento controlado

Mudanças entre níveis de defesa deverão obedecer a condições verificáveis.

**Referências:**

* REQ-057
* SPEC-053

---

## AC-DEF-004 — Emergência limitada

Privilégios emergenciais deverão possuir escopo, validade e condições de encerramento definidos.

**Referências:**

* REQ-058
* SPEC-054

---

## AC-DEF-005 — Ausência de poder emergencial permanente

O modo de emergência não poderá criar autoridade permanente sobre a Rede Zero.

**Referências:**

* REQ-059
* SPEC-054
* INV-010

---

## AC-DEF-006 — Defesa distribuída

A capacidade de defesa não deverá depender de uma única identidade ou participante.

**Referências:**

* REQ-060
* SPEC-053

---

## AC-DEF-007 — Credenciais temporárias

Credenciais defensivas deverão possuir escopo limitado e poderão ser revogadas conforme as regras do protocolo.

**Referências:**

* REQ-061
* SPEC-054

---

## AC-DEF-008 — Contribuição verificável

Registros ou recompensas relacionadas à defesa deverão depender de contribuição verificável conforme as regras definidas.

**Referências:**

* REQ-062
* SPEC-057

---

## AC-DEF-009 — Encerramento verificável

Um incidente somente poderá ser encerrado quando as condições protocolárias de encerramento forem satisfeitas.

**Referências:**

* REQ-063
* SPEC-056

---

## AC-DEF-010 — Preservação de evidências

O encerramento de um incidente não deverá apagar evidências necessárias à auditoria ou análise posterior.

**Referências:**

* REQ-054
* SPEC-056

---

# 20. Navegador Zero

## AC-BRW-001 — Interface Exonet

O Navegador Zero deverá permitir acesso às funcionalidades da Exonet por meio do protocolo definido.

**Referências:**

* REQ-064
* SPEC-058

---

## AC-BRW-002 — Interface não possui autoridade protocolar

O Navegador Zero não deverá possuir autoridade para alterar regras fundamentais da Rede Zero simplesmente por ser a interface oficial.

**Referências:**

* REQ-065
* INV-008

---

## AC-BRW-003 — Interfaces alternativas

Uma implementação alternativa compatível deverá poder interagir com a Exonet segundo as mesmas regras protocolárias.

**Referências:**

* REQ-066
* SPEC-059

---

## AC-BRW-004 — Autenticação protocolar

O acesso a recursos protegidos deverá depender das provas criptográficas e regras do protocolo, e não da identificação do software utilizado.

**Referências:**

* REQ-067
* SPEC-058

---

# 21. Comunidades

## AC-COM-001 — Comunidades como aplicações da Exonet

Uma Comunidade deverá operar dentro das regras da Exonet sem possuir autoridade sobre o protocolo global.

**Referências:**

* REQ-068
* SPEC-060

---

## AC-COM-002 — Publicação válida

Uma Comunidade somente poderá ser considerada oficial quando satisfizer os requisitos técnicos e de governança aplicáveis.

**Referências:**

* REQ-069
* REQ-071
* REQ-072

---

## AC-COM-003 — Regras objetivas

A validação de Comunidades deverá utilizar critérios objetivos e documentados.

**Referências:**

* REQ-073
* SPEC-060

---

## AC-COM-004 — Isolamento

Uma Comunidade não poderá alterar arbitrariamente:

* consenso;
* saldos globais;
* política monetária;
* outras Comunidades;
* regras fundamentais da Rede Zero.

**Referências:**

* REQ-074
* THREAT_MODEL.md T-044
* SPEC-061

---

## AC-COM-005 — Distribuição

A arquitetura deverá permitir distribuição ou replicação das partes da Comunidade que forem definidas como distribuíveis pelo protocolo.

**Referências:**

* REQ-070
* SPEC-060

---

# 22. Atualizações

## AC-UPD-001 — Atualização verificável

Uma atualização somente poderá ser reconhecida como válida quando satisfizer os mecanismos de autenticidade e integridade definidos.

**Referências:**

* REQ-087
* SPEC-063
* SPEC-064

---

## AC-UPD-002 — Atualização adulterada

Uma atualização cujo conteúdo não corresponda à autenticidade esperada deverá ser rejeitada.

**Referências:**

* THREAT_MODEL.md T-063
* SPEC-064

---

## AC-UPD-003 — Compatibilidade explícita

Alterações incompatíveis deverão ser identificáveis segundo as regras de versionamento do protocolo.

**Referências:**

* REQ-088
* SPEC-065

---

## AC-UPD-004 — Ausência de atualização arbitrária

Nenhuma identidade individual deverá possuir autoridade permanente para substituir unilateralmente o software protocolar aceito pela rede.

**Referências:**

* REQ-005
* THREAT_MODEL.md T-065
* SPEC-063

---

# 23. Desenvolvimento aberto

## AC-DEV-001 — Código público

O código necessário para compreender e verificar a implementação da Rede Zero deverá ser disponibilizado de forma compatível com a política de código aberto do projeto.

**Referências:**

* REQ-003
* REQ-075

---

## AC-DEV-002 — Contribuições externas

O processo de desenvolvimento deverá permitir contribuições de participantes externos.

**Referências:**

* REQ-076

---

## AC-DEV-003 — Revisão

Alterações relevantes deverão possuir processo documentado de revisão.

**Referências:**

* REQ-077

---

## AC-DEV-004 — Decisões documentadas

Decisões técnicas relevantes deverão possuir registro suficiente para que terceiros possam compreender sua motivação e impacto.

**Referências:**

* REQ-079

---

## AC-DEV-005 — Forkabilidade

O projeto deverá permanecer tecnicamente capaz de continuar por meio de forks ou cópias independentes.

**Referências:**

* REQ-080
* REQ-090

---

# 24. Independência da infraestrutura

## AC-INFRA-001 — Ausência de dependência crítica única

A operação da Rede Zero não deverá depender obrigatoriamente de uma única infraestrutura externa.

**Referências:**

* REQ-089
* THREAT_MODEL.md T-030

---

## AC-INFRA-002 — Independência do GitHub

A indisponibilidade do GitHub não deverá tornar impossível recuperar o conhecimento necessário para continuar o projeto.

**Referências:**

* REQ-090
* SPEC-063

---

## AC-INFRA-003 — Conhecimento distribuído

Informações essenciais para construção, operação e manutenção da Rede Zero deverão ser documentadas de forma que possam ser utilizadas por múltiplos participantes.

**Referências:**

* REQ-091
* REQ-097

---

# 25. Continuidade

## AC-CONT-001 — Nenhum indivíduo indispensável

A continuidade técnica da Rede Zero não deverá depender permanentemente de uma única pessoa.

**Referências:**

* REQ-096
* REQ-099

---

## AC-CONT-002 — Sucessão técnica

Outro grupo de contribuidores deverá poder assumir a manutenção técnica utilizando o conhecimento público disponível.

**Referências:**

* REQ-098

---

## AC-CONT-003 — Continuidade sem fundador

A ausência do fundador não deverá, por si só, impedir o desenvolvimento ou funcionamento da Rede Zero.

**Referências:**

* REQ-099
* THREAT_MODEL.md T-076

---

# 26. Invariantes fundamentais

Os seguintes invariantes deverão permanecer verdadeiros em qualquer estado aceito:

## AC-INV-001

Nenhuma transação inválida poderá produzir estado válido.

**Referência:** `INV-001`

---

## AC-INV-002

Nenhum Node individual poderá possuir autoridade monetária absoluta.

**Referência:** `INV-002`

---

## AC-INV-003

ZERO não poderá ser criado arbitrariamente.

**Referência:** `INV-003`

---

## AC-INV-004

Uma assinatura inválida não poderá autorizar uma operação protegida.

**Referência:** `INV-004`

---

## AC-INV-005

Uma operação incompatível com o estado não poderá ser aceita como válida.

**Referência:** `INV-005`

---

## AC-INV-006

O comprometimento de um Node não deverá conceder automaticamente controle sobre os demais Nodes.

**Referência:** `INV-006`

---

## AC-INV-007

Uma Comunidade não deverá possuir autoridade sobre o consenso global.

**Referência:** `INV-007`

---

## AC-INV-008

Uma interface não deverá possuir autoridade sobre o protocolo simplesmente por ser uma interface.

**Referência:** `INV-008`

---

## AC-INV-009

Credenciais defensivas deverão permanecer limitadas ao escopo para o qual foram concedidas.

**Referência:** `INV-009`

---

## AC-INV-010

Um modo de emergência não poderá criar autoridade permanente.

**Referência:** `INV-010`

---

# 27. Critérios críticos

Os seguintes grupos serão considerados críticos para a conformidade do protocolo:

```text
Integridade criptográfica
Integridade das transações
Integridade do estado
Consenso
Integridade monetária
Proteção contra double spend
Atualizações
Ausência de autoridade absoluta
Isolamento de componentes comprometidos
Determinismo
```

Uma implementação que viole qualquer propriedade crítica não deverá ser considerada conforme, independentemente de seu desempenho ou quantidade de funcionalidades adicionais.

---

# 28. Critérios bloqueados

Quando uma propriedade depender de uma decisão técnica ainda não especificada, seu critério poderá existir neste documento, mas seu método de verificação permanecerá pendente.

Exemplo:

```text
AC-SYB-001
Status: BLOCKED

Motivo:
mecanismo definitivo de resistência a Sybil ainda não especificado.
```

`BLOCKED` não significa `PASS`.

Também não significa que a propriedade tenha sido dispensada.

---

# 29. Rastreabilidade

Cada critério deverá possuir, sempre que aplicável, rastreabilidade para:

```text
Requirement
      ↓
Threat
      ↓
Architecture
      ↓
Specification
      ↓
Acceptance Criterion
      ↓
Test Plan
      ↓
Test Case
      ↓
Test Result
```

A ausência de uma etapa deverá ser explicitamente justificada.

---

# 30. Critério de conformidade da implementação

Uma implementação poderá declarar conformidade somente quando:

1. os requisitos aplicáveis estiverem especificados;
2. os critérios de aceitação aplicáveis estiverem definidos;
3. os critérios críticos forem satisfeitos;
4. os testes correspondentes tiverem sido executados;
5. não existirem violações críticas conhecidas;
6. a implementação produzir resultados compatíveis com o protocolo;
7. a documentação necessária estiver disponível.

---

# 31. Critério para novas funcionalidades

Uma nova funcionalidade somente poderá ser considerada concluída quando possuir:

```text
Requisito
    ↓
Especificação
    ↓
Critério de aceitação
    ↓
Plano de teste
    ↓
Casos de teste
    ↓
Resultado verificável
```

Uma funcionalidade implementada sem esses elementos deverá permanecer explicitamente incompleta.

---

# 32. Critério para correções

Uma correção relacionada a uma falha deverá atualizar os artefatos afetados quando necessário.

Fluxo:

```text
Falha
 ↓
Correção
 ↓
Critério afetado
 ↓
Teste correspondente
 ↓
Regressão
```

Uma correção não deverá simplesmente remover ou enfraquecer o critério que revelou o problema.

---

# 33. Critério para vulnerabilidades

Quando uma vulnerabilidade alterar a compreensão de uma ameaça, deverão ser avaliados:

* `THREAT_MODEL.md`;
* `ARCHITECTURE.md`;
* `SPECIFICATIONS.md`;
* especificações especializadas;
* critérios de aceitação;
* planos de teste;
* casos de teste.

**Referências:**

* REQ-081
* REQ-082
* REQ-083

---

# 34. Relação com os próximos documentos

Este documento define **o que deve ser verdadeiro**.

Os próximos documentos deverão definir:

### `TEST_PLANS.md`

**Como, onde, quando e em quais condições os critérios serão verificados.**

### `TEST_CASES.md`

**Quais cenários individuais serão executados.**

### `TEST_RESULTS.md`

**Quais foram os resultados reais dessas execuções.**

---

# 35. Estado atual

**Versão:** `0.1.0`

**Status:**

> **Critérios de aceitação iniciais — não congelados.**

Diversos critérios dependem de especificações especializadas que ainda estão em desenvolvimento.

Isso é intencional.

Critérios não deverão inventar decisões técnicas que ainda não foram tomadas.

---

# 36. Regra fundamental

A Rede Zero não deverá considerar uma propriedade verdadeira apenas porque ela foi declarada em documentação.

A propriedade deverá possuir:

```text
REQUISITO
    ↓
ESPECIFICAÇÃO
    ↓
CRITÉRIO DE ACEITAÇÃO
    ↓
TESTE
    ↓
EVIDÊNCIA
```

> **O critério de aceitação define o que significa estar correto.**
>
> **O teste demonstra se isso realmente acontece.**

---

**Status final:** `ACCEPTANCE_CRITERIA.md v0.1.0`
