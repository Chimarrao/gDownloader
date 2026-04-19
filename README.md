# gDownloader

![Electron](https://img.shields.io/badge/Electron-39-47848F?logo=electron&logoColor=white)
![Vue 3](https://img.shields.io/badge/Vue-3-4FC08D?logo=vue.js&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-CE422B?logo=rust&logoColor=white)
![TypeScript](https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-supported-000000?logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-supported-FCC624?logo=linux&logoColor=black)
![Windows](https://img.shields.io/badge/Windows-supported-0078D4?logo=windows&logoColor=white)

<!-- keywords: download manager, electron app, rust backend, mega downloader, mediafire downloader, google drive downloader, pixeldrain, gerenciador de downloads, cliente de downloads, desktop app, fila de downloads, axum, vue3, tokio -->

> Gerenciador de downloads open-source com interface desktop (Electron/Vue 3) e backend em Rust/Axum.
> Baixe arquivos do Mega, MediaFire, Google Drive, PixelDrain, 1Fichier, Drime, Terabox e links públicos de OneDrive/SharePoint com uma interface limpa, suporte a filas, retries automáticos e controle de velocidade.

> As preferências e contas locais de teste ficam em `settings.json` na raiz do projeto, arquivo ignorado pelo Git.

---

## 📦 Provedores suportados

| Ícone | Provedor | Tipos suportados | Status | Observações |
|-------|----------|-----------------|--------|-------------|
| <img src="src/renderer/src/assets/provider-icons/mega.svg" alt="Mega" width="18" /> | **Mega** | Arquivo único, pasta pública | 🟢 Estável | Download sequencial de pastas; suporte a links `/file/` e formato legado `#!` |
| <img src="src/renderer/src/assets/provider-icons/mediafire.svg" alt="MediaFire" width="18" /> | **MediaFire** | Arquivo único, pasta pública | 🟢 Estável | Usa API pública `folder/get_content`; suporta subpasta via fragmento `#folderkey` |
| <img src="src/renderer/src/assets/provider-icons/googledrive.svg" alt="Google Drive" width="18" /> | **Google Drive** | Arquivo único público | 🟡 Parcial | Requer arquivo compartilhado publicamente; falta suporte a arquivos grandes com confirmação |
| 🟠 | **PixelDrain** | Arquivo único, lista | 🟢 Estável | Suporte a fragmento `#item=N` para listas; sem fragmento usa o primeiro arquivo |
| 1️⃣ | **1Fichier** | Arquivo único, pasta | 🟡 Parcial | Suporte a pastas, rate limit detectado via texto da página; download gratuito depende de cooldown |
| <img src="src/renderer/src/assets/provider-icons/onedrive.svg" alt="OneDrive" width="18" /> | **OneDrive / SharePoint** | Arquivo único público | 🟡 Parcial | O provider já reconhece o host; links que caem em `login.microsoftonline.com` exigem autenticação Microsoft e retornam erro claro |
| <img src="src/renderer/src/assets/provider-icons/terabox.svg" alt="Terabox" width="18" /> | **Terabox** | Arquivo único, pasta pública | 🟡 Parcial | Login via browser integrado (BrowserWindow isolado); captura cookies de sessão automaticamente |
| 💧 | **Drime** | Arquivo único, pasta pública | 🟡 Parcial | Share público com download resolvido via `shareable_link`; falta rodada maior de smoke com mais amostras |
| ⚡ | **Rapidgator** | Arquivo único (conta gratuita) | 🟡 Parcial | Suporte a reCaptcha v2 inline e rate limit; conta premium não testada |
| ☁️ | GoFile | — | ⚪ Planejado | API razoável, mas com mudanças frequentes |

**Legenda:** 🟢 Estável · 🟡 Parcial · ⚪ Planejado

Para mais detalhes sobre provedores futuros e dificuldades por hoster, veja [docs/provedores-futuros-dificuldades.md](docs/provedores-futuros-dificuldades.md).

---

## 🧭 Provedores planejados

### Mapeados primeiro

Drime, GoFile, Sendnow, Terabox, 1Fichier, BRUpload.

### Hosters, mirrors e serviços comuns em sites de download

FreeDL, DailyUploads, Uploady, UsersDrive, MixDrop, HexUpload, Clicknupload, UploadCloud, Racaty, KatFile, Rapidgator, NitroFlare, Turbobit, Keep2Share, FileJoker, DDownload, UploadGig, FastClick, Send.cm, Fikper, FileFactory, FileFox, Uploadboy, Up-4ever, MegaUp, BayFiles, AnonFiles-like mirrors, Uloz.to, FileRio, Drop.download, Upload-4ever, ModsFire, GameBanana downloads, Nexus Mods CDN/public links, CurseForge media, MediaFire mirror clones, File-upload.com, UploadEE, MirrorAce, MultiUp, Paste-like download pages, KrakenFiles, Qiwi.gg / Qiwi links, Lumpics / image host downloads, WorkUpload, EasyUpload, UploadNow.io, Uploadrar, DLFree.fr, Desiupload, Alfafile, HitFile, TakeFile, MexaShare, K2S-like mirrors, CosmoBox, FileSpace, Downace, Rosefile, Douploads, Uploadraja, MirrorUpload, MirrorCreator, DepositFiles, Uploaded / Ul.to-like mirrors, Zippyshare-like clones, DailyMotion attachments / mirrors, StreamTape, StreamWish, FileMoon, VidGuard, Voe.sx / Voe-like, doodstream, Uploadrar clones, DLUpload.

### Android, software, mods, datasets e arquivos públicos

AndroidFileHost, SourceForge mirror pages, Fosshub, APKMirror downloads, APKPure files, APKCombo downloads, Pling / OpenDesktop files, ModDB files, IndieDB files, Archive.org direct item files.

### Hosts simples, temporários e anônimos

Catbox, Litterbox, Pomf-like hosts, Pomf2 / uguu-like hosts, Uguu.se, file.io, tmpfiles.org, Temp.sh, AnonTransfer, Transfer.sh clones, Oshi.at, fileditch, pixeldrain-like mirrors.

### Clouds e wrappers que aparecem como mirror secundário

OneDrive, Proton Drive, Dropbox, pCloud, Box, Google Drive, MediaFire, Mega, PixelDrain, Nextcloud/public shares, onedrive short-link wrappers.

---

## 🛠 Tech Stack

| Camada | Tecnologia |
|--------|-----------|
| Interface desktop | [Electron 39](https://www.electronjs.org/) |
| Framework frontend | [Vue 3](https://vuejs.org/) + [TypeScript 5](https://www.typescriptlang.org/) |
| UI components | [PrimeVue 4](https://primevue.org/) |
| Build frontend | [electron-vite](https://electron-vite.org/) + [Vite 7](https://vitejs.dev/) |
| Backend HTTP | [Rust](https://www.rust-lang.org/) + [Axum](https://github.com/tokio-rs/axum) |
| Async runtime | [Tokio](https://tokio.rs/) |
| Comunicação | WebSocket (progresso em tempo real) + REST |
| Persistência | SQLite via [rusqlite](https://github.com/rusqlite/rusqlite) (modo WAL; retomada após reinício) |

---

## 📋 Requisitos

- **Node.js** >= 20
- **npm** >= 10
- **Rust** (stable, via [rustup](https://rustup.rs/))
- **cargo-watch** (opcional, para recompilação automática do backend em dev): `cargo install cargo-watch`
- **Plataformas testadas:** macOS (fluxo principal validado); Linux e Windows com base técnica preparada

---

## 🚀 Instalação (desenvolvimento)

```bash
# 1. Clone o repositório
git clone https://github.com/lucasreolon/gDownloader.git
cd gDownloader

# 2. Instale as dependências do frontend
npm install

# 3. Compile o backend Rust
cd backend && cargo build --release && cd ..

# 4. Inicie em modo desenvolvimento (frontend + backend em paralelo)
npm run dev
```

> O comando `npm run dev` inicia o backend Rust com `cargo watch` e o Electron/Vue com hot-reload simultaneamente.

### Build para distribuição

```bash
# Build completo (typecheck + electron-vite + cargo --release)
npm run build

# Empacotar por plataforma
npm run build:mac    # macOS (.dmg)
npm run build:win    # Windows (.exe)
npm run build:linux  # Linux (.AppImage / .deb)
```

---

## ⚙️ Configurações

Acessíveis pelo painel de configurações dentro do app:

- **Diretório de destino** — pasta padrão para salvar os downloads
- **Downloads simultâneos** — limite de quantos downloads rodam em paralelo
- **Limite de velocidade** — throttle global em KB/s (0 = sem limite)
- **Retries automáticos** — número de tentativas em caso de falha
- **Notificações nativas** — alertar ao concluir ou falhar um download
- **Tema** — dark / light
- **Cor de destaque** — personalize a cor primária da interface via color picker
- **NoPecha API key** — resolução automática de captchas (reCaptcha v2 / hCaptcha) via [NoPecha](https://nopecha.com/)
- **Contas** — Terabox: login via browser integrado (não armazenamos senha); captura cookies de sessão automaticamente

---

## 🧪 Testes

### Backend (Rust)

```bash
cd backend

# Todos os testes (unitários + integração)
cargo test

# Testes com links reais (smoke tests)
# Copie o arquivo de exemplo e preencha com seus links:
cp .env.test.example .env.test.local
cargo test -- --include-ignored
```

Smoke tests reais cobertos hoje:

- leitura de metadados de arquivo/pasta via `.env.test.local`
- início de download e aborto controlado após o primeiro progresso

Links reais já preparados para esse fluxo:

- `TEST_MEDIAFIRE_FILE_URL`
- `TEST_MEDIAFIRE_FOLDER_URL`
- `TEST_MEGA_FILE_URL`
- `TEST_MEGA_FOLDER_URL`
- `TEST_PIXELDRAIN_URL`

### Frontend (TypeScript)

```bash
# Verificação de tipos
npm run typecheck

# Testes unitários (Vitest)
npm run test
```

> Links reais de teste ficam em `backend/.env.test.local`, ignorado pelo Git. Use `backend/.env.test.example` como modelo.
