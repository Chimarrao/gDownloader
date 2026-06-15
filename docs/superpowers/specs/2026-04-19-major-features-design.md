# gDownloader — Design Spec: Major Features
**Date:** 2026-04-19  
**Status:** Approved

---

## 1. Settings Completo

**Problema:** O campo de cor de acento customizada não é persistido no `settings.json`.

**Solução:**
- Adicionar `accentColor?: string` ao `PersistedSettings` em `src/shared/types.ts`
- Salvar junto com o tema no fluxo existente (`settings:save` IPC)
- No load, aplicar via CSS variable `--p-primary-color` no `<html>`
- Sem mudança arquitetural — campo extra no JSON

---

## 2. Terabox Login via BrowserView

**Problema:** Fluxo RSA/email+senha quebra frequentemente com mudanças de API do Terabox.

**Solução:** Electron `BrowserView` com CSS injection.

**Fluxo:**
1. Usuário clica "Conectar Terabox" em AccountSettings
2. Main process abre um `BrowserView` carregando `https://www.terabox.com/login`
3. Injeta CSS que esconde header, footer, banners — mostra só o formulário de login
4. Monitora `session.cookies.get()` em polling de 500ms
5. Quando detectar cookies de sessão válidos (`ndus`, `ndut` etc.), fecha o BrowserView
6. Cookies passam pra `settings.json` → backend usa no header das requisições
7. Se aparecer captcha durante login: ele aparece naturalmente no formulário (sem extração extra)

**Remoção:** Deletar código RSA de `src/main/index.ts` linhas 277–448. Manter handlers IPC `auth:*`.

---

## 3. Persistência SQLite de Downloads

**Biblioteca:** `rusqlite` no backend Rust  
**Arquivo:** `~/.config/gDownloader/downloads.db` (Electron `userData` path, passado como arg ao backend)

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS downloads (
  id           TEXT PRIMARY KEY,
  url          TEXT NOT NULL,
  provider     TEXT,
  filename     TEXT,
  dest_path    TEXT,
  size         INTEGER DEFAULT 0,
  bytes_downloaded INTEGER DEFAULT 0,
  status       TEXT DEFAULT 'pending',
  error        TEXT,
  retry_count  INTEGER DEFAULT 0,
  retry_at     INTEGER,
  created_at   INTEGER,
  updated_at   INTEGER
);
```

**Write path:**
- Insert ao adicionar download
- Update a cada mudança de status
- Update de `bytes_downloaded` a cada 5 segundos (não por chunk)

**Startup recovery:**
1. Backend lê downloads com status `downloading` ou `paused`
2. Para cada um: faz `HEAD` request na URL
   - 404/403/gone → marca `error: "Link expirado ou indisponível"`
   - Suporta `Accept-Ranges: bytes` → coloca em fila com `bytes_downloaded` preservado
   - Não suporta range → reinicia do zero
3. Frontend recebe via `GET /downloads` — sem mudança de interface

---

## 4. 1Fichier — Integração Completa

**Rename:** `backend/src/providers/fichier.rs` → `backend/src/providers/1fichier.rs`  
Em `mod.rs`: `#[path = "1fichier.rs"] mod fichier;` (mantém o ident Rust como `fichier`)

**Funcionalidades:**
- Suporte a pastas (listar arquivos via HTML scraping da página de pasta)
- Download individual e em lote (respeitar `selected_children`)
- Rate limit handling:
  - Parsear tempo de espera da resposta HTML (ex: `"Vous devez attendre X minutes"` / "You must wait X minutes")
  - Guardar `retry_at = now + wait_seconds` no SQLite
  - UI mostra countdown timer no card do download
  - Backend agenda retry automático quando timer expirar

**Provider name:** `"1Fichier"` (com o 1) no campo `provider` e nos badges da UI

---

## 5. Commit de Pendências

Commitar tudo que está staged/modified antes de começar implementação de novos itens.

---

## 6. Smart Retry Timing por Servidor

**Problema:** Cada servidor tem tempo de bloqueio diferente (1h, 8h...). Não podemos usar intervalo fixo.

**Solução:** Cada provider parseia o tempo de espera da própria resposta.

**Por provider:**
- **1Fichier:** HTML com texto `"wait X minutes"` ou `"attendre X minutes"` — regex para extrair minutos
- **Mega:** Resposta JSON de quota com campo `wait_s` (segundos até reset)
- **Rapidgator:** Resposta HTML/JSON com `"Try again in X hours"` ou similar
- **Genérico:** Header `Retry-After` (RFC 7231) se presente

**Struct no Rust:**
```rust
pub enum ProviderError {
    RateLimit { retry_after_secs: u64, message: String },
    // ...outros variants
}
```

**UI:**
- Card do download mostra badge `"Aguardando Xh Xm"` com countdown em tempo real
- Countdown atualizado pelo frontend via JS `setInterval`
- Quando zerar: backend dispara retry automático

---

## 7. Sistema de Captcha

### 7a. Captcha Manual (inline no card)

**Quando acionado:** Provider retorna `ProviderError::CaptchaRequired { challenge_url, captcha_type }` 

**Fluxo:**
1. Download trava com status `waiting_captcha`
2. Card do download expande mostrando webview inline
3. Webview carrega `challenge_url` com CSS injection ocultando tudo exceto o widget do captcha
4. Usuário resolve → webview captura o token via `executeJavaScript`
5. Token vai para o provider → download continua

**Tipos suportados:** reCaptcha v2, hCaptcha

### 7b. NoPecha API (auto-resolve)

**Config:** Campo `nopechaApiKey?: string` em `PersistedSettings`

**Fluxo:**
1. Antes de mostrar captcha pro usuário, tenta resolver via NoPecha API
2. Se `nopechaApiKey` configurada: POST para NoPecha com `sitekey` + `pageurl` + tipo
3. Polling até ter token (max 120s) → passa ao provider
4. Se falhar ou sem key: mostra captcha manual

**Settings UI:** Campo na aba de configurações para inserir NoPecha API key

---

## 8. Provider Rapidgator

**URL pattern:** `rapidgator.net/file/`

**Funcionalidades:**
- Scraping de metadados (nome, tamanho) da página de download
- Detecção de rate limit (free users têm wait time)
- Detecção de captcha (reCaptcha v2 na página de download)
- Integração com sistema de captcha do item 7
- Integração com smart retry do item 6
- Download direto após resolver captcha + obter link

**Arquivo:** `backend/src/providers/rapidgator.rs`

---

## 9. README Update

Atualizar `README.md` ao final com:
- Todos os providers suportados (incluindo 1Fichier, Terabox, Rapidgator)
- Menção ao sistema de captcha e NoPecha
- Menção à persistência SQLite e resume de downloads
- Atualizar badges se necessário

---

## Ordem de Implementação

1. Commit de pendências atuais
2. Settings: `accentColor` persistido
3. SQLite: schema + write path + startup recovery
4. 1Fichier: rename + pasta + rate limit
5. Smart retry timing (genérico, beneficia 1Fichier + Mega)
6. Terabox: BrowserView login (remover RSA)
7. Captcha system: infra + NoPecha + UI inline
8. Rapidgator provider
9. README
