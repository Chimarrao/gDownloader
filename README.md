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
> Baixe arquivos do Mega, MediaFire, Google Drive e PixelDrain com uma interface limpa, suporte a filas, retries automáticos e controle de velocidade.

---

## ✨ Funcionalidades

- Download de arquivos e pastas para todos os provedores suportados
- Fila de downloads com limite configurável de simultâneos
- Retry automático configurável + retry manual por item
- Pause e resume por item e em lote
- Controle de velocidade global (speed limit em KB/s)
- Notificação nativa ao concluir download
- Link Grabber — inspeciona e enfileira múltiplos links com preview de pastas
- Extração de arquivos (ZIP, RAR, 7z, TAR) integrada
- Download em paralelo com múltiplas partes
- Suporte a tema dark/light com paleta roxa personalizável
- Histórico de downloads persistido na sessão
- Gráfico de velocidade global em tempo real (sparkline)
- Ícones de arquivo por categoria com fallback por extensão e MIME

---

## 📦 Provedores suportados

| Ícone | Provedor | Tipos suportados | Status | Observações |
|-------|----------|-----------------|--------|-------------|
| <img src="src/renderer/src/assets/provider-icons/mega.svg" alt="Mega" width="18" /> | **Mega** | Arquivo único, pasta pública | 🟢 Estável | Download sequencial de pastas; suporte a links `/file/` e formato legado `#!` |
| <img src="src/renderer/src/assets/provider-icons/mediafire.svg" alt="MediaFire" width="18" /> | **MediaFire** | Arquivo único, pasta pública | 🟢 Estável | Usa API pública `folder/get_content` para listagem de pastas |
| <img src="src/renderer/src/assets/provider-icons/googledrive.svg" alt="Google Drive" width="18" /> | **Google Drive** | Arquivo único público | 🟡 Parcial | Requer arquivo compartilhado publicamente; falta suporte a arquivos grandes com confirmação |
| 🟠 | **PixelDrain** | Arquivo único, lista | 🟢 Estável | Suporte a fragmento `#item=N` para listas; sem fragmento usa o primeiro arquivo |
| <img src="src/renderer/src/assets/provider-icons/onedrive.svg" alt="OneDrive" width="18" /> | OneDrive | — | ⚪ Planejado | Alto valor como mirror em sites de download |
| <img src="src/renderer/src/assets/provider-icons/terabox.svg" alt="Terabox" width="18" /> | Terabox | — | ⚪ Planejado | Requer tratamento de tokens e cookies |
| ☁️ | GoFile | — | ⚪ Planejado | API razoável, mas com mudanças frequentes |
| 📦 | 1Fichier | — | ⚪ Planejado | Cooldown/captcha forte |

**Legenda:** 🟢 Estável · 🟡 Parcial · ⚪ Planejado

Para mais detalhes sobre provedores futuros e dificuldades por hoster, veja [docs/provedores-futuros-dificuldades.md](docs/provedores-futuros-dificuldades.md).

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
- **Extração automática** — extrair arquivos comprimidos ao concluir
- **Notificações nativas** — alertar ao concluir ou falhar um download
- **Tema** — dark / light com paleta de cores personalizável

---

## 🗺 Roadmap

Consulte [docs/roadmap-pos-itens-atuais.md](docs/roadmap-pos-itens-atuais.md) para o planejamento detalhado. Destaques:

- [ ] Suporte a OneDrive (links públicos e mirrors)
- [ ] Suporte a Terabox
- [ ] Suporte a GoFile
- [ ] Suporte a 1Fichier
- [ ] Suporte a Proton Drive
- [ ] Suporte a FreeDL
- [ ] Contas premium (aumento de velocidade e sem cooldown)
- [ ] Bypass de encurtadores de link
- [ ] Agendamento de downloads

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

> Os smoke tests de arquivo baixam apenas um pequeno `Range` do arquivo para validar o fluxo real de resolução sem consumir banda desnecessária.

### Frontend (TypeScript)

```bash
# Verificação de tipos
npm run typecheck

# Testes unitários (Vitest)
npm run test
```

> Links reais de teste ficam em `backend/.env.test.local`, ignorado pelo Git. Use `backend/.env.test.example` como modelo.

---

## 🤝 Contribuindo

Contribuições são bem-vindas! Para começar:

1. Faça um fork do repositório
2. Crie uma branch para sua feature: `git checkout -b feat/minha-feature`
3. Implemente e escreva testes quando aplicável
4. Abra um Pull Request descrevendo o que foi feito

Para adicionar um novo provedor, veja como os providers existentes estão implementados em `backend/src/providers/` e siga o mesmo padrão de trait.

Antes de enviar um PR com links reais nos testes, certifique-se de que eles estão em `.env.test.local` (ignorado pelo Git) e não commitados.

---

## 📄 Licença

Este projeto ainda não possui um arquivo de licença formal. Por enquanto, o código é disponibilizado publicamente para fins de estudo e contribuição. Uma licença open-source será adicionada em breve.

---

<sub>gDownloader — gerenciador de downloads desktop, mega downloader, mediafire downloader, google drive downloader, pixeldrain client, electron rust app</sub>
