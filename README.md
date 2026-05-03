# gDownloader

![Electron](https://img.shields.io/badge/Electron-39-47848F?logo=electron&logoColor=white)
![Vue 3](https://img.shields.io/badge/Vue-3-4FC08D?logo=vue.js&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-CE422B?logo=rust&logoColor=white)
![TypeScript](https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white)

Gerenciador de downloads desktop com Electron + Vue no frontend e backend local em Rust/Axum, com fila persistente em SQLite.

## Visão Geral

- UI desktop em Electron/Vue.
- `preload` faz a ponte segura entre renderer e Electron/main.
- `main` cuida de janelas auxiliares, captcha, auth por navegador integrado e bootstrap do backend.
- backend Rust expõe REST + WebSocket para providers, fila, scheduler, config e cache.
- SQLite local persiste fila, settings públicas, segredos locais, histórico e cache de metadados.

## Onde os dados ficam

Desenvolvimento:

- banco SQLite: `backend/database/gdownloader.db`
- logs do backend: `backend/logs/`

App empacotado:

- banco SQLite: `app.getPath('userData')/backend/database/gdownloader.db`
- logs do backend: `app.getPath('userData')/backend/logs/`

O `settings.json` legado só é lido para migração. O app atual usa SQLite como fonte única.

## O que fica salvo no SQLite

- settings públicas: tema, idioma, pasta de saída, concorrência, retries, limite de velocidade, partes paralelas, notificações, zoom e destaque
- settings seguras: `NoPecha API key`, cookies/sessões locais de contas suportadas
- fila completa de downloads
- histórico
- cache local de `file-info`
- histórico de migração legada

## Providers

### Suportados diretamente

- `Mega`
- `MediaFire`
- `Google Drive`
- `PixelDrain`
- `1Fichier`
- `Drime`
- `OneDrive / SharePoint`
- `Rapidgator`

### Suportados com fluxo assistido por navegador

- `TeraBox`
- `BRupload`
- `BRFiles`
- `AkiraBox`
- `Katfile`

### Planejados ou dependentes de ajuste do host

- nenhum host listado aqui no momento
- quando o host muda HTML, captcha, cooldown ou challenge, isso vira manutenção do provider atual

### Observações por host

- `TeraBox`: suporta arquivo e pasta. Usa navegador integrado quando o host exige sessão real. Se o host criar uma cópia temporária na conta para liberar o download, o app tenta limpar depois.
- `BRupload`: usa navegador integrado para contornar fluxo real do host. Conta free pode ser conectada dentro do app e a sessão fica só no SQLite local.
- `BRFiles`: suporta arquivo e pasta. Para pasta, o app retoma de onde parou quando o host impõe espera por IP.
- `Rapidgator`: mostra mensagens claras para arquivo removido, captcha, rate limit e premium obrigatório.
- `AkiraBox`: usa helper de navegador por causa de Cloudflare/challenge.
- `Katfile`: usa helper de navegador; links removidos retornam erro explícito.

## Captcha, conta e rate-limit

- Se houver `NoPecha` configurado, o app tenta resolver automaticamente primeiro.
- Se não resolver, o captcha abre em uma janela modal da própria página do host, não em `localhost`.
- Quando o host limita por IP ou por plano gratuito, o backend tenta extrair o tempo real de espera e a UI mostra contagem regressiva.
- Bloqueios de rate-limit não devem consumir as tentativas normais de erro do download.

## Cache local de metadados

No capturador de links:

- o app consulta primeiro o cache local de `file-info`
- depois sempre faz checagem online
- a UI mostra se o item está `online`, `offline` ou só veio do `cache local`

Isso acelera a leitura sem esconder quando o arquivo já caiu do host.

## Containers de links

O capturador aceita drag-and-drop de `.dlc`, `.ccf` e `.rsdf`.

- o backend recebe o upload em `POST /links/import-container`
- containers com URLs em texto puro são importados localmente
- containers criptografados são enviados ao decodificador remoto `dlc.piratejd.io`
- se o serviço remoto estiver indisponível, a UI mostra erro claro e os links já colados não são alterados

## Click'n'Load e extensão

O backend sobe um servidor local compatível com Click'n'Load em `127.0.0.1:9666`.

- `GET /jdcheck.js` permite que sites detectem o app como receptor Click'n'Load
- `POST /flash/add`, `/flash/addcrypted` e `/flash/addcrypted2` aceitam payload `form-encoded` com `urls`, `url`, `source`, `source_url`, `crypted` e `password`
- links recebidos são enviados para a mesma fila persistente do app, respeitando pasta de destino, retries, limite de velocidade e partes paralelas das settings
- payloads com URLs em texto puro ou `crypted` em base64 são importados; variantes criptografadas específicas de sites podem depender de suporte adicional

A pasta `browser-extension/` contém uma extensão MV3 para Chrome, Edge, Brave e Firefox. Ela detecta links suportados na página, mostra um botão flutuante e adiciona ações de menu de contexto para enviar links ao gDownloader. Veja `browser-extension/README.md` para instalar em modo desenvolvedor.

## Monitor de clipboard

Nas configurações, a seção `Integrações` tem a opção `Monitorar área de transferência`.

- quando ativada, o processo Electron verifica o clipboard a cada `800ms`
- URLs copiadas são validadas contra o `/detect` do backend, usando a mesma lógica dos providers
- se o link for suportado, o app abre o `Capturador de Links` e preenche a URL automaticamente
- a preferência fica salva no SQLite junto das settings públicas

## Arquitetura resumida

```text
Renderer (Vue)
  -> Preload (IPC + fetch REST + WS)
    -> Electron Main
      -> Backend Rust (Axum)
        -> Providers / Scheduler / SQLite
```

Fluxos especiais:

- auth/browser helper: `TeraBox`, `BRupload`, `AkiraBox`, `Katfile`
- mirrors: SSE do backend Rust
- progresso da fila: WebSocket

## Requisitos

- Node.js 20+
- npm 10+
- Rust stable

## Desenvolvimento

```bash
npm install
cd backend && cargo build --release && cd ..
npm run dev
```

## Verificações úteis

```bash
npm run typecheck:web
npm run typecheck:node
cd backend && cargo check
cd backend && cargo test
npx electron-vite build
```

## Troubleshooting

### O app mostra captcha e volta para a mesma página

- alguns hosts exigem fluxo real do navegador
- resolva o desafio na janela do host
- se houver `NoPecha`, configure a chave nas settings para a tentativa automática

### O download ficou aguardando por muito tempo

- isso costuma ser rate-limit por IP ou limite do plano gratuito do host
- veja o contador no item da fila; ele usa `retry_at` quando o host informa tempo

### O arquivo aparece no cache, mas offline no host

- isso é esperado
- o cache local acelera a leitura, mas a checagem online continua rodando para informar disponibilidade real

### Conta e cookies ficam onde?

- só no SQLite local do app
- não devem voltar no payload normal de `settings`

## Observação

Alguns hosters mudam HTML, countdown, captcha e política de limite com frequência. O projeto tenta expor erro claro e manter o fluxo resiliente, mas regressões por mudança do host são sempre possíveis.
