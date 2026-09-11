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

| Ícone | Provider |
| --- | --- |
| <img src="src/renderer/src/assets/provider-icons/mega.svg" width="22" height="22" alt="Mega"> | Mega |
| <img src="src/renderer/src/assets/provider-icons/mediafire.svg" width="22" height="22" alt="MediaFire"> | MediaFire |
| <img src="src/renderer/src/assets/provider-icons/googledrive.svg" width="22" height="22" alt="Google Drive"> | Google Drive |
| <img src="src/renderer/src/assets/provider-icons/pixeldrain.svg" width="22" height="22" alt="PixelDrain"> | PixelDrain |
| <img src="src/renderer/src/assets/provider-icons/1fichier.svg" width="22" height="22" alt="1Fichier"> | 1Fichier |
| <img src="src/renderer/src/assets/provider-icons/drime.svg" width="22" height="22" alt="Drime"> | Drime |
| <img src="src/renderer/src/assets/provider-icons/onedrive.svg" width="22" height="22" alt="OneDrive"> | OneDrive / SharePoint |
| <img src="src/renderer/src/assets/provider-icons/rapidgator.svg" width="22" height="22" alt="Rapidgator"> | Rapidgator |
| <img src="src/renderer/src/assets/provider-icons/internetarchive.svg" width="22" height="22" alt="Internet Archive"> | Internet Archive — arquivo e item/pasta |

### Suportados com fluxo assistido por navegador

| Ícone | Provider |
| --- | --- |
| <img src="src/renderer/src/assets/provider-icons/terabox.svg" width="22" height="22" alt="TeraBox"> | TeraBox |
| <img src="src/renderer/src/assets/provider-icons/brfiles.svg" width="22" height="22" alt="BRFiles"> | BRupload / BRFiles |
| <img src="src/renderer/src/assets/provider-icons/akirabox.svg" width="22" height="22" alt="AkiraBox"> | AkiraBox |
| <img src="src/renderer/src/assets/provider-icons/katfile.svg" width="22" height="22" alt="Katfile"> | Katfile |
| <img src="src/renderer/src/assets/provider-icons/sendnow.png" width="22" height="22" alt="Send.now"> | Send.now — links unitários e pastas |

### Planejados ou dependentes de ajuste do host

- nenhum host listado aqui no momento
- quando o host muda HTML, captcha, cooldown ou challenge, isso vira manutenção do provider atual

### Observações por host

- `TeraBox`: suporta arquivo e pasta. Usa navegador integrado quando o host exige sessão real. Se o host criar uma cópia temporária na conta para liberar o download, o app tenta limpar depois.
- `BRupload`: usa navegador integrado para contornar fluxo real do host. Conta free pode ser conectada dentro do app e a sessão fica só no SQLite local.
- `BRFiles`: suporta arquivo e pasta. Para pasta, o app retoma de onde parou quando o host impõe espera por IP.
- `Rapidgator`: mostra mensagens claras para arquivo removido, captcha, rate limit e premium obrigatório. Quando aparecer `Turnstile`/`reCAPTCHA`, usa o solver universal (mesmo de Katfile).
- `AkiraBox`: usa helper de navegador por causa de Cloudflare/challenge. Agora pode chamar `turnstileService.solve` universal se o challenge virar `Turnstile`.
- `Katfile`: usa helper de navegador + **solver universal 1-4** (`EzSolver`/`Icemellow`/`Surafel`/`FlareSolverr`) com fallback manual. `Tor` opcional bypassa `120min` via `IsolateSOCKSAuth`. Links removidos retornam erro explícito.
- `Send.now`: suporta pasta e link unitário. A sessão de navegador é persistida apenas para resolver o URL temporário quando o host aplica Cloudflare; o arquivo é baixado diretamente pelo backend.
- `Internet Archive`: URLs de item (`/download/<identificador>`) mostram os arquivos originais como grupo; URLs de arquivo continuam disponíveis individualmente.
- **Novos hosters:** basta adicionar o host em `providers/mod.rs` e, se tiver captcha, chamar `turnstileService.solve({sitekey, pageurl, type, provider})` — o manifesto `resources/solver-manifest.json` permite adicionar novo solver sem release do app.

