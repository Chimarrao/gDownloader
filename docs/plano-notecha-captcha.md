# Plano de Integracao de Solver de Captcha (`Notecha` / `NopeCHA`-like)

## Objetivo

Permitir que o `gDownloader` resolva automaticamente captchas encontrados em:

- hosters com countdown e botao final
- encurtadores tipo `adf.ly`, `shrtfly` e semelhantes
- wrappers intermediarios que bloqueiam o link final ate a resolucao do desafio

Aqui estou assumindo que `Notecha` significa um servico externo no estilo `NopeCHA`:

- API externa
- chave de acesso do usuario
- cobranca/consumo por resolucao

Se o servico final escolhido for outro, a arquitetura abaixo continua valida.

## Resultado esperado

Quando o app detectar que um fluxo exige captcha:

1. tenta resolver automaticamente via servico configurado
2. se resolver, continua o fluxo e obtém o link final
3. se falhar, informa claramente:
   - captcha nao resolvido
   - saldo/chave invalida
   - tipo de captcha nao suportado

## Escopo funcional

### Tipos de captcha a considerar

- reCAPTCHA v2 checkbox
- reCAPTCHA v2 invisible
- hCaptcha
- Turnstile
- image captcha simples

### Fora do escopo inicial

- Arkose/FunCaptcha pesado
- desafios que exigem navegador completo com execucao forte de JS
- fluxo com login humano permanente

## Arquitetura sugerida

### Backend

Criar um pacote novo:

- `backend/src/captcha/mod.rs`
- `backend/src/captcha/base.rs`
- `backend/src/captcha/notecha.rs`

### Trait base

Em Rust, o equivalente da “classe mae” aqui deve ser uma trait:

- `name()`
- `is_configured()`
- `supports(kind)`
- `solve(request) -> SolveResult`

### Tipos principais

- `CaptchaKind`
  - `RecaptchaV2`
  - `HCaptcha`
  - `Turnstile`
  - `Image`
- `CaptchaRequest`
  - `site_key`
  - `page_url`
  - `action`
  - `extra_payload`
  - `image_bytes`
- `CaptchaSolveResult`
  - `token`
  - `provider`
  - `cost_hint`
  - `latency_ms`
  - `raw_reference`

## Integracao com fluxos do app

### Link Grabber

Quando o `Link Grabber` detectar:

- pagina com captcha bloqueando o link final

deve mudar o estado para:

- `Captcha detectado`
- `Tentando resolver`
- `Captcha resolvido`
- `Falha ao resolver captcha`

### Resolvedor de shorteners

O plano do arquivo [bypass-shorteners.md](./bypass-shorteners.md) deve poder chamar o solver quando:

- um engine de shortener identificar pagina travada por captcha

### Providers futuros

Providers com fluxo premium/free ou anti-bot podem consultar o mesmo modulo.

## Configuracoes de UI

Adicionar em `Configuracoes` um bloco novo:

### Solver de Captcha

Campos sugeridos:

- `Ativar solver de captcha`
- `Provedor`
- `API Key`
- `Timeout maximo por captcha`
- `Numero maximo de tentativas`
- `Usar solver apenas para shorteners`
- `Usar solver tambem para hosters`
- `Modo seguro`
  - so tenta em dominios reconhecidos

### Estado visual

- chave configurada / nao configurada
- ultimo teste bem sucedido
- erro da ultima tentativa
- opcional: consumo estimado

## API interna sugerida

### Rotas backend

- `GET /captcha/status`
- `POST /captcha/test`
- `POST /captcha/solve`

### Teste de configuracao

O app deve permitir “Testar integracao” sem baixar nada.

## Politica de seguranca

- nunca logar a API key em texto claro
- mascarar a chave na UI
- guardar a chave em settings locais de forma separada do restante quando possivel
- aplicar timeout curto por requisicao
- validar dominio antes de enviar desafio para o solver

## Riscos tecnicos

- custo recorrente por captcha
- dependencia de servico externo
- mudancas no formato dos desafios
- hosters podem considerar isso fluxo hostil
- alguns captchas exigem browser/headless, e nao apenas token

## Politica de fallback

Se o solver falhar:

- exibir erro legivel
- permitir retry manual
- nao travar a fila inteira
- seguir com outros downloads normalmente

## Etapas de implementacao

### Fase 1

- criar configuracao persistida
- criar trait/base do solver
- criar integracao com um provider externo
- criar rota de teste

### Fase 2

- integrar com shorteners
- integrar com `Link Grabber`
- melhorar mensagens de erro

### Fase 3

- integrar com hosters que exigirem captcha
- registrar metricas de resolucao
- considerar browser/headless so se necessario

## Criterios de pronto

- usuario consegue colocar a chave nas configuracoes
- app consegue testar a chave
- shortener com captcha simples consegue ser resolvido
- mensagens de erro ficam compreensiveis
- nenhum segredo aparece em logs comuns
