# Plano de Integracao de Contas Premium

## Objetivo

Adicionar suporte a contas premium por provedor para melhorar:

- velocidade
- limites/quota
- disponibilidade
- downloads bloqueados para anonimos

Comecando por:

1. Mega

Depois abrindo a arquitetura para:

- 1Fichier
- Rapidgator
- NitroFlare
- Keep2Share
- outros hosters que tenham modo premium

## Resultado esperado

O usuario deve conseguir:

- cadastrar conta premium nas configuracoes
- validar login/token
- ver status da conta
- escolher usar premium automaticamente quando houver suporte

O provider deve:

- usar fluxo premium quando a conta estiver valida
- cair para fluxo publico/free quando fizer sentido
- expor mensagens claras de quota, expiracao e autenticacao

## Arquitetura sugerida

### Camada de autenticacao por provider

Criar uma interface padronizada no backend:

- `backend/src/accounts/mod.rs`
- `backend/src/accounts/base.rs`
- `backend/src/accounts/mega.rs`

### Trait base

Em Rust, o ideal aqui e uma trait de autenticacao por provider:

- `provider_name()`
- `login(credentials)`
- `logout()`
- `status()`
- `refresh_if_needed()`
- `get_account_info()`

### Estruturas sugeridas

- `ProviderAccountStatus`
  - `logged_in`
  - `tier`
  - `quota_total`
  - `quota_used`
  - `expires_at`
  - `last_error`

- `ProviderCredentials`
  - para Mega:
    - email
    - password
  - no futuro:
    - token
    - cookie
    - api_key

## Mega como primeiro alvo

### Fluxo de login

Para Mega, o app pode reaproveitar a stack que ja usa `megajs` no projeto.

O ideal e:

- login com email/senha
- manter sessao/token local
- consultar informacoes da conta
- reutilizar autenticacao em downloads suportados

### Ganhos esperados

- menos impacto de limite temporario para anonimos
- acesso a arquivos/fluxos que dependem de conta
- melhora de estabilidade em downloads longos

### Riscos

- sessao expirar
- 2FA
- limite de banda mesmo para premium em certas condicoes
- mudancas do SDK/fluxo do Mega

## UI e configuracoes

Adicionar em `Configuracoes` uma secao nova:

### Contas Premium

Cada provider com um card:

- nome e icone do provider
- status:
  - conectado
  - desconectado
  - sessao expirada
  - erro de autenticacao
- campos de credencial
- botao `Conectar`
- botao `Desconectar`
- botao `Testar`

### Campos para Mega

- `Email`
- `Senha`
- opcionalmente `Lembrar sessao`

### Exibicao de informacoes

- tipo de conta
- espaco usado/total
- eventual quota relevante para download

## Politica de armazenamento de credenciais

- nunca guardar senha em texto claro em logs
- preferir armazenar sessao/token quando o provider permitir
- mascarar senha na UI
- se possivel no futuro, usar armazenamento seguro por plataforma:
  - macOS Keychain
  - Windows Credential Manager
  - Secret Service no Linux

## Integracao com o scheduler/download

Quando um download for criado:

1. detectar provider
2. verificar se ha conta premium ativa para ele
3. escolher fluxo:
   - premium
   - publico/free

### Campos extras uteis no modelo

- `auth_mode`
  - `public`
  - `premium`
- `account_used`
  - nome do provider/autenticacao

## Integracao com mensagens de limite

A tela deve diferenciar:

- limite do host anonimo
- falha de login premium
- sessao premium expirada
- quota premium esgotada

Isso evita mensagens genericas do tipo “erro do servidor”.

## API interna sugerida

### Rotas

- `GET /accounts`
- `GET /accounts/:provider/status`
- `POST /accounts/:provider/login`
- `POST /accounts/:provider/logout`
- `POST /accounts/:provider/test`

## Ordem recomendada de implementacao

### Fase 1

- trait/base de contas
- modelo persistido de credenciais/sessao
- secao de configuracao na UI
- Mega login/test/logout

### Fase 2

- usar sessao Mega nos downloads
- melhorar mensagens de erro por autenticacao/quota
- expor informacoes da conta

### Fase 3

- abrir arquitetura para outros hosters premium
- adicionar refresh de sessao
- historico de falha de autenticacao

## Criterios de pronto

- usuario conecta conta Mega pela UI
- status da conta aparece corretamente
- downloads do Mega conseguem usar a sessao premium
- falhas de login/sessao aparecem com mensagem clara
- credenciais nao vazam em logs comuns