## Captcha, solver universal e rate-limit

- **Solver universal (1,2,3,4) — atualizável como `yt-dlp`:** `EzSolver` (107★), `Icemellow V2` (dual `nodriver`+`camoufox`), `Surafel` (`patchright`) e `FlareSolverr` (15k★) ficam em `userData/turnstile/<id>/` com `venv` isolado. Cada solver é baixado como zip do GitHub (`main`/`master`) e instalado via `pip` exatamente como `yt-dlp` (`ytdlp-service.ts:121` `fetchLatestVersion`/`downloadBin`). `resources/solver-manifest.json` permite **auto-pull** de novos solvers sem atualizar o app — basta adicionar o repo ao manifesto e o próximo `ensureReady` baixa (mesmo `6h` cache do `yt-dlp`).
- **Uso universal:** qualquer hoster (`Katfile`, `Rapidgator`, futuros) chama `turnstileService.solve({sitekey, pageurl, type, provider, proxy})` que tenta em ordem `icemellow → ezsolver → surafelabeje → flaresolverr` com fallback. `type` padrão `turnstile`, mas filtra por `supportedTypes` do solver (`recaptcha2`/`hcaptcha` quando expandirmos). `proxy` `socks5://user:pass@127.0.0.1:9150` via Tor `IsolateSOCKSAuth` faz o solver ver o mesmo IP do download (evita token inválido).
- **Status visível:** enquanto resolve, o `Katfile` reporta `solving_captcha` (`katfile-service.ts:302` `solvingSolver`/`solvingStage`) que o backend (`katfile.rs:42` `solving_solver`) espelha como progresso `child_path: "solver:Resolvendo captcha com Icemellow..."`. A lista mostra chip roxo `Resolvendo captcha com <solver>` (mesma cor de `waiting_captcha` `download-display.ts:79` `#8b5cf6`). Se **nenhum** dos pacotes resolver, cai para **manual como último caso** — a janela `Katfile - conclua a etapa manual` abre após `6s` (`katfile-service.ts:483` `showTimeout`) exatamente como antes.
- **Se houver `NoPecha` configurado**, ainda tenta primeiro (legado), depois os solvers locais.
- Quando o host limita por IP ou por plano gratuito, o backend tenta extrair o tempo real de espera e a UI mostra contagem regressiva.
- Bloqueios de rate-limit não devem consumir as tentativas normais de erro do download.
- **Katfile + Tor:** `Delay between downloads must be not less than 120 minutes` é bypassado com Tor `IsolateSOCKSAuth` (`gdl-katfile-<id>:pass@127.0.0.1:9150` `src/main/index.ts:442` `per_file_socks_user` `mod.rs:492`). Cada arquivo usa circuito/IP distinto — `4/5` limpos em ≤3 tentativas no bench `docs/bench-turnstile-10-katfile-tor.md`.

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
- Go 1.22+ (para os 6 módulos migrados: `migrations`, `models`, `config`, `captcha`, `health`, `history`)

## Desenvolvimento

```bash
npm install
# Rust + Go são buildados juntos (Go compartilha o mesmo SQLite em WAL)
npm run dev              # concurrently: cargo watch (Rust) + go run (Go sidecar) + electron-vite
# ou separado:
# npm run dev:backend:rust  # só Rust
# npm run dev:backend:go    # só Go (go run . ../backend/database/gdownloader.db)
```

Go sidecar (`backend-go/`) porta `migrations.go:1` (22 migrações idempotentes), `models.go`, `config`, `captcha`, `health`, `history` — mesmo `app_kv` e `download_history` do Rust. `src/preload/index.ts:15` tenta Go primeiro para `/health /config/* /captcha* /history*` e cai para Rust se Go não estiver pronto.

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
