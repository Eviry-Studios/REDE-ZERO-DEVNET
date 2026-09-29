# Política de Segurança

A Rede Zero é um projeto de código aberto e **pesquisa de segurança é bem-vinda** (`REQ-050`, `SPEC §50`). Ler o código, compilar, testar, fazer fuzzing no seu próprio ambiente e publicar ferramentas não é tratado como comportamento malicioso.

## Estado atual

O projeto está em fase **DEVNET**: não há rede principal, e o ZERO da DEVNET **não possui valor econômico**. Ainda assim, vulnerabilidades no protocolo ou na implementação devem ser tratadas com cuidado, porque o código e as decisões de hoje serão a base das próximas fases.

## Como reportar uma vulnerabilidade

1. **Não abra uma issue pública** para vulnerabilidades que afetem integridade monetária, consenso, chaves, privacidade ou execução remota de código.
2. Use o recurso de **relato privado de vulnerabilidades** do GitHub: aba **Security → Report a vulnerability** deste repositório.
3. Inclua, se possível:
   * componente afetado e versão/commit;
   * descrição do impacto;
   * passos de reprodução ou prova de conceito;
   * ameaças relacionadas em `docs/THREAT_MODEL.md`, se souber.

Problemas de baixo impacto (documentação, robustez sem impacto de segurança) podem ser reportados em issues públicas normalmente.

## O que esperar

* Confirmação de recebimento assim que um mantenedor analisar o relato.
* Discussão coordenada da correção e do prazo de divulgação.
* Crédito público ao pesquisador, se desejar.

## Limites

Pedimos que a pesquisa **não** envolva:

* ataques contra Nodes, computadores ou infraestrutura de terceiros sem autorização;
* negação de serviço contra ambientes compartilhados;
* acesso a dados de outras pessoas.

Use sua própria DEVNET local para testes (ver `README.md`).

## Segredos no repositório

Este repositório é **público**. Nenhuma chave privada, token, senha ou dado pessoal deve ser versionado. Chaves de DEVNET são geradas localmente e ficam fora do controle de versão (ver `.gitignore`).
